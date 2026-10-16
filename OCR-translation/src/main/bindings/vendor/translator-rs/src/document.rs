//! Whole-document translation (txt/odt/epub/pdf) over the per-format
//! pipelines, with one progress/cancel contract for every host.

#[cfg(feature = "pdf")]
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::api::ScriptedLanguage;
use crate::font_provider::FontProvider;
use crate::txt::TxtLayout;
use crate::{LanguageCode, TranslatorSession};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentFormat {
    Txt,
    #[cfg(feature = "odt")]
    Odt,
    #[cfg(feature = "epub")]
    Epub,
    #[cfg(feature = "pdf")]
    Pdf,
}

impl DocumentFormat {
    pub const ALL: &'static [DocumentFormat] = &[
        DocumentFormat::Txt,
        #[cfg(feature = "odt")]
        DocumentFormat::Odt,
        #[cfg(feature = "epub")]
        DocumentFormat::Epub,
        #[cfg(feature = "pdf")]
        DocumentFormat::Pdf,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            DocumentFormat::Txt => "txt",
            #[cfg(feature = "odt")]
            DocumentFormat::Odt => "odt",
            #[cfg(feature = "epub")]
            DocumentFormat::Epub => "epub",
            #[cfg(feature = "pdf")]
            DocumentFormat::Pdf => "pdf",
        }
    }

    /// Formats this build can translate; a format compiled out is unknown here.
    pub fn from_extension(extension: &str) -> Option<Self> {
        let extension = extension.to_ascii_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|format| format.extension() == extension)
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let extension = Path::new(path).extension()?.to_str()?;
        Self::from_extension(extension)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DocumentProgress {
    Preparing,
    /// PDF-only: emitted once after inventory, before any pass starts, so a UI
    /// can show three labelled bars (text / images / raster pages) with their
    /// totals known up-front. `raster_pages` is an upper bound; the raster pass
    /// refines it by reporting a smaller `total` in its ticks.
    PdfPlan {
        text_pages: u32,
        image_xobjects: u32,
        raster_pages: u32,
    },
    /// Smooth, source-length weighted completion fraction in `[0.0, 1.0]` for
    /// every text path (txt/odt/epub and the PDF text pass).
    TranslatingText {
        fraction: f32,
    },
    TranslatingImages {
        current: u32,
        total: u32,
    },
    TranslatingRasterPages {
        current: u32,
        total: u32,
    },
    Writing,
}

#[derive(Debug)]
pub enum DocumentError {
    Cancelled,
    Other(String),
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DocumentError::Cancelled => f.write_str("cancelled"),
            DocumentError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for DocumentError {}

pub struct DocumentOptions<'a> {
    /// `None` lets odt/epub/pdf detect the source per fragment; txt has no
    /// structure to detect over and requires it.
    pub forced_source_code: Option<&'a str>,
    pub target_code: &'a str,
    pub translate_pdf_images: bool,
    pub txt_layout: TxtLayout,
    pub fonts: &'a (dyn FontProvider + Send + Sync),
}

fn installed_languages(session: &TranslatorSession) -> Vec<ScriptedLanguage> {
    session
        .language_rows()
        .into_iter()
        .filter(|row| row.availability.translator_files() || row.language.is_english())
        .map(|row| row.language.scripted())
        .collect()
}

