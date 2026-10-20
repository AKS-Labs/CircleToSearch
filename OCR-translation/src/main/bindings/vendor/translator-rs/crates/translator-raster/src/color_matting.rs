//! Per-detection color matting: take the ink model's per-box matte for
//! each recognised text region and produce an inpainted rectified strip
//! suitable for use as a live-overlay background. Replaces the hardcoded
//! dark "pill" with a real per-pixel reconstruction of the camera
//! background under the source text.
//!
//! ## Why a rectified strip
//!
//! The strip is built in **rectified coordinates**, axis-aligned to the
//! detection's oriented box. That is the frame the GPU compositor warps
//! back per-frame, and it matches the frame the ink model's matte was
//! rendered in, so the matte registers onto the strip without a separate
//! reprojection.
//!
//! At acquire time, the `MattedStrip` produced here feeds the GPU overlay
//! compositor as a textured quad (its coverage alpha lets the camera show
//! through between strokes), and the GPU warps the baked overlay per-frame by
//! the planar-tracker homography (canonical → viewport coords).
//!
//! ## Pipeline per detection
//!
//! 1. Take the ink model's soft 0..255 matte for the box (from
//!    `PpocrEngine::ink_masks`, rendered in the box's oriented-box frame).
//! 2. Project every box's matte into one image-space *union* ink mask:
//!    walk the box's source-space bounding box, map each pixel back to
//!    the matte via the oriented frame, and set the union where the alpha
//!    clears `INK_ALPHA_CUT`. The union lets a tall strip erase the
//!    neighbouring lines that fall inside its padding.
//! 3. Derive the box's foreground colour as the median of its ink pixels —
//!    the model says *what* is ink, the pixels say what colour it is — for
//!    the translated text.
//! 4. Dewarp the camera + union ink mask into a rectified strip aligned
//!    to the oriented box, growing the fill mask by a height-proportional
//!    radius so the original ink's anti-aliased rim is erased too.
//! 5. Replace the masked pixels with a block-median background field
//!    (`background_field`): smooth, gradient-following, and robust in
//!    dense text where a directional inpaint can't find clean samples.
//!    The strip's alpha records that coverage so consumers paint only the
//!    erased pixels.
//!
//! Output: rectified RGBA strip + the oriented-box parameters
//! needed to re-warp it back to canonical-frame coords.

use image::imageops::FilterType;
use image::{GrayImage, Rgb, RgbImage, Rgba, RgbaImage};

use translator_core::ocr::{DetectedTextBox, InkColor};

/// Per-detection matting result, shared by the still-image and live-camera
/// overlays. The `strip_rgba` is a rectified RGBA image — an opaque "pill": its
/// RGB is the original sampled background everywhere except under the strokes,
/// where it is the reconstructed background field, and the **alpha is opacity**,
/// full inside the text band and feathered to zero across the outer padding. The
/// live path uploads it as a warped quad; because the pill content already
/// matches the scene, a slightly-off tracker homography slides a matching patch
/// instead of exposing the ink-shaped mask. To map it back into canonical-frame
/// coords, treat the strip as an axis-aligned bitmap centred at
/// `(canonical_cx, canonical_cy)` with dimensions `canonical_width` ×
/// `canonical_height`, rotated by `canonical_angle_radians`.
#[derive(Clone, Debug)]
pub struct MattedStrip {
    /// Index of the source detection in the original `boxes` slice.
    pub box_index: usize,
    /// Rectified strip RGBA, row-major, 4 bytes per pixel. RGB is the
    /// reconstructed background field; alpha is the (dilated) ink coverage.
    pub strip_rgba: Vec<u8>,
    /// Strip dimensions in pixels.
    pub strip_width: u32,
    pub strip_height: u32,
    /// Centre of the strip in canonical (oriented-frame) coords.
    pub canonical_cx: f32,
    pub canonical_cy: f32,
    /// Rotation of the strip — the strip's local x-axis maps to this
    /// direction in canonical coords.
    pub canonical_angle_radians: f32,
    /// Footprint of the strip in canonical coords. Includes the
    /// ascender/descender padding around the text bbox so callers can
    /// render translated text that may exceed the source's vertical
    /// extent.
    pub canonical_width: f32,
    pub canonical_height: f32,
    /// Foreground colour for translated text: the median of the source
    /// ink pixels (the real ink colour, not a binary dark/light pick).
    /// Higher byte is alpha. The ink samples are the confident stroke
    /// cores, so this is free of the anti-aliased-rim contamination that
    /// skewed the old background-colour estimate.
    pub fg_argb: u32,
}

