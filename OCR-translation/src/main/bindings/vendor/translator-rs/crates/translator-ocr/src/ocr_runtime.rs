use std::sync::Mutex;

use rayon::prelude::*;

use crate::ppocr::{
    InkChannelOrder, PpocrEngine, PpocrProfile, PpocrScriptClass, PpocrScriptPrediction,
};
use translator_core::api::{LanguageCode, TranslatorError};
use translator_core::catalog::CatalogSnapshot;
use translator_core::catalog::{OcrPack, PackKind, PpocrScript};
use translator_core::ocr::{
    DetectedTextBox, OcrSourceSelection, OrientedRect, RecognizedTextLine, TextLine,
};
use translator_core::ocr::{PreparedImageOverlay, ReadingOrder, Rect, TextBlock};
use translator_core::settings::BackgroundMode;
use translator_raster::live_frame::StillImage;
use translator_raster::overlay::prepare_overlay_image;
use translator_raster::text_metrics::{LineMetrics, measure_line};
use translator_translate::bergamot::BergamotEngine;
use translator_translate::language_detect::detect_language_robust_code;
use translator_translate::translate::Translator;

/// Build the still image's display-orient RGB + detector downscale once. The caller (e.g. the
/// `OcrImage` handle) caches it so a staged detect→ocr pass doesn't rebuild it.
pub fn build_still_image(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    max_image_size: u32,
) -> Result<StillImage, TranslatorError> {
    let det_max_pixels = saturating_square(max_image_size);
    StillImage::build_still_rgb(rgba_bytes, width, height, det_max_pixels)
        .map_err(|e| TranslatorError::ocr(format!("ppocr build still image failed: {e}")))
}

