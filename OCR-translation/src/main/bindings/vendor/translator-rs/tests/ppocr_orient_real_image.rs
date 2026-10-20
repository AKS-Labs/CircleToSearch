//! Diagnostic 4-rotation test using a real photo (`files/screen-rot.jpg`)
//! instead of synthetic text. For each rotation in {R0, R90, R180, R270}:
//!
//!   1. Detect text boxes with PPOCR.
//!   2. Replicate `estimate_canonical_quadrant`'s strip-selection +
//!      H-normalize + window-sampling pipeline, dumping every AABB strip
//!      and every classifier-input window to disk.
//!   3. Classify each window in both its raw form AND a 180°-rotated
//!      copy (matches the production validation-pair logic). Print the
//!      label+confidence side-by-side per window.
//!   4. Print the final `estimate_canonical_quadrant` result.
//!
//! Output layout:
//!   smoke-out/orient-real/{R0,R90,R180,R270}/
//!     rotated.png                       full rotated input
//!     box-{idx}-aabb.png                raw AABB crop per detected box
//!     window-{idx}-raw.png              160×80 window seen by classifier
//!     window-{idx}-raw180.png           the 180°-rotated pair member
//!     summary.txt                       per-window labels + final canonical
//!
//! Skips when the bucket models or `files/screen-rot.jpg` are missing —
//! drop the photo at `<repo>/files/screen-rot.jpg` to run.

#![cfg(all(feature = "ppocr", feature = "planar-tracker"))]

use std::fs;
use std::path::{Path, PathBuf};

use image::{DynamicImage, imageops::FilterType};

use translator::ppocr::{
    PpocrEngine, PpocrProfile, PpocrRecognizerSpec, TextlineOriCandidate, TextlineOriLabel,
    contour_principal_axis_angle, dewarp_contour_to_strip,
};
use translator::{DetectedTextBox, PpocrScript};
use translator_ocr::orientation::estimate_canonical_quadrant;

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const INPUT_PATH: &str = "files/screen-rot.jpg";
const DUMP_DIR: &str = "smoke-out/orient-real";

// Mirror the constants in `estimate_canonical_quadrant`.
const MIN_LONG_PX: f32 = 40.0;
const MIN_DET_SCORE: f32 = 0.5;
const TOP_N: usize = 10;
const WINDOW_W: u32 = 160;
const WINDOW_H: u32 = 80;
const MAX_WINDOWS_PER_STRIP: u32 = 3;