/// Confidence above which a model ink-mask pixel is treated as ink (to
/// erase). The model emits a soft 0..255 alpha; the fill mask is then
/// grown by a height-proportional radius to catch the anti-aliased rim,
/// so a low cut is enough without bleeding into the background.
const INK_ALPHA_CUT: u8 = crate::text_metrics::INK_CUT as u8;
/// Fraction of a line's *peak* matte alpha above which a pixel counts as a
/// confident stroke core for sampling the foreground ink colour. Relative (not
/// an absolute cut) so it adapts to the matte strength: a faint line still keeps
/// its strongest pixels instead of falling back to the rim, which blends toward
/// the page and washes the colour out.
/// Fraction of a line's matte-gated pixels, on the ink side of the luma
/// distribution, used to estimate the foreground ink colour — i.e. sample the
/// stroke cores (farthest from the page), not the washed median of the whole
/// stroke.
pub const FG_INK_FRACTION: f32 = 0.30;
/// Minimum WCAG contrast ratio the translated ink is guaranteed against its
/// reconstructed background. A floor, not a stretch: faithful colours above this
/// pass through untouched; only genuinely low-contrast lines (device chrome,
/// shadowed page edges) get pushed darker/lighter to stay readable.
const FG_MIN_CONTRAST: f32 = 3.5;
/// Minimum ink pixels in a strip to bother matting it. Below this the
/// model found essentially no ink in the box — return `None` and let the
/// caller fall back to default-pill rendering.
const MIN_INK_PIXELS: usize = 6;
/// Luma distance from the ink colour below which a "background" pixel is rejected as a
/// matte-missed stroke (same colour as the ink). Wide enough to drop missed strokes and the
/// dark feather, narrower than any readable ink↔background contrast so it never eats real
/// background. See [`fg_from_samples`].
const INK_LUMA_MARGIN: i32 = 64;
/// Side of the square tiles the background field is reconstructed on, in
/// strip pixels. Each tile takes the median of its non-ink pixels; the
/// grid is then bilinearly upsampled, so this trades smoothness (larger)
/// against following tight background detail (smaller).
pub(crate) const BG_BLOCK: u32 = 10;

/// Pixels the matte ink mask is grown by before erasing, so the ink's anti-aliased rim (the
/// matte edge sits just inside it) is replaced too. Height-proportional. Shared by the per-box
/// erase (`ocr::matte_erase_oriented`) and any whole-page erase-mask consumer so they can't drift.
pub fn fill_radius(oriented_height: f32) -> u32 {
    ((oriented_height * 0.06).round() as u32).clamp(1, 6)
}

/// Compute matted strips for each detection from the ink model's per-box
/// masks. `ink_masks` is 1:1 with `boxes` (the output of
/// `PpocrEngine::ink_masks`); each mask is a soft 0..255 alpha in the
/// box's oriented-box rectified space.
///
/// The masks are first projected into a single image-space *union* ink
/// mask. A strip is padded ~75% of the line height beyond the text band,
/// so a tall strip overlaps neighbouring lines; sampling the union (not
/// just this box's mask) erases those neighbours' glyphs inside the strip
/// too. Without it, a later strip would composite a neighbour's untouched
/// original text back over a line an earlier strip had already erased.
///
/// Returns one strip per box that matted, each tagged with its `box_index`;
/// boxes with no model mask, a degenerate oriented box, or no ink are simply
/// absent (the caller renders those with the flat-fill fallback).
pub fn mat_detections(
    rgba: &RgbaImage,
    boxes: &[DetectedTextBox],
    ink_masks: &[Option<GrayImage>],
    src_maps: &[Option<Vec<(f32, f32)>>],
    model_colors: &[Option<InkColor>],
) -> Vec<MattedStrip> {
    let (w, h) = rgba.dimensions();
    let mut union_ink = vec![false; (w as usize) * (h as usize)];
    let mut fg: Vec<Option<u32>> = Vec::with_capacity(boxes.len());
    fg.resize_with(boxes.len(), || None);
    for (idx, b) in boxes.iter().enumerate() {
        let Some(Some(mask)) = ink_masks.get(idx) else {
            continue;
        };
        let src_map = src_maps.get(idx).and_then(|m| m.as_deref());
        let model_color = model_colors.get(idx).copied().flatten();
        fg[idx] = project_box_ink(rgba, b, mask, src_map, w, h, &mut union_ink, model_color);
    }

    boxes
        .iter()
        .enumerate()
        .filter_map(|(idx, b)| mat_strip_for_detection(rgba, idx, b, &union_ink, w, h, fg[idx]?))
        .collect()
}