pub fn translate_document_bytes(
    session: &TranslatorSession,
    format: DocumentFormat,
    input_bytes: &[u8],
    options: &DocumentOptions<'_>,
    on_progress: &(dyn Fn(DocumentProgress) + Sync),
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<Vec<u8>, DocumentError> {
    let check_cancelled = || {
        if is_cancelled() {
            Err(DocumentError::Cancelled)
        } else {
            Ok(())
        }
    };
    // The text translators report a fraction per sentence from slimt worker
    // threads; forward only when it advances by ≥0.1% so the host's UI thread
    // (or FFI boundary) is not flooded.
    let last_permille = AtomicUsize::new(0);
    let report_text = |fraction: f32| {
        let permille = (fraction * 1000.0) as usize;
        let prev = last_permille.fetch_max(permille, Ordering::Relaxed);
        if permille > prev || fraction >= 1.0 {
            on_progress(DocumentProgress::TranslatingText { fraction });
        }
    };

    check_cancelled()?;
    on_progress(DocumentProgress::Preparing);
    let target_code = options.target_code;
    let target = session
        .scripted_language(&LanguageCode::from(target_code))
        .ok_or_else(|| {
            DocumentError::Other(format!(
                "target language {target_code} is not in the catalog"
            ))
        })?;
    let available = installed_languages(session);
    check_cancelled()?;

    let output_bytes = match format {
        DocumentFormat::Txt => {
            let source_code = options.forced_source_code.ok_or_else(|| {
                DocumentError::Other("source language is required for text documents".to_string())
            })?;
            let text = String::from_utf8(input_bytes.to_vec()).map_err(|error| {
                DocumentError::Other(format!("text document is not UTF-8: {error}"))
            })?;
            crate::txt::translate_txt_with_progress(
                session,
                &text,
                source_code,
                target_code,
                options.txt_layout,
                report_text,
            )
            .map_err(|error| match error {
                crate::txt::TxtTranslateError::Cancelled => DocumentError::Cancelled,
                crate::txt::TxtTranslateError::Translation(message) => {
                    DocumentError::Other(format!("failed to translate text: {message}"))
                }
            })?
            .into_bytes()
        }
        #[cfg(feature = "odt")]
        DocumentFormat::Odt => crate::odt::translate_odt_with_progress(
            session,
            input_bytes,
            options.forced_source_code,
            target_code,
            &available,
            report_text,
        )
        .map_err(|error| match error {
            crate::odt::OdtTranslateError::Cancelled => DocumentError::Cancelled,
            other => DocumentError::Other(format!("failed to translate ODT: {other}")),
        })?,
        #[cfg(feature = "epub")]
        DocumentFormat::Epub => crate::epub::translate_epub_with_progress(
            session,
            input_bytes,
            options.forced_source_code,
            target_code,
            &available,
            report_text,
        )
        .map_err(|error| match error {
            crate::epub::EpubTranslateError::Cancelled => DocumentError::Cancelled,
            other => DocumentError::Other(format!("failed to translate EPUB: {other}")),
        })?,
        #[cfg(feature = "pdf")]
        DocumentFormat::Pdf => translate_pdf(
            session,
            input_bytes,
            &target,
            &available,
            options,
            is_cancelled,
            on_progress,
            &report_text,
        )?,
    };
    let _ = (&target, &available);

    check_cancelled()?;
    Ok(output_bytes)
}

/// Read `input_path`, translate it, and write the result to `output_path`,
/// creating parent directories as needed. The format comes from the input
/// path's extension.
pub fn translate_document_path(
    session: &TranslatorSession,
    input_path: &str,
    output_path: &str,
    options: &DocumentOptions<'_>,
    on_progress: &(dyn Fn(DocumentProgress) + Sync),
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), DocumentError> {
    let format = DocumentFormat::from_path(input_path)
        .ok_or_else(|| DocumentError::Other(format!("unsupported document type: {input_path}")))?;
    let input_bytes = fs::read(input_path)
        .map_err(|error| DocumentError::Other(format!("failed to read document: {error}")))?;
    let output_bytes = translate_document_bytes(
        session,
        format,
        &input_bytes,
        options,
        on_progress,
        is_cancelled,
    )?;

    on_progress(DocumentProgress::Writing);
    if is_cancelled() {
        return Err(DocumentError::Cancelled);
    }
    if let Some(parent) = Path::new(output_path).parent() {
        fs::create_dir_all(parent).map_err(|error| {
            DocumentError::Other(format!("failed to create output dir: {error}"))
        })?;
    }
    fs::write(output_path, output_bytes).map_err(|error| {
        DocumentError::Other(format!("failed to write translated document: {error}"))
    })
}

/// Pipeline order: text translation first, then image-XObject translation,
/// then page-raster overlay. Each later pass must not see its own output:
/// text surgery after the overlay would re-process the overlay's `Tj` ops
/// and embed duplicate fonts, and XObject re-encoding after the raster pass
/// would bake redundant translated text into images the overlay also covers.
#[cfg(feature = "pdf")]
#[allow(clippy::too_many_arguments)]
fn translate_pdf(
    session: &TranslatorSession,
    input_bytes: &[u8],
    target: &ScriptedLanguage,
    available: &[ScriptedLanguage],
    options: &DocumentOptions<'_>,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    on_progress: &(dyn Fn(DocumentProgress) + Sync),
    report_text: &(dyn Fn(f32) + Sync),
) -> Result<Vec<u8>, DocumentError> {
    let source_code = options.forced_source_code;
    let fonts = options.fonts;
    // Image passes OCR the page, which needs a known source language; without
    // one only the text pass runs.
    let image_source = source_code.filter(|_| options.translate_pdf_images);

    #[cfg(feature = "pdf-image-translate")]
    let overlay_pages: HashSet<usize> = if image_source.is_some() {
        crate::pdf_image_translate::log_page_inventory(input_bytes);
        let pages = crate::pdf_image_translate::pages_without_extractable_text(input_bytes);
        if let Some(inv) = crate::pdf_image_translate::pdf_translation_inventory(input_bytes) {
            on_progress(DocumentProgress::PdfPlan {
                text_pages: inv.total_pages,
                image_xobjects: inv.image_xobjects,
                raster_pages: inv.raster_pages,
            });
        }
        pages
    } else {
        HashSet::new()
    };

    let translations = match crate::pdf_translate::translate_pdf_with_progress(
        session,
        input_bytes,
        source_code,
        target,
        available,
        report_text,
    ) {
        Ok(translations) => translations,
        // No native text, but image translation may still add overlay
        // content; the writer round-trips the bytes for an empty set.
        Err(crate::pdf_translate::PdfTranslateError::NoTextFound) => Vec::new(),
        Err(crate::pdf_translate::PdfTranslateError::Cancelled) => {
            return Err(DocumentError::Cancelled);
        }
        Err(error) => {
            return Err(DocumentError::Other(format!(
                "failed to translate PDF: {error}"
            )));
        }
    };
    let after_text = crate::pdf_write::write_translated_pdf(input_bytes, &translations, fonts)
        .map_err(|error| DocumentError::Other(format!("failed to write PDF: {error}")))?;

    #[cfg(not(feature = "pdf-image-translate"))]
    {
        let _ = (image_source, is_cancelled);
        Ok(after_text)
    }
    #[cfg(feature = "pdf-image-translate")]
    {
        let Some(source_code) = image_source else {
            return Ok(after_text);
        };
        let xobject_progress = |current: usize, total: usize| {
            on_progress(DocumentProgress::TranslatingImages {
                current: current as u32,
                total: total as u32,
            });
        };
        let xobject_output = crate::pdf_image_translate::translate_pdf_images_in_place(
            &after_text,
            session,
            source_code,
            target.as_str(),
            fonts,
            &overlay_pages,
            is_cancelled,
            xobject_progress,
        )
        .map_err(|error| {
            DocumentError::Other(format!("failed to translate PDF images: {error}"))
        })?;
        if is_cancelled() {
            return Err(DocumentError::Cancelled);
        }

        // Pages whose visible content was already translated via image
        // XObjects don't need a raster overlay stamped on the translated bitmap.
        let raster_pages: HashSet<usize> = overlay_pages
            .difference(&xobject_output.translated_pages)
            .copied()
            .collect();
        let page_progress = |current: usize, total: usize| {
            on_progress(DocumentProgress::TranslatingRasterPages {
                current: current as u32,
                total: total as u32,
            });
        };
        let final_bytes = crate::pdf_image_translate::translate_pdf_pages_as_raster_in_place(
            &xobject_output.bytes,
            session,
            source_code,
            target,
            fonts,
            &raster_pages,
            is_cancelled,
            page_progress,
        )
        .map_err(|error| DocumentError::Other(format!("failed to rasterize PDF pages: {error}")))?;
        if is_cancelled() {
            return Err(DocumentError::Cancelled);
        }
        Ok(final_bytes)
    }
}
