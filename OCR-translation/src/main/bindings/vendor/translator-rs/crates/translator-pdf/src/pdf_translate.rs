//! Glue: extract → translate per page.

use crate::pdf::PageTranslationResult;
use crate::pdf::PdfError;
use crate::pdf_text::extract_text;
use translator_core::api::{ScriptedLanguage, TranslatorError};
use translator_core::settings::BackgroundMode;
use translator_translate::document_translator::DocumentTranslator;

#[derive(Debug)]
pub enum PdfTranslateError {
    Pdf(PdfError),
    Translator(TranslatorError),
    NoTextFound,
    Cancelled,
}

impl From<PdfError> for PdfTranslateError {
    fn from(value: PdfError) -> Self {
        Self::Pdf(value)
    }
}

impl From<TranslatorError> for PdfTranslateError {
    fn from(value: TranslatorError) -> Self {
        Self::Translator(value)
    }
}

impl std::fmt::Display for PdfTranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pdf(err) => write!(f, "{err}"),
            Self::Translator(err) => write!(f, "translator: {err:?}"),
            Self::NoTextFound => write!(f, "no extractable text found in PDF"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for PdfTranslateError {}

/// Extract every page's text via mupdf, run each through the structured
/// translation path, and return per-page translated blocks.
///
/// To also translate raster image XObjects embedded in the PDF, run
/// `pdf_image_translate::translate_pdf_images_in_place` on the input bytes
/// first and pass the result here — image translation lives in the
/// `image-translate` feature, not here.
pub fn translate_pdf(
    translator: &dyn DocumentTranslator,
    pdf_bytes: &[u8],
    forced_source_code: Option<&str>,
    target: &translator_core::api::ScriptedLanguage,
    available_languages: &[ScriptedLanguage],
) -> Result<Vec<PageTranslationResult>, PdfTranslateError> {
    translate_pdf_with_progress(
        translator,
        pdf_bytes,
        forced_source_code,
        target,
        available_languages,
        |_| {},
    )
}

/// Translate every page's text in a single bergamot call (slimt's batcher packs
/// all pages' sentences across the worker pool). Progress is reported per
/// sentence from worker threads via `on_progress` (cheap, non-blocking,
/// thread-safe), mapped onto the page count. Cancellation is requested
/// out-of-band via the host's `cancel_ongoing_work` and surfaces as
/// [`PdfTranslateError::Cancelled`].
pub fn translate_pdf_with_progress(
    translator: &dyn DocumentTranslator,
    pdf_bytes: &[u8],
    forced_source_code: Option<&str>,
    target: &translator_core::api::ScriptedLanguage,
    available_languages: &[ScriptedLanguage],
    on_progress: impl Fn(f32) + Sync,
) -> Result<Vec<PageTranslationResult>, PdfTranslateError> {
    translator.begin_document_translation();
    let extracted = extract_text(pdf_bytes)?;
    if extracted.iter().all(|page| page.fragments.is_empty()) {
        return Err(PdfTranslateError::NoTextFound);
    }

    on_progress(0.0);

    let pages_fragments = extracted
        .iter()
        .map(|page| page.fragments.as_slice())
        .collect::<Vec<_>>();
    let report = |done: usize, total: usize| {
        if total > 0 {
            on_progress(done as f32 / total as f32);
        }
    };
    let translated = match crate::styled::translate_structured_fragments_batch_ctx(
        translator,
        &pages_fragments,
        forced_source_code,
        target.as_str(),
        available_languages,
        BackgroundMode::BlackOnWhite,
        &report,
    ) {
        Ok(Some(translated)) => translated,
        Ok(None) => return Err(PdfTranslateError::Cancelled),
        Err(message) => {
            return Err(PdfTranslateError::Translator(TranslatorError::translation(
                message,
            )));
        }
    };

    let results = extracted
        .into_iter()
        .zip(translated)
        .map(|(page, result)| PageTranslationResult {
            page_index: page.page_index,
            page: page.page,
            blocks: result.blocks,
            error: result.error_message,
            target_language: target.clone(),
        })
        .collect();

    on_progress(1.0);

    Ok(results)
}