/// Build the image-space union ink mask for `boxes` from their model mattes —
/// the same projection [`mat_detections`] uses internally, exposed for callers
/// that erase in place on the full image (the still-image overlay path) rather
/// than producing rectified strips. Row-major `y * width + x`; a pixel is set
/// where some box's matte clears the ink-alpha cut.
pub fn union_ink_mask(
    rgba: &RgbaImage,
    boxes: &[DetectedTextBox],
    ink_masks: &[Option<GrayImage>],
    src_maps: &[Option<Vec<(f32, f32)>>],
) -> Vec<bool> {
    let (w, h) = rgba.dimensions();
    let mut union = vec![false; (w as usize) * (h as usize)];
    for (idx, b) in boxes.iter().enumerate() {
        if let Some(Some(mask)) = ink_masks.get(idx) {
            let src_map = src_maps.get(idx).and_then(|m| m.as_deref());
            let _ = project_box_ink(rgba, b, mask, src_map, w, h, &mut union, None);
        }
    }
    union
}

/// Project one box's model mask into the shared image-space `union_ink`
/// (set a source pixel when its `(u, v)` in the box's oriented frame maps
/// to a model-mask alpha above the cut), and derive the box's foreground
/// colour — the median of its ink pixels — for the translated text.
/// Iterates the box's source-space bounding box directly, so the union is
/// dense at full resolution with no projection gaps. Returns `None` (the
/// box renders with the flat-fill fallback) when the model found
/// essentially no ink.
fn project_box_ink(
    image: &RgbaImage,
    detected: &DetectedTextBox,
    ink_mask: &GrayImage,
    src_map: Option<&[(f32, f32)]>,
    w: u32,
    h: u32,
    union_ink: &mut [bool],
    model_color: Option<InkColor>,
) -> Option<u32> {
    // Contour-dewarped strips carry a per-pixel source map (the strip is curl-straightened,
    // so there is no single affine inverse). Scatter the matte back through it; the sparse
    // hits a low-res strip leaves on large text are closed by the caller's fill dilation.
    if let Some(map) = src_map {
        return scatter_box_ink(image, ink_mask, map, w, h, union_ink, model_color);
    }
    let o = detected.oriented_box;
    if o.width <= 1.0 || o.height <= 1.0 {
        return None;
    }
    let cos_a = o.angle_radians.cos();
    let sin_a = o.angle_radians.sin();
    let half_w = o.width * 0.5;
    let half_h = o.height * 0.5;
    let mw = ink_mask.width() as f32;
    let mh = ink_mask.height() as f32;

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (u, v) in [
        (-half_w, -half_h),
        (half_w, -half_h),
        (half_w, half_h),
        (-half_w, half_h),
    ] {
        let px = u * cos_a - v * sin_a + o.cx;
        let py = u * sin_a + v * cos_a + o.cy;
        min_x = min_x.min(px);
        max_x = max_x.max(px);
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }
    let x0 = min_x.floor().max(0.0) as u32;
    let y0 = min_y.floor().max(0.0) as u32;
    let x1 = (max_x.ceil().max(0.0) as u32).min(w);
    let y1 = (max_y.ceil().max(0.0) as u32).min(h);

    let w_us = w as usize;
    let mut ink: Vec<Rgba<u8>> = Vec::new();
    let mut bg_lumas: Vec<u8> = Vec::new();
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 + 0.5 - o.cx;
            let dy = py as f32 + 0.5 - o.cy;
            let u = dx * cos_a + dy * sin_a;
            let v = -dx * sin_a + dy * cos_a;
            if u.abs() > half_w || v.abs() > half_h {
                continue;
            }
            let mx = (((u + half_w) / o.width) * mw).floor().clamp(0.0, mw - 1.0) as u32;
            let my = (((v + half_h) / o.height) * mh)
                .floor()
                .clamp(0.0, mh - 1.0) as u32;
            let pixel = *image.get_pixel(px, py);
            if ink_mask.get_pixel(mx, my)[0] >= INK_ALPHA_CUT {
                union_ink[(py as usize) * w_us + px as usize] = true;
                ink.push(pixel);
            } else {
                bg_lumas.push(luma(pixel));
            }
        }
    }

    fg_resolve(ink, bg_lumas, model_color)
}

