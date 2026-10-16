//! Synthetic-warp tests for the planar tracker.
//!
//! These tests take real text photos and apply a *known* homography to
//! produce a "current frame". The tracker then has to recover the same
//! homography. Asserting "recovered.project(x) ≈ known.project(x)" gives
//! us a 1-line exact oracle for every algorithmic change.
//!
//! Caveats: synthetic warps are *too* clean — no motion blur, no
//! lighting change, no rolling shutter, no JPEG re-encode per frame.
//! Real-world handheld video loses 30–50% more matches. A green run
//! here only proves we aren't algorithmically broken. On-device
//! validation is still required.
#![cfg(feature = "planar-tracker")]

use std::f32::consts::PI;
use std::path::PathBuf;

use image::imageops::FilterType;
use image::{DynamicImage, GrayImage, ImageBuffer, Luma};
use imageproc::geometric_transformations::{Interpolation, Projection, warp};

use translator::homography::project as project_h;
use translator::planar_tracker::{
    LivePlanarTracker, TrackerConfig, build_anchor, track_against_anchor,
};

/// Where Phase A's bench photos live.
fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("files");
    p.push("live-overlay");
    p.push(name);
    p
}

/// Resize the image so its short edge is `short_edge_px`, convert to
/// grayscale, return it. Matches the analyzer pipeline's grayscale
/// 720-ish-pixel working frame.
fn load_gray(name: &str, short_edge_px: u32) -> GrayImage {
    let img = image::open(fixture(name)).expect("open fixture");
    let (w, h) = (img.width(), img.height());
    let (nw, nh) = if w <= h {
        let nw = short_edge_px;
        let nh = (h as f32 * (short_edge_px as f32 / w as f32)).round() as u32;
        (nw, nh)
    } else {
        let nh = short_edge_px;
        let nw = (w as f32 * (short_edge_px as f32 / h as f32)).round() as u32;
        (nw, nh)
    };
    img.resize_exact(nw, nh, FilterType::Triangle).to_luma8()
}

/// Apply a 3x3 homography to `src` using bilinear sampling. Output
/// canvas matches the source dimensions; pixels that fall outside the
/// source are filled with 0 (black). Matches what we'd see if the
/// camera ran the same homography.
fn warp_with_h(src: &GrayImage, h: &[f32; 9]) -> GrayImage {
    let projection = Projection::from_matrix(*h).expect("projection invertible");
    warp(src, &projection, Interpolation::Bilinear, Luma([0u8]))
}

/// Test points distributed across the source image (corners + interior
/// grid). We measure homography agreement by projecting these and
/// taking the max distance.
fn test_points(w: u32, h: u32) -> Vec<(f32, f32)> {
    let mut pts = Vec::new();
    for fx in 0..5 {
        for fy in 0..5 {
            let x = (fx as f32 + 0.5) * (w as f32 / 5.0);
            let y = (fy as f32 + 0.5) * (h as f32 / 5.0);
            pts.push((x, y));
        }
    }
    pts
}

/// Max pixel disagreement between two homographies when projecting a
/// shared point set. Returns `f32::INFINITY` if either projection is
/// degenerate at one of the sample points.
fn max_point_error(h_a: &[f32; 9], h_b: &[f32; 9], pts: &[(f32, f32)]) -> f32 {
    let mut worst = 0.0f32;
    for &(x, y) in pts {
        let Some((ax, ay)) = project_h(h_a, x, y) else {
            return f32::INFINITY;
        };
        let Some((bx, by)) = project_h(h_b, x, y) else {
            return f32::INFINITY;
        };
        let dx = ax - bx;
        let dy = ay - by;
        worst = worst.max((dx * dx + dy * dy).sqrt());
    }
    worst
}

/// A test-tuned config: lots of features, generous RANSAC budget so
/// numerical noise doesn't intermittently fail tests on different
/// hardware.
fn test_config() -> TrackerConfig {
    TrackerConfig {
        fast_threshold: 20,
        fast_threshold_fallback: 10,
        fast_min_keypoints: 100,
        max_features: 800,
        lowe_ratio: 0.8,
        lowe_ratio_locked: 0.9,
        ransac_residual_px: 3.0,
        ransac_iters: 400,
        min_inliers: 20,
        min_inliers_keep_locked: 6,
        nms_radius: 3,
        guided_search_radius_px: 30.0,
    }
}

