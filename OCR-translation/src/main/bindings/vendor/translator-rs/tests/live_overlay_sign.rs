#![cfg(feature = "ppocr")]

use std::path::{Path, PathBuf};

use image::ImageReader;
use translator::DetectedTextBox;
use translator::PpocrScript;
use translator::live_frame::OrientedImage;
use translator::ocr::{OrientedRect, Rect, TextBlock, TextLine, group_live_lines_into_blocks};
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};

const DET_MAX_PIXELS: u32 = 350_000;
const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";

#[test]
fn live_overlay_sign_lines_group_into_one_translation_unit() {
    let Some(output) = run_live_fixture("files/live-overlay/sign.jpg") else {
        return;
    };
    eprintln!("live sign recognised lines: {:?}", output.recognised_text);
    eprintln!("live sign blocks: {:?}", block_texts(&output.blocks));

    assert!(
        output.recognised_text.len() >= 3,
        "expected at least 3 sign lines, got {}: {:?}",
        output.recognised_text.len(),
        output.recognised_text,
    );
    assert!(
        output
            .recognised_text
            .iter()
            .any(|line| line.contains("Levensgevaarlijke")),
        "missing expected first sign line: {:?}",
        output.recognised_text,
    );

    assert_eq!(
        output.blocks.len(),
        1,
        "expected one live translation group"
    );
    let block_text = output.blocks[0].translation_text();
    assert!(block_text.contains("Levensgevaarlijke"));
    assert!(block_text.contains("obstakels"));
    assert!(block_text.contains("water"));
}

#[test]
fn live_overlay_medicine_keeps_compound_label_and_bottom_copy_as_groups() {
    let Some(output) = run_live_fixture("files/live-overlay/medicine.jpg") else {
        return;
    };
    eprintln!(
        "live medicine recognised lines: {:?}",
        output.recognised_text
    );
    let blocks = block_texts(&output.blocks);
    eprintln!("live medicine blocks: {blocks:?}");

    assert!(
        output
            .recognised_text
            .iter()
            .any(|line| line == "HOOGWAARDIGE"),
        "missing HOOGWAARDIGE: {:?}",
        output.recognised_text,
    );
    assert!(
        output
            .recognised_text
            .iter()
            .any(|line| line == "KWALITEIT"),
        "missing KWALITEIT: {:?}",
        output.recognised_text,
    );

    assert!(
        blocks
            .iter()
            .any(|block| { block.contains("HOOGWAARDIGE") && block.contains("KWALITEIT") }),
        "expected HOOGWAARDIGE/KWALITEIT to be one translation unit: {blocks:?}",
    );
    assert!(
        blocks.iter().any(|block| {
            block.contains("OPTIMAAL")
                && block.contains("OPNEEMBAAR")
                && block.contains("IS GOED VOOR")
                && block.contains("VERMOEIDHEID")
        }),
        "expected bottom-right supplement copy to be one translation unit: {blocks:?}",
    );
    assert!(
        !blocks
            .iter()
            .any(|block| block.contains("200") && block.contains("mg")),
        "200 and mg should not be merged into a translatable phrase: {blocks:?}",
    );
}

#[test]
fn live_overlay_book_keeps_title_lines_as_one_group_without_absorbing_surroundings() {
    assert_book_title_fixture("files/live-overlay/book.jpg");
}

#[test]
fn live_overlay_book_angle_keeps_title_lines_as_one_group_without_absorbing_surroundings() {
    assert_book_title_fixture("files/live-overlay/book-angle.jpg");
}

fn assert_book_title_fixture(image: &str) {
    let Some(output) = run_live_fixture(image) else {
        return;
    };
    eprintln!("live book recognised lines: {:?}", output.recognised_text);
    let blocks = block_texts(&output.blocks);
    eprintln!("live book blocks: {blocks:?}");

    assert!(
        output
            .recognised_text
            .iter()
            .any(|line| line == "Designing"),
        "missing title opener: {:?}",
        output.recognised_text,
    );
    assert!(
        blocks.iter().any(|block| {
            block.contains("Designing")
                && block.contains("Data-Intensive")
                && block.contains("Applications")
        }),
        "expected title lines to be one translation unit: {blocks:?}",
    );
    assert!(
        !blocks
            .iter()
            .any(|block| { block.contains("O'REILLY") && block.contains("Designing") }),
        "publisher label should not merge into title: {blocks:?}",
    );
    assert!(
        !blocks
            .iter()
            .any(|block| { block.contains("Applications") && block.contains("Martin Kleppmann") }),
        "author should not merge into title: {blocks:?}",
    );
}

struct LiveFixtureOutput {
    recognised_text: Vec<String>,
    blocks: Vec<TextBlock>,
}