/// Scatter a contour-strip matte into the image-space `union_ink` via its forward
/// source map (one `(x, y)` per matte texel), and derive the box's foreground ink
/// colour the same way [`project_box_ink`] does. `src_map` is row-major over the
/// matte's own `width × height`. The strip is 48px tall, so for source text taller
/// than that the hits are sparse; the caller's fill dilation closes the gaps.
fn scatter_box_ink(
    image: &RgbaImage,
    ink_mask: &GrayImage,
    src_map: &[(f32, f32)],
    w: u32,
    h: u32,
    union_ink: &mut [bool],
    model_color: Option<InkColor>,
) -> Option<u32> {
    let mw = ink_mask.width();
    let mh = ink_mask.height();
    if src_map.len() != (mw * mh) as usize {
        return None;
    }
    let w_us = w as usize;
    let mut ink: Vec<Rgba<u8>> = Vec::new();
    let mut bg_lumas: Vec<u8> = Vec::new();
    for my in 0..mh {
        for mx in 0..mw {
            let (sx, sy) = src_map[(my * mw + mx) as usize];
            let pxi = sx.floor() as i32;
            let pyi = sy.floor() as i32;
            if pxi < 0 || pyi < 0 || pxi >= w as i32 || pyi >= h as i32 {
                continue;
            }
            let pixel = *image.get_pixel(pxi as u32, pyi as u32);
            if ink_mask.get_pixel(mx, my)[0] >= INK_ALPHA_CUT {
                union_ink[(pyi as usize) * w_us + pxi as usize] = true;
                ink.push(pixel);
            } else {
                bg_lumas.push(luma(pixel));
            }
        }
    }
    fg_resolve(ink, bg_lumas, model_color)
}

/// Resolve a box's foreground colour: the model-predicted ink colour when the strip has a
/// colour head (only the WCAG legibility floor is applied to it, against the model's own
/// pooled background luma), otherwise the sampled-pixel derivation ([`fg_from_samples`]).
/// The `MIN_INK_PIXELS` gate applies either way — a box the matte found essentially no ink
/// in renders with the default pill regardless of how its colour would be picked.
fn fg_resolve(ink: Vec<Rgba<u8>>, bg_lumas: Vec<u8>, model_color: Option<InkColor>) -> Option<u32> {
    if ink.len() < MIN_INK_PIXELS {
        return None;
    }
    match model_color {
        Some(c) => Some(enforce_contrast(c.fg_argb, c.bg_luma, FG_MIN_CONTRAST)),
        None => fg_from_samples(ink, bg_lumas),
    }
}

