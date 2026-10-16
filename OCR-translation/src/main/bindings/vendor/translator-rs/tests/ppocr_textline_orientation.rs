//! Synthetic 4-rotation test for `estimate_canonical_quadrant` and the
//! canonical-quadrant-aware dewarp inside `crop_text_strips`.
//!
//! Renders horizontal English text on a white background, then rotates it
//! to R0 / R90 / R180 / R270. For each rotation we:
//!   1. Detect text boxes via ppocr.
//!   2. Run `estimate_canonical_quadrant_in_oriented_image` and assert the
//!      result matches the expected quadrant.
//!   3. Recognise the lines with that quadrant and assert each line contains
//!      its expected substring (case-insensitive).
//!
//! Dumps every rotated image and recognised text to `smoke-out/ppocr-orient/`
//! for visual / log debugging when the assertion fails.
//!
//! Skips when the bucket models or the DejaVu font are missing.

#![cfg(all(feature = "ppocr", feature = "planar-tracker"))]

use std::path::{Path, PathBuf};

use ab_glyph::{FontArc, PxScale};
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;

use translator::coords::Quadrant;
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};
use translator::{DetectedTextBox, PpocrScript};
use translator_ocr::orientation::estimate_canonical_quadrant;

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf";
const DUMP_DIR: &str = "smoke-out/ppocr-orient";

const LINES: &[&str] = &["DESIGNING DATA", "INTENSIVE", "APPLICATIONS"];

#[test]
fn estimator_and_dewarp_handle_all_four_rotations() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some((det, rec, keys, textline_ori)) = ppocr_paths() else {
        eprintln!("PPOCR bucket files missing under {MODEL_DIR}; skipping");
        return;
    };
    let font_bytes = match std::fs::read(FONT_PATH) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("font {FONT_PATH} missing; skipping");
            return;
        }
    };
    let font = FontArc::try_from_vec(font_bytes).expect("parse font");

    std::fs::create_dir_all(DUMP_DIR).expect("create dump dir");

    let engine = PpocrEngine::load(
        &det,
        None,
        Some(&textline_ori),
        vec![PpocrRecognizerSpec {
            script: PpocrScript::Latin,
            model_path: rec,
            keys_path: keys,
        }],
        1,
        None,
    )
    .expect("load ppocr");

    let base_rgb = DynamicImage::ImageRgba8(render_text(&font, LINES));
    base_rgb
        .save(Path::new(DUMP_DIR).join("base_r0.png"))
        .expect("save base");

    let cases: [(&str, Quadrant, DynamicImage); 4] = [
        ("R0", Quadrant::R0, base_rgb.clone()),
        ("R90", Quadrant::R90, base_rgb.rotate90()),
        ("R180", Quadrant::R180, base_rgb.rotate180()),
        ("R270", Quadrant::R270, base_rgb.rotate270()),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (label, expected, rotated) in &cases {
        if let Err(e) = run_case(&engine, label, *expected, rotated) {
            failures.push(format!("{label}: {e}"));
        }
    }

    if !failures.is_empty() {
        panic!(
            "orientation test failures:\n  - {}",
            failures.join("\n  - ")
        );
    }
}

fn run_case(
    engine: &PpocrEngine,
    label: &str,
    expected: Quadrant,
    rotated: &DynamicImage,
) -> Result<(), String> {
    let rotated_path = Path::new(DUMP_DIR).join(format!("rotated_{label}.png"));
    rotated
        .save(&rotated_path)
        .map_err(|e| format!("save rotated: {e}"))?;

    let det_boxes: Vec<DetectedTextBox> = engine
        .detect_only_image(rotated, PpocrProfile::Still)
        .map_err(|e| format!("detect: {e:?}"))?;
    eprintln!(
        "[{}] detected {} boxes (expected ≥ {})",
        label,
        det_boxes.len(),
        LINES.len()
    );
    if det_boxes.is_empty() {
        return Err(format!("no detections (expected ≥ {})", LINES.len()));
    }

    let quadrant = estimate_canonical_quadrant(engine, rotated, &rotated.to_luma8(), &det_boxes);
    eprintln!("[{label}] estimator → {quadrant:?} (expected {expected:?})");

    let estimated = quadrant.ok_or_else(|| {
        format!("estimator returned None (expected Some({expected:?})); not enough consensus")
    })?;
    if estimated != expected {
        return Err(format!(
            "estimator returned {estimated:?}, expected {expected:?}"
        ));
    }

    let scripts = vec![PpocrScript::Latin; det_boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(
            rotated,
            &det_boxes,
            &scripts,
            PpocrProfile::Still,
            Some(estimated),
        )
        .map_err(|e| format!("recognize: {e:?}"))?;
    let recognised: Vec<String> = lines.iter().map(|l| l.text.clone()).collect();
    eprintln!("[{label}] recognised: {recognised:?}");
    std::fs::write(
        Path::new(DUMP_DIR).join(format!("recognised_{label}.txt")),
        recognised.join("\n"),
    )
    .ok();

    let recognised_lower: Vec<String> = recognised.iter().map(|s| s.to_ascii_lowercase()).collect();
    let mut missing: Vec<&str> = Vec::new();
    for needle in LINES {
        let needle_lc = needle.to_ascii_lowercase();
        let primary = needle_lc.split_whitespace().next().unwrap_or(&needle_lc);
        let found = recognised_lower.iter().any(|l| l.contains(primary));
        if !found {
            missing.push(needle);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "missing expected text snippets: {:?} in recognised: {:?}",
            missing, recognised
        ));
    }
    Ok(())
}

fn render_text(font: &FontArc, lines: &[&str]) -> RgbaImage {
    let scale = PxScale::from(56.0);
    let line_h = 80_i32;
    let pad_x = 30_i32;
    let pad_y = 40_i32;
    let max_chars: usize = lines.iter().map(|l| l.len()).max().unwrap_or(20);
    // approx 28 px per glyph at this scale
    let approx_w = max_chars as i32 * 28;
    let width = (approx_w + 2 * pad_x).max(360) as u32;
    let height = (line_h * lines.len() as i32 + 2 * pad_y) as u32;

    let bg = Rgba([255u8, 255, 255, 255]);
    let fg = Rgba([20u8, 20, 20, 255]);

    let mut img = RgbaImage::from_pixel(width, height, bg);
    for (i, line) in lines.iter().enumerate() {
        let y = pad_y + i as i32 * line_h;
        draw_text_mut(&mut img, fg, pad_x, y, scale, font, line);
    }
    img
}

fn ppocr_paths() -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let det = env_path("OCR_ORIENT_DET")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn"));
    let rec = env_path("OCR_ORIENT_REC")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn"));
    let keys = env_path("OCR_ORIENT_KEYS")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_keys.txt"));
    let textline_ori = env_path("OCR_ORIENT_TEXTLINE")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("textline_ori_x0_25_wq8.mnn"));
    if det.exists() && rec.exists() && keys.exists() && textline_ori.exists() {
        Some((det, rec, keys, textline_ori))
    } else {
        None
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    let raw = std::env::var(key).ok()?;
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(rest))
    } else {
        Some(PathBuf::from(raw))
    }
}
