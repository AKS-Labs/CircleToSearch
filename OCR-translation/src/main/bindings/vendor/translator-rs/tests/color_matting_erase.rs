//! Integration test for the production color-matting erase path
//! (`translator::color_matting::mat_detections`).
//!
//! Per case:
//!   1. PPOCR detect text boxes on the photo.
//!   2. `mat_detections` produces an inpainted rectified strip per box.
//!   3. Composite every strip back into the photo at its oriented-box
//!      placement — the same thing the live compositor does with the
//!      baked overlay.
//!   4. PPOCR detect again on the composited result. Text that was
//!      matted must no longer be detectable: a re-detection whose area
//!      mostly lies inside the union of matted source boxes counts as
//!      residual (the erase left the original glyphs legible).
//!
//! Diagnostics land in `smoke-out/color-matting-erase/<case>/`:
//!   01-detections.png  original + detected boxes
//!   02-erased.png      strips composited back
//!   03-residual.png    erased image + re-detected boxes (red = residual)
//!
//! Run:
//!   cargo test --release --features ppocr --test color_matting_erase -- --nocapture

#![cfg(feature = "ppocr")]

use std::path::{Path, PathBuf};
use std::time::Instant;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, Rgba, RgbaImage};
use imageproc::drawing::draw_hollow_rect_mut;
use imageproc::rect::Rect as ImgRect;
use translator::DetectedTextBox;
use translator::color_matting::{MattedStrip, mat_detections, union_ink_mask};
use translator::ocr::{ReadingOrder, Rect, TextBlock, TextLine};
use translator::overlay::prepare_overlay_image;
use translator::ppocr::{PpocrEngine, PpocrProfile};
use translator::settings::BackgroundMode;

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const DUMP_ROOT: &str = "smoke-out/color-matting-erase";
const DET_MAX_SIDE: u32 = 1500;

/// A re-detection counts as residual when at least this fraction of its
/// area lies inside a matted source box. Below this it is either text
/// the matting never claimed to erase (a `None` strip) or a neighbour's
/// spill-over.
const RESIDUAL_COVERAGE: f32 = 0.5;
/// Fraction of matted boxes allowed to leave residual text behind.
const MAX_RESIDUAL_FRACTION: f32 = 0.1;
/// Fraction of detections that must produce a matted strip at all.
const MIN_MATTED_FRACTION: f32 = 0.5;
/// Replaced pixels must be locally smooth (p99 of deviation from their
/// own 3×3 neighbourhood). Scratches, ghost edges and seams are
/// high-frequency by nature; a low-frequency reconstruction is not.
const MAX_CHANGED_ROUGHNESS: f32 = 25.0;

#[test]
fn erase_colors_label() {
    run_case("files/live-overlay/colors.jpg", "colors");
}

#[test]
fn erase_book() {
    run_case("files/live-overlay/book.jpg", "book");
}

#[test]
fn erase_book_angle() {
    run_case("files/live-overlay/book-angle.jpg", "book-angle");
}

#[test]
fn erase_book_page() {
    run_case("files/live-overlay/book page.jpg", "book-page");
}

#[test]
fn erase_book_page_cropped() {
    run_case(
        "files/live-overlay/book_page_cropped.jpg",
        "book-page-cropped",
    );
}

#[test]
fn erase_medicine() {
    run_case("files/live-overlay/medicine.jpg", "medicine");
}

#[test]
fn erase_sign() {
    run_case("files/live-overlay/sign.jpg", "sign");
}

