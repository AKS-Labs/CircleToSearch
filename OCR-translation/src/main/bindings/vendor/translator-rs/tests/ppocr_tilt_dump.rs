//! Diagnostic for tilt-estimation: loads a tilted real-world image, runs PaddleOCR through
//! the same pipeline as production, and prints each detection's recognised text together with
//! the oriented-box angle. The tilt estimator itself emits `log::debug!` lines per contour
//! (top_slope, bot_slope, residual-filter diff, decision) when run with RUST_LOG=debug, so
//! between this dump and those logs we can see exactly which contours stopped tilting and
//! why.
//!
//! Run with the cardboard fixture:
//!     PPOCR_TILT_IMAGE=files/tilted-multilang.jpg \
//!     PPOCR_TILT_DET=~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5/PP-OCRv5_mobile_det.mnn \
//!     PPOCR_TILT_REC=~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5/latin_PP-OCRv5_mobile_rec_infer.mnn \
//!     PPOCR_TILT_KEYS=~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5/latin_PP-OCRv5_keys.txt \
//!     RUST_LOG=translator::ppocr=debug \
//!     cargo test --features ppocr --test ppocr_tilt_dump -- --nocapture
//!
//! Skips silently when the env vars aren't set so cargo test --all stays green.

#![cfg(feature = "ppocr")]

use std::path::PathBuf;

use image::{DynamicImage, ImageReader};
use translator::PpocrScript;
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};

#[test]
fn dump_tilt_for_real_image() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some(image_path) = env_var_path("PPOCR_TILT_IMAGE") else {
        eprintln!("PPOCR_TILT_IMAGE not set — skipping diagnostic");
        return;
    };
    let Some(det_path) = env_var_path("PPOCR_TILT_DET") else {
        eprintln!("PPOCR_TILT_DET not set — skipping diagnostic");
        return;
    };
    let Some(rec_path) = env_var_path("PPOCR_TILT_REC") else {
        eprintln!("PPOCR_TILT_REC not set — skipping diagnostic");
        return;
    };
    let Some(keys_path) = env_var_path("PPOCR_TILT_KEYS") else {
        eprintln!("PPOCR_TILT_KEYS not set — skipping diagnostic");
        return;
    };

    let dyn_image: DynamicImage = ImageReader::open(&image_path)
        .expect("open image")
        .decode()
        .expect("decode image");

    let recognizer_spec = PpocrRecognizerSpec {
        script: PpocrScript::Latin,
        model_path: rec_path.clone(),
        keys_path: keys_path.clone(),
    };
    let engine = PpocrEngine::load(&det_path, None, None, vec![recognizer_spec], 1, None)
        .expect("load ppocr");
    let det_boxes = engine
        .detect_only_image(&dyn_image, PpocrProfile::Still)
        .expect("detection succeeds");
    let scripts = vec![PpocrScript::Latin; det_boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(&dyn_image, &det_boxes, &scripts, PpocrProfile::Still, None)
        .expect("recognize succeeds");

    eprintln!("\n=== {} detections ===", lines.len());
    for (i, line) in lines.iter().enumerate() {
        let angle_deg = line.oriented_box.angle_radians.to_degrees();
        eprintln!(
            "[{:>2}] angle={:+6.2}°  conf={:.2}  text={:?}",
            i, angle_deg, line.confidence, line.text,
        );
    }
    eprintln!("=========================\n");
}

fn env_var_path(key: &str) -> Option<PathBuf> {
    let raw = std::env::var(key).ok()?;
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").ok()?;
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(raw)
    };
    if !expanded.exists() {
        eprintln!("{} → {} does not exist; skipping", key, expanded.display());
        return None;
    }
    Some(expanded)
}