pub fn detect_boxes_from_still(
    ppocr: &PpocrEngine,
    still: &StillImage,
    width: u32,
    height: u32,
) -> Result<Vec<DetectedTextBox>, TranslatorError> {
    Ok(ppocr
        .detect_only_image(&still.rgb_det, PpocrProfile::Still)
        .map_err(|e| TranslatorError::ocr(format!("ppocr detection failed: {e}")))?
        .into_iter()
        .map(|b| scale_detected_box(b, still.det_to_full, width, height))
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub fn translate_image_rgba_ppocr_in_snapshot(
    engine: &Mutex<BergamotEngine>,
    ppocr: &PpocrEngine,
    snapshot: &CatalogSnapshot,
    still: &StillImage,
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    source_selection: &OcrSourceSelection,
    target_code: &LanguageCode,
    min_confidence: u32,
    background_mode: BackgroundMode,
    reading_order: Option<ReadingOrder>,
    // Detected boxes from a prior `detect_boxes_from_still` pass; when `Some`, detection is
    // skipped so the staged detect→translate path runs the detector only once.
    detection: Option<Vec<DetectedTextBox>>,
) -> Result<PreparedImageOverlay, TranslatorError> {
    let rgb = &still.rgb;
    let det_boxes: Vec<DetectedTextBox> = match detection {
        Some(boxes) => boxes,
        None => detect_boxes_from_still(ppocr, still, width, height)?,
    };

    // Still images are display-oriented, so the canonical reading frame is R0.
    // Passing it explicitly (instead of None) pins the dewarp direction of
    // near-vertical strips — CJK vertical columns — to top-char-first; with
    // None the PCA sign for those strips is per-column noise.
    let scripts = match source_selection {
        OcrSourceSelection::Auto => {
            let predictions = ppocr
                .classify_text_boxes_image(
                    rgb,
                    &det_boxes,
                    Some(translator_core::coords::Quadrant::R0),
                )
                .map_err(|e| {
                    TranslatorError::ocr(format!("ppocr script classification failed: {e}"))
                })?;
            route_ppocr_predictions(ppocr, &predictions, &det_boxes)?
        }
        OcrSourceSelection::Specific { language_code } => {
            let script = recognizer_script_for_language(snapshot, language_code)?;
            vec![script; det_boxes.len()]
        }
    };

    // No requested reading order: infer it. Only CJK pages can read
    // vertically (modern Korean is horizontal, so the gate is the shared
    // Chinese+Japanese recognizer script, not "any CJK language"), and within
    // those the detections' own long-axis orientations carry the answer.
    let reading_order = reading_order.unwrap_or_else(|| {
        let cj_boxes = scripts.iter().filter(|s| **s == PpocrScript::Cj).count();
        let cjk_dominant = cj_boxes * 2 >= scripts.len() && !scripts.is_empty();
        let resolved =
            if cjk_dominant && translator_core::ocr::detected_lines_read_vertically(&det_boxes) {
                ReadingOrder::TopToBottomRightToLeft
            } else {
                ReadingOrder::LeftToRight
            };
        log::info!(
            "ppocr auto reading order: {} cj boxes of {} → {:?}",
            cj_boxes,
            scripts.len(),
            resolved,
        );
        resolved
    });

    let t_dewarp = std::time::Instant::now();
    let mut strips =
        ppocr.dewarp_strips(rgb, &det_boxes, Some(translator_core::coords::Quadrant::R0));
    let dewarp_ms = t_dewarp.elapsed().as_secs_f32() * 1000.0;

    let lines = ppocr
        .recognize_from_strips(
            &strips,
            &det_boxes,
            &scripts,
            PpocrProfile::Still,
            width,
            height,
            dewarp_ms,
        )
        .map_err(|e| TranslatorError::ocr(format!("ppocr recognition failed: {e}")))?;

    // A box recognition rejected (empty / low-score) never reaches the overlay, so its matte is
    // wasted inference — drop it before inking. The dewarp already ran (shared with rec).
    for (i, line) in lines.iter().enumerate() {
        if line.text.trim().is_empty() {
            strips[i] = None;
        }
    }

    // Per-box ink mattes feed both paragraph grouping (x-height + baseline-tilt
    // recovery, applied to each line before it groups) and the overlay erase
    // (the union ink mask). Compute them once, here, before grouping needs them.
    let (ink_strips, ink_rgba) = if ppocr.has_ink() {
        let ink_strips =
            ppocr.ink_strips_from(&strips, InkChannelOrder::Bgr, (dewarp_ms * 1000.0) as u128);
        let t_ink_copy = std::time::Instant::now();
        let rgba = image::RgbaImage::from_raw(width, height, rgba_bytes.to_vec())
            .expect("rgba image from caller-owned bytes");
        log::info!(
            "ppocr ink rgba copy (union mask): {width}x{height} — {:.1}ms",
            t_ink_copy.elapsed().as_secs_f32() * 1000.0,
        );
        (ink_strips, Some(rgba))
    } else {
        (Vec::new(), None)
    };
    // The matte (ch0) drives grouping metrics + the overlay union mask; the bold channel
    // (ch1), pooled per box and thresholded, is the typography weight when present.
    let t_clone = std::time::Instant::now();
    let ink_masks: Vec<Option<image::GrayImage>> = ink_strips
        .iter()
        .map(|s| s.as_ref().map(|s| s.matte.clone()))
        .collect();
    // Erase mask = matte ∪ rule, so under/strike/over-line rules are erased too. Kept separate
    // from `ink_masks` (pure matte) so the rule never distorts x-height/baseline/grouping metrics.
    // Identical to the matte for 1-/2-channel models (no rule channel) — graceful downgrade.
    let erase_masks: Vec<Option<image::GrayImage>> = ink_strips
        .iter()
        .map(|s| s.as_ref().map(|s| s.erase_mask()))
        .collect();
    let ink_src_maps: Vec<Option<Vec<(f32, f32)>>> = ink_strips
        .iter()
        .map(|s| s.as_ref().and_then(|s| s.src_map.clone()))
        .collect();
    let clone_ms = t_clone.elapsed().as_secs_f32() * 1000.0;
    let t_bold_pool = std::time::Instant::now();
    let model_bold: Vec<Option<bool>> = ink_strips
        .iter()
        .map(|s| {
            s.as_ref()
                .and_then(|s| s.pooled_bold())
                .map(|p| p >= translator_raster::text_metrics::MODEL_BOLD_THRESHOLD)
        })
        .collect();
    let model_colors: Vec<Option<translator_core::ocr::InkColor>> = ink_strips
        .iter()
        .map(|s| s.as_ref().and_then(|s| s.pooled_color()))
        .collect();
    let bold_pool_ms = t_bold_pool.elapsed().as_secs_f32() * 1000.0;
    let t_box_metrics = std::time::Instant::now();
    let text_metrics = box_line_metrics(&det_boxes, &ink_masks);
    let metrics_ms = t_box_metrics.elapsed().as_secs_f32() * 1000.0;

    // Per-word style from the ink channels + the line's CTC firings: bold from the bold channel
    // (falling back to a whole-line range when the model pooled bold but firings weren't usable —
    // RTL, multi-chunk), and under/strike/over-line decoration from the rule channel. Bold and
    // decoration ranges may overlap; they carry distinct `StyleKind`s.
    let t_bold = std::time::Instant::now();
    let line_style_ranges: Vec<Vec<translator_core::ocr::StyleRange>> = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let strip = ink_strips.get(i).and_then(|s| s.as_ref());
            let firings: Vec<(char, f32)> = line
                .firings
                .iter()
                .map(|f| (char::from_u32(f.ch).unwrap_or('\u{fffd}'), f.at))
                .collect();
            let is_cjk = scripts[i] == PpocrScript::Cj;
            let mut ranges: Vec<translator_core::ocr::StyleRange> = Vec::new();

            let bold_words = match strip.and_then(|s| s.bold_profile()) {
                Some(profile) => translator_raster::text_metrics::word_bold_ranges(
                    &line.text,
                    &firings,
                    is_cjk,
                    &profile,
                    translator_raster::text_metrics::MODEL_BOLD_THRESHOLD,
                ),
                None => Vec::new(),
            };
            if !bold_words.is_empty() {
                ranges.extend(bold_words.into_iter().map(|(start, end)| {
                    translator_core::ocr::StyleRange {
                        start,
                        end,
                        kind: translator_core::ocr::StyleKind::Bold,
                    }
                }));
            } else if model_bold.get(i).copied().flatten().unwrap_or(false) && !line.text.is_empty()
            {
                ranges.push(translator_core::ocr::StyleRange {
                    start: 0,
                    end: line.text.len() as u32,
                    kind: translator_core::ocr::StyleKind::Bold,
                });
            }

            if let Some(profile) = strip.and_then(|s| s.rule_profile()) {
                ranges.extend(
                    translator_raster::text_metrics::word_decoration_ranges(
                        &line.text, &firings, is_cjk, &profile,
                    )
                    .into_iter()
                    .map(|(start, end, dec)| {
                        translator_core::ocr::StyleRange {
                            start,
                            end,
                            kind: translator_core::ocr::StyleKind::Decoration(dec),
                        }
                    }),
                );
            }

            // Emphasis colour: words whose ink colour is an outlier from the line's dominant ink
            // (a red word in black body). The line's base colour stays geometric (assigned per
            // line at render); only these overrides cross translation.
            if let (Some(strip), Some(src)) = (strip, ink_rgba.as_ref()) {
                ranges.extend(line_emphasis_colors(strip, src, &line.text, &firings));
            }
            ranges
        })
        .collect();
    let bold_ms = t_bold.elapsed().as_secs_f32() * 1000.0;

    // Per-word source boxes from the CTC firings, in recognition order. Built here while the
    // recognized `lines` are still in hand (grouping below consumes them) and the translation
    // hasn't run yet, so the boxes register against the original recognized text.
    let t_words = std::time::Instant::now();
    let source_words = still_source_words(&lines, &scripts);
    let words_ms = t_words.elapsed().as_secs_f32() * 1000.0;

    let t_blocks = std::time::Instant::now();
    let blocks = still_ppocr_lines_to_blocks(
        &det_boxes,
        lines,
        &text_metrics,
        &line_style_ranges,
        &model_colors,
        min_confidence,
        reading_order,
    );
    log::info!(
        "ppocr post-ink: clones={clone_ms:.1}ms model_bold={bold_pool_ms:.1}ms \
         box_metrics={metrics_ms:.1}ms bold_ranges={bold_ms:.1}ms \
         source_words={words_ms:.1}ms blocks+grouping={:.1}ms",
        t_blocks.elapsed().as_secs_f32() * 1000.0,
    );
    let source_code = match source_selection {
        OcrSourceSelection::Specific { language_code } => language_code.clone(),
        OcrSourceSelection::Auto => {
            let blocks_text = blocks
                .iter()
                .map(TextBlock::translation_text)
                .collect::<Vec<_>>()
                .join("\n");
            match ocr_source_from_text(snapshot, &blocks_text, Some(target_code)) {
                Some(detected) => detected,
                None => {
                    // Detection declined (short/ambiguous text, or no installed
                    // detected->target pair). Recognition already ran with the
                    // per-box classified scripts, so keep its words instead of
                    // failing the whole plan: fall back to source=target, which
                    // the translate stage treats as identity — the original text
                    // stays on screen and copy gets correct words in every script
                    // (never the target script force-re-recognition, which turned
                    // non-Latin text into garbage).
                    log::info!(
                        "still image source detection declined; keeping recognized \
                         words with source=target {}",
                        target_code.as_str()
                    );
                    target_code.clone()
                }
            }
        }
    };

    // Per-block source detection for Auto: every block gets its own CLD pass, so a
    // screen mixing languages translates each block from its true source instead of
    // forcing the image-level dominant language onto all of them. Explicit Specific
    // selections keep a single source (the user's choice).
    let block_sources: Vec<LanguageCode> = match source_selection {
        OcrSourceSelection::Auto => blocks
            .iter()
            .map(|block| {
                let text = block.translation_text();
                if text.trim().is_empty() {
                    return source_code.clone();
                }
                if let Some(detected) = ocr_source_from_text(snapshot, &text, Some(target_code)) {
                    return detected;
                }
                if ocr_source_from_text(snapshot, &text, None).is_some() {
                    // Detected, but the detected->target pair is not installed:
                    // keep the original text (identity) rather than mistranslate
                    // it as another language.
                    return target_code.clone();
                }
                // Too short/ambiguous on its own: trust the image-level source.
                source_code.clone()
            })
            .collect(),
        OcrSourceSelection::Specific { .. } => vec![source_code.clone(); blocks.len()],
    };

    // The same per-box mattes → one full-image union mask. The overlay erase
    // replaces just the inked pixels with a reconstructed background instead of
    // flat-filling each line's rect. `None` when no ink model is installed →
    // flat-fill fallback.
    let ink_union = ink_rgba.as_ref().map(|rgba| {
        translator_raster::color_matting::union_ink_mask(
            rgba,
            &det_boxes,
            &erase_masks,
            &ink_src_maps,
        )
    });

    let mut overlay = finalize_image_overlay(
        engine,
        snapshot,
        rgba_bytes,
        width,
        height,
        &block_sources,
        target_code,
        blocks,
        background_mode,
        reading_order,
        ink_union.as_deref(),
    )?;
    let rtl_lines: std::collections::HashSet<u32> = scripts
        .iter()
        .enumerate()
        .filter(|(_, s)| s.is_rtl())
        .map(|(i, _)| i as u32)
        .collect();
    overlay.source_words = translator_core::ocr::order_words_visually(source_words, &rtl_lines);
    Ok(overlay)
}

