use crate::translate::TranslationWithAlignment;
use translator_core::api::{LanguageCode, TranslatorError};

/// The translation capability the document formats need from the host
/// session: reset of the per-document cancel flag, plus the cancellable,
/// progress-reporting translate calls. Implemented by the facade session so
/// this crate stays free of the session/catalog/engine types.
pub trait DocumentTranslator: Sync {
    fn begin_document_translation(&self);

    fn translate_texts_ctx(
        &self,
        from_code: &str,
        to_code: &str,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Vec<String>, TranslatorError>;

    fn translate_texts_with_alignment_ctx(
        &self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Option<Vec<TranslationWithAlignment>>, TranslatorError>;
}
