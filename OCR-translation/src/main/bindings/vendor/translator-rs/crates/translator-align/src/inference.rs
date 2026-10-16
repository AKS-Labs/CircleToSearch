use std::path::Path;

use mnn_sys::{InferenceConfig, MemoryMode, ModuleEngine, PrecisionMode};

use translator_core::api::{TranslatorError, TranslatorErrorKind};

pub(crate) const MODEL_INPUT_NAME: &str = "img";
pub(crate) const POINTS_OUTPUT_NAME: &str = "points";
pub(crate) const HAS_OBJ_OUTPUT_NAME: &str = "has_obj";

pub(crate) fn load_doc_align_engine(
    model_path: &Path,
    intra_threads: usize,
) -> Result<ModuleEngine, TranslatorError> {
    // High precision (fp32) + High memory: corner regression is sensitive, so keep the
    // accurate conv paths rather than the fp16 / int8-GEMM modes the OCR models use.
    let config = InferenceConfig::new()
        .with_threads(intra_threads as i32)
        .with_precision(PrecisionMode::High)
        .with_memory(MemoryMode::High);
    ModuleEngine::from_file(
        model_path,
        &[MODEL_INPUT_NAME],
        &[POINTS_OUTPUT_NAME, HAS_OBJ_OUTPUT_NAME],
        Some(config),
    )
    .map_err(|error| {
        TranslatorError::new(
            TranslatorErrorKind::Internal,
            format!(
                "failed to load MNN doc-align model at {}: {error}",
                model_path.display()
            ),
        )
    })
}
