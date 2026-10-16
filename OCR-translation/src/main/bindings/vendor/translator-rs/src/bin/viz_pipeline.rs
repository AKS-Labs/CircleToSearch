//! Pipeline visualization dumper for the PP-OCR detection/recognition path.
//!
//! Produces the intermediate images that the OCR blog post references: the raw
//! input, the detection probability heatmap, the detected boxes, recognition
//! overlays (with and without deskew, to show the failure mode), the per-box
//! cropped strips (axis-aligned, squashed to 48px, and PCA/parabola-straightened),
//! and the DocAligner perspective dewarp + proposed-quad overlay.
//!
//! Build and run:
//!     cargo run --release --features viz --bin viz_pipeline -- <image> [opts]
//!
//! Defaults look for PP-OCR models at
//! `~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5/` and the DocAligner model at
//! `~/AndroidStudioProjects/bucket/support/1/docaligner_lcnet050.onnx`; override
//! with `--model-dir` / `--docaligner`.
//!
//! Run `--list` to see the stage names.

use std::fs;
use std::path::{Path, PathBuf};

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, GrayImage, Rgba, RgbaImage};
use imageproc::drawing::{
    draw_filled_rect_mut, draw_line_segment_mut, draw_polygon_mut, draw_text_mut,
};
use imageproc::geometric_transformations::{Interpolation, rotate_about_center};
use imageproc::point::Point;
use imageproc::rect::Rect as IpRect;

use translator::doc_align::{DocAligner, DocumentPoint, DocumentQuad, suggested_output_dims, warp};
use translator::font_provider::{FontHandle, FontProvider, FontRequest};
use translator::image_render::{RenderOptions, render_overlay};
use translator::ocr::{
    DetectedTextBox, OrientedRect, PositionedWord, ReadingOrder, RecognizedTextLine, TextBlock,
    TextLine,
};
use translator::overlay::prepare_overlay_image;
use translator::ppocr::{
    PpocrEngine, PpocrProfile, PpocrRecognizerSpec, dewarp_contour_to_strip_rgb,
    dewarp_contour_to_strip_rgb_with_map,
};
use translator::{BackgroundMode, PpocrScript};

const REC_TARGET_HEIGHT: u32 = 48;

const USAGE: &str = "\
usage: viz_pipeline <image> [options]

options:
  --out <dir>          output directory (default: ./viz-out)
  --model-dir <dir>    PP-OCR model dir (default: ~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5)
  --det <path>         detection model file, overriding the model-dir picker
  --ink <path>         ink-matte .mnn (default: ink.mnn in the model dir, if present)
  --docaligner <path>  DocAligner .onnx (default: ~/AndroidStudioProjects/bucket/support/1/docaligner_lcnet050.onnx)
  --script <slug>      recognizer script: latin, cyrillic, arabic, devanagari,
                       korean, el, eslav, ta, te, th (default: latin)
  --font <path>        TTF for overlay labels (default: DejaVuSans)
  --rotate <dir>       rotate input before processing: cw|ccw|180 (default: none)
  --stages <a,b,...>   comma-separated stages to run (default: all available)
  --list               list stage names and exit
  -h, --help           show this help

the dewarp/corners stages use DocAligner (onnxruntime); the ort crate needs
  ORT_DYLIB_PATH=/path/to/libonnxruntime.so";

// ---------------------------------------------------------------------
// Stages
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Input,
    Heatmap,
    Boxes,
    OrientedBoxes,
    Contours,
    BoxHeights,
    XHeight,
    RecognizeDeskew,
    RecognizeNoDeskew,
    BboxStrips,
    Squashed,
    Deskewed,
    Ink,
    CharFirings,
    Rewrite,
    Dewarp,
    Corners,
}

impl Stage {
    const ALL: [Stage; 17] = [
        Stage::Input,
        Stage::Heatmap,
        Stage::Boxes,
        Stage::OrientedBoxes,
        Stage::Contours,
        Stage::BoxHeights,
        Stage::XHeight,
        Stage::RecognizeDeskew,
        Stage::RecognizeNoDeskew,
        Stage::BboxStrips,
        Stage::Squashed,
        Stage::Deskewed,
        Stage::Ink,
        Stage::CharFirings,
        Stage::Rewrite,
        Stage::Dewarp,
        Stage::Corners,
    ];

    fn slug(self) -> &'static str {
        match self {
            Stage::Input => "input",
            Stage::Heatmap => "heatmap",
            Stage::Boxes => "boxes",
            Stage::OrientedBoxes => "oriented-boxes",
            Stage::Contours => "contours",
            Stage::BoxHeights => "box-heights",
            Stage::XHeight => "x-height",
            Stage::RecognizeDeskew => "recognize-deskew",
            Stage::RecognizeNoDeskew => "recognize-nodeskew",
            Stage::BboxStrips => "bbox-strips",
            Stage::Squashed => "squashed",
            Stage::Deskewed => "deskewed",
            Stage::Ink => "ink",
            Stage::CharFirings => "char-firings",
            Stage::Rewrite => "rewrite",
            Stage::Dewarp => "dewarp",
            Stage::Corners => "corners",
        }
    }

    fn from_slug(slug: &str) -> Option<Stage> {
        Stage::ALL.into_iter().find(|s| s.slug() == slug)
    }

    fn description(self) -> &'static str {
        match self {
            Stage::Input => "raw input image",
            Stage::Heatmap => "detection probability map, colorized over the input",
            Stage::Boxes => "detected boxes (AABB), one color per box",
            Stage::OrientedBoxes => "detected boxes (tight oriented rect), one color per box",
            Stage::Contours => {
                "raw detection contour polygon per box, labeled with its PCA principal-axis angle"
            }
            Stage::BoxHeights => {
                "tight (green) vs unclip-inflated (magenta) rects, labeled tight→inflated height"
            }
            Stage::XHeight => {
                "inflated search box (magenta), tight detection core (green), ink-matte x-height band (cyan); needs ink model"
            }
            Stage::RecognizeDeskew => "recognition overlay, deskewed strips (the working path)",
            Stage::RecognizeNoDeskew => "recognition overlay, axis-aligned strips (the failure)",
            Stage::BboxStrips => "per-box axis-aligned crops, pre-deskew",
            Stage::Squashed => "per-box crops squashed to 48px height",
            Stage::Deskewed => "per-box PCA/parabola-straightened 48px strips",
            Stage::Ink => "per-box ink matte (alpha) from the optional ink model",
            Stage::CharFirings => {
                "per-box straightened strips with a vertical line at each CTC character firing X"
            }
            Stage::Rewrite => {
                "real erase+render path (prepare_overlay_image → render_overlay), overlaying the per-word boxes reported to the app: render-layout words (red) vs CTC source words (green); needs ink model for the erase"
            }
            Stage::Dewarp => "full-page perspective correction (DocAligner)",
            Stage::Corners => "DocAligner proposed document quad overlay",
        }
    }
}

// ---------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Rotate {
    None,
    Cw,
    Ccw,
    Half,
}

impl Rotate {
    fn apply(self, img: DynamicImage) -> DynamicImage {
        match self {
            Rotate::None => img,
            Rotate::Cw => img.rotate90(),
            Rotate::Ccw => img.rotate270(),
            Rotate::Half => img.rotate180(),
        }
    }
}

struct Cli {
    input: PathBuf,
    out_dir: PathBuf,
    model_dir: PathBuf,
    det: Option<PathBuf>,
    ink: Option<PathBuf>,
    docaligner: PathBuf,
    script: PpocrScript,
    font: PathBuf,
    stages: Vec<Stage>,
    rotate: Rotate,
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(s)
}

fn script_from_slug(slug: &str) -> Option<PpocrScript> {
    Some(match slug {
        "arabic" => PpocrScript::Arabic,
        "cj" => PpocrScript::Cj,
        "cyrillic" => PpocrScript::Cyrillic,
        "devanagari" => PpocrScript::Devanagari,
        "el" => PpocrScript::El,
        "eslav" => PpocrScript::Eslav,
        "georgian" => PpocrScript::Georgian,
        "hebrew" => PpocrScript::Hebrew,
        "indic" => PpocrScript::Indic,
        "korean" => PpocrScript::Korean,
        "latin" => PpocrScript::Latin,
        "ta" => PpocrScript::Ta,
        "te" => PpocrScript::Te,
        "th" => PpocrScript::Th,
        _ => return None,
    })
}