fn run_live_fixture(default_image: &str) -> Option<LiveFixtureOutput> {
    let _ = env_logger::builder().is_test(true).try_init();

    let image_path = fixture_path(default_image)?;
    let Some((det_path, rec_path, keys_path)) = ppocr_paths() else {
        eprintln!("PPOCR live fixture model files missing; skipping");
        return None;
    };

    let rgba = ImageReader::open(&image_path)
        .expect("open live fixture")
        .decode()
        .expect("decode live fixture")
        .to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let crop = Rect {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let frame =
        OrientedImage::build_with_rgb(&rgba.into_raw(), width, height, 0, crop, DET_MAX_PIXELS)
            .expect("build live frame");
    let rgb = frame.rgb.as_ref().expect("rgb populated");
    let rgb_det = frame.rgb_det.as_ref().expect("rgb_det populated");

    let recognizer_spec = PpocrRecognizerSpec {
        script: PpocrScript::Latin,
        model_path: rec_path.clone(),
        keys_path: keys_path.clone(),
    };
    let engine = PpocrEngine::load(&det_path, None, None, vec![recognizer_spec], 1, None)
        .expect("load ppocr");
    let det_boxes = engine
        .detect_only_image(rgb_det, PpocrProfile::Live)
        .expect("live detection succeeds");
    let boxes: Vec<_> = det_boxes
        .into_iter()
        .map(|b| scale_detected_box(b, frame.det_to_full.0, rgb.width(), rgb.height()))
        .collect();
    let scripts = vec![PpocrScript::Latin; boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(rgb, &boxes, &scripts, PpocrProfile::Live, None)
        .expect("live recognition succeeds");
    let recognised: Vec<_> = lines
        .into_iter()
        .zip(boxes.iter())
        .filter(|(line, _)| !line.text.trim().is_empty())
        .collect();
    let recognised_text = recognised
        .iter()
        .map(|(line, _)| line.text.clone())
        .collect();
    let text_lines: Vec<TextLine> = recognised
        .into_iter()
        .map(|(line, b)| TextLine {
            text: line.text,
            bounding_box: line.rect,
            oriented_box: line.oriented_box,
            tight_box: b.tight_box,
            word_rects: vec![line.rect],
            style_ranges: Vec::new(),
        })
        .collect();
    let blocks = group_live_lines_into_blocks(text_lines);

    Some(LiveFixtureOutput {
        recognised_text,
        blocks,
    })
}

fn fixture_path(default_image: &str) -> Option<PathBuf> {
    let path =
        env_var_path("LIVE_OVERLAY_FIXTURE_IMAGE").unwrap_or_else(|| PathBuf::from(default_image));
    if !path.exists() {
        eprintln!("live overlay fixture {} missing; skipping", path.display());
        return None;
    }
    Some(path)
}

fn ppocr_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let det = env_var_path("LIVE_OVERLAY_FIXTURE_DET")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn"));
    let rec = env_var_path("LIVE_OVERLAY_FIXTURE_REC")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn"));
    let keys = env_var_path("LIVE_OVERLAY_FIXTURE_KEYS")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_keys.txt"));
    if det.exists() && rec.exists() && keys.exists() {
        Some((det, rec, keys))
    } else {
        None
    }
}

fn env_var_path(key: &str) -> Option<PathBuf> {
    let raw = std::env::var(key).ok()?;
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(rest))
    } else {
        Some(PathBuf::from(raw))
    }
}

fn block_texts(blocks: &[TextBlock]) -> Vec<String> {
    blocks.iter().map(TextBlock::translation_text).collect()
}

fn scale_detected_box(b: DetectedTextBox, scale: f32, max_w: u32, max_h: u32) -> DetectedTextBox {
    let left = ((b.rect.left as f32) * scale).max(0.0) as u32;
    let top = ((b.rect.top as f32) * scale).max(0.0) as u32;
    let right = ((b.rect.right as f32) * scale).min(max_w as f32) as u32;
    let bottom = ((b.rect.bottom as f32) * scale).min(max_h as f32) as u32;
    let rect = Rect {
        left: left.min(right.saturating_sub(1)),
        top: top.min(bottom.saturating_sub(1)),
        right: right.max(left + 1),
        bottom: bottom.max(top + 1),
    };
    DetectedTextBox {
        rect,
        oriented_box: scale_oriented(b.oriented_box, scale),
        tight_box: scale_oriented(b.tight_box, scale),
        contour: b.contour.into_iter().map(|v| v * scale).collect(),
        score: b.score,
    }
}

fn scale_oriented(rect: OrientedRect, scale: f32) -> OrientedRect {
    OrientedRect {
        cx: rect.cx * scale,
        cy: rect.cy * scale,
        width: rect.width * scale,
        height: rect.height * scale,
        angle_radians: rect.angle_radians,
    }
}