/// Flatten the recognized lines into per-word source boxes in image space, in recognition
/// order. Each line's CTC firings drive the word split + positions (see
/// [`translator_raster::text_metrics::firing_word_boxes`]); CJK lines split per glyph.
fn still_source_words(
    lines: &[translator_core::ocr::RecognizedTextLine],
    scripts: &[PpocrScript],
) -> Vec<translator_core::ocr::PositionedWord> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(i, line)| {
            let firings: Vec<(char, f32)> = line
                .visual_firings
                .iter()
                .map(|f| (char::from_u32(f.ch).unwrap_or('\u{fffd}'), f.at))
                .collect();
            let script = scripts.get(i).copied();
            if script.is_some_and(|s| s.is_rtl()) {
                // The firings are in visual (left-to-right) order, so carve the boxes against the
                // visual text (`reverse_visual_to_logical` is its own inverse), then flip each
                // word's glyphs back to logical for the copied text. Word order is handled later
                // by `order_words_visually` reversing the RTL line.
                let visual_text = crate::ppocr::reverse_visual_to_logical(&line.text);
                let mut words = translator_raster::text_metrics::firing_word_boxes(
                    &visual_text,
                    &firings,
                    false,
                    &line.oriented_box,
                    i as u32,
                );
                for w in &mut words {
                    w.text = crate::ppocr::reverse_visual_to_logical(&w.text);
                }
                return words;
            }
            let is_cjk = script == Some(PpocrScript::Cj);
            translator_raster::text_metrics::firing_word_boxes(
                &line.text,
                &firings,
                is_cjk,
                &line.oriented_box,
                i as u32,
            )
        })
        .collect()
}

