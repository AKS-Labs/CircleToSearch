use std::fs;
use std::path::PathBuf;

use translator_convert::{convert, WeightQuant};

/// Real ONNX→MNN conversion using the repo bucket's docaligner if present.
/// Skips silently when the private bucket file is absent. This also exercises
/// the downstream linking of the MNN converter (the consuming binary must pull
/// in `libMNNConvertDeps`/`libMNN`/protobuf via `mnn-sys`).
#[test]
fn converts_docaligner_onnx_to_mnn() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .map(|home| home.join("AndroidStudioProjects/bucket/support/1/docaligner_lcnet050.onnx"))
        .expect("repo layout has a home dir");
    if !src.exists() {
        return;
    }

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("translator-convert-smoke-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    let mnn = dir.join("docaligner_lcnet050.mnn");

    convert(&src, &mnn, WeightQuant::Bits(8)).expect("conversion should succeed");

    assert!(mnn.exists(), "converted .mnn must exist");
    assert!(
        mnn.metadata().unwrap().len() > 0,
        "converted .mnn must be non-empty"
    );

    fs::remove_dir_all(&dir).ok();
}