#[test]
fn identity_roundtrip_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");
    assert!(
        anchor.len() >= 100,
        "expected ≥100 anchor features, got {}",
        anchor.len()
    );

    let result = track_against_anchor(&anchor, &gray, &cfg).expect("track succeeds");
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &identity, &pts);
    assert!(
        err < 1.0,
        "identity roundtrip should map points within 1 px; max err={:.3} px, inliers={}, residual={:.3}",
        err,
        result.inliers,
        result.median_residual_px
    );
}

#[test]
fn pure_translation_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let tx = 12.0f32;
    let ty = -7.0f32;
    let known = [1.0, 0.0, tx, 0.0, 1.0, ty, 0.0, 0.0, 1.0];
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 1.5,
        "translation recovery should be within 1.5 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn rotation_90_deg_book() {
    // Portrait → horizontal: the on-phone failure case before oriented
    // BRIEF landed. 90° is the worst rotation for fixed-pattern BRIEF
    // (samples land on entirely different gradients) — only rotation-
    // invariant descriptors can possibly match here.
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let theta = 90.0f32 * PI / 180.0;
    let known = h_rotate_about(cx, cy, theta);
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 8.0,
        "90° rotation recovery should be within 8 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn rotation_45_deg_book() {
    // Real test of rotation invariance: 45° is past the point where
    // un-oriented BRIEF descriptors stop matching, so this only passes
    // because the descriptor's sample pattern is rotated by each
    // keypoint's dominant gradient direction.
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let theta = 45.0f32 * PI / 180.0;
    let known = h_rotate_about(cx, cy, theta);
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 6.0,
        "45° rotation recovery should be within 6 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn rotation_10_deg_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let theta = 10.0f32 * PI / 180.0;
    let known = h_rotate_about(cx, cy, theta);
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 3.0,
        "10° rotation recovery should be within 3 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn scale_1p15_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let known = h_scale_about(cx, cy, 1.15);
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 3.0,
        "1.15x zoom recovery should be within 3 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn perspective_tilt_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let known = h_perspective_tilt(gray.width(), gray.height(), 0.20);
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 4.0,
        "perspective-tilt recovery should be within 4 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn composed_translation_rotation_scale_book() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let t = [1.0, 0.0, 8.0, 0.0, 1.0, -5.0, 0.0, 0.0, 1.0];
    let r = h_rotate_about(cx, cy, 6.0 * PI / 180.0);
    let s = h_scale_about(cx, cy, 1.08);
    // Apply in order: scale, then rotate, then translate.
    let known = mat3_mul_test(&t, &mat3_mul_test(&r, &s));
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 4.0,
        "composed transform recovery should be within 4 px; max err={:.3}, inliers={}",
        err,
        result.inliers
    );
}

#[test]
fn unrelated_image_returns_none() {
    let gray = load_gray("book.jpg", 480);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built");

    let other = load_gray("sign.jpg", 480);
    // Pad / crop sign to the same dims as `book` so dimensional checks
    // can't masquerade as the failure signal.
    let resized = image::DynamicImage::ImageLuma8(other)
        .resize_exact(gray.width(), gray.height(), FilterType::Triangle)
        .to_luma8();
    let result = track_against_anchor(&anchor, &resized, &cfg);
    assert!(
        result.is_none(),
        "tracker should reject an unrelated scene; got {:?}",
        result.as_ref().map(|r| (r.inliers, r.matches))
    );
}

#[test]
fn random_dots_translation_synthetic() {
    // Procedural fixture: scattered dark spots on white background. Each
    // spot produces a clean FAST corner because exactly one contiguous
    // run of the Bresenham circle is darker — unlike a checkerboard,
    // where the two halves are 8/8 and FAST-9 finds nothing. Used to
    // verify the whole pipeline works without photo fixtures.
    let gray = random_dots(480, 360);
    let cfg = test_config();
    let anchor = build_anchor(&gray, &cfg, 0).expect("anchor built from random dots");
    assert!(
        anchor.len() >= 100,
        "random-dots fixture should give plenty of corners, got {}",
        anchor.len()
    );

    let known = [1.0, 0.0, 18.0, 0.0, 1.0, -11.0, 0.0, 0.0, 1.0];
    let warped = warp_with_h(&gray, &known);

    let result = track_against_anchor(&anchor, &warped, &cfg).expect("track succeeds");
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &known, &pts);
    assert!(
        err < 1.5,
        "random-dots translation should be near-pixel; max err={:.3}",
        err
    );
}