/// Detect-only pass over a still image: builds the oriented image and runs the PPOCR detector,
/// returning the text boxes in image-pixel space. The caller feeds these back into
/// `translate_image_rgba_ppocr_in_snapshot` so recognition + translation skip a second detection.
pub fn detect_image_boxes_ppocr(
    ppocr: &PpocrEngine,
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    max_image_size: u32,
) -> Result<Vec<DetectedTextBox>, TranslatorError> {
    let still = build_still_image(rgba_bytes, width, height, max_image_size)?;
    detect_boxes_from_still(ppocr, &still, width, height)
}

fn saturating_square(side: u32) -> u32 {
    let n = (side as u64).saturating_mul(side as u64);
    n.min(u32::MAX as u64) as u32
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
    let scale_oriented = |o: OrientedRect| OrientedRect {
        cx: o.cx * scale,
        cy: o.cy * scale,
        width: o.width * scale,
        height: o.height * scale,
        angle_radians: o.angle_radians,
    };
    let contour = b.contour.iter().map(|v| v * scale).collect();
    DetectedTextBox {
        rect,
        oriented_box: scale_oriented(b.oriented_box),
        tight_box: scale_oriented(b.tight_box),
        contour,
        score: b.score,
    }
}

/// Per-box ink-matte typography (x-height + baseline tilt), 1:1 with `boxes`.
/// `None` for a box with no matte (no ink model, degenerate box, or no coherent
/// ink band); the caller then keeps the box's own tight height and angle.
fn box_line_metrics(
    boxes: &[DetectedTextBox],
    masks: &[Option<image::GrayImage>],
) -> Vec<Option<LineMetrics>> {
    boxes
        .par_iter()
        .enumerate()
        .map(|(i, b)| {
            let mask = masks.get(i)?.as_ref()?;
            measure_line(mask, b.oriented_box.width, b.oriented_box.height)
        })
        .collect()
}

/// Pair the still-path detector boxes with their recognised lines, carrying the
/// tight oriented rect forward into [`TextLine`] so paragraph grouping has the
/// glyph-tight metric (the `RecognizedTextLine` shape doesn't carry it on its
/// own). Where the ink matte resolved a line's typography, rebuild that tight box
/// from the matte band — its height becomes the glyph-content-stable x-height, its
/// centre snaps to the band centreline, and its angle is corrected by the recovered
/// baseline tilt — so grouping keys size *and* spacing on the real ink, not the
/// detection box. The render `oriented_box` gets the same tilt correction.
fn still_ppocr_lines_to_blocks(
    boxes: &[DetectedTextBox],
    lines: Vec<RecognizedTextLine>,
    text_metrics: &[Option<LineMetrics>],
    line_style_ranges: &[Vec<translator_core::ocr::StyleRange>],
    model_colors: &[Option<translator_core::ocr::InkColor>],
    min_confidence: u32,
    reading_order: ReadingOrder,
) -> Vec<TextBlock> {
    // The user's confidence setting (0–100) is a stricter line gate on top of the
    // recognizer's built-in `rec_drop_score`: lines the model accepted but whose mean
    // CTC score sits below the user's bar are dropped before paragraph grouping.
    let min_score = min_confidence as f32 / 100.0;
    let text_lines: Vec<TextLine> = boxes
        .iter()
        .zip(lines.into_iter())
        .zip(text_metrics.iter())
        .zip(line_style_ranges.iter())
        .zip(model_colors.iter())
        .filter(|((((_, line), _), _), _)| {
            !line.text.trim().is_empty() && line.confidence >= min_score
        })
        .map(|((((b, line), metrics), style_ranges), ink_color)| {
            // Line orientation is the detection reading frame — its adaptive clamp already
            // keeps a genuine lean and snaps short labels to the frame. The ink matte resolves
            // typography (x-height, centre, decoration, bold) but no longer re-decides the angle.
            let angle = line.oriented_box.angle_radians;
            let oriented_box = line.oriented_box;

            // Re-fit the grouping box to the actual ink (x-height, ink width,
            // centred on the ink) where the matte resolved it; otherwise keep the
            // detection box. Both carry the resolved line angle.
            let tight_box = match metrics {
                Some(m) => m.refit(b.tight_box, angle),
                None => OrientedRect {
                    angle_radians: angle,
                    ..b.tight_box
                },
            };
            TextLine {
                text: line.text,
                bounding_box: line.rect,
                oriented_box,
                tight_box,
                word_rects: vec![line.rect],
                style_ranges: style_ranges.clone(),
                ink_color: *ink_color,
            }
        })
        .collect();
    match reading_order {
        ReadingOrder::LeftToRight => {
            translator_core::ocr::group_lines_into_paragraphs(text_lines, Default::default())
        }
        ReadingOrder::TopToBottomRightToLeft => {
            translator_core::ocr::group_vertical_lines_into_paragraphs(
                text_lines,
                Default::default(),
            )
        }
    }
}

