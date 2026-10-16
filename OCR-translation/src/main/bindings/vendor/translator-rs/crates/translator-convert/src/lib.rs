use std::path::{Path, PathBuf};

pub use mnn_sys::WeightQuant;

#[derive(Debug)]
pub struct ConvertError {
    pub onnx: PathBuf,
    pub source: mnn_sys::MnnError,
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to convert `{}`: {}",
            self.onnx.display(),
            self.source
        )
    }
}

impl std::error::Error for ConvertError {}

pub struct ConvertJob {
    pub onnx: PathBuf,
    pub mnn: PathBuf,
    pub quant: WeightQuant,
}

pub struct ConvertProgress<'a> {
    pub index: usize,
    pub total: usize,
    pub job: &'a ConvertJob,
}

pub fn convert(onnx: &Path, mnn: &Path, quant: WeightQuant) -> Result<(), ConvertError> {
    // Convert into a sibling temp, then atomically rename. A crash mid-conversion
    // must not leave a truncated `.mnn`: the migration planner treats any existing
    // `.mnn` as a finished conversion and deletes the source `.onnx`, which would
    // turn an interrupted run into an unrecoverable corrupt model.
    let mut tmp = mnn.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);

    mnn_sys::convert_onnx_to_mnn(onnx, &tmp, quant).map_err(|source| ConvertError {
        onnx: onnx.to_path_buf(),
        source,
    })?;

    std::fs::rename(&tmp, mnn).map_err(|error| {
        let _ = std::fs::remove_file(&tmp);
        ConvertError {
            onnx: onnx.to_path_buf(),
            source: mnn_sys::MnnError::RuntimeError(format!(
                "failed to finalize `{}`: {error}",
                mnn.display()
            )),
        }
    })
}

// MNN's runtime allocator is not safe across concurrent module conversions, so
// jobs run sequentially; `on_start` fires before each so callers can drive a
// per-file progress UI.
pub fn convert_all(
    jobs: &[ConvertJob],
    mut on_start: impl FnMut(ConvertProgress),
) -> Vec<Result<(), ConvertError>> {
    let total = jobs.len();
    jobs.iter()
        .enumerate()
        .map(|(index, job)| {
            on_start(ConvertProgress { index, total, job });
            convert(&job.onnx, &job.mnn, job.quant)
        })
        .collect()
}