fn parse_cli() -> Result<Cli, String> {
    let mut input: Option<PathBuf> = None;
    let mut out_dir = PathBuf::from("viz-out");
    let mut model_dir = expand_tilde("~/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5");
    let mut det: Option<PathBuf> = None;
    let mut ink: Option<PathBuf> = None;
    let mut docaligner =
        expand_tilde("~/AndroidStudioProjects/bucket/support/1/docaligner_lcnet050.onnx");
    let mut script = PpocrScript::Latin;
    let mut font = PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
    let mut stages = Stage::ALL.to_vec();
    let mut rotate = Rotate::None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let mut next = |name: &str| {
            args.next()
                .ok_or_else(|| format!("missing value for {name}"))
        };
        match arg.as_str() {
            "-h" | "--help" => return Err(USAGE.to_string()),
            "--list" => {
                let mut out = String::from("stages:\n");
                for s in Stage::ALL {
                    out.push_str(&format!("  {:<20} {}\n", s.slug(), s.description()));
                }
                return Err(out);
            }
            "--out" => out_dir = PathBuf::from(next("--out")?),
            "--model-dir" => model_dir = PathBuf::from(next("--model-dir")?),
            "--det" => det = Some(PathBuf::from(next("--det")?)),
            "--ink" => ink = Some(PathBuf::from(next("--ink")?)),
            "--docaligner" => docaligner = PathBuf::from(next("--docaligner")?),
            "--font" => font = PathBuf::from(next("--font")?),
            "--rotate" => {
                let v = next("--rotate")?;
                rotate = match v.as_str() {
                    "cw" | "90" => Rotate::Cw,
                    "ccw" | "270" => Rotate::Ccw,
                    "180" => Rotate::Half,
                    "0" | "none" => Rotate::None,
                    other => return Err(format!("unknown --rotate: {other} (cw|ccw|180)")),
                };
            }
            "--script" => {
                let slug = next("--script")?;
                script = script_from_slug(&slug)
                    .ok_or_else(|| format!("unknown script slug: {slug}"))?;
            }
            "--stages" => {
                let csv = next("--stages")?;
                let mut parsed = Vec::new();
                for slug in csv.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    parsed.push(
                        Stage::from_slug(slug).ok_or_else(|| format!("unknown stage: {slug}"))?,
                    );
                }
                if parsed.is_empty() {
                    return Err("--stages was empty".to_string());
                }
                stages = parsed;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown option: {other}"));
            }
            other => {
                if input.is_some() {
                    return Err(format!("unexpected extra argument: {other}"));
                }
                input = Some(PathBuf::from(other));
            }
        }
    }

    let input = input.ok_or_else(|| "missing <image> argument".to_string())?;
    Ok(Cli {
        input,
        out_dir,
        model_dir,
        det,
        ink,
        docaligner,
        script,
        font,
        stages,
        rotate,
    })
}

// ---------------------------------------------------------------------
// Model file discovery
// ---------------------------------------------------------------------