/// Map a PULC script class to the best installed PPOCR recognizer script. Tries the
/// specialist first (Eslav for Cyrillic), then a general fallback. Returns `None`
/// when nothing applicable is installed.
fn ppocr_script_for_class(ppocr: &PpocrEngine, class: PpocrScriptClass) -> Option<PpocrScript> {
    let candidates: &[PpocrScript] = match class {
        PpocrScriptClass::Arabic => &[PpocrScript::Arabic],
        PpocrScriptClass::Chinese | PpocrScriptClass::Japanese => &[PpocrScript::Cj],
        PpocrScriptClass::Cyrillic => &[PpocrScript::Eslav, PpocrScript::Cyrillic],
        PpocrScriptClass::Devanagari => &[PpocrScript::Devanagari],
        // PULC's Kannada class is the only one of the merged Indic scripts (bn/gu/kn/ml)
        // it can name; the other three have no PULC class and reach the Indic recognizer
        // only via forced-source or the dominant-pack fallback.
        PpocrScriptClass::Kannada => &[PpocrScript::Indic],
        PpocrScriptClass::Korean => &[PpocrScript::Korean],
        PpocrScriptClass::Tamil => &[PpocrScript::Ta],
        PpocrScriptClass::Telugu => &[PpocrScript::Te],
        PpocrScriptClass::Latin => &[PpocrScript::Latin],
    };
    let installed: std::collections::HashSet<PpocrScript> = ppocr.installed_scripts().collect();
    candidates.iter().find(|s| installed.contains(s)).copied()
}

pub fn recognizer_script_for_language(
    snapshot: &CatalogSnapshot,
    language_code: &LanguageCode,
) -> Result<PpocrScript, TranslatorError> {
    let pack_id = snapshot
        .catalog
        .ocr_pack_id_for_engine(language_code, "ppocr")
        .ok_or_else(|| {
            TranslatorError::missing_asset(format!(
                "no ppocr pack for language {}",
                language_code.as_str()
            ))
        })?;
    let pack = snapshot.catalog.pack(&pack_id).ok_or_else(|| {
        TranslatorError::missing_asset(format!("ppocr pack {pack_id} missing from catalog"))
    })?;
    match &pack.kind {
        PackKind::Ocr(OcrPack::PpocrRecognizer { script }) => Ok(*script),
        _ => Err(TranslatorError::missing_asset(format!(
            "pack {pack_id} is not a ppocr recognizer"
        ))),
    }
}

const PPOCR_ROUTE_DOMINANT_MIN_RATIO: f32 = 0.55;
const PPOCR_ROUTE_MINOR_KEEP_RATIO: f32 = 0.20;
const PPOCR_ROUTE_SMOOTH_MIN_CLASSIFIED: usize = 8;
/// Minority-script predictions at or above this softmax confidence are treated as a
/// genuine second language on mixed-language screens and are never folded into the
/// dominant script; only lower-confidence strays (classifier noise) fold.
const PPOCR_ROUTE_MINOR_TRUST_SCORE: f32 = 0.95;