/// Reduce a box's gathered ink pixels + background lumas to a foreground colour. The three
/// matting paths (`project_box_ink`, `scatter_box_ink`, `ocr::matte_erase_oriented`) gather
/// pixels in different coordinate systems but share this reduction, so the median + polarity
/// can't drift apart between them. `bg_luma` is the **median** background — robust to an
/// under-marking matte that dumps missed strokes into `bg_lumas`, where a mean would drift
/// toward the ink colour and flip the fg/bg polarity. `None` when there's too little ink.
pub(crate) fn fg_from_samples(ink: Vec<Rgba<u8>>, bg_lumas: Vec<u8>) -> Option<u32> {
    if ink.len() < MIN_INK_PIXELS {
        return None;
    }
    // Drop background samples that match the *ink core* colour — the matte-missed strokes that
    // leak into the background set and, on an ink-dominated box, out-vote the real background and
    // flip the polarity. The clean reference must be the confident stroke colour (the luma core),
    // NOT the full-ink median: when the matte over-marks it pulls bright gap pixels into the ink
    // set and drags the median to mid-gray, so cleaning around the median would delete the real
    // background instead. The core sits on the actual strokes and doesn't overshoot. A rough
    // background median orients which luma-extreme of the ink is the real core (the side farther
    // from the page). Cleaning by colour keeps the true between-stroke background, gradient and all.
    let mut raw = bg_lumas.clone();
    raw.sort_unstable();
    let rough_bg = raw[raw.len() / 2] as i32;
    let mut ink_l: Vec<u8> = ink.iter().map(|&p| luma(p)).collect();
    ink_l.sort_unstable();
    let k = ((ink_l.len() as f32 * FG_INK_FRACTION).ceil() as usize).clamp(1, ink_l.len());
    let dark_core = ink_l[(k / 2).min(ink_l.len() - 1)] as i32;
    let light_core = ink_l[ink_l.len() - 1 - (k / 2).min(ink_l.len() - 1)] as i32;
    let ink_core = if (dark_core - rough_bg).abs() >= (light_core - rough_bg).abs() {
        dark_core
    } else {
        light_core
    };
    let mut bg: Vec<u8> = bg_lumas
        .iter()
        .copied()
        .filter(|&l| (l as i32 - ink_core).abs() > INK_LUMA_MARGIN)
        .collect();
    bg.sort_unstable();
    let bg_luma = bg.get(bg.len() / 2).copied().unwrap_or(255);
    Some(ink_core_argb(ink, bg_luma))
}

/// Overlay colour for a model-predicted [`InkColor`]: the faithful predicted ink colour with
/// only the WCAG legibility floor applied (against the model's pooled background luma). This
/// is the whole colour derivation for colour-head models — no sampling, no luma-extreme core,
/// no polarity guess; those exist only as the fallback for models without the colour head.
pub fn model_fg_argb(c: InkColor) -> u32 {
    enforce_contrast(c.fg_argb, c.bg_luma, FG_MIN_CONTRAST)
}

/// Still-path foreground colour over a box AABB: gather ink + between-stroke/margin background
/// from `pixels` (row-major, `aw`×`ah`) gated by `sub` (the ink mask), then reduce via
/// [`fg_from_samples`]. Prefers the real between-stroke background; falls back to the AABB
/// margin rows when the matte over-marks and leaves too few between-stroke pixels. Shared by
/// `ocr::matte_erase_oriented` and the viz still-path dump so the decision is reproducible.
pub fn still_fg_argb(pixels: &[Rgba<u8>], sub: &[bool], aw: u32, ah: u32) -> Option<u32> {
    let near_ink = dilate(sub, aw, ah, 2);
    let margin = (ah / 8).max(2);
    let mut ink: Vec<Rgba<u8>> = Vec::new();
    let mut bg_between: Vec<u8> = Vec::new();
    let mut bg_margin: Vec<u8> = Vec::new();
    for (i, (p, &m)) in pixels.iter().zip(sub.iter()).enumerate() {
        if m {
            ink.push(*p);
            continue;
        }
        if !near_ink[i] {
            bg_between.push(luma(*p));
        }
        let ly = (i as u32) / aw;
        if ly < margin || ly >= ah - margin {
            bg_margin.push(luma(*p));
        }
    }
    let bg_lumas = if bg_between.len() >= 16 {
        bg_between
    } else {
        bg_margin
    };
    fg_from_samples(ink, bg_lumas)
}

