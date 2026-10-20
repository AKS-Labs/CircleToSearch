//! Visual smoke test for per-detection ink-mask + per-column background inpaint.
//!
//! Pipeline per detection box:
//!   1. Compute a ring-median background colour from a thin annulus just outside
//!      the contour (excluding pixels claimed by any other detection).
//!   2. Inside an ROI that is the contour bbox expanded vertically by ~0.4 line
//!      heights (catches ascenders/descenders the contour misses), classify each
//!      pixel as ink when its distance from the ring median exceeds a threshold.
//!   3. Morph-close the ink mask so glyph interiors are filled.
//!   4. Extract a per-contour foreground colour as the median of the
//!      top-quartile-by-bg-distance ink pixels (robust to anti-aliased edges).
//!
//! Globally:
//!   5. Build the union ink mask, then inpaint by per-column nearest-non-ink
//!      linear interpolation, with a horizontal fallback for columns that are
//!      entirely masked.
//!
//! Outputs:
//!   01-contours.png    detection boxes + contour polygons
//!   02-ink-mask.png    union per-pixel ink mask
//!   03-erased.png      inpainted background
//!   04-repainted-debug.png  erased background + 'a'-stamped glyphs in fg colour
//!
//! Run:
//!
//!   OCR_COLOR_IMAGE=files/live-overlay/colors.jpg \
//!   OCR_COLOR_DUMP_DIR=smoke-out/ocr-color-matting \
//!   cargo test --features ppocr --test ppocr_color_matting -- --nocapture

#![cfg(feature = "ppocr")]

use std::path::{Path, PathBuf};
use std::time::Instant;

use ab_glyph::{FontArc, PxScale};
use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageReader, Luma, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_hollow_rect_mut, draw_line_segment_mut, draw_polygon_mut, draw_text_mut,
};
use imageproc::point::Point;
use imageproc::rect::Rect as ImgRect;
use translator::ocr::Rect;
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};
use translator::{DetectedTextBox, PpocrScript};

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const DEFAULT_IMAGE: &str = "files/live-overlay/colors.jpg";
const DEFAULT_DUMP_DIR: &str = "smoke-out/ocr-color-matting";
const FONT_PATH: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