/// Resolve per-strip PULC predictions into per-strip PPOCR recognizer scripts. Strips
/// PULC could not classify fall back to the dominant classified script; minority
/// scripts below `PPOCR_ROUTE_MINOR_KEEP_RATIO` are folded into the dominant; Latin
/// strips in an otherwise single non-Latin batch fold into that non-Latin script
/// (PPOCR's non-Latin recognizers can handle Latin glyphs but not vice versa).
pub fn route_ppocr_predictions(
    ppocr: &PpocrEngine,
    predictions: &[Option<PpocrScriptPrediction>],
    boxes: &[DetectedTextBox],
) -> Result<Vec<PpocrScript>, TranslatorError> {
    let latin_installed = ppocr
        .installed_scripts()
        .any(|s| s == PpocrScript::Latin)
        .then_some(PpocrScript::Latin);
    let mut routed: Vec<Option<PpocrScript>> = Vec::with_capacity(predictions.len());
    let mut classified = Vec::new();
    let mut missing_indices = Vec::new();
    for (idx, prediction) in predictions.iter().enumerate() {
        let Some(prediction) = prediction else {
            missing_indices.push(idx);
            routed.push(None);
            continue;
        };
        let script = ppocr_script_for_class(ppocr, prediction.class);
        if let Some(box_) = boxes.get(idx) {
            log::debug!(
                "ppocr route strip={} class={} score={:.3} det_score={:.3} width={} height={} area={} script={:?}",
                idx,
                prediction.class.name(),
                prediction.score,
                box_.score,
                box_.rect.width(),
                box_.rect.height(),
                box_.rect.width().saturating_mul(box_.rect.height()),
                script.as_ref().map(PpocrScript::as_slug),
            );
        }
        match script {
            Some(script) => {
                classified.push(script);
                routed.push(Some(script));
            }
            None => {
                missing_indices.push(idx);
                routed.push(None);
            }
        }
    }

    let fallback = dominant_script(&classified);
    if !missing_indices.is_empty() {
        let Some(fallback) = fallback else {
            return Err(TranslatorError::ocr(
                "could not classify script for any detected text strip",
            ));
        };
        for idx in missing_indices {
            log::debug!(
                "ppocr route strip={} script=unknown -> {} (fallback=dominant)",
                idx,
                fallback.as_slug(),
            );
            routed[idx] = Some(fallback);
        }
    }

    let prediction_scores: Vec<Option<f32>> = predictions
        .iter()
        .map(|p| p.as_ref().map(|pr| pr.score))
        .collect();
    smooth_dominant_routes(&mut routed, &classified, &prediction_scores);

    if let Some(latin) = latin_installed {
        let non_latin: std::collections::HashSet<PpocrScript> = routed
            .iter()
            .filter_map(|s| *s)
            .filter(|s| *s != latin)
            .collect();
        if non_latin.len() == 1 {
            let target = *non_latin.iter().next().unwrap();
            for slot in &mut routed {
                if *slot == Some(latin) {
                    *slot = Some(target);
                }
            }
            log::debug!(
                "ppocr route merged latin into {} for mixed-script batch",
                target.as_slug(),
            );
        }
    }

    Ok(routed
        .into_iter()
        .map(|s| s.expect("all ppocr routes populated"))
        .collect())
}

fn dominant_script(scripts: &[PpocrScript]) -> Option<PpocrScript> {
    scripts
        .iter()
        .fold(
            std::collections::HashMap::<PpocrScript, usize>::new(),
            |mut counts, script| {
                *counts.entry(*script).or_default() += 1;
                counts
            },
        )
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(script, _)| script)
}

fn smooth_dominant_routes(
    routed: &mut [Option<PpocrScript>],
    classified: &[PpocrScript],
    scores: &[Option<f32>],
) {
    if classified.len() < PPOCR_ROUTE_SMOOTH_MIN_CLASSIFIED {
        return;
    }
    let counts: std::collections::HashMap<PpocrScript, usize> =
        classified
            .iter()
            .fold(std::collections::HashMap::new(), |mut counts, script| {
                *counts.entry(*script).or_default() += 1;
                counts
            });
    let Some((&dominant, &dominant_count)) = counts.iter().max_by_key(|(_, count)| **count) else {
        return;
    };
    let total = classified.len() as f32;
    let dominant_ratio = dominant_count as f32 / total;
    if dominant_ratio < PPOCR_ROUTE_DOMINANT_MIN_RATIO {
        return;
    }

    let minority: std::collections::HashSet<PpocrScript> = counts
        .iter()
        .filter_map(|(&script, &count)| {
            let ratio = count as f32 / total;
            (script != dominant && ratio < PPOCR_ROUTE_MINOR_KEEP_RATIO).then_some(script)
        })
        .collect();
    for script in &minority {
        log::debug!(
            "ppocr route smoothing: folding script={} into dominant={} dominant_ratio={:.2}",
            script.as_slug(),
            dominant.as_slug(),
            dominant_ratio,
        );
    }
    for (idx, slot) in routed.iter_mut().enumerate() {
        if let Some(s) = slot {
            if minority.contains(s) {
                let score = scores.get(idx).copied().flatten().unwrap_or(0.0);
                if score >= PPOCR_ROUTE_MINOR_TRUST_SCORE {
                    // Confident minority script on a mixed-language screen: genuine
                    // second language, keep it (recognition needs its own model).
                    continue;
                }
                *slot = Some(dominant);
            }
        }
    }
}

/// Run CLD over recognized text and pick the best installed source language. When
/// `target_code` is given, the picked language must also be translatable to that
/// target (otherwise returns `None`). Used both still-mode (where the caller turns
/// `None` into a `MissingAsset`) and live-mode (where the caller keeps `None` so the
/// frame renders as untranslated text).
fn ocr_source_from_text(
    snapshot: &CatalogSnapshot,
    text: &str,
    target_code: Option<&LanguageCode>,
) -> Option<LanguageCode> {
    let available = snapshot
        .availability_by_code
        .keys()
        .filter_map(|code| {
            snapshot
                .catalog
                .scripted_language(&LanguageCode::from(code.as_str()))
        })
        .collect::<Vec<_>>();
    let detected = detect_language_robust_code(text, None, &available)?;
    if let Some(target) = target_code {
        if !snapshot.can_translate(&detected, target) {
            return None;
        }
    }
    Some(detected)
}