/// Foreground ink colour from a line's matte-gated source pixels: the ink-side
/// luma *extreme* (the stroke cores), not the median of the whole stroke. The
/// 48px matte is coarser than the source, so a high-alpha matte texel maps to a
/// cluster of source pixels — the dark stroke centre plus its lighter
/// anti-aliased edges — and the median lands between them, washing the colour
/// toward the page (low-contrast lines render near-invisible). Sorting by luma
/// and keeping the extreme fraction on the ink side recovers the true ink. Polarity
/// is the ink extreme *farther* from `bg_luma`, which works for dark-on-light and
/// light-on-dark alike and is robust to a matte that mixed background pixels into the
/// ink set — those sit near `bg_luma`, so they lose the farther-extreme test to the real
/// strokes (the matte-median-vs-bg test instead inverted on heavy display type, where the
/// matte under-marks the strokes). Shared by the live (`project_box_ink`) and still
/// (`ocr::matte_erase_oriented`) paths so both colour text identically.
/// `ink` must be non-empty.
pub(crate) fn ink_core_argb(mut ink: Vec<Rgba<u8>>, bg_luma: u8) -> u32 {
    ink.sort_by_key(|&p| luma(p));
    let k = ((ink.len() as f32 * FG_INK_FRACTION).ceil() as usize).clamp(1, ink.len());
    let dark = median_color(&ink[..k]);
    let light = median_color(&ink[ink.len() - k..]);
    let dist = |c: Rgba<u8>| (luma(c) as i32 - bg_luma as i32).abs();
    let core = if dist(dark) >= dist(light) {
        dark
    } else {
        light
    };
    enforce_contrast(rgba_to_argb(core), bg_luma, FG_MIN_CONTRAST)
}

/// sRGB channel (0..1) → linear light.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light → sRGB channel (0..1).
fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Push the foreground colour away from the background until their WCAG contrast
/// ratio reaches `min_ratio`, preserving hue. The faithful ink colour is great
/// when the source has contrast, but genuinely low-contrast lines (device chrome,
/// shadowed page edges, faded print) reproduce as unreadable; this is a floor,
/// not a stretch — anything already above `min_ratio` is returned unchanged.
/// Darkening (the common dark-on-light case) scales the colour toward black in
/// linear light, which keeps chromaticity exactly; lightening blends toward white.
/// The background is approximated as a neutral gray of `bg_luma` (paper/screen is
/// near-neutral), which is all the WCAG luminance needs.
fn enforce_contrast(argb: u32, bg_luma: u8, min_ratio: f32) -> u32 {
    let (r, g, b) = (
        ((argb >> 16) & 0xff) as f32 / 255.0,
        ((argb >> 8) & 0xff) as f32 / 255.0,
        (argb & 0xff) as f32 / 255.0,
    );
    let (lr, lg, lb) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
    let l_fg = 0.2126 * lr + 0.7152 * lg + 0.0722 * lb;
    let l_bg = srgb_to_linear(bg_luma as f32 / 255.0);
    let ratio = if l_fg > l_bg {
        (l_fg + 0.05) / (l_bg + 0.05)
    } else {
        (l_bg + 0.05) / (l_fg + 0.05)
    };
    if ratio >= min_ratio {
        return argb;
    }
    let dark = l_fg < l_bg;
    let out = if dark {
        // Target luminance so (l_bg+0.05)/(target+0.05) == min_ratio, then scale
        // toward black in linear light (uniform scale ⇒ hue preserved exactly).
        let target = ((l_bg + 0.05) / min_ratio - 0.05).max(0.0);
        let t = if l_fg > 1e-6 {
            (target / l_fg).clamp(0.0, 1.0)
        } else {
            1.0
        };
        [lr * t, lg * t, lb * t]
    } else {
        // Lighten toward white.
        let target = (min_ratio * (l_bg + 0.05) - 0.05).min(1.0);
        let (df, dt) = (1.0 - l_fg, 1.0 - target);
        let t = if df > 1e-6 {
            (dt / df).clamp(0.0, 1.0)
        } else {
            1.0
        };
        [
            1.0 - (1.0 - lr) * t,
            1.0 - (1.0 - lg) * t,
            1.0 - (1.0 - lb) * t,
        ]
    };
    let to8 = |c: f32| (linear_to_srgb(c.clamp(0.0, 1.0)) * 255.0).round() as u32;
    0xFF00_0000 | (to8(out[0]) << 16) | (to8(out[1]) << 8) | to8(out[2])
}

/// BT.601 luma of an RGBA pixel (alpha ignored).
pub(crate) fn luma(c: Rgba<u8>) -> u8 {
    ((c[0] as u32 * 299 + c[1] as u32 * 587 + c[2] as u32 * 114) / 1000).min(255) as u8
}