#[test]
fn visual_color_matting_for_live_label() {
    let _ = env_logger::builder().is_test(true).try_init();

    let image_path =
        env_var_path("OCR_COLOR_IMAGE").unwrap_or_else(|| PathBuf::from(DEFAULT_IMAGE));
    if !image_path.exists() {
        eprintln!("{} missing; skipping", image_path.display());
        return;
    }
    let Some((det_path, rec_path, keys_path)) = ppocr_paths() else {
        eprintln!("PPOCR color fixture model files missing; skipping");
        return;
    };
    let dump_dir =
        env_var_path("OCR_COLOR_DUMP_DIR").unwrap_or_else(|| PathBuf::from(DEFAULT_DUMP_DIR));
    std::fs::create_dir_all(&dump_dir).expect("create dump dir");

    let raw_image = ImageReader::open(&image_path)
        .expect("open fixture")
        .decode()
        .expect("decode fixture");
    let dyn_image = downscale_to_max_side(raw_image, 1500);
    let rgba = dyn_image.to_rgba8();
    let gray = dyn_image.to_luma8();

    let recognizer_spec = PpocrRecognizerSpec {
        script: PpocrScript::Latin,
        model_path: rec_path,
        keys_path,
    };
    let ink_path = det_path
        .parent()
        .map(|d| d.join("ink.mnn"))
        .filter(|p| p.exists());
    let engine = PpocrEngine::load(
        &det_path,
        None,
        None,
        vec![recognizer_spec],
        1,
        ink_path.as_deref(),
    )
    .expect("load ppocr");
    let t_detect = Instant::now();
    let boxes = engine
        .detect_only_image(&dyn_image, PpocrProfile::Still)
        .expect("detect fixture");
    let detect_ms = t_detect.elapsed().as_secs_f64() * 1000.0;
    assert!(
        !boxes.is_empty(),
        "expected at least one text detection, got {}",
        boxes.len()
    );

    let t_rec = Instant::now();
    let scripts = vec![PpocrScript::Latin; boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(&dyn_image, &boxes, &scripts, PpocrProfile::Still, None)
        .expect("recognize fixture");
    let rec_ms = t_rec.elapsed().as_secs_f64() * 1000.0;

    let (w, h) = rgba.dimensions();

    let t_contour = Instant::now();
    let contour_masks: Vec<ContourMask> = boxes
        .iter()
        .enumerate()
        .filter_map(|(idx, b)| rasterize_contour_mask(w, h, b, idx))
        .collect();
    let contour_occupancy = build_contour_occupancy(w, h, &contour_masks);
    let contour_ms = t_contour.elapsed().as_secs_f64() * 1000.0;

    let t_ink = Instant::now();
    let mut ink_mask = vec![false; (w as usize) * (h as usize)];
    let mut detections = Vec::with_capacity(contour_masks.len());
    for cmask in &contour_masks {
        let Some(det) = build_detection(&rgba, cmask, &contour_occupancy, w, h) else {
            continue;
        };
        for &(px, py) in &det.ink_pixels {
            ink_mask[(py as usize) * (w as usize) + px as usize] = true;
        }
        detections.push(det);
    }
    let ink_ms = t_ink.elapsed().as_secs_f64() * 1000.0;

    let t_inpaint_half = Instant::now();
    let inpainted = inpaint_columns(&rgba, &ink_mask);
    let inpaint_half_ms = t_inpaint_half.elapsed().as_secs_f64() * 1000.0;

    let t_inpaint_full = Instant::now();
    let inpainted_full = inpaint_full_res(&rgba, &ink_mask);
    let inpaint_full_ms = t_inpaint_full.elapsed().as_secs_f64() * 1000.0;

    let t_inpaint_quarter = Instant::now();
    let inpainted_quarter = inpaint_quarter_res(&rgba, &ink_mask);
    let inpaint_quarter_ms = t_inpaint_quarter.elapsed().as_secs_f64() * 1000.0;

    let t_inpaint_dewarp = Instant::now();
    let inpainted_dewarp = inpaint_dewarp(&rgba, &ink_mask, &boxes, &detections);
    let inpaint_dewarp_ms = t_inpaint_dewarp.elapsed().as_secs_f64() * 1000.0;

    let t_fg = Instant::now();
    for det in detections.iter_mut() {
        det.fg = estimate_fg(&rgba, &inpainted, &det.ink_pixels);
    }
    let fg_ms = t_fg.elapsed().as_secs_f64() * 1000.0;

    // Dump 01: contours and boxes on the original image.
    let mut contours_img = rgba.clone();
    for det in &boxes {
        draw_box_and_contour(&mut contours_img, det);
    }

    // Dump 02: ink mask with detection contours overlaid in red. Lets us spot
    // regions where PPOCR *did* detect text but the mask produced little or
    // nothing — those contours show as red outlines around mostly-black areas.
    let mut mask_img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));
    for y in 0..h {
        for x in 0..w {
            if ink_mask[(y as usize) * (w as usize) + x as usize] {
                mask_img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
    for det in &boxes {
        if det.contour.len() >= 4 {
            let n = det.contour.len() / 2;
            for i in 0..n {
                let j = (i + 1) % n;
                draw_line_segment_mut(
                    &mut mask_img,
                    (det.contour[i * 2], det.contour[i * 2 + 1]),
                    (det.contour[j * 2], det.contour[j * 2 + 1]),
                    Rgba([255, 0, 0, 255]),
                );
            }
        }
    }

    // Dump 04: erased + 'a'-stamps in fg colour per line.
    let font = load_font();
    let mut repainted = inpainted.clone();
    for det in &detections {
        let Some(line) = lines.get(det.box_index) else {
            continue;
        };
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        let scale = (det.contour_rect.height() as f32 * 0.75).clamp(10.0, 32.0);
        draw_text_mut(
            &mut repainted,
            det.fg,
            det.contour_rect.left as i32,
            det.contour_rect.top as i32,
            PxScale::from(scale),
            &font,
            &replace_non_whitespace_with_a(text),
        );
    }

    let strong_contrast = detections
        .iter()
        .filter(|d| rgb_dist2(d.fg, d.bg_ring_median) >= 35.0 * 35.0)
        .count();
    let total_ink: usize = detections.iter().map(|d| d.ink_pixels.len()).sum();

    contours_img
        .save(dump_dir.join("01-contours.png"))
        .expect("save contours");
    mask_img
        .save(dump_dir.join("02-ink-mask.png"))
        .expect("save mask");
    inpainted
        .save(dump_dir.join("03-erased.png"))
        .expect("save erased (half-res)");
    inpainted_full
        .save(dump_dir.join("03b-erased-fullres.png"))
        .expect("save erased (full-res)");
    inpainted_quarter
        .save(dump_dir.join("03c-erased-quarterres.png"))
        .expect("save erased (quarter-res)");
    inpainted_dewarp
        .save(dump_dir.join("03d-erased-dewarp.png"))
        .expect("save erased (dewarp)");
    repainted
        .save(dump_dir.join("04-repainted-debug.png"))
        .expect("save repainted");

    let fallback_count = detections.iter().filter(|d| d.fell_back).count();
    let mut coverages: Vec<u32> = detections.iter().map(|d| d.poly_coverage_pct).collect();
    coverages.sort_unstable();
    let p10 = coverages.get(coverages.len() / 10).copied().unwrap_or(0);
    let p50 = coverages.get(coverages.len() / 2).copied().unwrap_or(0);
    eprintln!(
        "coverage p10={}%, p50={}%, fallbacks={}/{}",
        p10,
        p50,
        fallback_count,
        detections.len()
    );
    eprintln!(
        "color matting: boxes={} detections={} strong_contrast={} ink_px={} timings_ms detect={:.1} rec={:.1} contour={:.1} ink={:.1} inpaint_half={:.1} inpaint_full={:.1} inpaint_quarter={:.1} inpaint_dewarp={:.1} fg={:.1} dump={}",
        boxes.len(),
        detections.len(),
        strong_contrast,
        total_ink,
        detect_ms,
        rec_ms,
        contour_ms,
        ink_ms,
        inpaint_half_ms,
        inpaint_full_ms,
        inpaint_quarter_ms,
        inpaint_dewarp_ms,
        fg_ms,
        dump_dir.display()
    );
    assert!(
        detections.len() > boxes.len() / 2,
        "too few detections produced ink masks: {} of {}",
        detections.len(),
        boxes.len()
    );
    assert!(
        strong_contrast > detections.len() / 2,
        "too many fg/bg estimates had weak contrast: {}/{}",
        strong_contrast,
        detections.len()
    );
}

struct ContourMask {
    box_index: usize,
    rect: Rect,
    width: u32,
    height: u32,
    bits: Vec<bool>,
}

impl ContourMask {
    fn contains(&self, x: u32, y: u32) -> bool {
        if x < self.rect.left || x >= self.rect.right || y < self.rect.top || y >= self.rect.bottom
        {
            return false;
        }
        let lx = x - self.rect.left;
        let ly = y - self.rect.top;
        self.bits[(ly as usize) * (self.width as usize) + lx as usize]
    }
}

struct Detection {
    box_index: usize,
    contour_rect: Rect,
    bg_ring_median: Rgba<u8>,
    fg: Rgba<u8>,
    ink_pixels: Vec<(u32, u32)>,
    fell_back: bool,
    poly_coverage_pct: u32,
}

fn build_detection(
    image: &RgbaImage,
    cmask: &ContourMask,
    contour_occupancy: &[bool],
    w: u32,
    h: u32,
) -> Option<Detection> {
    let line_h = cmask.rect.height().max(1);
    // Vertical breathing room for ascenders/descenders, in pixels.
    let pad_y = (line_h / 3).clamp(3, 14);
    // Ring annulus around the dilated polygon. Pixels in the ring are guaranteed
    // outside the ascender/descender band, so they're true background.
    let ring_thickness = (line_h / 3).clamp(4, 14);

    // ROI must hold the dilated polygon (classify_region) AND the ring annulus
    // around it. Without the +ring_thickness margin the ring would be clipped
    // at the ROI edge and end up empty, so the bg median falls back to noise.
    let pad_x = pad_y + ring_thickness;
    let pad_y_total = pad_y + ring_thickness;
    let roi = inflate_rect_xy(cmask.rect, pad_x, pad_y_total, w, h);
    let roi_w = roi.width().max(1);
    let roi_h = roi.height().max(1);
    let roi_w_us = roi_w as usize;

    // Local copy of the contour polygon, repositioned to ROI coordinates.
    let mut polygon_local = vec![false; (roi_w as usize) * (roi_h as usize)];
    for cy in 0..cmask.height {
        for cx in 0..cmask.width {
            if !cmask.bits[(cy as usize) * (cmask.width as usize) + cx as usize] {
                continue;
            }
            let gx = cmask.rect.left + cx;
            let gy = cmask.rect.top + cy;
            let lx = gx - roi.left;
            let ly = gy - roi.top;
            polygon_local[(ly as usize) * roi_w_us + lx as usize] = true;
        }
    }

    // Classification region: polygon dilated by pad_y. Catches ascenders and
    // descenders without flaring out sideways into the next column of text.
    let classify_region = dilate(&polygon_local, roi_w, roi_h, pad_y);
    // Ring region: classify_region dilated by ring_thickness, minus
    // classify_region itself, minus pixels claimed by any *other* detection's
    // contour polygon.
    let ring_outer = dilate(&classify_region, roi_w, roi_h, ring_thickness);

    let mut ring_samples = Vec::new();
    for ly in 0..roi_h {
        for lx in 0..roi_w {
            let idx = (ly as usize) * roi_w_us + lx as usize;
            if !ring_outer[idx] || classify_region[idx] {
                continue;
            }
            let gx = roi.left + lx;
            let gy = roi.top + ly;
            // Skip pixels claimed by other detections' contours. (This detection's
            // own contour is inside classify_region and already excluded above.)
            if contour_occupancy[(gy as usize) * (w as usize) + gx as usize]
                && !cmask.contains(gx, gy)
            {
                continue;
            }
            ring_samples.push(*image.get_pixel(gx, gy));
        }
    }
    if ring_samples.len() < 24 {
        return None;
    }
    let bg_median = median_color(&ring_samples);
    let bg_mad = mad_distance(&ring_samples, bg_median);

    // Otsu's threshold on luma, computed from *polygon-internal* pixels only.
    // PPOCR's contour is the most reliable spatial prior for "where text is";
    // computing Otsu inside it ignores bg noise in the surrounding dilation
    // band. The ring is used only afterwards to decide which class is ink.
    let mut luma_hist = [0u32; 256];
    let mut poly_count = 0usize;
    for idx in 0..polygon_local.len() {
        if !polygon_local[idx] {
            continue;
        }
        let lx = (idx % roi_w_us) as u32;
        let ly = (idx / roi_w_us) as u32;
        let gx = roi.left + lx;
        let gy = roi.top + ly;
        let p = image.get_pixel(gx, gy);
        luma_hist[luma(*p) as usize] += 1;
        poly_count += 1;
    }
    if poly_count < 16 {
        return None;
    }
    let otsu_threshold = otsu_split(&luma_hist);
    let ink_is_dark = decide_ink_class(
        image,
        roi,
        polygon_local.as_slice(),
        roi_w_us,
        otsu_threshold,
        bg_median,
    )?;

    let on_ink_side = |p: Rgba<u8>| -> bool {
        if ink_is_dark {
            luma(p) < otsu_threshold
        } else {
            luma(p) > otsu_threshold
        }
    };

    // Seed: polygon pixels on the ink side of Otsu's split.
    let mut seed = vec![false; polygon_local.len()];
    for idx in 0..polygon_local.len() {
        if !polygon_local[idx] {
            continue;
        }
        let lx = (idx % roi_w_us) as u32;
        let ly = (idx / roi_w_us) as u32;
        let p = *image.get_pixel(roi.left + lx, roi.top + ly);
        if on_ink_side(p) {
            seed[idx] = true;
        }
    }
    // Candidate: classify_region pixels on the ink side, two-tier threshold:
    //   - inside polygon → strict Otsu (clean ink/bg split where contour is
    //     ground truth).
    //   - outside polygon (asc/desc territory) → looser cut by ~18 luma units
    //     so alpha-blended rim pixels qualify. They only reach the final mask
    //     via hysteresis from a polygon seed, so they can't flood pure bg.
    let outside_bias = 18i32;
    let outside_cut = if ink_is_dark {
        (otsu_threshold as i32 + outside_bias).min(255) as u8
    } else {
        (otsu_threshold as i32 - outside_bias).max(0) as u8
    };
    let on_ink_side_at = |p: Rgba<u8>, cut: u8| -> bool {
        if ink_is_dark {
            luma(p) < cut
        } else {
            luma(p) > cut
        }
    };
    let mut candidate = vec![false; polygon_local.len()];
    for idx in 0..polygon_local.len() {
        if !classify_region[idx] {
            continue;
        }
        let lx = (idx % roi_w_us) as u32;
        let ly = (idx / roi_w_us) as u32;
        let gx = roi.left + lx;
        let gy = roi.top + ly;
        if !cmask.contains(gx, gy) && contour_occupancy[(gy as usize) * (w as usize) + gx as usize]
        {
            continue;
        }
        let p = *image.get_pixel(gx, gy);
        let cut = if polygon_local[idx] {
            otsu_threshold
        } else {
            outside_cut
        };
        if on_ink_side_at(p, cut) {
            candidate[idx] = true;
        }
    }
    let ink_core = hysteresis_flood_with_cca(&seed, &candidate, roi_w, roi_h, line_h);

    // Mask = (contour polygon + 10% line height padding) ∪ (Otsu pixels that
    // sit OUTSIDE the polygon, dilated for rim). PPOCR's contour is the
    // ground truth for "where text is"; we trust it as the floor regardless
    // of what Otsu finds. Otsu's role is reduced to extending the mask up/
    // down where ascenders/descenders genuinely poke beyond the contour.
    // 12% padding covers the bulk of the contour interior cleanly without
    // making high-contrast rows look like solid blobs. Ascender/descender
    // pixels poking further out are picked up by the Otsu extension below
    // (which uses a looser luma threshold *outside* the polygon).
    let small_pad = ((line_h * 12 + 50) / 100).max(2);
    let base_mask = dilate(&polygon_local, roi_w, roi_h, small_pad);

    let extension_seed: Vec<bool> = ink_core
        .iter()
        .zip(polygon_local.iter())
        .map(|(&core, &poly)| core && !poly)
        .collect();
    // Cap the extension to ~25% of line height beyond the polygon. Real
    // ascenders extend ~30% of x-height; PPOCR contours usually cover slightly
    // more than x-height, so this is plenty without letting the flood drag
    // unrelated neighbour-line pixels into the mask.
    let extension_cap_radius = (line_h / 4).clamp(2, 8);
    let extension_cap = dilate(&polygon_local, roi_w, roi_h, extension_cap_radius);
    let extension_dilated = dilate(&extension_seed, roi_w, roi_h, 1);
    let extension_capped: Vec<bool> = extension_dilated
        .iter()
        .zip(extension_cap.iter())
        .map(|(&e, &c)| e && c)
        .collect();

    let ink: Vec<bool> = base_mask
        .iter()
        .zip(extension_capped.iter())
        .map(|(&a, &b)| a || b)
        .collect();

    // Diagnostic: how much of the polygon Otsu actually found ink in.
    let mut poly_total = 0usize;
    let mut core_in_poly = 0usize;
    for idx in 0..polygon_local.len() {
        if polygon_local[idx] {
            poly_total += 1;
            if ink_core[idx] {
                core_in_poly += 1;
            }
        }
    }
    let fell_back = false;

    let mut ink_pixels = Vec::new();
    for ly in 0..roi_h {
        for lx in 0..roi_w {
            if ink[(ly as usize) * roi_w_us + lx as usize] {
                ink_pixels.push((roi.left + lx, roi.top + ly));
            }
        }
    }
    if ink_pixels.len() < 6 {
        return None;
    }

    let poly_coverage_pct = if poly_total > 0 {
        (core_in_poly * 100 / poly_total) as u32
    } else {
        0
    };
    Some(Detection {
        box_index: cmask.box_index,
        contour_rect: cmask.rect,
        bg_ring_median: bg_median,
        fg: bg_median, // refined in estimate_fg after inpaint is built
        ink_pixels,
        fell_back,
        poly_coverage_pct,
    })
}

fn estimate_fg(image: &RgbaImage, inpainted: &RgbaImage, ink_pixels: &[(u32, u32)]) -> Rgba<u8> {
    if ink_pixels.is_empty() {
        return Rgba([0, 0, 0, 255]);
    }
    let mut scored: Vec<(f32, Rgba<u8>)> = ink_pixels
        .iter()
        .map(|&(x, y)| {
            let p = *image.get_pixel(x, y);
            let bg = *inpainted.get_pixel(x, y);
            (rgb_dist2(p, bg), p)
        })
        .collect();
    scored.sort_unstable_by(|a, b| b.0.total_cmp(&a.0));
    let take = (scored.len() / 4).max(1);
    let core: Vec<Rgba<u8>> = scored.iter().take(take).map(|(_, p)| *p).collect();
    median_color(&core)
}

/// Inpaint masked pixels. Strategy: downsample image + mask to half resolution,
/// run the 4-direction inpaint at half-res, then upsample (bilinearly) only the
/// masked pixels back into a clone of the original. Half-res cuts inpaint work
/// to ~1/4 with negligible visual loss because the masked regions are then
/// overwritten by translated text anyway.
fn inpaint_columns(image: &RgbaImage, mask: &[bool]) -> RgbaImage {
    let indices = mask_indices(mask);
    // Buffer = scale at full-res. Excluding these pixels from the bg avg in
    // the downsample prevents descender rim from contaminating lo_image.
    let (w, h) = image.dimensions();
    let sample_mask = dilate(mask, w, h, 2);
    inpaint_at_scale(image, mask, &indices, &sample_mask, 2)
}

fn inpaint_quarter_res(image: &RgbaImage, mask: &[bool]) -> RgbaImage {
    let indices = mask_indices(mask);
    let (w, h) = image.dimensions();
    let sample_mask = dilate(mask, w, h, 4);
    inpaint_at_scale(image, mask, &indices, &sample_mask, 4)
}

fn mask_indices(mask: &[bool]) -> Vec<usize> {
    mask.iter()
        .enumerate()
        .filter_map(|(i, &m)| if m { Some(i) } else { None })
        .collect()
}

/// Inpaint via downsample → low-res inpaint → bilinear upsample-into-mask.
/// `scale` is the downsample factor (2 for half-res, 4 for quarter-res). All
/// per-pixel work happens at low-res (downsample is O(W*H) but with trivial
/// per-pixel work; the dilate that protects descender rim runs on the lo-res
/// mask inside `inpaint_native`).
fn inpaint_at_scale(
    image: &RgbaImage,
    mask: &[bool],
    indices: &[usize],
    sample_mask: &[bool],
    scale: u32,
) -> RgbaImage {
    let (w, h) = image.dimensions();
    let w_us = w as usize;

    let (lo_image, lo_mask, lw, lh) = downsample_for_inpaint(image, mask, sample_mask, scale);

    // Sample buffer is already baked into lo_image via the bg avg exclusion;
    // sample_radius=0 in inpaint_native.
    let max_search = (40 / scale).max(4);
    let lo_out = inpaint_native(&lo_image, &lo_mask, lw, lh, max_search, 0, 1);

    // Upsample: iterate over precomputed masked-pixel positions only.
    let mut out = image.clone();
    let lw_us = lw as usize;
    let max_lx = lw - 1;
    let max_ly = lh - 1;
    let inv_scale = 1.0 / scale as f32;
    let half_offset = (scale as f32 - 1.0) * 0.5;
    for &idx in indices {
        let x = (idx % w_us) as u32;
        let y = (idx / w_us) as u32;
        // Map full-res pixel centre to lo-res coords via the same block-centre
        // offset used by the NxN downsample.
        let fx = (x as f32 - half_offset) * inv_scale;
        let fy = (y as f32 - half_offset) * inv_scale;
        let lx0 = if fx < 0.0 {
            0
        } else {
            (fx.floor() as u32).min(max_lx)
        };
        let ly0 = if fy < 0.0 {
            0
        } else {
            (fy.floor() as u32).min(max_ly)
        };
        let lx1 = (lx0 + 1).min(max_lx);
        let ly1 = (ly0 + 1).min(max_ly);
        let tx = (fx - lx0 as f32).clamp(0.0, 1.0);
        let ty = (fy - ly0 as f32).clamp(0.0, 1.0);

        let i00 = (ly0 as usize) * lw_us + lx0 as usize;
        let i10 = (ly0 as usize) * lw_us + lx1 as usize;
        let i01 = (ly1 as usize) * lw_us + lx0 as usize;
        let i11 = (ly1 as usize) * lw_us + lx1 as usize;
        let w00 = (1.0 - tx) * (1.0 - ty);
        let w10 = tx * (1.0 - ty);
        let w01 = (1.0 - tx) * ty;
        let w11 = tx * ty;

        let mut wsum = 0.0f32;
        let mut rsum = 0.0f32;
        let mut gsum = 0.0f32;
        let mut bsum = 0.0f32;
        for (i, weight) in [(i00, w00), (i10, w10), (i01, w01), (i11, w11)] {
            if !lo_mask[i] {
                continue;
            }
            let p = lo_out[i];
            wsum += weight;
            rsum += p[0] as f32 * weight;
            gsum += p[1] as f32 * weight;
            bsum += p[2] as f32 * weight;
        }
        if wsum == 0.0 {
            continue;
        }
        let r = (rsum / wsum).round().clamp(0.0, 255.0) as u8;
        let g = (gsum / wsum).round().clamp(0.0, 255.0) as u8;
        let b = (bsum / wsum).round().clamp(0.0, 255.0) as u8;
        out.put_pixel(x, y, Rgba([r, g, b, 255]));
    }
    out
}

/// Full-resolution inpaint (reference quality). Same algorithm but at native
/// resolution with 2 smoothing passes and a 2-px sample buffer. Slower but
/// crisper at glyph edges.
fn inpaint_full_res(image: &RgbaImage, mask: &[bool]) -> RgbaImage {
    let (w, h) = image.dimensions();
    let pixels: Vec<Rgba<u8>> = image.pixels().copied().collect();
    let out = inpaint_native(&pixels, mask, w, h, 40, 2, 2);
    let mut img = RgbaImage::new(w, h);
    for (i, p) in out.iter().enumerate() {
        let x = (i % w as usize) as u32;
        let y = (i / w as usize) as u32;
        img.put_pixel(x, y, *p);
    }
    img
}

/// Per-detection oriented inpaint. For each detection, build a dewarped strip
/// from `oriented_box` (text becomes horizontal in strip coords) with enough
/// padding above/below to give the 4-direction sweep clean bg samples on both
/// sides of the band. Run the existing `inpaint_native` on each strip, then
/// inverse-warp the inpainted pixels back to original coords — but only at
/// positions that were originally masked.
fn inpaint_dewarp(
    image: &RgbaImage,
    mask: &[bool],
    boxes: &[DetectedTextBox],
    detections: &[Detection],
) -> RgbaImage {
    let (w, h) = image.dimensions();
    let w_us = w as usize;
    let mut out = image.clone();

    for det in detections {
        let oriented = boxes[det.box_index].oriented_box;
        let cos_a = oriented.angle_radians.cos();
        let sin_a = oriented.angle_radians.sin();

        // 25% width / 75% height padding around the oriented box. Production
        // OCR uses ~1.4x, we need more for inpaint to find clean bg samples
        // above and below the text band.
        let pad_x = (oriented.width * 0.15).max(4.0);
        let pad_y = (oriented.height * 0.75).max(8.0);
        let strip_w = (oriented.width + 2.0 * pad_x).ceil().max(8.0) as u32;
        let strip_h = (oriented.height + 2.0 * pad_y).ceil().max(8.0) as u32;
        let sw_us = strip_w as usize;
        let strip_cx = strip_w as f32 * 0.5;
        let strip_cy = strip_h as f32 * 0.5;

        // Sample the global image + mask into strip coords using inverse warp.
        // Out-of-bounds (strip pixels that fall outside the image) are flagged
        // as masked so the inpaint walk doesn't sample garbage.
        let mut strip_image = vec![Rgba([0u8; 4]); (strip_w * strip_h) as usize];
        let mut strip_mask = vec![false; (strip_w * strip_h) as usize];
        for sy in 0..strip_h {
            for sx in 0..strip_w {
                let u = sx as f32 + 0.5 - strip_cx;
                let v = sy as f32 + 0.5 - strip_cy;
                let px = u * cos_a - v * sin_a + oriented.cx;
                let py = u * sin_a + v * cos_a + oriented.cy;
                let pxi = px.floor() as i32;
                let pyi = py.floor() as i32;
                let idx = (sy as usize) * sw_us + sx as usize;
                if pxi < 0 || pyi < 0 || pxi >= w as i32 || pyi >= h as i32 {
                    strip_mask[idx] = true;
                    continue;
                }
                strip_image[idx] = *image.get_pixel(pxi as u32, pyi as u32);
                strip_mask[idx] = mask[(pyi as usize) * w_us + pxi as usize];
            }
        }

        let strip_out = inpaint_native(&strip_image, &strip_mask, strip_w, strip_h, 40, 2, 1);

        // Forward-warp: for each of this detection's masked pixels, find strip
        // coords and bilinear-sample the inpainted strip.
        let max_sx = (strip_w - 1) as f32;
        let max_sy = (strip_h - 1) as f32;
        for &(px, py) in &det.ink_pixels {
            let dx = px as f32 + 0.5 - oriented.cx;
            let dy = py as f32 + 0.5 - oriented.cy;
            let u = dx * cos_a + dy * sin_a;
            let vv = -dx * sin_a + dy * cos_a;
            let sx = (u + strip_cx).clamp(0.0, max_sx);
            let sy = (vv + strip_cy).clamp(0.0, max_sy);
            let sx0 = sx.floor() as u32;
            let sy0 = sy.floor() as u32;
            let sx1 = (sx0 + 1).min(strip_w - 1);
            let sy1 = (sy0 + 1).min(strip_h - 1);
            let tx = sx - sx0 as f32;
            let ty = sy - sy0 as f32;
            let i00 = (sy0 as usize) * sw_us + sx0 as usize;
            let i10 = (sy0 as usize) * sw_us + sx1 as usize;
            let i01 = (sy1 as usize) * sw_us + sx0 as usize;
            let i11 = (sy1 as usize) * sw_us + sx1 as usize;
            let p00 = strip_out[i00];
            let p10 = strip_out[i10];
            let p01 = strip_out[i01];
            let p11 = strip_out[i11];
            let w00 = (1.0 - tx) * (1.0 - ty);
            let w10 = tx * (1.0 - ty);
            let w01 = (1.0 - tx) * ty;
            let w11 = tx * ty;
            let r = (p00[0] as f32 * w00
                + p10[0] as f32 * w10
                + p01[0] as f32 * w01
                + p11[0] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            let g = (p00[1] as f32 * w00
                + p10[1] as f32 * w10
                + p01[1] as f32 * w01
                + p11[1] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            let b = (p00[2] as f32 * w00
                + p10[2] as f32 * w10
                + p01[2] as f32 * w01
                + p11[2] as f32 * w11)
                .round()
                .clamp(0.0, 255.0) as u8;
            out.put_pixel(px, py, Rgba([r, g, b, 255]));
        }
    }
    out
}

/// N× downsample: scaled mask = OR of NxN of `mask`. Scaled image = average of
/// NxN over pixels *not* in `sample_mask` (which includes the visible mask
/// plus a buffer for descender/rim pixels). Excluding the buffered pixels
/// from the bg avg is critical at scale=4: each 4x4 block has 16 pixels and
/// even 2-3 descender pixels in the unmasked set would drag the bg avg dark
/// enough to reconstruct the glyph at lo-res.
fn downsample_for_inpaint(
    image: &RgbaImage,
    mask: &[bool],
    sample_mask: &[bool],
    scale: u32,
) -> (Vec<Rgba<u8>>, Vec<bool>, u32, u32) {
    let (w, h) = image.dimensions();
    let sw = w.div_ceil(scale);
    let sh = h.div_ceil(scale);
    let w_us = w as usize;
    let sw_us = sw as usize;
    let mut small_image = vec![Rgba([0u8; 4]); (sw * sh) as usize];
    let mut small_mask = vec![false; (sw * sh) as usize];
    for sy in 0..sh {
        for sx in 0..sw {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut bg_count = 0u32;
            let mut any_mask = false;
            let mut fallback = Rgba([0u8; 4]);
            for dy in 0..scale {
                for dx in 0..scale {
                    let fx = sx * scale + dx;
                    let fy = sy * scale + dy;
                    if fx >= w || fy >= h {
                        continue;
                    }
                    let p = *image.get_pixel(fx, fy);
                    fallback = p;
                    let s_idx = (fy as usize) * w_us + fx as usize;
                    if mask[s_idx] {
                        any_mask = true;
                    }
                    if !sample_mask[s_idx] {
                        r += p[0] as u32;
                        g += p[1] as u32;
                        b += p[2] as u32;
                        bg_count += 1;
                    }
                }
            }
            let avg = if bg_count > 0 {
                Rgba([
                    (r / bg_count) as u8,
                    (g / bg_count) as u8,
                    (b / bg_count) as u8,
                    255,
                ])
            } else {
                fallback
            };
            small_image[(sy as usize) * sw_us + sx as usize] = avg;
            small_mask[(sy as usize) * sw_us + sx as usize] = any_mask;
        }
    }
    (small_image, small_mask, sw, sh)
}

/// Native-resolution 4-direction inpaint with outlier rejection + N smoothing
/// passes. Skips empty rows (no masked pixels in the row or its max_search
/// neighbourhood) to avoid full-image overhead when masks are sparse.
fn inpaint_native(
    image: &[Rgba<u8>],
    mask: &[bool],
    w: u32,
    h: u32,
    max_search: u32,
    sample_radius: u32,
    smoothing_passes: u32,
) -> Vec<Rgba<u8>> {
    let w_us = w as usize;
    let h_us = h as usize;
    let n = w_us * h_us;
    let sample_mask = dilate(mask, w, h, sample_radius);
    let mask_for_walk = sample_mask.as_slice();

    let mut up = vec![(u32::MAX, Rgba([0u8; 4])); n];
    let mut down = vec![(u32::MAX, Rgba([0u8; 4])); n];
    let mut left = vec![(u32::MAX, Rgba([0u8; 4])); n];
    let mut right = vec![(u32::MAX, Rgba([0u8; 4])); n];

    // Row occupancy: a row needs sampling work only if it has any masked
    // pixel within max_search above or below (so the in-row sample state
    // matters somewhere it'll be read). For rows entirely outside this
    // band, up/down state at every pixel is "no sample" and we can skip
    // them; their pixels won't be written anyway.
    let mut row_has_mask = vec![false; h_us];
    for y in 0..h_us {
        let row = y * w_us;
        for x in 0..w_us {
            if mask_for_walk[row + x] {
                row_has_mask[y] = true;
                break;
            }
        }
    }

    for x in 0..w {
        let mut state: (u32, Rgba<u8>) = (u32::MAX, Rgba([0; 4]));
        for y in 0..h {
            let idx = (y as usize) * w_us + x as usize;
            if mask_for_walk[idx] {
                if state.0 != u32::MAX {
                    state.0 = state.0.saturating_add(1);
                    if state.0 > max_search {
                        state.0 = u32::MAX;
                    }
                }
            } else {
                state = (0, image[idx]);
            }
            up[idx] = state;
        }
        state = (u32::MAX, Rgba([0; 4]));
        for y in (0..h).rev() {
            let idx = (y as usize) * w_us + x as usize;
            if mask_for_walk[idx] {
                if state.0 != u32::MAX {
                    state.0 = state.0.saturating_add(1);
                    if state.0 > max_search {
                        state.0 = u32::MAX;
                    }
                }
            } else {
                state = (0, image[idx]);
            }
            down[idx] = state;
        }
    }
    for y in 0..h_us {
        if !row_has_mask[y] {
            continue;
        }
        let mut state: (u32, Rgba<u8>) = (u32::MAX, Rgba([0; 4]));
        for x in 0..w_us {
            let idx = y * w_us + x;
            if mask_for_walk[idx] {
                if state.0 != u32::MAX {
                    state.0 = state.0.saturating_add(1);
                    if state.0 > max_search {
                        state.0 = u32::MAX;
                    }
                }
            } else {
                state = (0, image[idx]);
            }
            left[idx] = state;
        }
        state = (u32::MAX, Rgba([0; 4]));
        for x in (0..w_us).rev() {
            let idx = y * w_us + x;
            if mask_for_walk[idx] {
                if state.0 != u32::MAX {
                    state.0 = state.0.saturating_add(1);
                    if state.0 > max_search {
                        state.0 = u32::MAX;
                    }
                }
            } else {
                state = (0, image[idx]);
            }
            right[idx] = state;
        }
    }

    let mut out = image.to_vec();
    let outlier_thr = 35.0f32 * 35.0;
    for y in 0..h_us {
        if !row_has_mask[y] {
            continue;
        }
        for x in 0..w_us {
            let idx = y * w_us + x;
            if !mask[idx] {
                continue;
            }
            let raw = [up[idx], down[idx], left[idx], right[idx]];
            // Compute median of valid samples (per channel, on small fixed
            // array — no Vec alloc).
            let mut count = 0usize;
            let mut rs = [0u8; 4];
            let mut gs = [0u8; 4];
            let mut bs = [0u8; 4];
            for &(d, c) in &raw {
                if d == u32::MAX {
                    continue;
                }
                rs[count] = c[0];
                gs[count] = c[1];
                bs[count] = c[2];
                count += 1;
            }
            if count == 0 {
                continue;
            }
            rs[..count].sort_unstable();
            gs[..count].sort_unstable();
            bs[..count].sort_unstable();
            let median = Rgba([rs[count / 2], gs[count / 2], bs[count / 2], 255]);

            let mut wsum = 0.0f32;
            let mut rsum = 0.0f32;
            let mut gsum = 0.0f32;
            let mut bsum = 0.0f32;
            for &(d, c) in &raw {
                if d == u32::MAX {
                    continue;
                }
                if rgb_dist2(c, median) > outlier_thr {
                    continue;
                }
                let weight = 1.0 / (d as f32 + 1.0);
                wsum += weight;
                rsum += weight * c[0] as f32;
                gsum += weight * c[1] as f32;
                bsum += weight * c[2] as f32;
            }
            out[idx] = if wsum == 0.0 {
                median
            } else {
                Rgba([
                    (rsum / wsum).round().clamp(0.0, 255.0) as u8,
                    (gsum / wsum).round().clamp(0.0, 255.0) as u8,
                    (bsum / wsum).round().clamp(0.0, 255.0) as u8,
                    255,
                ])
            };
        }
    }

    // N smoothing passes over masked pixels only — kills residual 4-direction
    // axis-aligned banding. Scratch buffer holds only the new values for
    // masked pixels rather than cloning the whole image.
    let mut updates: Vec<(usize, Rgba<u8>)> = Vec::new();
    for _ in 0..smoothing_passes {
        updates.clear();
        for y in 0..h_us {
            if !row_has_mask[y] {
                continue;
            }
            for x in 0..w_us {
                let idx = y * w_us + x;
                if !mask[idx] {
                    continue;
                }
                let x0 = x.saturating_sub(1);
                let y0 = y.saturating_sub(1);
                let x1 = (x + 1).min(w_us - 1);
                let y1 = (y + 1).min(h_us - 1);
                let mut r = 0u32;
                let mut g = 0u32;
                let mut b = 0u32;
                let mut cnt = 0u32;
                for yy in y0..=y1 {
                    let row = yy * w_us;
                    for xx in x0..=x1 {
                        let p = out[row + xx];
                        r += p[0] as u32;
                        g += p[1] as u32;
                        b += p[2] as u32;
                        cnt += 1;
                    }
                }
                updates.push((
                    idx,
                    Rgba([(r / cnt) as u8, (g / cnt) as u8, (b / cnt) as u8, 255]),
                ));
            }
        }
        for (idx, p) in updates.iter() {
            out[*idx] = *p;
        }
    }
    out
}

/// Hysteresis + connected-component fallback. Stage 1 floods from seed pixels
/// through candidate pixels (Canny-style). Stage 2 collects the candidate
/// components that received no seed, and keeps the ones whose shape passes a
/// loose text-likeness test: small-enough bbox in both dimensions, and ≥3 px.
/// This rescues low-contrast glyphs where the entire footprint sits below the
/// high threshold, without flooding the page with jpeg noise (noise components
/// are isolated single pixels or larger blobs not bounded by a text-shaped
/// bbox).
fn hysteresis_flood_with_cca(
    seed: &[bool],
    candidate: &[bool],
    w: u32,
    h: u32,
    line_h: u32,
) -> Vec<bool> {
    let w_us = w as usize;
    let mut out = vec![false; seed.len()];
    let mut stack: Vec<(u32, u32)> = Vec::new();

    // Stage 1: hysteresis flood from seeds.
    for y in 0..h {
        for x in 0..w {
            let idx = (y as usize) * w_us + x as usize;
            if seed[idx] {
                out[idx] = true;
                stack.push((x, y));
            }
        }
    }
    while let Some((cx, cy)) = stack.pop() {
        let x0 = cx.saturating_sub(1);
        let y0 = cy.saturating_sub(1);
        let x1 = (cx + 1).min(w - 1);
        let y1 = (cy + 1).min(h - 1);
        for ny in y0..=y1 {
            for nx in x0..=x1 {
                let nidx = (ny as usize) * w_us + nx as usize;
                if out[nidx] || !candidate[nidx] {
                    continue;
                }
                out[nidx] = true;
                stack.push((nx, ny));
            }
        }
    }

    // Stage 2: CCA over remaining candidates (those with no seed nearby).
    // Accept components whose bbox is glyph-shaped: height ≤ ~1.2·line_h and
    // both width and height ≥ 1, and pixel count between 3 and a generous
    // upper bound. This includes thin low-contrast letters while rejecting
    // long noise streaks and full-line bg artifacts.
    // Glyph-sized only: kerned letter groups already have seeds and grow via
    // the hysteresis stage. CCA here is just for single low-contrast glyphs.
    let max_dim = (line_h as f32 * 1.2) as u32 + 2;
    let min_pixels = 3usize;
    let max_pixels = (line_h * line_h) as usize;
    let mut visited = out.clone();
    for sy in 0..h {
        for sx in 0..w {
            let sidx = (sy as usize) * w_us + sx as usize;
            if visited[sidx] || !candidate[sidx] {
                continue;
            }
            // Flood this component.
            let mut comp: Vec<usize> = Vec::new();
            let mut min_x = sx;
            let mut max_x = sx;
            let mut min_y = sy;
            let mut max_y = sy;
            visited[sidx] = true;
            stack.push((sx, sy));
            while let Some((cx, cy)) = stack.pop() {
                let cidx = (cy as usize) * w_us + cx as usize;
                comp.push(cidx);
                if cx < min_x {
                    min_x = cx;
                }
                if cx > max_x {
                    max_x = cx;
                }
                if cy < min_y {
                    min_y = cy;
                }
                if cy > max_y {
                    max_y = cy;
                }
                let x0 = cx.saturating_sub(1);
                let y0 = cy.saturating_sub(1);
                let x1 = (cx + 1).min(w - 1);
                let y1 = (cy + 1).min(h - 1);
                for ny in y0..=y1 {
                    for nx in x0..=x1 {
                        let nidx = (ny as usize) * w_us + nx as usize;
                        if visited[nidx] || !candidate[nidx] {
                            continue;
                        }
                        visited[nidx] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            let bbox_w = max_x - min_x + 1;
            let bbox_h = max_y - min_y + 1;
            if comp.len() < min_pixels
                || comp.len() > max_pixels
                || bbox_h > max_dim
                || bbox_w > max_dim
            {
                continue;
            }
            for cidx in comp {
                out[cidx] = true;
            }
        }
    }
    out
}

fn dilate(mask: &[bool], w: u32, h: u32, radius: u32) -> Vec<bool> {
    if radius == 0 {
        return mask.to_vec();
    }
    let w_us = w as usize;
    let h_us = h as usize;
    let r = radius as usize;

    // Horizontal pass: per row, output[x] = any-true in mask[x-r..=x+r].
    // Tracked via a counter of "true" mask pixels within the sliding window.
    let mut tmp = vec![false; mask.len()];
    for y in 0..h_us {
        let row = y * w_us;
        let mut count: u32 = 0;
        // Prime the window with [0..=r].
        for x in 0..(r + 1).min(w_us) {
            if mask[row + x] {
                count += 1;
            }
        }
        for x in 0..w_us {
            tmp[row + x] = count > 0;
            let add_x = x + r + 1;
            if add_x < w_us && mask[row + add_x] {
                count += 1;
            }
            if x >= r && mask[row + (x - r)] {
                count -= 1;
            }
        }
    }
    // Vertical pass: per column, output[y] = any-true in tmp[y-r..=y+r].
    let mut out = vec![false; mask.len()];
    for x in 0..w_us {
        let mut count: u32 = 0;
        for y in 0..(r + 1).min(h_us) {
            if tmp[y * w_us + x] {
                count += 1;
            }
        }
        for y in 0..h_us {
            out[y * w_us + x] = count > 0;
            let add_y = y + r + 1;
            if add_y < h_us && tmp[add_y * w_us + x] {
                count += 1;
            }
            if y >= r && tmp[(y - r) * w_us + x] {
                count -= 1;
            }
        }
    }
    out
}

fn rasterize_contour_mask(
    w: u32,
    h: u32,
    det: &DetectedTextBox,
    box_index: usize,
) -> Option<ContourMask> {
    let rect = clamp_rect(det.rect, w, h)?;
    let local_w = rect.width().max(1);
    let local_h = rect.height().max(1);
    let mut img = GrayImage::from_pixel(local_w, local_h, Luma([0u8]));
    if det.contour.len() >= 6 {
        let points: Vec<Point<i32>> = det
            .contour
            .chunks_exact(2)
            .map(|p| {
                Point::new(
                    (p[0] - rect.left as f32).round() as i32,
                    (p[1] - rect.top as f32).round() as i32,
                )
            })
            .collect();
        if points.len() >= 3 {
            draw_polygon_mut(&mut img, &points, Luma([255u8]));
        }
    } else {
        for p in img.pixels_mut() {
            *p = Luma([255u8]);
        }
    }
    let bits: Vec<bool> = img.pixels().map(|p| p[0] != 0).collect();
    Some(ContourMask {
        box_index,
        rect,
        width: local_w,
        height: local_h,
        bits,
    })
}

fn build_contour_occupancy(w: u32, h: u32, masks: &[ContourMask]) -> Vec<bool> {
    let mut occ = vec![false; (w as usize) * (h as usize)];
    for m in masks {
        for ly in 0..m.height {
            for lx in 0..m.width {
                if !m.bits[(ly as usize) * (m.width as usize) + lx as usize] {
                    continue;
                }
                let x = m.rect.left + lx;
                let y = m.rect.top + ly;
                occ[(y as usize) * (w as usize) + x as usize] = true;
            }
        }
    }
    occ
}

fn draw_box_and_contour(image: &mut RgbaImage, det: &DetectedTextBox) {
    draw_hollow_rect_mut(
        image,
        ImgRect::at(det.rect.left as i32, det.rect.top as i32)
            .of_size(det.rect.width().max(1), det.rect.height().max(1)),
        Rgba([255, 40, 40, 255]),
    );
    if det.contour.len() >= 4 {
        let n = det.contour.len() / 2;
        for i in 0..n {
            let j = (i + 1) % n;
            draw_line_segment_mut(
                image,
                (det.contour[i * 2], det.contour[i * 2 + 1]),
                (det.contour[j * 2], det.contour[j * 2 + 1]),
                Rgba([0, 255, 255, 255]),
            );
        }
    }
}

fn replace_non_whitespace_with_a(s: &str) -> String {
    s.chars()
        .map(|ch| if ch.is_whitespace() { ch } else { 'a' })
        .collect()
}

fn median_u8(values: impl IntoIterator<Item = u8>) -> u8 {
    let mut v: Vec<u8> = values.into_iter().collect();
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn median_color(colors: &[Rgba<u8>]) -> Rgba<u8> {
    if colors.is_empty() {
        return Rgba([0, 0, 0, 255]);
    }
    let mut r: Vec<u8> = colors.iter().map(|c| c[0]).collect();
    let mut g: Vec<u8> = colors.iter().map(|c| c[1]).collect();
    let mut b: Vec<u8> = colors.iter().map(|c| c[2]).collect();
    r.sort_unstable();
    g.sort_unstable();
    b.sort_unstable();
    let mid = r.len() / 2;
    Rgba([r[mid], g[mid], b[mid], 255])
}

fn mad_distance(samples: &[Rgba<u8>], median: Rgba<u8>) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut dists: Vec<f32> = samples
        .iter()
        .map(|c| rgb_dist2(*c, median).sqrt())
        .collect();
    dists.sort_unstable_by(f32::total_cmp);
    dists[dists.len() / 2]
}

fn decide_ink_class(
    image: &RgbaImage,
    roi: Rect,
    polygon_local: &[bool],
    roi_w_us: usize,
    otsu_threshold: u8,
    bg_median: Rgba<u8>,
) -> Option<bool> {
    let mut dark_colors = Vec::new();
    let mut light_colors = Vec::new();
    for idx in 0..polygon_local.len() {
        if !polygon_local[idx] {
            continue;
        }
        let lx = (idx % roi_w_us) as u32;
        let ly = (idx / roi_w_us) as u32;
        let p = *image.get_pixel(roi.left + lx, roi.top + ly);
        if luma(p) < otsu_threshold {
            dark_colors.push(p);
        } else {
            light_colors.push(p);
        }
    }
    if dark_colors.is_empty() || light_colors.is_empty() {
        return None;
    }
    let dark_median = median_color(&dark_colors);
    let light_median = median_color(&light_colors);
    Some(rgb_dist2(dark_median, bg_median) >= rgb_dist2(light_median, bg_median))
}

fn luma(c: Rgba<u8>) -> u8 {
    // Rec. 601 luma; integer-only weights for speed.
    let r = c[0] as u32;
    let g = c[1] as u32;
    let b = c[2] as u32;
    ((299 * r + 587 * g + 114 * b) / 1000).min(255) as u8
}

/// Otsu's method: pick the luma threshold that maximises between-class
/// variance. Returns the threshold; pixels with luma < threshold are class 1.
fn otsu_split(hist: &[u32; 256]) -> u8 {
    let total: u64 = hist.iter().map(|&v| v as u64).sum();
    if total == 0 {
        return 128;
    }
    let mut sum_total: u64 = 0;
    for i in 0..256 {
        sum_total += (i as u64) * (hist[i] as u64);
    }
    let mut sum_bg: u64 = 0;
    let mut w_bg: u64 = 0;
    let mut best_var = -1.0f64;
    let mut best_t: u8 = 128;
    for t in 0..256 {
        w_bg += hist[t] as u64;
        if w_bg == 0 {
            continue;
        }
        let w_fg = total - w_bg;
        if w_fg == 0 {
            break;
        }
        sum_bg += (t as u64) * (hist[t] as u64);
        let mean_bg = sum_bg as f64 / w_bg as f64;
        let mean_fg = (sum_total - sum_bg) as f64 / w_fg as f64;
        let diff = mean_bg - mean_fg;
        let var = w_bg as f64 * w_fg as f64 * diff * diff;
        if var > best_var {
            best_var = var;
            best_t = t as u8;
        }
    }
    best_t
}

fn rgb_dist2(a: Rgba<u8>, b: Rgba<u8>) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    dr * dr + dg * dg + db * db
}

fn clamp_rect(rect: Rect, w: u32, h: u32) -> Option<Rect> {
    let out = Rect {
        left: rect.left.min(w),
        top: rect.top.min(h),
        right: rect.right.min(w),
        bottom: rect.bottom.min(h),
    };
    if out.right > out.left && out.bottom > out.top {
        Some(out)
    } else {
        None
    }
}

fn inflate_rect_xy(rect: Rect, pad_x: u32, pad_y: u32, w: u32, h: u32) -> Rect {
    Rect {
        left: rect.left.saturating_sub(pad_x),
        top: rect.top.saturating_sub(pad_y),
        right: rect.right.saturating_add(pad_x).min(w),
        bottom: rect.bottom.saturating_add(pad_y).min(h),
    }
}

fn ppocr_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    let det = env_var_path("OCR_COLOR_DET")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn"));
    let rec = env_var_path("OCR_COLOR_REC")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn"));
    let keys = env_var_path("OCR_COLOR_KEYS")
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

fn load_font() -> FontArc {
    let bytes = std::fs::read(FONT_PATH).expect("read font");
    FontArc::try_from_vec(bytes).expect("parse font")
}