fn run_case(image_path: &str, case: &str) {
    let _ = env_logger::builder().is_test(true).try_init();

    let image_path = PathBuf::from(image_path);
    if !image_path.exists() {
        eprintln!("fixture {} missing; skipping", image_path.display());
        return;
    }
    let det_path = Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn");
    if !det_path.exists() {
        eprintln!("PPOCR det model missing; skipping");
        return;
    }
    let dump_dir = PathBuf::from(DUMP_ROOT).join(case);
    std::fs::create_dir_all(&dump_dir).expect("create dump dir");

    // Apply EXIF orientation so the matting sees upright text, like the
    // production pipeline does (decode() alone ignores orientation).
    let mut decoder = ImageReader::open(&image_path)
        .expect("open fixture")
        .into_decoder()
        .expect("init decoder");
    let orientation = decoder.orientation().expect("read orientation");
    let mut raw = DynamicImage::from_decoder(decoder).expect("decode fixture");
    raw.apply_orientation(orientation);
    // Detection runs on a bounded-size image like production, but the
    // matting gets the full-resolution frame with the boxes scaled up —
    // the live pipeline mats from the camera frame, not the detector
    // input, and small text needs that resolution.
    let rgba = raw.to_rgba8();
    let dyn_image = downscale_to_max_side(raw, DET_MAX_SIDE);

    let ink_path = Path::new(MODEL_DIR).join("ink.mnn");
    if !ink_path.exists() {
        eprintln!("ink model missing; skipping");
        return;
    }
    let engine =
        PpocrEngine::load(&det_path, None, None, vec![], 1, Some(&ink_path)).expect("load ppocr");
    let det_boxes = engine
        .detect_only_image(&dyn_image, PpocrProfile::Still)
        .expect("detect fixture");
    assert!(!det_boxes.is_empty(), "expected text detections on {case}");
    let sx = rgba.width() as f32 / dyn_image.width() as f32;
    let sy = rgba.height() as f32 / dyn_image.height() as f32;
    let boxes: Vec<DetectedTextBox> = det_boxes.iter().map(|b| b.scaled_xy(sx, sy)).collect();

    // Matte against the full-res frame, the same way the live compositor does.
    let full = DynamicImage::ImageRgba8(rgba.clone());
    let t_mat = Instant::now();
    let ink_masks = engine.ink_masks(&full, &boxes, None);
    let strips = mat_detections(&rgba, &boxes, &ink_masks, &[], &[]);
    let mat_ms = t_mat.elapsed().as_secs_f64() * 1000.0;

    let matted: Vec<(usize, &MattedStrip)> = strips.iter().map(|s| (s.box_index, s)).collect();

    if std::env::var_os("MATTING_DUMP_STRIPS").is_some() {
        let strips_dir = dump_dir.join("strips");
        std::fs::create_dir_all(&strips_dir).expect("create strips dir");
        for (i, strip) in &matted {
            let img = RgbaImage::from_raw(
                strip.strip_width,
                strip.strip_height,
                strip.strip_rgba.clone(),
            )
            .expect("strip buffer");
            img.save(strips_dir.join(format!("{i:02}.png")))
                .expect("save strip");
        }
        for (i, m) in ink_masks.iter().enumerate() {
            if let Some(m) = m {
                m.save(strips_dir.join(format!("{i:02}-mask.png")))
                    .expect("save mask");
            }
        }
    }

    let (w, h) = rgba.dimensions();
    let mut erased = rgba.clone();
    let mut written = vec![false; (w as usize) * (h as usize)];
    let mut strip_stats: Vec<(usize, usize, usize)> = Vec::new();
    for (i, strip) in &matted {
        let (wrote, changed) = composite_strip(&mut erased, strip, &rgba, &mut written);
        strip_stats.push((*i, wrote, changed));
    }

    // The erase may only act inside the strips' authority regions:
    // every visibly changed pixel must have been written by a strip.
    let mut outside_changes = 0usize;
    for y in 0..h {
        for x in 0..w {
            let idx = (y as usize) * (w as usize) + x as usize;
            if !written[idx] && visibly_differs(*erased.get_pixel(x, y), *rgba.get_pixel(x, y)) {
                outside_changes += 1;
            }
        }
    }

    // The replacement must be smooth: any changed pixel that deviates
    // strongly from its own 3×3 neighbourhood in the erased image is a
    // high-frequency artefact — a scratch, a ghost edge, a seam. p99
    // over changed pixels tolerates the odd resampling outlier on
    // angled strips while still failing on visible damage.
    let _ = &strip_stats;
    let mut roughness: Vec<f32> = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let idx = (y as usize) * (w as usize) + x as usize;
            if !written[idx] || !visibly_differs(*erased.get_pixel(x, y), *rgba.get_pixel(x, y)) {
                continue;
            }
            let mut acc = [0.0f32; 3];
            for yy in y - 1..=y + 1 {
                for xx in x - 1..=x + 1 {
                    let p = erased.get_pixel(xx, yy);
                    for c in 0..3 {
                        acc[c] += p[c] as f32;
                    }
                }
            }
            let p = erased.get_pixel(x, y);
            let r = (0..3)
                .map(|c| (p[c] as f32 - acc[c] / 9.0).abs())
                .fold(0.0f32, f32::max);
            roughness.push(r);
        }
    }
    let p99_roughness = if roughness.is_empty() {
        0.0
    } else {
        let i = (roughness.len() as f32 * 0.99) as usize;
        let i = i.min(roughness.len() - 1);
        roughness.select_nth_unstable_by(i, f32::total_cmp);
        roughness[i]
    };

    let redetected = engine
        .detect_only_image(
            &DynamicImage::ImageRgba8(erased.clone()),
            PpocrProfile::Still,
        )
        .expect("detect erased");

    let matted_rects: Vec<Rect> = matted.iter().map(|(i, _)| boxes[*i].rect).collect();
    let residual: Vec<&DetectedTextBox> = redetected
        .iter()
        .filter(|r| matted_area_coverage(r.rect, &matted_rects) >= RESIDUAL_COVERAGE)
        .collect();

    let mut detections_img = rgba.clone();
    for b in &boxes {
        draw_rect(&mut detections_img, b.rect, Rgba([255, 40, 40, 255]));
    }
    let mut residual_img = erased.clone();
    for r in &redetected {
        let is_residual = matted_area_coverage(r.rect, &matted_rects) >= RESIDUAL_COVERAGE;
        let color = if is_residual {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 200, 255, 255])
        };
        draw_rect(&mut residual_img, r.rect, color);
    }
    detections_img
        .save(dump_dir.join("01-detections.png"))
        .expect("save detections");
    erased
        .save(dump_dir.join("02-erased.png"))
        .expect("save erased");
    residual_img
        .save(dump_dir.join("03-residual.png"))
        .expect("save residual");

    eprintln!(
        "{case}: boxes={} matted={} redetected={} residual={} outside_changes={} p99_roughness={:.1} mat_ms={:.1} dump={}",
        boxes.len(),
        matted.len(),
        redetected.len(),
        residual.len(),
        outside_changes,
        p99_roughness,
        mat_ms,
        dump_dir.display()
    );
    for r in &residual {
        eprintln!(
            "  residual at ({}, {})-({}, {})",
            r.rect.left, r.rect.top, r.rect.right, r.rect.bottom
        );
    }

    let min_matted = ((boxes.len() as f32) * MIN_MATTED_FRACTION).ceil() as usize;
    assert!(
        matted.len() >= min_matted,
        "{case}: only {} of {} detections produced a matted strip (need >= {})",
        matted.len(),
        boxes.len(),
        min_matted
    );
    let max_residual = ((matted.len() as f32) * MAX_RESIDUAL_FRACTION).floor() as usize;
    assert!(
        residual.len() <= max_residual,
        "{case}: {} re-detections still legible inside matted regions (allowed {})",
        residual.len(),
        max_residual
    );
    assert!(
        outside_changes == 0,
        "{case}: {} pixels changed outside any strip's authority region",
        outside_changes
    );
    assert!(
        p99_roughness <= MAX_CHANGED_ROUGHNESS,
        "{case}: replaced pixels have p99 roughness {:.1} (max {}) — high-frequency artefacts",
        p99_roughness,
        MAX_CHANGED_ROUGHNESS
    );
}