pub fn ocr_source_for_lines(
    snapshot: &CatalogSnapshot,
    lines: &[RecognizedTextLine],
    target_code: Option<&LanguageCode>,
) -> Option<LanguageCode> {
    let text = lines
        .iter()
        .map(|line| line.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    ocr_source_from_text(snapshot, &text, target_code)
}

fn finalize_image_overlay(
    engine: &Mutex<BergamotEngine>,
    snapshot: &CatalogSnapshot,
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    block_sources: &[LanguageCode],
    target_code: &LanguageCode,
    blocks: Vec<TextBlock>,
    background_mode: BackgroundMode,
    reading_order: ReadingOrder,
    ink_mask: Option<&[bool]>,
) -> Result<PreparedImageOverlay, TranslatorError> {
    let t_translate = std::time::Instant::now();
    let (translated_blocks, block_style_ranges) = {
        let mut engine_guard = engine.lock().expect("bergamot engine lock poisoned");
        translate_block_texts(
            &mut engine_guard,
            snapshot,
            block_sources,
            target_code,
            &blocks,
        )?
    };
    let translate_ms = t_translate.elapsed().as_secs_f32() * 1000.0;

    let t_overlay = std::time::Instant::now();
    let mut overlay = prepare_overlay_image(
        rgba_bytes,
        width,
        height,
        &blocks,
        &translated_blocks,
        &block_style_ranges,
        background_mode,
        reading_order,
        ink_mask,
    )
    .map_err(TranslatorError::ocr)?;
    // Carry the source-language styles (pre-remap, over the source text) so a later language switch
    // can re-remap them onto the new translation instead of stranding `style_spans` at these offsets.
    for (pb, tb) in overlay.blocks.iter_mut().zip(blocks.iter()) {
        let (src_text, src_ranges) = tb.translation_text_with_styles();
        pb.source_style_spans =
            translator_core::ocr::style_spans_from_styles(src_text.len(), &src_ranges);
        pb.source_text = src_text;
    }
    let overlay_ms = t_overlay.elapsed().as_secs_f32() * 1000.0;
    log::info!(
        "finalize_image_overlay: {} blocks — translate={:.1}ms overlay_prep={:.1}ms",
        blocks.len(),
        translate_ms,
        overlay_ms,
    );
    Ok(overlay)
}

type BlockStyles = Vec<translator_core::ocr::StyleRange>;

/// Emphasis colour ranges for one line: a `Color` [`StyleRange`] for each word whose ink colour is
/// an outlier from the line's dominant ink. Colour-head models read the predicted F field
/// directly (per-word α²-pooled, in strip space — no source mapping needed); legacy models
/// sample source pixels through `src_map`, so `None` `src_map` yields nothing there.
fn line_emphasis_colors(
    strip: &crate::ppocr::InkStrip,
    source: &image::RgbaImage,
    text: &str,
    firings: &[(char, f32)],
) -> Vec<translator_core::ocr::StyleRange> {
    let spans = match strip.fg.as_ref() {
        Some(fg) => translator_raster::text_metrics::word_emphasis_colors_model(
            text,
            firings,
            strip.color_pool_alpha(),
            fg,
        ),
        None => {
            let Some(src_map) = strip.src_map.as_ref() else {
                return Vec::new();
            };
            translator_raster::text_metrics::word_emphasis_colors(
                text,
                firings,
                &strip.matte,
                src_map,
                source,
            )
        }
    };
    spans
        .into_iter()
        .map(|(start, end, argb)| translator_core::ocr::StyleRange {
            start,
            end,
            kind: translator_core::ocr::StyleKind::Color(argb),
        })
        .collect()
}

/// Remap a block's source style ranges onto its translation, one [`StyleKind`] at a time so the
/// alignment remap's within-call merge never fuses ranges of different kinds (a bold range and an
/// underline range — or two differently-coloured emphasis runs — over the same translated word
/// must stay distinct). Iterates the distinct kinds actually present, so open-ended
/// `Color(u32)` kinds are each remapped on their own.
fn remap_block_styles(
    src: &[translator_core::ocr::StyleRange],
    twa: &translator_translate::translate::TranslationWithAlignment,
) -> BlockStyles {
    let mut kinds: Vec<translator_core::ocr::StyleKind> = Vec::new();
    for r in src {
        if !kinds.contains(&r.kind) {
            kinds.push(r.kind);
        }
    }
    let mut out = BlockStyles::new();
    for kind in kinds {
        let ranges: Vec<(u32, u32)> = src
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| (r.start, r.end))
            .collect();
        out.extend(
            translator_translate::translate::remap_byte_ranges_through_alignment(&ranges, twa)
                .into_iter()
                .map(|(start, end)| translator_core::ocr::StyleRange { start, end, kind }),
        );
    }
    out
}

