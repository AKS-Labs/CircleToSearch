use std::path::Path;

use mnn_sys::{InferenceConfig, InferenceEngine, MemoryMode, PrecisionMode};

use translator_core::api::{TranslatorError, TranslatorErrorKind};

pub(crate) struct MnnSession {
    engine: InferenceEngine,
}

/// `MemoryMode::Low` selects MNN's dynamic int8 GEMM, which is only fast on
/// cores with sdot (armv8.2 dotprod). Without it (e.g. SDM670) that path is
/// ~3-4x slower, so gate on the hwcap. The fallback must be `High`, not
/// `Normal`: with `MNN_CPU_WEIGHT_DEQUANT_GEMM` compiled in, MNN routes any
/// mode below High into the per-tile weight-dequant GEMM executor, which
/// skips the Strassen-1x1/Winograd conv paths and is ~2x slower again. High
/// fully dequantizes weights at load, re-enabling those paths.
#[cfg(all(
    target_arch = "aarch64",
    any(target_os = "linux", target_os = "android")
))]
fn default_memory_mode() -> MemoryMode {
    const HWCAP_ASIMDDP: libc::c_ulong = 1 << 20;
    if unsafe { libc::getauxval(libc::AT_HWCAP) } & HWCAP_ASIMDDP != 0 {
        MemoryMode::Low
    } else {
        MemoryMode::High
    }
}

#[cfg(not(all(
    target_arch = "aarch64",
    any(target_os = "linux", target_os = "android")
)))]
fn default_memory_mode() -> MemoryMode {
    MemoryMode::Low
}

impl MnnSession {
    pub fn load(model_path: &Path, intra_threads: usize) -> Result<Self, TranslatorError> {
        Self::load_with_modes(
            model_path,
            intra_threads,
            PrecisionMode::Low,
            default_memory_mode(),
        )
    }

    /// Load with `MemoryMode::High`: fully dequantizes quantized weights at load,
    /// re-enabling the Strassen-1x1 / Winograd conv paths. Right for conv-only models
    /// (e.g. the ink matte) — `Low` would route them into the slow per-tile
    /// weight-dequant GEMM that skips those paths.
    pub fn load_conv(model_path: &Path, intra_threads: usize) -> Result<Self, TranslatorError> {
        Self::load_with_modes(
            model_path,
            intra_threads,
            PrecisionMode::Low,
            MemoryMode::High,
        )
    }

    pub fn load_with_modes(
        model_path: &Path,
        intra_threads: usize,
        precision: PrecisionMode,
        memory: MemoryMode,
    ) -> Result<Self, TranslatorError> {
        let config = InferenceConfig::new()
            .with_threads(intra_threads as i32)
            .with_precision(precision)
            .with_memory(memory);
        let engine = InferenceEngine::from_file(model_path, Some(config)).map_err(|error| {
            TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!(
                    "failed to load MNN model at {}: {error}",
                    model_path.display()
                ),
            )
        })?;
        Ok(Self { engine })
    }

    pub fn run(
        &self,
        input: &[f32],
        input_shape: &[usize],
    ) -> Result<(Vec<f32>, Vec<usize>), TranslatorError> {
        self.engine
            .run_dynamic_raw(input, input_shape)
            .map_err(|e| {
                TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    format!("MNN inference failed: {e}"),
                )
            })
    }
}