#[test]
fn live_tracker_acquire_then_track() {
    let gray = load_gray("book.jpg", 480);
    let mut tracker = LivePlanarTracker::with_config(test_config());

    assert!(tracker.acquire(&gray, 0), "acquire should succeed");
    let result = tracker.track(&gray).expect("track succeeds on same frame");
    let identity = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let pts = test_points(gray.width(), gray.height());
    let err = max_point_error(&result.homography, &identity, &pts);
    assert!(err < 1.0, "live-tracker identity err={:.3}", err);
}

// -- helpers ---------------------------------------------------------------

fn random_dots(w: u32, h: u32) -> GrayImage {
    let mut img: ImageBuffer<Luma<u8>, Vec<u8>> = ImageBuffer::from_pixel(w, h, Luma([240u8]));
    let mut s: u64 = 0x0BAD_F00D_CAFE_FACE;
    let next = |state: &mut u64| {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    };
    // ~250 dots, each ~3x3 px. Plenty of features, no two identical
    // locally so descriptors are discriminative.
    for _ in 0..250 {
        let cx = (next(&mut s) as u32 % (w - 8)) + 4;
        let cy = (next(&mut s) as u32 % (h - 8)) + 4;
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                let x = cx as i32 + dx;
                let y = cy as i32 + dy;
                img.put_pixel(x as u32, y as u32, Luma([20u8]));
            }
        }
    }
    // Add light noise so descriptor differences are deterministic but
    // not entirely uniform (matches what photo input looks like).
    for y in 0..h {
        for x in 0..w {
            let n = ((x.wrapping_mul(2654435761)) ^ (y.wrapping_mul(40503))) % 7;
            let p = img.get_pixel(x, y)[0] as i32;
            let v = (p + n as i32 - 3).clamp(0, 255) as u8;
            img.put_pixel(x, y, Luma([v]));
        }
    }
    img
}

fn h_rotate_about(cx: f32, cy: f32, theta: f32) -> [f32; 9] {
    let (s, c) = theta.sin_cos();
    let t_pos = [1.0, 0.0, cx, 0.0, 1.0, cy, 0.0, 0.0, 1.0];
    let t_neg = [1.0, 0.0, -cx, 0.0, 1.0, -cy, 0.0, 0.0, 1.0];
    let r = [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0];
    let m = mat3_mul_test(&r, &t_neg);
    mat3_mul_test(&t_pos, &m)
}

fn h_scale_about(cx: f32, cy: f32, s: f32) -> [f32; 9] {
    let t_pos = [1.0, 0.0, cx, 0.0, 1.0, cy, 0.0, 0.0, 1.0];
    let t_neg = [1.0, 0.0, -cx, 0.0, 1.0, -cy, 0.0, 0.0, 1.0];
    let sc = [s, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 1.0];
    let m = mat3_mul_test(&sc, &t_neg);
    mat3_mul_test(&t_pos, &m)
}

/// Build a homography that simulates an out-of-plane "tilt" by mapping
/// the source rectangle to a trapezoid (foreshorten the bottom edge by
/// `amount`). For amount=0 → identity. For amount=0.5 → bottom edge is
/// half its original width.
fn h_perspective_tilt(w: u32, h: u32, amount: f32) -> [f32; 9] {
    let w = w as f32;
    let h = h as f32;
    // Source corners: TL, TR, BR, BL.
    let src = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
    let inset = (w * amount) / 2.0;
    let dst = [(0.0, 0.0), (w, 0.0), (w - inset, h), (inset, h)];
    // Solve for the homography mapping src → dst using fit_homography.
    let pairs: Vec<(f32, f32, f32, f32)> = src
        .iter()
        .zip(dst.iter())
        .map(|(&(sx, sy), &(dx, dy))| (sx, sy, dx, dy))
        .collect();
    translator::homography::fit_homography(&pairs).expect("trapezoid homography solvable")
}

fn mat3_mul_test(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    let mut out = [0.0_f32; 9];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0_f32;
            for k in 0..3 {
                s += a[i * 3 + k] * b[k * 3 + j];
            }
            out[i * 3 + j] = s;
        }
    }
    out
}

#[allow(dead_code)]
fn save_debug(img: &GrayImage, name: &str) {
    let path = std::env::temp_dir().join(name);
    DynamicImage::ImageLuma8(img.clone())
        .save(&path)
        .expect("save");
    eprintln!("wrote {}", path.display());
}