fn find_in_dir(dir: &Path, pred: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(&pred)
                .unwrap_or(false)
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

fn detector_path(model_dir: &Path) -> Result<PathBuf, String> {
    // Prefer fp16 over the full-precision graph; never the int8 quant.
    find_in_dir(model_dir, |n| n.contains("det") && n.ends_with("fp16.mnn"))
        .or_else(|| {
            find_in_dir(model_dir, |n| {
                n.contains("det") && n.ends_with(".mnn") && !n.contains("int8")
            })
        })
        .ok_or_else(|| {
            format!(
                "no detection model (*det*.mnn) found in {}",
                model_dir.display()
            )
        })
}

fn recognizer_spec(model_dir: &Path, script: PpocrScript) -> Result<PpocrRecognizerSpec, String> {
    let slug = script.as_slug();
    let model_path = find_in_dir(model_dir, |n| {
        n.starts_with(slug) && n.contains("rec_infer") && n.ends_with(".mnn") && !n.contains("int8")
    })
    .ok_or_else(|| {
        format!(
            "no recognizer model ({slug}*rec_infer*.mnn) in {}",
            model_dir.display()
        )
    })?;
    let keys_path = find_in_dir(model_dir, |n| {
        n.starts_with(slug) && n.ends_with("_keys.txt")
    })
    .ok_or_else(|| format!("no keys file ({slug}*_keys.txt) in {}", model_dir.display()))?;
    Ok(PpocrRecognizerSpec {
        script,
        model_path,
        keys_path,
    })
}

// ---------------------------------------------------------------------
// Drawing helpers (pure-ish; mutate the passed buffer)
// ---------------------------------------------------------------------

const COLOR_BOX: Rgba<u8> = Rgba([255, 64, 64, 255]);
const COLOR_LABEL_BG: Rgba<u8> = Rgba([0, 0, 0, 255]);
const COLOR_LABEL_FG: Rgba<u8> = Rgba([255, 255, 0, 255]);
const COLOR_REC_BG: Rgba<u8> = Rgba([0, 0, 0, 255]);
const COLOR_REC_FG: Rgba<u8> = Rgba([255, 255, 255, 255]);
// Alternating colors so adjacent per-character firing lines stay distinguishable.
const COLOR_FIRING_A: Rgba<u8> = Rgba([0, 220, 0, 255]);
const COLOR_FIRING_B: Rgba<u8> = Rgba([255, 0, 255, 255]);

fn line_thickness(img: &RgbaImage) -> i32 {
    (img.width().max(img.height()) / 500).max(1) as i32
}

fn draw_thick_line(img: &mut RgbaImage, a: (f32, f32), b: (f32, f32), color: Rgba<u8>, t: i32) {
    let half = t / 2;
    for ox in -half..=half {
        for oy in -half..=half {
            draw_line_segment_mut(
                img,
                (a.0 + ox as f32, a.1 + oy as f32),
                (b.0 + ox as f32, b.1 + oy as f32),
                color,
            );
        }
    }
}

fn draw_closed_polyline(img: &mut RgbaImage, pts: &[(f32, f32)], color: Rgba<u8>, t: i32) {
    if pts.len() < 2 {
        return;
    }
    for i in 0..pts.len() {
        draw_thick_line(img, pts[i], pts[(i + 1) % pts.len()], color, t);
    }
}

/// A distinct, well-spread color per box index via golden-ratio hue stepping.
fn box_color(i: usize) -> Rgba<u8> {
    let hue = (i as f32 * 0.618_034).fract();
    let (h6, s, v) = (hue * 6.0, 0.85f32, 1.0f32);
    let c = v * s;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h6 as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    Rgba([
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
        255,
    ])
}

/// Tight AABB (left, top, right, bottom) of the raw contour points. `b.rect`
/// from detection is the *expanded* box (DET_BOX_BORDER padding), so for an
/// honest box overlay we bound the contour directly.
fn contour_aabb(contour: &[f32]) -> Option<(f32, f32, f32, f32)> {
    let mut it = contour.chunks_exact(2);
    let first = it.next()?;
    let (mut l, mut t, mut r, mut bo) = (first[0], first[1], first[0], first[1]);
    for c in contour.chunks_exact(2) {
        l = l.min(c[0]);
        t = t.min(c[1]);
        r = r.max(c[0]);
        bo = bo.max(c[1]);
    }
    Some((l, t, r, bo))
}

fn contour_points(contour: &[f32]) -> Vec<(f32, f32)> {
    contour.chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

/// Dump the strip's per-pixel source-coordinate map so a Python compositor can
/// splat a strip-space erase back onto the full image. Layout: u32 LE width,
/// u32 LE height, then width*height*2 f32 LE (src_x, src_y), row-major.
fn write_coordmap(path: &Path, width: u32, height: u32, map: &[(f32, f32)]) -> Result<(), String> {
    let mut buf = Vec::with_capacity(8 + map.len() * 8);
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    for &(sx, sy) in map {
        buf.extend_from_slice(&sx.to_le_bytes());
        buf.extend_from_slice(&sy.to_le_bytes());
    }
    fs::write(path, &buf).map_err(|e| format!("write coordmap {path:?}: {e}"))
}

fn draw_label(img: &mut RgbaImage, font: &FontRef, x: i32, y: i32, text: &str, px: f32) {
    if text.is_empty() {
        return;
    }
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let width: f32 = text
        .chars()
        .map(|c| scaled.h_advance(font.glyph_id(c)))
        .sum();
    let pad = 3;
    let bg_w = width.ceil() as i32 + pad * 2;
    let bg_h = px.ceil() as i32 + pad * 2;
    let bx = x.max(0);
    let by = (y - bg_h).max(0);
    draw_filled_rect_mut(
        img,
        IpRect::at(bx, by).of_size(
            bg_w.min(img.width() as i32 - bx).max(1) as u32,
            bg_h.min(img.height() as i32 - by).max(1) as u32,
        ),
        COLOR_LABEL_BG,
    );
    draw_text_mut(img, COLOR_LABEL_FG, bx + pad, by + pad, scale, font, text);
}

/// Fill the oriented box (a rotated rectangle) with `color`.
fn fill_oriented_box(img: &mut RgbaImage, o: &OrientedRect, color: Rgba<u8>) {
    if o.width < 1.0 || o.height < 1.0 {
        return;
    }
    let pts: Vec<Point<i32>> = o
        .corners()
        .iter()
        .map(|(x, y)| Point::new(x.round() as i32, y.round() as i32))
        .collect();
    // draw_polygon_mut wants an open polygon (last != first) and panics on a
    // degenerate one where the first and last points coincide.
    if pts.first() == pts.last() {
        return;
    }
    draw_polygon_mut(img, &pts, color);
}

/// Draw `text` white, rotated to the box's reading direction and centred on it.
/// Glyphs are sized to the box height and shrunk to fit its width. The text is
/// rendered onto a transparent buffer, rotated, then alpha-composited so the
/// caller can fill all backgrounds first without clobbering it.
fn draw_text_in_oriented_box(img: &mut RgbaImage, font: &FontRef, o: &OrientedRect, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let bw = o.width.max(1.0);
    let bh = o.height.max(1.0);
    let measure = |px: f32| -> f32 {
        let scaled = font.as_scaled(PxScale::from(px));
        text.chars()
            .map(|c| scaled.h_advance(font.glyph_id(c)))
            .sum()
    };
    let mut px = (bh * 0.78).max(6.0);
    let w_avail = bw * 0.97;
    let width = measure(px);
    if width > w_avail {
        px = (px * w_avail / width).max(6.0);
    }
    let text_w = measure(px).ceil() as i32 + 4;
    let text_h = (px * 1.3).ceil() as i32;
    // Square buffer big enough that rotation about the centre never clips.
    let side = (((text_w * text_w + text_h * text_h) as f32).sqrt().ceil() as i32).max(1) as u32;
    let mut buf = RgbaImage::from_pixel(side, side, Rgba([0, 0, 0, 0]));
    let tx = (side as i32 - text_w) / 2 + 2;
    let ty = (side as i32 - text_h) / 2;
    draw_text_mut(
        &mut buf,
        COLOR_REC_FG,
        tx,
        ty,
        PxScale::from(px),
        font,
        text,
    );
    let rotated = rotate_about_center(
        &buf,
        o.angle_radians,
        Interpolation::Bilinear,
        Rgba([0, 0, 0, 0]),
    );
    blend_centered(img, &rotated, o.cx, o.cy);
}

/// Alpha-composite `src` onto `dst` so `src`'s centre lands on `(cx, cy)`.
fn blend_centered(dst: &mut RgbaImage, src: &RgbaImage, cx: f32, cy: f32) {
    let ox = (cx - src.width() as f32 / 2.0).round() as i32;
    let oy = (cy - src.height() as f32 / 2.0).round() as i32;
    for (x, y, p) in src.enumerate_pixels() {
        let a = p.0[3] as f32 / 255.0;
        if a <= 0.0 {
            continue;
        }
        let dx = ox + x as i32;
        let dy = oy + y as i32;
        if dx < 0 || dy < 0 || dx >= dst.width() as i32 || dy >= dst.height() as i32 {
            continue;
        }
        let d = dst.get_pixel_mut(dx as u32, dy as u32);
        for ch in 0..3 {
            d.0[ch] = (d.0[ch] as f32 * (1.0 - a) + p.0[ch] as f32 * a).round() as u8;
        }
    }
}

/// Colorize a probability value `0..1` along a blue->cyan->green->yellow ramp.
fn heat_color(v: f32) -> [u8; 3] {
    let v = v.clamp(0.0, 1.0);
    let stops = [
        (0.0f32, [0.0, 0.0, 0.5]),
        (0.33, [0.0, 0.7, 1.0]),
        (0.66, [0.1, 0.9, 0.4]),
        (1.0, [1.0, 1.0, 0.0]),
    ];
    for w in stops.windows(2) {
        let (v0, c0) = w[0];
        let (v1, c1) = w[1];
        if v <= v1 {
            let f = if v1 > v0 { (v - v0) / (v1 - v0) } else { 0.0 };
            return [
                ((c0[0] + (c1[0] - c0[0]) * f) * 255.0) as u8,
                ((c0[1] + (c1[1] - c0[1]) * f) * 255.0) as u8,
                ((c0[2] + (c1[2] - c0[2]) * f) * 255.0) as u8,
            ];
        }
    }
    [255, 255, 0]
}

/// Discrete threshold bands: each pixel is colored by the highest threshold
/// its probability clears, so the area a given DET_SCORE_THRESHOLD would
/// binarize to is directly visible as everything at-or-above that band's
/// color. The outermost (lowest) band approximates "threshold ~ 0".
const HEAT_BANDS: [(f32, [u8; 3]); 6] = [
    (0.05, [40, 40, 160]),
    (0.10, [0, 170, 220]),
    (0.20, [0, 180, 60]),
    (0.30, [240, 220, 0]),
    (0.50, [250, 140, 0]),
    (0.70, [230, 30, 30]),
];

fn overlay_heatmap_bands(base: &RgbaImage, heat: &GrayImage, font: &FontRef) -> RgbaImage {
    let mut out = base.clone();
    for (x, y, px) in out.enumerate_pixels_mut() {
        let v = heat.get_pixel(x, y).0[0] as f32 / 255.0;
        let Some((_, c)) = HEAT_BANDS.iter().rev().find(|(t, _)| v >= *t) else {
            continue;
        };
        for ch in 0..3 {
            px.0[ch] = (px.0[ch] as f32 * 0.15 + c[ch] as f32 * 0.85).round() as u8;
        }
    }
    let scale = PxScale::from(16.0);
    for (i, (t, c)) in HEAT_BANDS.iter().enumerate() {
        let y = 4 + i as i32 * 20;
        draw_filled_rect_mut(
            &mut out,
            IpRect::at(4, y).of_size(16, 16),
            Rgba([c[0], c[1], c[2], 255]),
        );
        draw_text_mut(
            &mut out,
            Rgba([255, 255, 255, 255]),
            24,
            y,
            scale,
            font,
            &format!(">= {t:.2}"),
        );
    }
    out
}

/// Alpha-blend the colorized heatmap over the base image; low-probability areas
/// stay close to the original so text regions glow.
fn overlay_heatmap(base: &RgbaImage, heat: &GrayImage) -> RgbaImage {
    let mut out = base.clone();
    for (x, y, px) in out.enumerate_pixels_mut() {
        let v = heat.get_pixel(x, y).0[0] as f32 / 255.0;
        let a = v * 0.75;
        let c = heat_color(v);
        for ch in 0..3 {
            px.0[ch] = (px.0[ch] as f32 * (1.0 - a) + c[ch] as f32 * a).round() as u8;
        }
    }
    out
}

// ---------------------------------------------------------------------
// Per-box strip crops (functional core)
// ---------------------------------------------------------------------

fn bbox_crop(image: &DynamicImage, b: &DetectedTextBox) -> DynamicImage {
    // Crop the tight contour AABB, not the DET_BOX_BORDER-padded `b.rect`.
    let (l, t, r, bo) = contour_aabb(&b.contour).unwrap_or((
        b.rect.left as f32,
        b.rect.top as f32,
        b.rect.right as f32,
        b.rect.bottom as f32,
    ));
    let left = l.max(0.0).floor() as u32;
    let top = t.max(0.0).floor() as u32;
    let right = (r.ceil() as u32).min(image.width());
    let bottom = (bo.ceil() as u32).min(image.height());
    image.crop_imm(
        left,
        top,
        right.saturating_sub(left).max(1),
        bottom.saturating_sub(top).max(1),
    )
}

fn squashed_to_rec_height(crop: &DynamicImage) -> DynamicImage {
    let (w, h) = (crop.width().max(1), crop.height().max(1));
    let target_w = ((w as f32 * REC_TARGET_HEIGHT as f32) / h as f32)
        .ceil()
        .max(1.0) as u32;
    crop.resize_exact(
        target_w,
        REC_TARGET_HEIGHT,
        image::imageops::FilterType::Triangle,
    )
}

// ---------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------

fn main() {
    let mut log_builder = env_logger::Builder::from_default_env();
    if std::env::var_os("RUST_LOG").is_none() {
        log_builder.filter_level(log::LevelFilter::Info);
    }
    log_builder.init();
    let cli = match parse_cli() {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(if msg.starts_with("usage") || msg.starts_with("stages") {
                0
            } else {
                2
            });
        }
    };
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), String> {
    let image = image::open(&cli.input)
        .map_err(|e| format!("failed to open {}: {e}", cli.input.display()))?;
    let image = cli.rotate.apply(image);
    let rgba = image.to_rgba8();
    let gray = image.to_luma8();
    println!(
        "loaded {} ({}x{})",
        cli.input.display(),
        image.width(),
        image.height()
    );

    fs::create_dir_all(&cli.out_dir)
        .map_err(|e| format!("create {}: {e}", cli.out_dir.display()))?;

    let font_bytes =
        fs::read(&cli.font).map_err(|e| format!("read font {}: {e}", cli.font.display()))?;
    let font = FontRef::try_from_slice(&font_bytes)
        .map_err(|e| format!("parse font {}: {e}", cli.font.display()))?;

    // Load PP-OCR for any stage that detects/recognizes — `dewarp` included,
    // since it re-runs recognition on the corrected page for dewarp-rec.png.
    let needs_engine = cli
        .stages
        .iter()
        .any(|s| !matches!(s, Stage::Input | Stage::Corners));
    let engine = if needs_engine {
        let det = match &cli.det {
            Some(path) => path.clone(),
            None => detector_path(&cli.model_dir)?,
        };
        let spec = recognizer_spec(&cli.model_dir, cli.script)?;
        println!(
            "loading ppocr: det={} rec={}",
            det.display(),
            spec.model_path.display()
        );
        // Optional ink model: --ink, else ink.mnn next to det.
        let ink_path = cli.ink.clone().or_else(|| {
            det.parent()
                .map(|d| d.join("ink.mnn"))
                .filter(|p| p.exists())
        });
        if let Some(p) = &ink_path {
            println!("ink model: {}", p.display());
        }
        Some(
            PpocrEngine::load(&det, None, None, vec![spec], 4, ink_path.as_deref())
                .map_err(|e| format!("load ppocr: {e:?}"))?,
        )
    } else {
        None
    };

    // Shared detection (boxes), computed once if any box-consuming stage runs.
    let needs_boxes = cli.stages.iter().any(|s| {
        matches!(
            s,
            Stage::Boxes
                | Stage::OrientedBoxes
                | Stage::Contours
                | Stage::BoxHeights
                | Stage::XHeight
                | Stage::RecognizeDeskew
                | Stage::RecognizeNoDeskew
                | Stage::BboxStrips
                | Stage::Squashed
                | Stage::Deskewed
                | Stage::CharFirings
                | Stage::Rewrite
                | Stage::Ink
        )
    });
    let mut boxes: Vec<DetectedTextBox> = Vec::new();
    if needs_boxes {
        let engine = engine.as_ref().expect("engine loaded for box stages");
        boxes = engine
            .detect_only_image(&image, PpocrProfile::Still)
            .map_err(|e| format!("detect: {e:?}"))?;
        // Stable, human-friendly ordering top-to-bottom then left-to-right.
        boxes.sort_by(|a, b| (a.rect.top, a.rect.left).cmp(&(b.rect.top, b.rect.left)));
        println!("detected {} boxes", boxes.len());
    }

    let mut index = serde_json::Map::new();
    index.insert(
        "input".into(),
        serde_json::json!(cli.input.display().to_string()),
    );
    index.insert("width".into(), serde_json::json!(image.width()));
    index.insert("height".into(), serde_json::json!(image.height()));
    index.insert("script".into(), serde_json::json!(cli.script.as_slug()));
    index.insert("box_count".into(), serde_json::json!(boxes.len()));

    let t = line_thickness(&rgba);

    for stage in &cli.stages {
        let stage = *stage;
        match stage {
            Stage::Input => {
                save_png(&rgba, &cli.out_dir.join("input.png"))?;
            }
            Stage::Boxes => {
                let mut canvas = rgba.clone();
                for (i, b) in boxes.iter().enumerate() {
                    let (l, top, r, bot) = contour_aabb(&b.contour).unwrap_or((
                        b.rect.left as f32,
                        b.rect.top as f32,
                        b.rect.right as f32,
                        b.rect.bottom as f32,
                    ));
                    let corners = [(l, top), (r, top), (r, bot), (l, bot)];
                    draw_closed_polyline(&mut canvas, &corners, box_color(i), t);
                }
                save_png(&canvas, &cli.out_dir.join("boxes.png"))?;
            }
            Stage::OrientedBoxes => {
                let mut canvas = rgba.clone();
                for (i, b) in boxes.iter().enumerate() {
                    draw_closed_polyline(&mut canvas, &b.tight_box.corners(), box_color(i), t);
                }
                save_png(&canvas, &cli.out_dir.join("oriented_boxes.png"))?;
            }
            Stage::Contours => {
                let mut canvas = rgba.clone();
                let scale = PxScale::from(20.0);
                for (i, b) in boxes.iter().enumerate() {
                    let pts = contour_points(&b.contour);
                    if pts.len() < 2 {
                        continue;
                    }
                    draw_closed_polyline(&mut canvas, &pts, box_color(i), t);
                    let angle = translator::ppocr::contour_principal_axis_angle(&pts)
                        .map(|a| a.to_degrees())
                        .unwrap_or(0.0);
                    let lx = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                    let ly = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                    draw_text_mut(
                        &mut canvas,
                        Rgba([255, 80, 0, 255]),
                        lx as i32,
                        (ly as i32 - 22).max(0),
                        scale,
                        &font,
                        &format!("{angle:.1}deg"),
                    );
                }
                save_png(&canvas, &cli.out_dir.join("contours.png"))?;
            }
            Stage::BoxHeights => {
                let mut canvas = rgba.clone();
                let scale = PxScale::from(22.0);
                for b in boxes.iter() {
                    draw_closed_polyline(
                        &mut canvas,
                        &b.oriented_box.corners(),
                        Rgba([230, 30, 200, 255]),
                        t,
                    );
                    draw_closed_polyline(
                        &mut canvas,
                        &b.tight_box.corners(),
                        Rgba([30, 220, 30, 255]),
                        t,
                    );
                    let tight_h = b.tight_box.height.max(1.0);
                    let label = format!(
                        "{:.0}->{:.0} {:.1}x {:.1}deg",
                        tight_h,
                        b.oriented_box.height,
                        b.oriented_box.height / tight_h,
                        b.tight_box.angle_radians.to_degrees(),
                    );
                    let corners = b.tight_box.corners();
                    let lx = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
                    let ly = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
                    draw_text_mut(
                        &mut canvas,
                        Rgba([255, 80, 0, 255]),
                        lx as i32,
                        (ly as i32 - 24).max(0),
                        scale,
                        &font,
                        &label,
                    );
                }
                save_png(&canvas, &cli.out_dir.join("box_heights.png"))?;
                let engine = engine.as_ref().expect("engine for box heights");
                let lines = recognize(engine, &image, &gray, &boxes, cli.script, true)
                    .map_err(|e| format!("recognize: {e:?}"))?;
                let mut csv = String::from("cx,cy,width,tight_h,inflated_h,ratio,angle_deg,text\n");
                for (b, line) in boxes.iter().zip(lines.iter()) {
                    let tight_h = b.tight_box.height.max(1.0);
                    csv.push_str(&format!(
                        "{:.0},{:.0},{:.0},{:.1},{:.1},{:.2},{:.2},{}\n",
                        b.tight_box.cx,
                        b.tight_box.cy,
                        b.tight_box.width,
                        tight_h,
                        b.oriented_box.height,
                        b.oriented_box.height / tight_h,
                        b.tight_box.angle_radians.to_degrees(),
                        line.text.replace(',', " "),
                    ));
                }
                fs::write(cli.out_dir.join("box_heights.csv"), csv)
                    .map_err(|e| format!("box_heights.csv: {e}"))?;
            }
            Stage::XHeight => {
                let engine = engine.as_ref().expect("engine for x-height");
                if !engine.has_ink() {
                    println!(
                        "  no ink model (pass --ink or put ink.mnn in the model dir); skipping"
                    );
                } else {
                    let strips = engine.ink_strips(
                        &image,
                        &boxes,
                        None,
                        translator::ppocr::InkChannelOrder::Rgb,
                    );
                    let lines = recognize(engine, &image, &gray, &boxes, cli.script, true)
                        .map_err(|e| format!("recognize: {e:?}"))?;
                    let mut canvas = rgba.clone();
                    let scale = PxScale::from(20.0);
                    let mut csv = String::from(
                        "cx,cy,width,tight_h,x_height,centerline,tilt_deg,stroke,weight,model_bold_p,bold,text\n",
                    );
                    let metrics: Vec<Option<translator::text_metrics::LineMetrics>> = boxes
                        .iter()
                        .zip(strips.iter())
                        .map(|(b, strip)| {
                            let s = strip.as_ref()?;
                            translator::text_metrics::measure_line(
                                &s.matte,
                                b.oriented_box.width,
                                b.oriented_box.height,
                            )
                        })
                        .collect();
                    // Per-box pooled bold prob from the ink model's bold channel (when present).
                    let model_bold_p: Vec<Option<f32>> = strips
                        .iter()
                        .map(|s| s.as_ref().and_then(|s| s.pooled_bold()))
                        .collect();
                    // Thin lines so adjacent lines' boxes stay distinguishable.
                    let thin = 1;
                    for (i, (b, line)) in boxes.iter().zip(lines.iter()).enumerate() {
                        // Inflated oriented box (magenta) — the region the ink model
                        // actually searches — and the tight detection core (green).
                        draw_closed_polyline(
                            &mut canvas,
                            &b.oriented_box.corners(),
                            Rgba([230, 30, 200, 255]),
                            thin,
                        );
                        draw_closed_polyline(
                            &mut canvas,
                            &b.tight_box.corners(),
                            Rgba([30, 220, 30, 255]),
                            thin,
                        );
                        let Some(m) = metrics[i] else { continue };
                        // Model bold (pooled ch1 ≥ 0.65) where the bold channel exists; else
                        // the geometric fallback — matching the runtime decision.
                        let mp = model_bold_p[i];
                        let is_bold = mp.map(|p| p >= 0.65).unwrap_or(false);
                        // Matte band in cyan: box re-fit to actual ink on both axes, at the
                        // detection reading angle — the same angle the still pipeline renders.
                        let angle = b.tight_box.angle_radians;
                        let band = m.refit(b.tight_box, angle);
                        draw_closed_polyline(
                            &mut canvas,
                            &band.corners(),
                            Rgba([0, 200, 255, 255]),
                            thin,
                        );

                        let corners = b.tight_box.corners();
                        let lx = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
                        let ly = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
                        draw_text_mut(
                            &mut canvas,
                            Rgba([0, 160, 255, 255]),
                            lx as i32,
                            (ly as i32 - 22).max(0),
                            scale,
                            &font,
                            &format!(
                                "xh{:.0} w{:.2}{}{}",
                                m.x_height,
                                m.weight_ratio(),
                                mp.map(|p| format!(" m{p:.2}")).unwrap_or_default(),
                                if is_bold { " BOLD" } else { "" },
                            ),
                        );
                        csv.push_str(&format!(
                            "{:.0},{:.0},{:.0},{:.1},{:.1},{:.1},{:.2},{:.2},{:.2},{},{},{}\n",
                            b.tight_box.cx,
                            b.tight_box.cy,
                            b.tight_box.width,
                            b.tight_box.height,
                            m.x_height,
                            m.centerline_offset,
                            m.baseline_angle_delta.to_degrees(),
                            m.stroke_width,
                            m.weight_ratio(),
                            mp.map(|p| format!("{p:.3}")).unwrap_or_default(),
                            is_bold,
                            line.text.replace(',', " "),
                        ));
                    }
                    save_png(&canvas, &cli.out_dir.join("x_height.png"))?;
                    fs::write(cli.out_dir.join("x_height.csv"), csv)
                        .map_err(|e| format!("x_height.csv: {e}"))?;
                }
            }
            Stage::Heatmap => {
                let engine = engine.as_ref().expect("engine for heatmap");
                let heat = engine
                    .detect_heatmap(&image)
                    .map_err(|e| format!("heatmap: {e:?}"))?;
                let out = overlay_heatmap(&rgba, &heat);
                save_png(&out, &cli.out_dir.join("heatmap.png"))?;
                save_gray_png(&heat, &cli.out_dir.join("heatmap-raw.png"))?;
                let bands = overlay_heatmap_bands(&rgba, &heat, &font);
                save_png(&bands, &cli.out_dir.join("heatmap-bands.png"))?;
            }
            Stage::RecognizeDeskew | Stage::RecognizeNoDeskew => {
                let deskew = stage == Stage::RecognizeDeskew;
                let engine = engine.as_ref().expect("engine for recognition");
                let lines = recognize(engine, &image, &gray, &boxes, cli.script, deskew)
                    .map_err(|e| format!("recognize: {e:?}"))?;
                let canvas = draw_recognition(&rgba, &boxes, &lines, &font);
                save_png(&canvas, &cli.out_dir.join(format!("{}.png", stage.slug())))?;
                let txt: String = lines
                    .iter()
                    .filter(|l| !l.text.trim().is_empty())
                    .map(|l| format!("[{:.2}] {}\n", l.confidence, l.text.trim()))
                    .collect();
                fs::write(cli.out_dir.join(format!("{}.txt", stage.slug())), &txt)
                    .map_err(|e| format!("write recognition text: {e}"))?;
                let arr: Vec<serde_json::Value> = lines
                    .iter()
                    .map(|l| serde_json::json!({ "text": l.text, "confidence": l.confidence }))
                    .collect();
                index.insert(stage.slug().into(), serde_json::json!(arr));
            }
            Stage::BboxStrips => {
                let dir = cli.out_dir.join("bbox-strips");
                fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
                for (i, b) in boxes.iter().enumerate() {
                    let crop = bbox_crop(&image, b);
                    save_dynamic_png(&crop, &dir.join(format!("box-{i:03}.png")))?;
                }
            }
            Stage::Squashed => {
                let dir = cli.out_dir.join("squashed");
                fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
                for (i, b) in boxes.iter().enumerate() {
                    let crop = squashed_to_rec_height(&bbox_crop(&image, b));
                    save_dynamic_png(&crop, &dir.join(format!("box-{i:03}.png")))?;
                }
            }
            Stage::Deskewed => {
                let dir = cli.out_dir.join("deskewed");
                fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
                let rgb = image.to_rgb8();
                let mut dewarped = 0usize;
                for (i, b) in boxes.iter().enumerate() {
                    let contour = contour_points(&b.contour);
                    match dewarp_contour_to_strip_rgb_with_map(&rgb, &contour, None, 0.0) {
                        Some((strip, map)) => {
                            strip
                                .save(dir.join(format!("box-{i:03}.png")))
                                .map_err(|e| format!("save deskewed strip: {e}"))?;
                            write_coordmap(
                                &dir.join(format!("box-{i:03}.map")),
                                strip.width(),
                                strip.height(),
                                &map,
                            )?;
                            dewarped += 1;
                        }
                        // No contour or too-small span: the pipeline would fall
                        // back to the axis-aligned crop here, so mirror that.
                        None => {
                            let crop = squashed_to_rec_height(&bbox_crop(&image, b));
                            save_dynamic_png(&crop, &dir.join(format!("box-{i:03}-fallback.png")))?;
                        }
                    }
                }
                index.insert("deskewed_count".into(), serde_json::json!(dewarped));
            }
            Stage::Ink => {
                let engine = engine.as_ref().expect("engine for ink");
                let dir = cli.out_dir.join("ink");
                fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
                if !engine.has_ink() {
                    println!(
                        "  no ink model (pass --ink or put ink.mnn in the model dir); skipping"
                    );
                } else {
                    let ink_strips = engine.ink_strips(
                        &image,
                        &boxes,
                        None,
                        translator::ppocr::InkChannelOrder::Rgb,
                    );
                    let masks: Vec<_> = ink_strips
                        .iter()
                        .map(|s| s.as_ref().map(|s| s.matte.clone()))
                        .collect();
                    let src_maps: Vec<_> = ink_strips
                        .iter()
                        .map(|s| s.as_ref().and_then(|s| s.src_map.clone()))
                        .collect();
                    let mut n = 0usize;
                    for (i, m) in masks.iter().enumerate() {
                        let Some(mask) = m else { continue };
                        let big = image::imageops::resize(
                            mask,
                            mask.width() * 3,
                            mask.height() * 3,
                            image::imageops::FilterType::Nearest,
                        );
                        big.save(dir.join(format!("box-{i:03}.png")))
                            .map_err(|e| format!("save ink mask: {e}"))?;
                        n += 1;
                    }
                    println!("  wrote {n}/{} ink masks", masks.len());
                    index.insert("ink_count".into(), serde_json::json!(n));

                    // Full-page overlays, each from the real prod constants/functions:
                    //   raw   = the model's matte (every texel ≥ INK_CUT), no scatter loss
                    //   core  = the FG_INK_FRACTION luma-extreme the *fallback* colour picker
                    //           samples (colour-head models don't use it for colour; it still
                    //           feeds bold pooling)
                    //   erase = union_ink_mask (prod) + the per-box fill dilation the erase applies
                    //   color = the matte repainted in each box's *resolved* overlay colour on a
                    //           neutral canvas — shows whether the picked ink colour is right,
                    //           which the red mask overlays can't
                    let rgba = image.to_rgba8();
                    let (iw, ih) = (rgba.width(), rgba.height());
                    let model_colors: Vec<Option<translator::ocr::InkColor>> = ink_strips
                        .iter()
                        .map(|s| s.as_ref().and_then(|s| s.pooled_color()))
                        .collect();
                    let strips = translator::color_matting::mat_detections(
                        &rgba,
                        &boxes,
                        &masks,
                        &src_maps,
                        &model_colors,
                    );
                    // The colour each box's overlay text will actually use (model colour when
                    // the strip has the colour head, sampled fallback otherwise, WCAG floor
                    // applied either way — mat_detections resolves all of that).
                    let mut box_fg: Vec<Option<u32>> = vec![None; boxes.len()];
                    for s in &strips {
                        box_fg[s.box_index] = Some(s.fg_argb);
                    }
                    let src_luma = |x: u32, y: u32| {
                        let p = rgba.get_pixel(x, y).0;
                        (p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000
                    };
                    let cut = translator::text_metrics::INK_CUT as u8;
                    let mut raw = vec![0u8; (iw * ih) as usize]; // image-space matte value
                    let mut core = vec![false; (iw * ih) as usize];
                    // Neutral mid-gray canvas so both dark and light inks stay visible.
                    let mut colorimg =
                        image::RgbaImage::from_pixel(iw, ih, image::Rgba([200, 200, 200, 255]));

                    for (bi, (mask, sm)) in masks.iter().zip(src_maps.iter()).enumerate() {
                        let (Some(mask), Some(sm)) = (mask, sm) else {
                            continue;
                        };
                        let fg_rgb = box_fg[bi].map(|c| c.to_be_bytes());
                        let (mw, mh) = mask.dimensions();
                        // dense sub-texel walk: bilinear src position so the 48px strip maps onto
                        // the page with no scatter holes (this is for *visualising the model's matte*;
                        // the erase below uses the real prod scatter instead).
                        let src_at = |fx: f32, fy: f32| -> (f32, f32) {
                            let x0 = (fx.floor() as u32).min(mw - 1);
                            let y0 = (fy.floor() as u32).min(mh - 1);
                            let x1 = (x0 + 1).min(mw - 1);
                            let y1 = (y0 + 1).min(mh - 1);
                            let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
                            let g = |x: u32, y: u32| sm[(y * mw + x) as usize];
                            let (a, b, c, d) = (g(x0, y0), g(x1, y0), g(x0, y1), g(x1, y1));
                            let l = |p: f32, q: f32, t: f32| p + (q - p) * t;
                            (
                                l(l(a.0, b.0, tx), l(c.0, d.0, tx), ty),
                                l(l(a.1, b.1, tx), l(c.1, d.1, tx), ty),
                            )
                        };
                        let span = |f: fn((f32, f32)) -> f32| {
                            sm.iter().map(|&p| f(p)).fold(f32::MIN, f32::max)
                                - sm.iter().map(|&p| f(p)).fold(f32::MAX, f32::min)
                        };
                        let step = ((span(|p| p.0) / mw as f32)
                            .max(span(|p| p.1) / mh as f32)
                            .ceil() as u32)
                            .clamp(1, 12);
                        // collect this box's ink pixels (idx, luma) for the colour-core selection
                        let mut ink_px: Vec<(usize, u8)> = Vec::new();
                        for j in 0..mh * step {
                            for i in 0..mw * step {
                                let (fx, fy) = (i as f32 / step as f32, j as f32 / step as f32);
                                let a = mask.get_pixel(fx as u32, fy as u32)[0];
                                let (sx, sy) = src_at(fx, fy);
                                if sx < 0.0 || sy < 0.0 || sx >= iw as f32 || sy >= ih as f32 {
                                    continue;
                                }
                                let idx = (sy as u32 * iw + sx as u32) as usize;
                                if a > raw[idx] {
                                    raw[idx] = a;
                                    if let Some([_, r, g, b]) = fg_rgb {
                                        let t = a as f32 / 255.0;
                                        let blend =
                                            |fgc: u8| (fgc as f32 * t + 200.0 * (1.0 - t)) as u8;
                                        colorimg.put_pixel(
                                            idx as u32 % iw,
                                            idx as u32 / iw,
                                            image::Rgba([blend(r), blend(g), blend(b), 255]),
                                        );
                                    }
                                }
                                if a >= cut {
                                    ink_px.push((idx, src_luma(sx as u32, sy as u32) as u8));
                                }
                            }
                        }
                        if ink_px.is_empty() {
                            continue;
                        }
                        // colour core: prod takes FG_INK_FRACTION on the ink side farther from bg.
                        ink_px.sort_by_key(|&(_, l)| l);
                        let med_ink = ink_px[ink_px.len() / 2].1;
                        let k = ((ink_px.len() as f32 * translator::color_matting::FG_INK_FRACTION)
                            .ceil() as usize)
                            .clamp(1, ink_px.len());
                        // box background luma (median over the box's non-ink-ish light pixels)
                        let dark_side = (med_ink as u32) < 128;
                        let chosen = if dark_side {
                            &ink_px[..k]
                        } else {
                            &ink_px[ink_px.len() - k..]
                        };
                        for &(idx, _) in chosen {
                            core[idx] = true;
                        }
                    }

                    // erase mask: the exact prod still-path mask (union_ink_mask + fill dilation)
                    let union =
                        translator::color_matting::union_ink_mask(&rgba, &boxes, &masks, &src_maps);
                    let mut erase = vec![false; (iw * ih) as usize];
                    for b in &boxes {
                        let fill_radius =
                            translator::color_matting::fill_radius(b.oriented_box.height);
                        let r = b.oriented_box.to_aabb();
                        let (x0, y0) = (r.left.min(iw), r.top.min(ih));
                        let (x1, y1) = (r.right.min(iw), r.bottom.min(ih));
                        let (aw, ah) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
                        if aw == 0 || ah == 0 {
                            continue;
                        }
                        let mut sub = vec![false; (aw * ah) as usize];
                        for ly in 0..ah {
                            for lx in 0..aw {
                                sub[(ly * aw + lx) as usize] =
                                    union[((y0 + ly) * iw + (x0 + lx)) as usize];
                            }
                        }
                        let d = translator::color_matting::dilate(&sub, aw, ah, fill_radius);
                        for ly in 0..ah {
                            for lx in 0..aw {
                                if d[(ly * aw + lx) as usize] {
                                    erase[((y0 + ly) * iw + (x0 + lx)) as usize] = true;
                                }
                            }
                        }
                    }

                    let paint = |name: &str, hit: &dyn Fn(usize) -> Option<f32>| {
                        let mut ov = rgba.clone();
                        for i in 0..(iw * ih) as usize {
                            if let Some(a) = hit(i) {
                                let p = ov.get_pixel_mut(i as u32 % iw, i as u32 / iw);
                                p[0] = (p[0] as f32 * (1.0 - a) + 255.0 * a) as u8;
                                p[1] = (p[1] as f32 * (1.0 - a)) as u8;
                                p[2] = (p[2] as f32 * (1.0 - a)) as u8;
                            }
                        }
                        let _ = ov.save(dir.join(name));
                    };
                    paint("ink-raw.png", &|i| {
                        (raw[i] > 0).then_some(raw[i] as f32 / 255.0 * 0.8)
                    });
                    paint("ink-core.png", &|i| core[i].then_some(0.85));
                    paint("ink-erase.png", &|i| erase[i].then_some(0.7));
                    let _ = colorimg.save(dir.join("ink-color.png"));
                    println!("  wrote ink-raw.png / ink-core.png / ink-erase.png / ink-color.png");
                    // Label each box's fg with its recognised text (so STRONGER etc. are findable);
                    // match recognised lines to boxes by rect-centre proximity.
                    let rec_lines = recognize(engine, &image, &gray, &boxes, cli.script, true)
                        .unwrap_or_default();
                    let text_for = |bi: usize| -> String {
                        if bi >= boxes.len() {
                            return String::new();
                        }
                        let r = &boxes[bi].rect;
                        let (bx, by) =
                            ((r.left + r.right) as i64 / 2, (r.top + r.bottom) as i64 / 2);
                        rec_lines
                            .iter()
                            .min_by_key(|l| {
                                let lx = (l.rect.left + l.rect.right) as i64 / 2;
                                let ly = (l.rect.top + l.rect.bottom) as i64 / 2;
                                (lx - bx).pow(2) + (ly - by).pow(2)
                            })
                            .map(|l| l.text.trim().chars().take(24).collect())
                            .unwrap_or_default()
                    };
                    let fgdir = cli.out_dir.join("ink-fg");
                    fs::create_dir_all(&fgdir).map_err(|e| format!("mkdir: {e}"))?;
                    for s in &strips {
                        let [_, r, g, b] = s.fg_argb.to_be_bytes();
                        let mut bgl: Vec<u32> = s
                            .strip_rgba
                            .chunks_exact(4)
                            .map(|p| {
                                (p[0] as u32 * 299 + p[1] as u32 * 587 + p[2] as u32 * 114) / 1000
                            })
                            .collect();
                        bgl.sort_unstable();
                        let bg = bgl.get(bgl.len() / 2).copied().unwrap_or(255);
                        let model = match model_colors[s.box_index] {
                            Some(c) => {
                                let [_, mr, mg, mb] = c.fg_argb.to_be_bytes();
                                format!("model=#{mr:02x}{mg:02x}{mb:02x} mbg={:3}", c.bg_luma)
                            }
                            None => "model=none (sampled fallback)".to_string(),
                        };
                        println!(
                            "  box {:03}: fg=#{r:02x}{g:02x}{b:02x} bg_luma={bg:3}  {model}  {:?}",
                            s.box_index,
                            text_for(s.box_index)
                        );
                        image::RgbImage::from_pixel(160, 48, image::Rgb([r, g, b]))
                            .save(fgdir.join(format!("fg-{:03}.png", s.box_index)))
                            .ok();
                        if let Some(strip) = image::RgbaImage::from_raw(
                            s.strip_width,
                            s.strip_height,
                            s.strip_rgba.clone(),
                        ) {
                            strip
                                .save(fgdir.join(format!("strip-{:03}.png", s.box_index)))
                                .ok();
                        }
                        // Re-render the ink shape (coverage alpha) in the PICKED fg
                        // colour over the inpainted background — i.e. what the overlay
                        // would actually draw for this line. Compare against the
                        // original crop to judge whether the picked colour is right.
                        let mut rendered = image::RgbImage::new(s.strip_width, s.strip_height);
                        for (idx, px) in s.strip_rgba.chunks_exact(4).enumerate() {
                            let a = px[3] as f32 / 255.0;
                            let blend =
                                |fg: u8, bg: u8| (fg as f32 * a + bg as f32 * (1.0 - a)) as u8;
                            let x = (idx as u32) % s.strip_width;
                            let y = (idx as u32) / s.strip_width;
                            rendered.put_pixel(
                                x,
                                y,
                                image::Rgb([blend(r, px[0]), blend(g, px[1]), blend(b, px[2])]),
                            );
                        }
                        rendered
                            .save(fgdir.join(format!("render-{:03}.png", s.box_index)))
                            .ok();
                    }
                }
            }
            Stage::CharFirings => {
                // The strip's reading axis is horizontal, so "perpendicular to the spine"
                // is a vertical line, full height. Each firing fraction maps onto the strip
                // width directly; the line marks the leading edge of that glyph's CTC run.
                let dir = cli.out_dir.join("char-firings");
                fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;
                let engine = engine.as_ref().expect("engine for char-firings");
                let rgb = image.to_rgb8();
                // 48px strips are too small to inspect; nearest-neighbour upscale keeps the
                // firing lines crisp against the pixels they sit on.
                const SCALE: u32 = 4;
                for (i, b) in boxes.iter().enumerate() {
                    let contour = contour_points(&b.contour);
                    let Some(strip) = dewarp_contour_to_strip_rgb(&rgb, &contour, None, 0.0) else {
                        continue;
                    };
                    let firings = engine
                        .recognize_strip_firings(
                            &DynamicImage::ImageRgb8(strip.clone()),
                            cli.script,
                        )
                        .map_err(|e| format!("strip firings: {e:?}"))?;
                    let big = image::imageops::resize(
                        &strip,
                        strip.width() * SCALE,
                        strip.height() * SCALE,
                        image::imageops::FilterType::Nearest,
                    );
                    let mut canvas = DynamicImage::ImageRgb8(big).to_rgba8();
                    let w = canvas.width() as f32;
                    let h = canvas.height() as f32;
                    for (k, f) in firings.iter().enumerate() {
                        let x = (f.at * w).clamp(0.0, w - 1.0);
                        let color = if k % 2 == 0 {
                            COLOR_FIRING_A
                        } else {
                            COLOR_FIRING_B
                        };
                        draw_thick_line(&mut canvas, (x, 0.0), (x, h - 1.0), color, 1);
                    }
                    save_png(&canvas, &dir.join(format!("box-{i:03}.png")))?;
                }
            }
            Stage::Rewrite => {
                let engine = engine.as_ref().expect("engine for rewrite");
                run_rewrite_stage(engine, &image, &gray, &boxes, &cli, &font)?;
            }
            Stage::Dewarp | Stage::Corners => {
                run_docaligner_stage(stage, &cli, &rgba, &font, t, engine.as_ref(), &mut index)?;
            }
        }
        println!("  wrote stage {}", stage.slug());
    }

    let index_path = cli.out_dir.join("index.json");
    fs::write(
        &index_path,
        serde_json::to_string_pretty(&serde_json::Value::Object(index)).unwrap(),
    )
    .map_err(|e| format!("write index.json: {e}"))?;
    println!("done -> {}", cli.out_dir.display());
    Ok(())
}

fn recognize(
    engine: &PpocrEngine,
    image: &DynamicImage,
    gray: &GrayImage,
    boxes: &[DetectedTextBox],
    script: PpocrScript,
    deskew: bool,
) -> Result<Vec<RecognizedTextLine>, translator::TranslatorError> {
    // The recognizer dewarps when a box carries a contour; clearing the contour
    // forces the axis-aligned crop + squash path (the no-deskew failure mode).
    let boxes: Vec<DetectedTextBox> = boxes
        .iter()
        .cloned()
        .map(|mut b| {
            if !deskew {
                b.contour.clear();
            }
            b
        })
        .collect();
    let scripts = vec![script; boxes.len()];
    engine.recognize_text_in_boxes_image(image, &boxes, &scripts, PpocrProfile::Still, None)
}

fn draw_recognition(
    base: &RgbaImage,
    boxes: &[DetectedTextBox],
    lines: &[RecognizedTextLine],
    font: &FontRef,
) -> RgbaImage {
    let mut canvas = base.clone();
    // Backgrounds first so a later box's fill can't erase an earlier box's text.
    for b in boxes {
        fill_oriented_box(&mut canvas, &b.oriented_box, COLOR_REC_BG);
    }
    for (b, line) in boxes.iter().zip(lines.iter()) {
        let label = if line.text.trim().is_empty() {
            "∅"
        } else {
            line.text.trim()
        };
        draw_text_in_oriented_box(&mut canvas, font, &b.oriented_box, label);
    }
    canvas
}

/// Hands the renderer one font for every request — the `--font` file. The real
/// apps return a script-aware chain; the viz just needs a face covering the
/// recognized script (override with `--font` for non-Latin scripts).
struct VizFonts {
    path: PathBuf,
}

impl FontProvider for VizFonts {
    fn locate(&self, _request: &FontRequest) -> Vec<FontHandle> {
        vec![FontHandle::from(self.path.clone())]
    }
}

/// Run the real still erase+render path and overlay the per-word boxes the app
/// receives. Each recognized line becomes one PerLine block whose translated
/// text is the recognized text itself, so `render_overlay` lays glyphs where
/// the app would draw the translation and its `translated_words` are the exact
/// boxes drag-to-copy/tap hit-tests against (red). The CTC `source_words`,
/// measured straight from recognizer firings, are drawn alongside as the
/// accurate reference (green) so a mis-sized render-layout box is obvious.
fn run_rewrite_stage(
    engine: &PpocrEngine,
    image: &DynamicImage,
    gray: &GrayImage,
    boxes: &[DetectedTextBox],
    cli: &Cli,
    font: &FontRef,
) -> Result<(), String> {
    let lines = recognize(engine, image, gray, boxes, cli.script, true)
        .map_err(|e| format!("recognize: {e:?}"))?;
    let is_cjk = cli.script == PpocrScript::Cj;

    let source_words: Vec<PositionedWord> = lines
        .iter()
        .enumerate()
        .flat_map(|(i, line)| {
            let firings: Vec<(char, f32)> = line
                .firings
                .iter()
                .map(|f| (char::from_u32(f.ch).unwrap_or('\u{fffd}'), f.at))
                .collect();
            translator::text_metrics::firing_word_boxes(
                &line.text,
                &firings,
                is_cjk,
                &line.oriented_box,
                i as u32,
            )
        })
        .collect();

    // Per-box ink strips drive both the erase (union mask) and per-word bold, exactly as the
    // still overlay path does — bold matters because bold runs render wider, and that's where the
    // per-word box geometry is most easily wrong.
    let strips = engine
        .has_ink()
        .then(|| engine.ink_strips(image, boxes, None, translator::ppocr::InkChannelOrder::Rgb));

    // One PerLine block per recognized line; the recognized text is fed back as the "translated"
    // text so we exercise the render path without translating. Per-word bold comes from the ink
    // bold channel + CTC firings, with a whole-line fallback (mirrors the still pipeline).
    let mut blocks: Vec<TextBlock> = Vec::new();
    let mut translated: Vec<String> = Vec::new();
    let mut style_ranges: Vec<Vec<translator::ocr::StyleRange>> = Vec::new();
    let src_rgba = image.to_rgba8();
    for (i, line) in lines.iter().enumerate() {
        if line.text.trim().is_empty() {
            continue;
        }
        let strip = strips
            .as_ref()
            .and_then(|s| s.get(i))
            .and_then(|s| s.as_ref());
        let firings: Vec<(char, f32)> = line
            .firings
            .iter()
            .map(|f| (char::from_u32(f.ch).unwrap_or('\u{fffd}'), f.at))
            .collect();
        // 0.65 mirrors text_metrics::MODEL_BOLD_THRESHOLD (pub(crate), so not nameable here).
        let word_ranges = match strip.and_then(|s| s.bold_profile()) {
            Some(profile) => translator::text_metrics::word_bold_ranges(
                &line.text, &firings, is_cjk, &profile, 0.65,
            ),
            None => Vec::new(),
        };
        let mut style: Vec<translator::ocr::StyleRange> = if !word_ranges.is_empty() {
            word_ranges
                .into_iter()
                .map(|(start, end)| translator::ocr::StyleRange {
                    start,
                    end,
                    kind: translator::ocr::StyleKind::Bold,
                })
                .collect()
        } else if strip
            .and_then(|s| s.pooled_bold())
            .map(|p| p >= 0.65)
            .unwrap_or(false)
        {
            vec![translator::ocr::StyleRange {
                start: 0,
                end: line.text.len() as u32,
                kind: translator::ocr::StyleKind::Bold,
            }]
        } else {
            Vec::new()
        };
        if let Some(profile) = strip.and_then(|s| s.rule_profile()) {
            style.extend(
                translator::text_metrics::word_decoration_ranges(
                    &line.text, &firings, is_cjk, &profile,
                )
                .into_iter()
                .map(|(start, end, dec)| translator::ocr::StyleRange {
                    start,
                    end,
                    kind: translator::ocr::StyleKind::Decoration(dec),
                }),
            );
        }
        if let Some(s) = strip {
            let spans = match s.fg.as_ref() {
                Some(fg) => translator::text_metrics::word_emphasis_colors_model(
                    &line.text,
                    &firings,
                    s.color_pool_alpha(),
                    fg,
                ),
                None => s
                    .src_map
                    .as_ref()
                    .map(|m| {
                        translator::text_metrics::word_emphasis_colors(
                            &line.text, &firings, &s.matte, m, &src_rgba,
                        )
                    })
                    .unwrap_or_default(),
            };
            style.extend(
                spans
                    .into_iter()
                    .map(|(start, end, argb)| translator::ocr::StyleRange {
                        start,
                        end,
                        kind: translator::ocr::StyleKind::Color(argb),
                    }),
            );
        }
        blocks.push(TextBlock {
            lines: vec![TextLine {
                text: line.text.clone(),
                bounding_box: line.rect,
                oriented_box: line.oriented_box,
                tight_box: line.oriented_box,
                word_rects: Vec::new(),
                style_ranges: style.clone(),
                ink_color: strip.and_then(|s| s.pooled_color()),
            }],
        });
        translated.push(line.text.clone());
        style_ranges.push(style);
    }

    let rgba = image.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());

    // Real ink-matte erase (union mask), exactly as the still overlay path uses.
    let ink_mask: Option<Vec<bool>> = strips.as_ref().map(|strips| {
        let masks: Vec<_> = strips
            .iter()
            .map(|s| s.as_ref().map(|s| s.matte.clone()))
            .collect();
        let src_maps: Vec<_> = strips
            .iter()
            .map(|s| s.as_ref().and_then(|s| s.src_map.clone()))
            .collect();
        translator::color_matting::union_ink_mask(&rgba, boxes, &masks, &src_maps)
    });

    let prepared = prepare_overlay_image(
        rgba.as_raw(),
        w,
        h,
        &blocks,
        &translated,
        &style_ranges,
        BackgroundMode::AutoDetect,
        ReadingOrder::LeftToRight,
        ink_mask.as_deref(),
    )
    .map_err(|e| format!("prepare_overlay_image: {e}"))?;

    let fonts = VizFonts {
        path: cli.font.clone(),
    };
    let opts = RenderOptions {
        language: cli.script.as_slug().to_string(),
        min_font_size_px: 8.0,
    };
    let rendered =
        render_overlay(&prepared, &fonts, &opts).map_err(|e| format!("render_overlay: {e}"))?;

    let mut canvas = RgbaImage::from_raw(w, h, rendered.rgba_bytes.clone())
        .ok_or_else(|| "rendered buffer size mismatch".to_string())?;
    let t = line_thickness(&canvas);
    draw_word_boxes(&mut canvas, &source_words, COLOR_FIRING_A, t.max(1), font);
    draw_word_boxes(&mut canvas, &rendered.translated_words, COLOR_BOX, t, font);
    save_png(&canvas, &cli.out_dir.join("rewrite.png"))?;

    // The same render-layout boxes over just the erased background, so the box
    // geometry is readable without the rendered glyphs sitting under it.
    let mut erased = RgbaImage::from_raw(w, h, prepared.rgba_bytes.clone())
        .ok_or_else(|| "erased buffer size mismatch".to_string())?;
    draw_word_boxes(&mut erased, &rendered.translated_words, COLOR_BOX, t, font);
    save_png(&erased, &cli.out_dir.join("rewrite-erased.png"))?;

    let words_json = serde_json::json!({
        "translated_words": words_to_json(&rendered.translated_words),
        "source_words": words_to_json(&source_words),
    });
    fs::write(
        cli.out_dir.join("rewrite_words.json"),
        serde_json::to_string_pretty(&words_json).unwrap(),
    )
    .map_err(|e| format!("write rewrite_words.json: {e}"))?;
    println!(
        "  rewrite: {} render-layout words (red), {} CTC source words (green)",
        rendered.translated_words.len(),
        source_words.len()
    );
    Ok(())
}

fn words_to_json(words: &[PositionedWord]) -> Vec<serde_json::Value> {
    words
        .iter()
        .map(|w| {
            serde_json::json!({
                "text": w.text,
                "line_index": w.line_index,
                "cx": w.bounds.cx,
                "cy": w.bounds.cy,
                "width": w.bounds.width,
                "height": w.bounds.height,
                "angle_deg": w.bounds.angle_radians.to_degrees(),
            })
        })
        .collect()
}

/// Outline each word's oriented box, labelling its index at the leading corner
/// so a box can be matched to its `rewrite_words.json` entry.
fn draw_word_boxes(
    img: &mut RgbaImage,
    words: &[PositionedWord],
    color: Rgba<u8>,
    t: i32,
    font: &FontRef,
) {
    for (i, w) in words.iter().enumerate() {
        let corners = w.bounds.corners();
        draw_closed_polyline(img, &corners, color, t);
        let (lx, ly) = corners[0];
        draw_text_mut(
            img,
            color,
            lx as i32,
            (ly as i32 - 14).max(0),
            PxScale::from(13.0),
            font,
            &i.to_string(),
        );
    }
}

fn run_docaligner_stage(
    stage: Stage,
    cli: &Cli,
    rgba: &RgbaImage,
    font: &FontRef,
    t: i32,
    engine: Option<&PpocrEngine>,
    index: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let aligner = DocAligner::load(&cli.docaligner, 4)
        .map_err(|e| format!("load DocAligner {}: {e:?}", cli.docaligner.display()))?;
    let (w, h) = (rgba.width(), rgba.height());
    let detection = aligner
        .detect(rgba.as_raw(), w, h)
        .map_err(|e| format!("docaligner detect: {e:?}"))?;
    let Some(detection) = detection else {
        eprintln!("  skip {}: DocAligner found no document", stage.slug());
        index.insert(
            "docaligner".into(),
            serde_json::json!("no document detected"),
        );
        return Ok(());
    };
    index.insert(
        "docaligner_confidence".into(),
        serde_json::json!(detection.confidence),
    );

    match stage {
        Stage::Corners => {
            let mut canvas = rgba.clone();
            let c = detection.quad.corners();
            let pts: Vec<(f32, f32)> = c.iter().map(|p| (p.x, p.y)).collect();
            draw_closed_polyline(&mut canvas, &pts, COLOR_BOX, t.max(2));
            let labels = ["TL", "TR", "BR", "BL"];
            for (p, name) in c.iter().zip(labels) {
                draw_label(&mut canvas, font, p.x as i32, p.y as i32, name, 22.0);
            }
            save_png(&canvas, &cli.out_dir.join("corners.png"))?;
        }
        Stage::Dewarp => {
            let quad = DocumentQuad::from_corners(
                detection
                    .quad
                    .corners()
                    .map(|p| DocumentPoint { x: p.x, y: p.y }),
            );
            let (out_w, out_h) = suggested_output_dims(&quad);
            let warped = warp(rgba.as_raw(), w, h, &quad, out_w, out_h, true)
                .map_err(|e| format!("docaligner warp: {e:?}"))?;
            let img: RgbaImage = RgbaImage::from_raw(warped.width, warped.height, warped.rgba)
                .ok_or_else(|| "warped image buffer size mismatch".to_string())?;
            save_png(&img, &cli.out_dir.join("dewarp.png"))?;

            // Re-run detection + recognition on the perspective-corrected page
            // and overlay the result, so the dewarp's effect on OCR is visible.
            if let Some(engine) = engine {
                let warped_dyn = DynamicImage::ImageRgba8(img.clone());
                let gray = warped_dyn.to_luma8();
                let mut wboxes = engine
                    .detect_only_image(&warped_dyn, PpocrProfile::Still)
                    .map_err(|e| format!("dewarp detect: {e:?}"))?;
                wboxes.sort_by(|a, b| (a.rect.top, a.rect.left).cmp(&(b.rect.top, b.rect.left)));
                let lines = recognize(engine, &warped_dyn, &gray, &wboxes, cli.script, true)
                    .map_err(|e| format!("dewarp recognize: {e:?}"))?;
                let rec = draw_recognition(&img, &wboxes, &lines, font);
                save_png(&rec, &cli.out_dir.join("dewarp-rec.png"))?;

                // Per-box deskewed strips cut from the perspective-corrected page
                // (the counterpart of `deskewed/`, but post-DocAligner).
                let warped_rgb = warped_dyn.to_rgb8();
                let strip_dir = cli.out_dir.join("dewarp-strips");
                fs::create_dir_all(&strip_dir).map_err(|e| format!("mkdir: {e}"))?;
                for (i, b) in wboxes.iter().enumerate() {
                    let contour = contour_points(&b.contour);
                    if let Some(strip) =
                        dewarp_contour_to_strip_rgb(&warped_rgb, &contour, None, 0.0)
                    {
                        strip
                            .save(strip_dir.join(format!("box-{i:03}.png")))
                            .map_err(|e| format!("save dewarp strip: {e}"))?;
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

// ---------------------------------------------------------------------
// IO
// ---------------------------------------------------------------------

fn save_png(img: &RgbaImage, path: &Path) -> Result<(), String> {
    img.save(path)
        .map_err(|e| format!("save {}: {e}", path.display()))
}

fn save_gray_png(img: &GrayImage, path: &Path) -> Result<(), String> {
    img.save(path)
        .map_err(|e| format!("save {}: {e}", path.display()))
}

fn save_dynamic_png(img: &DynamicImage, path: &Path) -> Result<(), String> {
    img.save(path)
        .map_err(|e| format!("save {}: {e}", path.display()))
}