#[test]
fn dump_real_image_classification_across_rotations() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some((det, rec, keys, textline_ori)) = ppocr_paths() else {
        eprintln!("PPOCR bucket files missing under {MODEL_DIR}; skipping");
        return;
    };
    let input_path = Path::new(INPUT_PATH);
    if !input_path.exists() {
        eprintln!("input photo {INPUT_PATH} missing; drop a real screen capture there and rerun");
        return;
    }
    let base = match image::open(input_path) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("failed to load {INPUT_PATH}: {e}; skipping");
            return;
        }
    };

    fs::create_dir_all(DUMP_DIR).expect("create dump dir");

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

    for (label, rotated) in [
        ("R0", base.clone()),
        ("R90", base.rotate90()),
        ("R180", base.rotate180()),
        ("R270", base.rotate270()),
    ] {
        let case_dir = Path::new(DUMP_DIR).join(label);
        fs::create_dir_all(&case_dir).expect("create case dir");
        let mut summary = String::new();

        summary.push_str(&format!(
            "=== {label} ({}×{}) ===\n",
            rotated.width(),
            rotated.height(),
        ));

        rotated
            .save(case_dir.join("rotated.png"))
            .expect("save rotated");

        let det_boxes: Vec<DetectedTextBox> =
            match engine.detect_only_image(&rotated, PpocrProfile::Still) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[{label}] detect failed: {e:?}");
                    continue;
                }
            };
        summary.push_str(&format!("detected {} boxes\n", det_boxes.len()));

        // Contour + size gate: same logic as estimate_canonical_quadrant.
        let mut selected: Vec<(usize, f32)> = det_boxes
            .iter()
            .enumerate()
            .filter_map(|(i, b)| {
                if b.score < MIN_DET_SCORE {
                    return None;
                }
                if b.contour.is_empty() || b.contour.len() % 2 != 0 {
                    return None;
                }
                let w = b.rect.right.saturating_sub(b.rect.left) as f32;
                let h = b.rect.bottom.saturating_sub(b.rect.top) as f32;
                let long = w.max(h);
                if long < MIN_LONG_PX {
                    return None;
                }
                Some((i, long))
            })
            .collect();
        selected
            .sort_unstable_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        selected.truncate(TOP_N);

        summary.push_str(&format!("contour-qualified: {}\n", selected.len()));

        // Dump every selected box's raw AABB AND its deskewed strip
        // (what production actually feeds the classifier).
        let gray = rotated.to_luma8();
        for (i, _) in &selected {
            let b = &det_boxes[*i];
            let w = b.rect.right.saturating_sub(b.rect.left).max(1);
            let h = b.rect.bottom.saturating_sub(b.rect.top).max(1);
            let aabb = rotated.crop_imm(b.rect.left, b.rect.top, w, h);
            aabb.save(case_dir.join(format!("box-{i}-aabb.png"))).ok();
            let contour: Vec<(f32, f32)> =
                b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect();
            if let Some(strip) = dewarp_contour_to_strip(&gray, &contour, None, 0.0) {
                let theta = contour_principal_axis_angle(&contour).unwrap_or(0.0);
                DynamicImage::ImageLuma8(strip)
                    .save(case_dir.join(format!(
                        "box-{i}-deskewed-theta{:.0}.png",
                        theta.to_degrees()
                    )))
                    .ok();
            }
        }

        // Build the same windows the production pipeline does.
        let mut windows: Vec<DynamicImage> = Vec::new();
        let mut window_meta: Vec<(usize, f32, u32)> = Vec::new(); // (box_idx, theta, window_idx_within_strip)
        for &(i, _) in &selected {
            let b = &det_boxes[i];
            let contour: Vec<(f32, f32)> =
                b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect();
            let Some(theta) = contour_principal_axis_angle(&contour) else {
                continue;
            };
            let Some(deskewed_luma) = dewarp_contour_to_strip(&gray, &contour, None, 0.0) else {
                continue;
            };
            let strip = DynamicImage::ImageLuma8(deskewed_luma);
            let ow = strip.width();
            let oh = strip.height();
            if oh == 0 || ow == 0 {
                continue;
            }
            let scale = WINDOW_H as f32 / oh as f32;
            let new_w = ((ow as f32) * scale).round().max(1.0) as u32;
            let normalized = strip.resize_exact(new_w, WINDOW_H, FilterType::Triangle);
            if new_w <= WINDOW_W {
                windows.push(normalized);
                window_meta.push((i, theta, 0));
                continue;
            }
            let span = new_w - WINDOW_W;
            let n = (new_w / WINDOW_W).clamp(1, MAX_WINDOWS_PER_STRIP);
            for k in 0..n {
                let x_start = if n == 1 {
                    span / 2
                } else {
                    (span * k) / (n - 1)
                };
                let win = normalized.crop_imm(x_start, 0, WINDOW_W, WINDOW_H);
                windows.push(win);
                window_meta.push((i, theta, k));
            }
        }

        summary.push_str(&format!("windows built: {}\n", windows.len()));

        let windows_180: Vec<DynamicImage> = windows.iter().map(|w| w.rotate180()).collect();
        let labels_raw = engine
            .textline_orientation_classify(&windows)
            .expect("classify raw");
        let labels_180 = engine
            .textline_orientation_classify(&windows_180)
            .expect("classify raw180");

        summary.push_str("\n  idx  box  θ°    w  raw                raw180             decision\n");
        for (idx, ((raw, raw180), (box_idx, theta, win_k))) in labels_raw
            .iter()
            .zip(labels_180.iter())
            .zip(window_meta.iter())
            .enumerate()
        {
            let theta_deg = theta.to_degrees();
            windows[idx]
                .save(case_dir.join(format!(
                    "window-{idx:02}-box{box_idx}-theta{theta_deg:.0}-w{win_k}-raw.png"
                )))
                .ok();
            windows_180[idx]
                .save(case_dir.join(format!(
                    "window-{idx:02}-box{box_idx}-theta{theta_deg:.0}-w{win_k}-raw180.png"
                )))
                .ok();
            let raw_s = fmt_cand(raw);
            let raw180_s = fmt_cand(raw180);
            let decision = decision_for(raw, raw180);
            summary.push_str(&format!(
                "  {idx:>3}  {box_idx:>3}  {theta_deg:>4.0}  w{win_k}  {raw_s:<18}  {raw180_s:<18} {decision}\n",
            ));
        }

        let canonical =
            estimate_canonical_quadrant(&engine, &rotated, &rotated.to_luma8(), &det_boxes);
        summary.push_str(&format!(
            "\nestimate_canonical_quadrant → {:?}\n",
            canonical
        ));

        eprintln!("{summary}");
        fs::write(case_dir.join("summary.txt"), &summary).ok();
    }
}

fn fmt_cand(cand: &Option<TextlineOriCandidate>) -> String {
    match cand {
        Some(c) => format!("{:?}@{:.3}", c.label, c.score),
        None => "—".to_string(),
    }
}

fn decision_for(
    raw: &Option<TextlineOriCandidate>,
    raw180: &Option<TextlineOriCandidate>,
) -> &'static str {
    match (raw, raw180) {
        (Some(r), Some(r180)) => {
            if r.label == r180.label {
                "BIAS-REJECT"
            } else if r.score < 0.7 || r180.score < 0.7 {
                "LOW-CONF"
            } else {
                match r.label {
                    TextlineOriLabel::Up => "VOTE-Up",
                    TextlineOriLabel::Flipped180 => "VOTE-Flipped180",
                }
            }
        }
        _ => "MISSING",
    }
}

fn ppocr_paths() -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let det = env_path("OCR_REAL_DET")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn"));
    let rec = env_path("OCR_REAL_REC")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn"));
    let keys = env_path("OCR_REAL_KEYS")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_keys.txt"));
    let textline_ori = env_path("OCR_REAL_TEXTLINE")
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