/// Dewarp a detection's oriented box into a rectified RGBA strip, sample
/// the shared `union_ink` into strip coords, then inpaint the masked
/// pixels. The strip is padded vertically by ~75% of the line height so
/// the inpaint walk has clean background above and below the text band;
/// horizontally by 15% for translated-text breathing room.
fn mat_strip_for_detection(
    image: &RgbaImage,
    box_index: usize,
    detected: &DetectedTextBox,
    union_ink: &[bool],
    w: u32,
    h: u32,
    fg_argb: u32,
) -> Option<MattedStrip> {
    let oriented = detected.oriented_box;
    if oriented.width <= 1.0 || oriented.height <= 1.0 {
        return None;
    }
    let cos_a = oriented.angle_radians.cos();
    let sin_a = oriented.angle_radians.sin();

    let pad_x = (oriented.width * 0.15).max(4.0);
    let pad_y = (oriented.height * 0.75).max(8.0);
    let strip_w = (oriented.width + 2.0 * pad_x).ceil().max(8.0) as u32;
    let strip_h = (oriented.height + 2.0 * pad_y).ceil().max(8.0) as u32;
    let sw_us = strip_w as usize;
    let w_us = w as usize;
    let strip_cx = strip_w as f32 * 0.5;
    let strip_cy = strip_h as f32 * 0.5;

    // Sample the image + union ink mask into strip coords via inverse
    // warp. Out-of-image pixels are flagged masked so the inpaint walk
    // skips them.
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
            strip_mask[idx] = union_ink[(pyi as usize) * w_us + pxi as usize];
        }
    }

    // The model mask is glyph-tight and sampled at 48px, so on large
    // glyphs its upscaled edge sits inside the original ink's anti-aliased
    // rim. Grow the *fill* region by a height-proportional radius so that
    // rim is replaced too, instead of surviving as a faint outline.
    let fill_radius = ((oriented.height * 0.06).round() as u32).clamp(1, 6);
    let strip_mask = dilate(&strip_mask, strip_w, strip_h, fill_radius);

    // Reconstruct the background as a smooth low-frequency field
    // (block-median over non-ink pixels, nearest-fill, bilinear upsample),
    // then replace the masked pixels with it. Robust where a directional
    // inpaint walk fails: gradients (the field follows them) and dense
    // text (a block almost always has some non-ink pixel nearby).
    let bg = background_field(&strip_image, &strip_mask, strip_w, strip_h, BG_BLOCK);
    // Opaque "pill": the strip carries the *original* sampled background everywhere except
    // under the strokes, where it carries the reconstructed field. The alpha is full inside
    // the text band and feathers to zero across the outer padding, so a slightly-off
    // homography slides a patch whose content already matches the scene instead of dragging
    // the ink-shaped coverage mask across it. (The old coverage alpha let the live camera
    // show through between glyphs, which on a small lighting shift drew a distracting
    // high-contrast outline along each stroke.)
    let feather = (strip_h as f32 * 0.25).max(2.0);
    let mut strip_bytes = Vec::with_capacity((strip_w * strip_h * 4) as usize);
    for sy in 0..strip_h {
        for sx in 0..strip_w {
            let i = (sy * strip_w + sx) as usize;
            let px = if strip_mask[i] {
                let b = bg[i];
                [b[0], b[1], b[2]]
            } else {
                let o = strip_image[i];
                [o[0], o[1], o[2]]
            };
            let edge = (sx.min(strip_w - 1 - sx)).min(sy.min(strip_h - 1 - sy)) as f32;
            let alpha = ((edge / feather).clamp(0.0, 1.0) * 255.0).round() as u8;
            strip_bytes.extend_from_slice(&[px[0], px[1], px[2], alpha]);
        }
    }

    Some(MattedStrip {
        box_index,
        strip_rgba: strip_bytes,
        strip_width: strip_w,
        strip_height: strip_h,
        canonical_cx: oriented.cx,
        canonical_cy: oriented.cy,
        canonical_angle_radians: oriented.angle_radians,
        canonical_width: strip_w as f32,
        canonical_height: strip_h as f32,
        fg_argb,
    })
}

