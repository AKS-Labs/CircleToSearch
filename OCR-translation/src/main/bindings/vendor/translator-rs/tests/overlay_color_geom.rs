//! End-to-end check that the real det→ink→overlay pipeline *produces* the per-line geometric
//! core colour we expect — not a hand-built overlay. Runs PPOCR detection and the ink model on the
//! synthetic `tests/fixtures` images, then reads `PreparedTextLine.foreground` (sampled from the
//! matte during erase) and asserts the colours track the input.
//!
//! Model-gated (skips when the bucket isn't present). Run:
//!   cargo test --release --features ppocr --test overlay_color_geom -- --nocapture
//!
//! Unlike the hermetic render test in translator-render, this exercises the matte sampling and
//! overlay population — the parts that would actually break on-device — so it's the test that says
//! "we get the expected geom from a real image". Recognition isn't needed: the colour comes from
//! the ink matte, not the decoded text.

#![cfg(feature = "ppocr")]

use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageReader};
use translator::DetectedTextBox;
use translator::color_matting::union_ink_mask;
use translator::ocr::{ReadingOrder, TextBlock, TextLine, sample_line_color};
use translator::overlay::prepare_overlay_image;
use translator::ppocr::{PpocrEngine, PpocrProfile};
use translator::settings::BackgroundMode;

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";

/// One detected line's core colour and its vertical position, for ordering top-to-bottom.
struct LineColor {
    y: f32,
    lum: i32,
}

fn run(image: &str) -> Option<Vec<LineColor>> {
    let det_path = Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn");
    let ink_path = Path::new(MODEL_DIR).join("ink.mnn");
    let img_path = PathBuf::from(image);
    if !img_path.exists() || !det_path.exists() || !ink_path.exists() {
        eprintln!("fixture or models missing; skipping {image}");
        return None;
    }
    let rgba = ImageReader::open(&img_path)
        .expect("open")
        .decode()
        .expect("decode")
        .to_rgba8();
    let (w, h) = rgba.dimensions();
    let dyn_image = DynamicImage::ImageRgba8(rgba.clone());

    let engine = PpocrEngine::load(&det_path, None, None, vec![], 1, Some(&ink_path))
        .expect("load ppocr (det + ink)");
    let boxes: Vec<DetectedTextBox> = engine
        .detect_only_image(&dyn_image, PpocrProfile::Still)
        .expect("detect");
    let ink_masks = engine.ink_masks(&dyn_image, &boxes, None);
    let union = union_ink_mask(&rgba, &boxes, &ink_masks, &[]);

    // One block per detected line; empty translation (we only read the sampled colours, nothing is
    // drawn). `style_ranges` empty — colour here is the geometric matte sample, not a style span.
    let blocks: Vec<TextBlock> = boxes
        .iter()
        .map(|b| TextBlock {
            lines: vec![TextLine {
                text: "x".to_string(),
                bounding_box: b.rect,
                oriented_box: b.oriented_box,
                tight_box: b.tight_box,
                word_rects: vec![b.rect],
                style_ranges: Vec::new(),
            }],
        })
        .collect();
    let translated = vec![String::new(); blocks.len()];
    let styles = vec![Vec::new(); blocks.len()];

    let prepared = prepare_overlay_image(
        rgba.as_raw(),
        w,
        h,
        &blocks,
        &translated,
        &styles,
        BackgroundMode::AutoDetect,
        ReadingOrder::LeftToRight,
        Some(&union),
    )
    .expect("overlay");

    let mut out: Vec<LineColor> = prepared
        .blocks
        .iter()
        .filter_map(|blk| {
            let line = blk.lines.first()?;
            let argb = sample_line_color(&line.foreground, 0.5)?;
            let (r, g, b) = ((argb >> 16) & 0xFF, (argb >> 8) & 0xFF, argb & 0xFF);
            Some(LineColor {
                y: line.oriented_box.cy,
                lum: ((r + g + b) / 3) as i32,
            })
        })
        .collect();
    out.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    Some(out)
}

/// The pipeline must reproduce the black→#888 ink gradient as a per-line colour gradient — proving
/// the matte sampling, not just the renderer, carries geometry across lines of one paragraph.
#[test]
fn gradient_image_yields_a_per_line_colour_gradient() {
    let Some(lines) = run("tests/fixtures/gradient_test.png") else {
        return;
    };
    let lums: Vec<i32> = lines.iter().map(|l| l.lum).collect();
    eprintln!("gradient per-line luminance (top→bottom): {lums:?}");

    assert!(
        lines.len() >= 4,
        "expected the 5 gradient lines (≥4 after detection), got {}",
        lines.len()
    );
    let first = lums.first().copied().unwrap();
    let last = lums.last().copied().unwrap();
    assert!(
        first < 60,
        "top line should be near-black ink, got luminance {first}"
    );
    assert!(
        last > 90,
        "bottom line should be mid-grey ink, got luminance {last}"
    );
    assert!(
        last - first >= 50,
        "expected a clear darkening→lightening gradient, got {lums:?} (a per-block collapse would be flat)"
    );
    // Trend is monotone up to sampling noise (no large reversals).
    for w in lums.windows(2) {
        assert!(
            w[1] >= w[0] - 20,
            "gradient should not reverse sharply between lines: {lums:?}"
        );
    }
}

/// Black body paragraph with a few coloured words. The per-line *geometric core* is the line's
/// dominant ink, so every line reads near-black regardless of the emphasis words — emphasis is a
/// per-word override (semantic, needs recognition firings), separate from this geometric colour.
/// This path runs det+ink only, so it checks the geometric core stays black; the per-word emphasis
/// detection itself is covered hermetically by `text_metrics::word_emphasis_colors`.
#[test]
fn emphasis_image_lines_read_as_black_body() {
    let Some(lines) = run("tests/fixtures/emphasis_test.png") else {
        return;
    };
    let lums: Vec<i32> = lines.iter().map(|l| l.lum).collect();
    eprintln!("emphasis per-line geometric-core luminance (black body): {lums:?}");

    assert!(
        lines.len() >= 3,
        "expected ≥3 body lines, got {}",
        lines.len()
    );
    for lum in &lums {
        assert!(
            *lum < 70,
            "black body line should read near-black, got {lum} (all {lums:?})"
        );
    }
}