fn translate_block_texts(
    engine: &mut BergamotEngine,
    snapshot: &CatalogSnapshot,
    block_sources: &[LanguageCode],
    target_code: &LanguageCode,
    blocks: &[TextBlock],
) -> Result<(Vec<String>, Vec<BlockStyles>), TranslatorError> {
    if blocks.is_empty() {
        return Err(TranslatorError::ocr("No text found in image"));
    }
    if block_sources.len() != blocks.len() {
        return Err(TranslatorError::ocr(
            "block source languages do not match recognized blocks",
        ));
    }
    let sources: Vec<(String, BlockStyles)> = blocks
        .iter()
        .map(TextBlock::translation_text_with_styles)
        .collect();

    // Single source (explicit selection, or a single-language screen): one batched
    // call, identical to the historical behavior (including the identity shortcut
    // and the all-empty "No text found" error inside translate_from_sources).
    let mut unique = block_sources.iter();
    if let Some(first) = unique.next() {
        if unique.all(|s| s == first) {
            return translate_from_sources(engine, snapshot, first, target_code, &sources);
        }
    }

    // Mixed-language screen: group blocks by detected source so each group runs its
    // own pair, then scatter the results back in original block order.
    let mut group_order: Vec<LanguageCode> = Vec::new();
    let mut groups: std::collections::HashMap<LanguageCode, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, src) in block_sources.iter().enumerate() {
        groups.entry(src.clone()).or_default().push(i);
    }
    group_order.extend(groups.keys().cloned());

    let mut translated_blocks: Vec<String> = vec![String::new(); blocks.len()];
    let mut target_styles: Vec<BlockStyles> = vec![Vec::new(); blocks.len()];
    for src in group_order {
        let idxs = &groups[&src];
        let sub: Vec<(String, BlockStyles)> = idxs
            .iter()
            .map(|&i| blocks[i].translation_text_with_styles())
            .collect();
        if sub.iter().all(|(t, _)| t.trim().is_empty()) {
            for (k, &i) in idxs.iter().enumerate() {
                translated_blocks[i] = sub[k].0.clone();
            }
            continue;
        }
        let (tr, st) = translate_from_sources(engine, snapshot, &src, target_code, &sub)?;
        for (k, &i) in idxs.iter().enumerate() {
            translated_blocks[i] = tr[k].clone();
            target_styles[i] = st[k].clone();
        }
    }
    Ok((translated_blocks, target_styles))
}

/// Translate each block's source text and remap its source-space style ranges onto the result.
/// `sources` is `(source_text, source_style_ranges)` per block. Shared by the initial overlay build
/// (ranges from the OCR blocks) and `retranslate` (ranges recovered from the cached overlay).
fn translate_from_sources(
    engine: &mut BergamotEngine,
    snapshot: &CatalogSnapshot,
    source_code: &LanguageCode,
    target_code: &LanguageCode,
    sources: &[(String, BlockStyles)],
) -> Result<(Vec<String>, Vec<BlockStyles>), TranslatorError> {
    let block_texts: Vec<String> = sources.iter().map(|(t, _)| t.clone()).collect();
    let non_empty_indices = block_texts
        .iter()
        .enumerate()
        .filter_map(|(index, text)| (!text.trim().is_empty()).then_some(index))
        .collect::<Vec<_>>();

    if non_empty_indices.is_empty() {
        return Err(TranslatorError::ocr("No text found in image"));
    }

    // Identity: no model runs, so the source style ranges already index the output text.
    if source_code == target_code {
        let styles = sources.iter().map(|(_, b)| b.clone()).collect();
        return Ok((block_texts, styles));
    }

    let texts_to_translate = non_empty_indices
        .iter()
        .map(|&index| block_texts[index].clone())
        .collect::<Vec<_>>();
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let on_progress = |_: usize, _: usize| {};
    let ctx = translator_translate::bergamot::TranslateCtx {
        cancel: &cancel,
        on_progress: &on_progress,
    };
    let aligned = Translator::new(engine, snapshot).translate_texts_with_alignment_ctx(
        source_code,
        target_code,
        &texts_to_translate,
        &ctx,
    )?;

    let mut translated_blocks = block_texts;
    let mut target_styles: Vec<BlockStyles> = vec![Vec::new(); sources.len()];
    match aligned {
        Some(results) => {
            for (slot, &bi) in non_empty_indices.iter().enumerate() {
                let twa = &results[slot];
                target_styles[bi] = remap_block_styles(&sources[bi].1, twa);
                translated_blocks[bi] = twa.translated_text.clone();
            }
        }
        None => {
            // No alignment plan for this pair — translate plainly and drop per-word style.
            let translated = Translator::new(engine, snapshot).translate_texts(
                source_code,
                target_code,
                &texts_to_translate,
            )?;
            for (slot, &bi) in non_empty_indices.iter().enumerate() {
                translated_blocks[bi] = translated[slot].clone();
            }
        }
    }
    Ok((translated_blocks, target_styles))
}

/// Re-translate a cached overlay to a new language in place, re-remapping the styles. Recovers each
/// block's source-language style ranges from `source_style_spans` (carried for exactly this) and
/// runs them through a fresh alignment, so emphasis/bold/decoration follow the new translation
/// instead of staying at the previous language's byte offsets.
pub fn retranslate_overlay(
    engine: &Mutex<BergamotEngine>,
    snapshot: &CatalogSnapshot,
    overlay: &mut PreparedImageOverlay,
    source_code: &LanguageCode,
    target_code: &LanguageCode,
) -> Result<(), TranslatorError> {
    let sources: Vec<(String, BlockStyles)> = overlay
        .blocks
        .iter()
        .map(|b| {
            (
                b.source_text.clone(),
                translator_core::ocr::style_ranges_from_spans(&b.source_style_spans),
            )
        })
        .collect();
    let (translated_blocks, target_styles) = {
        let mut engine_guard = engine.lock().expect("bergamot engine lock poisoned");
        translate_from_sources(
            &mut engine_guard,
            snapshot,
            source_code,
            target_code,
            &sources,
        )?
    };
    for (block, (text, styles)) in overlay
        .blocks
        .iter_mut()
        .zip(translated_blocks.into_iter().zip(target_styles))
    {
        block.style_spans = translator_core::ocr::style_spans_from_styles(text.len(), &styles);
        block.translated_text = text;
    }
    overlay.translated_text = overlay
        .blocks
        .iter()
        .map(|b| b.translated_text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(())
}