fn rgba_to_argb(c: Rgba<u8>) -> u32 {
    let a = c[3] as u32;
    let r = c[0] as u32;
    let g = c[1] as u32;
    let b = c[2] as u32;
    (a << 24) | (r << 16) | (g << 8) | b
}

/// Reconstruct the background under the ink as a smooth low-frequency
/// field. Tile the strip into `block`-px cells; each cell with enough
/// non-`exclude` (background) pixels takes their per-channel median.
/// Cells with too few background pixels (fully inside thick ink) are
/// filled from the nearest populated cell via a multi-source BFS, then
/// the coarse grid is bilinearly upsampled to full strip resolution.
///
/// Unlike a directional nearest-non-ink walk, this never fails to find a
/// sample in dense text (a neighbouring cell almost always has one) and
/// follows gradients smoothly, so the replacement matches a varying
/// background instead of smearing one edge colour across the glyph.
pub(crate) fn background_field(
    strip: &[Rgba<u8>],
    exclude: &[bool],
    w: u32,
    h: u32,
    block: u32,
) -> Vec<Rgb<u8>> {
    let gw = w.div_ceil(block);
    let gh = h.div_ceil(block);
    let w_us = w as usize;
    let mut grid = vec![[0u8; 3]; (gw * gh) as usize];
    let mut ok = vec![false; (gw * gh) as usize];

    for gy in 0..gh {
        for gx in 0..gw {
            let mut cell: Vec<Rgba<u8>> = Vec::new();
            for yy in (gy * block)..((gy + 1) * block).min(h) {
                for xx in (gx * block)..((gx + 1) * block).min(w) {
                    let i = (yy as usize) * w_us + xx as usize;
                    if !exclude[i] {
                        cell.push(strip[i]);
                    }
                }
            }
            if cell.len() >= 4 {
                let m = median_color(&cell);
                let gi = (gy * gw + gx) as usize;
                grid[gi] = [m[0], m[1], m[2]];
                ok[gi] = true;
            }
        }
    }

    if !ok.iter().any(|&b| b) {
        // Whole strip is ink: fall back to the median of every pixel.
        let all: Vec<Rgba<u8>> = strip.to_vec();
        let m = median_color(&all);
        return vec![Rgb([m[0], m[1], m[2]]); (w * h) as usize];
    }

    // Multi-source BFS from populated cells: each empty cell inherits the
    // colour of the nearest populated one.
    let mut filled = ok.clone();
    let mut queue: std::collections::VecDeque<u32> =
        (0..gw * gh).filter(|&i| ok[i as usize]).collect();
    while let Some(i) = queue.pop_front() {
        let (gx, gy) = (i % gw, i / gw);
        let visit = |nx: u32,
                     ny: u32,
                     grid: &mut [[u8; 3]],
                     filled: &mut [bool],
                     q: &mut std::collections::VecDeque<u32>| {
            let ni = (ny * gw + nx) as usize;
            if !filled[ni] {
                filled[ni] = true;
                grid[ni] = grid[i as usize];
                q.push_back(ny * gw + nx);
            }
        };
        if gx > 0 {
            visit(gx - 1, gy, &mut grid, &mut filled, &mut queue);
        }
        if gx + 1 < gw {
            visit(gx + 1, gy, &mut grid, &mut filled, &mut queue);
        }
        if gy > 0 {
            visit(gx, gy - 1, &mut grid, &mut filled, &mut queue);
        }
        if gy + 1 < gh {
            visit(gx, gy + 1, &mut grid, &mut filled, &mut queue);
        }
    }

    let small = RgbImage::from_fn(gw, gh, |x, y| Rgb(grid[(y * gw + x) as usize]));
    image::imageops::resize(&small, w, h, FilterType::Triangle)
        .pixels()
        .copied()
        .collect()
}

pub fn dilate(mask: &[bool], w: u32, h: u32, radius: u32) -> Vec<bool> {
    if radius == 0 {
        return mask.to_vec();
    }
    let w_us = w as usize;
    let h_us = h as usize;
    let r = radius as usize;

    let mut tmp = vec![false; mask.len()];
    for y in 0..h_us {
        let row = y * w_us;
        let mut count: u32 = 0;
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

pub(crate) fn median_color(colors: &[Rgba<u8>]) -> Rgba<u8> {
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
