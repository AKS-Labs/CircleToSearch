//! Paragraph grouping over a real photographed page, with the ink-matte typography
//! (x-height + centreline + ink width) feeding the grouper.
//!
//! `files/live-overlay/kindle-pic.jpg` is a 90°-rotated photo of an e-reader page.
//! Its second paragraph begins with an indented line, "She gave a small sigh ...
//! With a social", continued by "smile, she waved ...". The detection tight box's
//! height wobbles with each line's glyph content, and its width clips ~half a glyph
//! off each end; rebuilding each line's box from its ink matte (see
//! `LineMetrics::refit`) gives a glyph-content-stable size so those two lines
//! stay in one paragraph instead of the first being split off as a pseudo-title.
//!
//! Skips when the bucket models are missing.

#![cfg(feature = "ppocr")]

use std::path::{Path, PathBuf};

use translator::PpocrScript;
use translator::ocr::{TextBlock, TextLine, group_lines_into_paragraphs};
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};
use translator::text_metrics::measure_line;

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const IMAGE_PATH: &str = "files/live-overlay/kindle-pic.jpg";

fn model_paths() -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let dir = Path::new(MODEL_DIR);
    let det = dir.join("PP-OCRv5_mobile_det.mnn");
    let rec = dir.join("latin_PP-OCRv5_mobile_rec_infer.mnn");
    let keys = dir.join("latin_PP-OCRv5_keys.txt");
    let ink = dir.join("ink.mnn");
    [&det, &rec, &keys, &ink]
        .iter()
        .all(|p| p.exists())
        .then_some((det, rec, keys, ink))
}

/// Index of the block whose text contains `needle` (case-insensitive), if any.
fn block_of<'a>(blocks: &'a [TextBlock], needle: &str) -> Option<usize> {
    let needle = needle.to_lowercase();
    blocks.iter().position(|b| {
        b.lines
            .iter()
            .any(|l| l.text.to_lowercase().contains(&needle))
    })
}

#[test]
fn indented_paragraph_first_line_groups_with_its_body() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some((det, rec, keys, ink)) = model_paths() else {
        eprintln!("PPOCR bucket models missing under {MODEL_DIR}; skipping");
        return;
    };
    let Ok(image) = image::open(IMAGE_PATH) else {
        eprintln!("{IMAGE_PATH} missing; skipping");
        return;
    };
    // The photo is rotated 90°; bring the text upright so the horizontal grouper applies.
    let image = image.rotate90();

    let spec = PpocrRecognizerSpec {
        script: PpocrScript::Latin,
        model_path: rec,
        keys_path: keys,
    };
    let engine = PpocrEngine::load(&det, None, None, vec![spec], 1, Some(ink.as_path()))
        .expect("load ppocr engine");

    let boxes = engine
        .detect_only_image(&image, PpocrProfile::Still)
        .expect("detect");
    let scripts = vec![PpocrScript::Latin; boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(&image, &boxes, &scripts, PpocrProfile::Still, None)
        .expect("recognize");
    let masks = engine.ink_masks(&image, &boxes, None);

    // Same construction as the still pipeline: rebuild the grouping box from the
    // ink matte where available, otherwise keep the detection tight box.
    let text_lines: Vec<TextLine> = boxes
        .iter()
        .zip(lines)
        .enumerate()
        .filter(|(_, (_, line))| !line.text.trim().is_empty())
        .map(|(i, (b, line))| {
            let tight_box = masks
                .get(i)
                .and_then(|m| m.as_ref())
                .and_then(|m| measure_line(m, b.oriented_box.width, b.oriented_box.height))
                .map_or(b.tight_box, |m| {
                    m.refit(b.tight_box, b.tight_box.angle_radians)
                });
            TextLine {
                text: line.text,
                bounding_box: line.rect,
                oriented_box: line.oriented_box,
                tight_box,
                word_rects: vec![line.rect],
                style_ranges: Vec::new(),
            }
        })
        .collect();

    let blocks = group_lines_into_paragraphs(text_lines, Default::default());

    let first = block_of(&blocks, "she gave a small")
        .expect("recognised the indented first line 'She gave a small ...'");
    let body = block_of(&blocks, "smile, she waved")
        .expect("recognised the continuation line 'smile, she waved ...'");

    assert_eq!(
        first, body,
        "indented first line and its continuation must share one paragraph block \
         (first line in block {first}, continuation in block {body})",
    );
    assert!(
        blocks[first].lines.len() > 2,
        "the paragraph should gather its body lines, got {} line(s)",
        blocks[first].lines.len(),
    );
}