/// Paste a matted strip back into the canonical-frame image: for each
/// destination pixel inside the strip's rotated footprint, inverse-map
/// to strip coords and bilinear-sample. Mirrors what the overlay
/// compositor does with the baked strip bitmap. Marks written pixels
/// in `written` and returns `(written, visibly_changed)` counts for
/// this strip.
fn composite_strip(
    out: &mut RgbaImage,
    strip: &MattedStrip,
    original: &RgbaImage,
    written: &mut [bool],
) -> (usize, usize) {
    let mut written_count = 0usize;
    let mut changed_count = 0usize;
    let (w, h) = out.dimensions();
    let sw = strip.strip_width;
    let sh = strip.strip_height;
    let sw_us = sw as usize;
    let cos_a = strip.canonical_angle_radians.cos();
    let sin_a = strip.canonical_angle_radians.sin();
    let half_w = sw as f32 * 0.5;
    let half_h = sh as f32 * 0.5;

    let ext_x = half_w * cos_a.abs() + half_h * sin_a.abs();
    let ext_y = half_w * sin_a.abs() + half_h * cos_a.abs();
    let x0 = (strip.canonical_cx - ext_x).floor().max(0.0) as u32;
    let y0 = (strip.canonical_cy - ext_y).floor().max(0.0) as u32;
    let x1 = ((strip.canonical_cx + ext_x).ceil() as u32).min(w.saturating_sub(1));
    let y1 = ((strip.canonical_cy + ext_y).ceil() as u32).min(h.saturating_sub(1));

    for py in y0..=y1 {
        for px in x0..=x1 {
            let dx = px as f32 + 0.5 - strip.canonical_cx;
            let dy = py as f32 + 0.5 - strip.canonical_cy;
            let u = dx * cos_a + dy * sin_a + half_w;
            let v = -dx * sin_a + dy * cos_a + half_h;
            if u < 0.0 || v < 0.0 || u >= sw as f32 || v >= sh as f32 {
                continue;
            }
            let sx0 = (u - 0.5).max(0.0).floor() as u32;
            let sy0 = (v - 0.5).max(0.0).floor() as u32;
            let sx1 = (sx0 + 1).min(sw - 1);
            let sy1 = (sy0 + 1).min(sh - 1);
            let tx = (u - 0.5 - sx0 as f32).clamp(0.0, 1.0);
            let ty = (v - 0.5 - sy0 as f32).clamp(0.0, 1.0);
            let alpha_at = |x: u32, y: u32| -> u8 {
                strip.strip_rgba[((y as usize) * sw_us + x as usize) * 4 + 3]
            };
            if alpha_at(
                u.round().min((sw - 1) as f32) as u32,
                v.round().min((sh - 1) as f32) as u32,
            ) == 0
            {
                continue;
            }
            let sample = |x: u32, y: u32| -> [f32; 3] {
                let o = ((y as usize) * sw_us + x as usize) * 4;
                let p = &strip.strip_rgba[o..o + 3];
                [p[0] as f32, p[1] as f32, p[2] as f32]
            };
            let p00 = sample(sx0, sy0);
            let p10 = sample(sx1, sy0);
            let p01 = sample(sx0, sy1);
            let p11 = sample(sx1, sy1);
            let mut rgb = [0u8; 3];
            for c in 0..3 {
                let top = p00[c] * (1.0 - tx) + p10[c] * tx;
                let bot = p01[c] * (1.0 - tx) + p11[c] * tx;
                rgb[c] = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(px, py, Rgba([rgb[0], rgb[1], rgb[2], 255]));
            written[(py as usize) * (w as usize) + px as usize] = true;
            written_count += 1;
            if visibly_differs(
                Rgba([rgb[0], rgb[1], rgb[2], 255]),
                *original.get_pixel(px, py),
            ) {
                changed_count += 1;
            }
        }
    }
    (written_count, changed_count)
}

/// Per-channel tolerance absorbs the bilinear resampling blur the
/// composite warp itself introduces on angled strips.
fn visibly_differs(a: Rgba<u8>, b: Rgba<u8>) -> bool {
    (0..3).any(|c| (a[c] as i16 - b[c] as i16).abs() > 12)
}

/// Fraction of `r`'s area covered by the union of `rects`. Computed by
/// row-scanning interval union so overlapping matted boxes don't double
/// count.
fn matted_area_coverage(r: Rect, rects: &[Rect]) -> f32 {
    let area = (r.width() as u64) * (r.height() as u64);
    if area == 0 {
        return 0.0;
    }
    let mut covered: u64 = 0;
    for y in r.top..r.bottom {
        let mut spans: Vec<(u32, u32)> = rects
            .iter()
            .filter(|m| y >= m.top && y < m.bottom)
            .map(|m| (m.left.max(r.left), m.right.min(r.right)))
            .filter(|(l, rr)| rr > l)
            .collect();
        spans.sort_unstable();
        let mut end = 0u32;
        for (l, rr) in spans {
            let l = l.max(end);
            if rr > l {
                covered += (rr - l) as u64;
                end = rr;
            }
        }
    }
    covered as f32 / area as f32
}

fn draw_rect(img: &mut RgbaImage, r: Rect, color: Rgba<u8>) {
    if r.width() == 0 || r.height() == 0 {
        return;
    }
    draw_hollow_rect_mut(
        img,
        ImgRect::at(r.left as i32, r.top as i32).of_size(r.width(), r.height()),
        color,
    );
}

fn downscale_to_max_side(image: DynamicImage, max_side: u32) -> DynamicImage {
    let (w, h) = (image.width(), image.height());
    let longest = w.max(h);
    if longest <= max_side {
        return image;
    }
    let scale = max_side as f32 / longest as f32;
    let new_w = ((w as f32) * scale).round().max(1.0) as u32;
    let new_h = ((h as f32) * scale).round().max(1.0) as u32;
    image.resize_exact(new_w, new_h, FilterType::Triangle)
}

/// Drives the real still-image overlay erase (`prepare_overlay_image` with the
/// model union mask) — the path the app actually uses — and dumps the erased
/// background to `smoke-out/color-matting-erase/<case>/04-overlay-erased.png`.
#[test]
fn overlay_erase_medicine() {
    overlay_case("files/live-overlay/medicine.jpg", "medicine");
}

#[test]
fn overlay_erase_book_page() {
    overlay_case("files/live-overlay/book page.jpg", "book-page");
}

fn overlay_case(image_path: &str, case: &str) {
    let _ = env_logger::builder().is_test(true).try_init();
    let image_path = PathBuf::from(image_path);
    let det_path = Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn");
    let ink_path = Path::new(MODEL_DIR).join("ink.mnn");
    if !image_path.exists() || !det_path.exists() || !ink_path.exists() {
        eprintln!("fixture or model missing; skipping");
        return;
    }
    let dump_dir = PathBuf::from(DUMP_ROOT).join(case);
    std::fs::create_dir_all(&dump_dir).expect("create dump dir");

    let mut decoder = ImageReader::open(&image_path)
        .expect("open")
        .into_decoder()
        .expect("decoder");
    let orientation = decoder.orientation().expect("orientation");
    let mut raw = DynamicImage::from_decoder(decoder).expect("decode");
    raw.apply_orientation(orientation);
    let rgba = raw.to_rgba8();
    let dyn_image = downscale_to_max_side(raw, DET_MAX_SIDE);

    let engine =
        PpocrEngine::load(&det_path, None, None, vec![], 1, Some(&ink_path)).expect("load ppocr");
    let det = engine
        .detect_only_image(&dyn_image, PpocrProfile::Still)
        .expect("detect");
    let sx = rgba.width() as f32 / dyn_image.width() as f32;
    let sy = rgba.height() as f32 / dyn_image.height() as f32;
    let boxes: Vec<DetectedTextBox> = det.iter().map(|b| b.scaled_xy(sx, sy)).collect();

    let full = DynamicImage::ImageRgba8(rgba.clone());
    let ink_masks = engine.ink_masks(&full, &boxes, None);
    let union = union_ink_mask(&rgba, &boxes, &ink_masks, &[]);

    // One block per detected line, empty translation so nothing is drawn over
    // the erased background — we only want to inspect the erase itself.
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
                ink_color: None,
            }],
        })
        .collect();
    let translated = vec![String::new(); blocks.len()];
    let block_bold_ranges = vec![Vec::new(); blocks.len()];

    let (w, h) = rgba.dimensions();
    let prepared = prepare_overlay_image(
        rgba.as_raw(),
        w,
        h,
        &blocks,
        &translated,
        &block_bold_ranges,
        BackgroundMode::AutoDetect,
        ReadingOrder::LeftToRight,
        Some(&union),
    )
    .expect("overlay");

    let erased = RgbaImage::from_raw(prepared.width, prepared.height, prepared.rgba_bytes)
        .expect("erased buffer");
    erased
        .save(dump_dir.join("04-overlay-erased.png"))
        .expect("save erased");

    // Re-detect on the erased image; text inside the matted boxes should be gone.
    let redet = engine
        .detect_only_image(&DynamicImage::ImageRgba8(erased), PpocrProfile::Still)
        .expect("redetect");
    let before = det.len();
    let after = redet.len();
    eprintln!("overlay erase {case}: boxes={before} redetected_after_erase={after}");
    assert!(
        after * 4 <= before.max(1) * 3,
        "{case}: erase left too much detectable text ({after} of {before})"
    );
}
