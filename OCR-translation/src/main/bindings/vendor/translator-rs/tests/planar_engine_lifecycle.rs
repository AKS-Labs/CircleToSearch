//! Lifecycle + LRU cache tests for `planar_engine::LivePlanarEngine`.
//!
//! These cover Phases D, E, F, G state-machine behaviour:
//!   - Idle waits for IMU stability before reporting Acquiring
//!   - acquire_now → Locked
//!   - Locked frame with same scene → still Locked, fresh homography
//!   - Locked frame with unrelated scene → eventually Lost
//!   - Lost with no recovery → eventually Idle
//!   - LRU: A → B → A returns the *same* anchor id (page-flip case)
//!   - Refresh: anchor older than `anchor_refresh_age_ns` triggers refresh
//!
//! Uses synthetic warps as the per-frame "current image" so we have
//! exact ground truth and no I/O variability.
#![cfg(feature = "planar-tracker")]

use std::f32::consts::PI;
use std::path::PathBuf;

use image::imageops::FilterType;
use image::{GrayImage, Luma};
use imageproc::geometric_transformations::{Interpolation, Projection, warp};

use translator::planar_engine::{
    AnchorId, CanonicalOverlay, EngineConfig, LivePlanarEngine, TrackerCommand,
};
use translator::planar_tracker::TrackerConfig;

fn fixture(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("files");
    p.push("live-overlay");
    p.push(name);
    p
}

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

fn warp_with_h(src: &GrayImage, h: &[f32; 9]) -> GrayImage {
    let projection = Projection::from_matrix(*h).expect("projection invertible");
    warp(src, &projection, Interpolation::Bilinear, Luma([0u8]))
}

fn test_engine_config() -> EngineConfig {
    EngineConfig {
        tracker: TrackerConfig {
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
        },
        anchor_cache_size: 5,
        // No cooldown for tests — they re-acquire intentionally.
        acquire_cooldown_ns: 0,
        lost_after_frames: 3,
        give_up_after_frames: 5,
        stable_required_ns: 100_000_000,
        anchor_refresh_age_ns: 5_000_000_000,
        // Disable handoff by default in lifecycle tests so the
        // single-anchor semantics keep matching expectations. The
        // dedicated chain tests override these to force handoffs.
        handoff_min_inliers: 0,
        handoff_min_visible_ratio: 0.0,
        handoff_scale_log_threshold: f32::INFINITY,
        handoff_cooldown_ns: u64::MAX / 2,
        sanity_gate_drop_ratio: 0.3,
        sanity_gate_min_ema: 60.0,
        sanity_gate_max_consecutive: 3,
        inlier_ema_alpha: 0.2,
        degraded_inlier_threshold: 0,
        degraded_max_frames: u32::MAX,
        default_canonical_quadrant: translator::coords::Quadrant::R0,
        // Disable in lifecycle tests: they assert on exact emitted H
        // values, which the blend would intentionally perturb across
        // handoff/snap frames.
        anchor_switch_blend_frames: 0,
        anchor_switch_blend_threshold_px: f32::INFINITY,
        max_chain_depth: u32::MAX,
        // Disabled in lifecycle tests: per-frame quality gating
        // would block handoff on synthetic data where the per-fit
        // residual and inlier ratio aren't representative.
        handoff_max_median_residual_px: f32::INFINITY,
        handoff_min_inlier_ratio: 0.0,
        // Disabled in lifecycle tests: the EKF intentionally biases
        // the per-frame H toward its temporal estimate, which would
        // perturb the projection-equality assertions these tests use.
        // Dedicated EKF behaviour is exercised by the `h_ekf_*` tests.
        use_h_ekf: false,
        h_ekf_r_var: 4.0,
        // Disabled in lifecycle tests for the same reason as the
        // EKF: the running mean would shift `h_root_to_canonical`
        // between frames in the chain test's exact-projection
        // assertions.
        use_chain_refine: false,
        chain_refine_frames: 0,
    }
}

#[test]
fn acquire_then_track_same_frame() {
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);

    let id = engine.acquire_now(&gray, 1_000_000).expect("acquire");
    assert!(matches!(
        engine.process_frame(&gray, 2_000_000),
        TrackerCommand::Locked {
            anchor_id, is_new: false, ..
        } if anchor_id == id
    ));
    assert_eq!(engine.cache_len(), 1);
}

#[test]
fn locked_through_known_translation() {
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);

    let id = engine.acquire_now(&gray, 1_000_000).expect("acquire");
    let translated = warp_with_h(&gray, &[1.0, 0.0, 8.0, 0.0, 1.0, -4.0, 0.0, 0.0, 1.0]);
    let cmd = engine.process_frame(&translated, 2_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id,
            inliers,
            is_new,
            ..
        } => {
            assert_eq!(anchor_id, id);
            assert!(!is_new);
            assert!(inliers >= 20, "expected ≥20 inliers, got {}", inliers);
        }
        other => panic!("expected Locked, got {:?}", other),
    }
}

#[test]
fn lost_then_recovered_via_same_anchor() {
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let other = GrayImage::from_pixel(gray.width(), gray.height(), Luma([128u8]));

    let id = engine.acquire_now(&gray, 1_000_000).expect("acquire");
    // Three lost frames → Lost.
    for i in 0..test_engine_config().lost_after_frames {
        let cmd = engine.process_frame(&other, 2_000_000 + i as u64 * 1_000_000);
        if i == test_engine_config().lost_after_frames - 1 {
            assert!(
                matches!(cmd, TrackerCommand::Lost { last_anchor_id } if last_anchor_id == id),
                "expected Lost at i={}, got {:?}",
                i,
                cmd
            );
        }
    }
    // Re-show original frame → should re-lock via cached anchor.
    let cmd = engine.process_frame(&gray, 10_000_000);
    assert!(
        matches!(cmd, TrackerCommand::Locked { anchor_id, .. } if anchor_id == id),
        "expected re-lock onto cached anchor; got {:?}",
        cmd
    );
}

#[test]
fn lost_to_idle_after_give_up() {
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let other = GrayImage::from_pixel(gray.width(), gray.height(), Luma([128u8]));

    engine.acquire_now(&gray, 1_000_000).expect("acquire");
    // Feed enough unrelated frames to walk through Locked-loss then Lost-give-up.
    let total = test_engine_config().lost_after_frames + test_engine_config().give_up_after_frames;
    let mut last_cmd = TrackerCommand::Idle;
    for i in 0..(total + 1) {
        last_cmd = engine.process_frame(&other, 2_000_000 + i as u64 * 1_000_000);
    }
    assert!(
        matches!(last_cmd, TrackerCommand::Idle),
        "expected Idle after give-up; got {:?}",
        last_cmd
    );
}

#[test]
fn lru_page_flip_returns_same_anchor() {
    // Capture two distinct scenes, flip back to the first, assert the
    // engine snaps onto the cached anchor (same id), no re-OCR needed.
    let mut cfg = test_engine_config();
    cfg.acquire_cooldown_ns = 0;
    let mut engine = LivePlanarEngine::new(cfg);
    let scene_a = load_gray("book.jpg", 480);
    let scene_b = load_gray("sign.jpg", 480);
    let scene_b = image::DynamicImage::ImageLuma8(scene_b)
        .resize_exact(scene_a.width(), scene_a.height(), FilterType::Triangle)
        .to_luma8();

    let id_a = engine.acquire_now(&scene_a, 1_000_000).expect("acquire A");
    // Force-acquire B (cooldown=0).
    let id_b = engine.acquire_now(&scene_b, 2_000_000).expect("acquire B");
    assert_ne!(id_a, id_b);

    // Flip back to scene A. Process_frame should match the cached A
    // anchor and report Locked with id_a.
    let cmd = engine.process_frame(&scene_a, 3_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id, is_new, ..
        } => {
            assert_eq!(anchor_id, id_a, "expected cached A, got {}", anchor_id);
            assert!(!is_new, "cached re-lock should not set is_new");
        }
        other => panic!("expected Locked re-onto A, got {:?}", other),
    }
}

#[test]
fn lru_eviction_at_capacity() {
    let mut cfg = test_engine_config();
    cfg.anchor_cache_size = 2;
    let mut engine = LivePlanarEngine::new(cfg);
    let scene = load_gray("book.jpg", 480);

    // Forcibly create more distinct anchors than the cache holds.
    // Different timestamps; each acquire creates a new anchor since we
    // ignore the cooldown in test config.
    let id1 = engine.acquire_now(&scene, 1_000).expect("a1");
    let id2 = engine.acquire_now(&scene, 2_000).expect("a2");
    let id3 = engine.acquire_now(&scene, 3_000).expect("a3");

    // Each `acquire_now` makes its own root (no chaining across calls),
    // so root ids == handle ids here. `cached_root_ids` is the right
    // accessor for tests asserting cache content from outside the
    // engine module — `cached_handle_ids` exposes internal handle
    // ids and is intentionally `pub(crate)`.
    let ids = engine.cached_root_ids();
    assert_eq!(engine.cache_len(), 2);
    assert!(ids.contains(&id3), "newest must remain");
    assert!(ids.contains(&id2), "second-newest must remain");
    assert!(!ids.contains(&id1), "oldest must have been evicted");
}

#[test]
fn refresh_after_age_threshold() {
    let mut cfg = test_engine_config();
    cfg.anchor_refresh_age_ns = 1_000_000_000; // 1s for test
    let mut engine = LivePlanarEngine::new(cfg);
    let gray = load_gray("book.jpg", 480);

    let id = engine.acquire_now(&gray, 0).expect("acquire");
    assert!(!engine.should_refresh(id, 500_000_000));
    assert!(engine.should_refresh(id, 1_500_000_000));
}

#[test]
fn overlays_project_through_homography() {
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let id = engine.acquire_now(&gray, 1_000_000).expect("acquire");

    let overlays = vec![CanonicalOverlay {
        id: 42,
        quad: [
            (100.0, 100.0),
            (200.0, 100.0),
            (200.0, 160.0),
            (100.0, 160.0),
        ],
        payload: "hello".to_string(),
    }];
    assert!(engine.set_overlays(id, overlays.clone()));

    // Apply a known rotation about the image centre and check that the
    // projected quad lands where we expect.
    let cx = gray.width() as f32 / 2.0;
    let cy = gray.height() as f32 / 2.0;
    let theta = 12.0 * PI / 180.0;
    let h = h_rotate_about(cx, cy, theta);

    let projected = engine.project_overlays(id, &h);
    assert_eq!(projected.len(), 1);
    let p = &projected[0];
    assert_eq!(p.id, 42);
    assert_eq!(p.payload, "hello");

    let (s, c) = theta.sin_cos();
    for (i, &(x, y)) in overlays[0].quad.iter().enumerate() {
        let dx = x - cx;
        let dy = y - cy;
        let ex = cx + c * dx - s * dy;
        let ey = cy + s * dx + c * dy;
        let (ax, ay) = p.quad[i];
        let err = ((ax - ex).powi(2) + (ay - ey).powi(2)).sqrt();
        assert!(err < 1e-3, "overlay corner {} drift {}", i, err);
    }
}

#[test]
fn force_acquire_during_lost_state() {
    // After we give up on tracking, a fresh acquire_now should still
    // work (no permanent failure state).
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let other = load_gray("sign.jpg", 480);
    let other = image::DynamicImage::ImageLuma8(other)
        .resize_exact(gray.width(), gray.height(), FilterType::Triangle)
        .to_luma8();

    engine.acquire_now(&gray, 1_000_000).expect("acquire 1");
    for i in 0..30 {
        engine.process_frame(&other, 2_000_000 + i * 1_000_000);
    }
    let new_id = engine.acquire_now(&gray, 1_000_000_000).expect("acquire 2");
    let cmd = engine.process_frame(&gray, 1_001_000_000);
    assert!(
        matches!(cmd, TrackerCommand::Locked { anchor_id, .. } if anchor_id == new_id),
        "expected new lock, got {:?}",
        cmd
    );
}

#[test]
fn bitmap_overlay_outline_inside_outside_alpha() {
    // Rasterizer unit test for the Phase 2 bitmap path: place a known
    // canonical overlay quad, render the bitmap with empty translated
    // text (so only the cyan outline is drawn), and verify pixels on
    // the outline have non-zero alpha while pixels far away don't.
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let anchor_id = engine.acquire_now(&gray, 1_000).expect("acquire");

    let canonical_quad = [
        (100.0_f32, 100.0_f32),
        (200.0_f32, 100.0_f32),
        (200.0_f32, 150.0_f32),
        (100.0_f32, 150.0_f32),
    ];
    let overlay = CanonicalOverlay {
        id: 42,
        quad: canonical_quad,
        payload: String::new(),
    };
    assert!(engine.set_overlays(anchor_id, vec![overlay]));

    let w = gray.width();
    let h = gray.height();
    let items = vec![translator::planar_engine::TextRenderItem {
        id: 42,
        quad: canonical_quad,
        translated_text: String::new(),
        source_text: String::new(),
        language: String::new(),
        bg_argb: 0,
        fg_argb: 0,
        suggested_font_px: 16.0,
    }];
    let bitmap = engine
        .render_text_overlay_bitmap(w, h, &items, &translator::font_provider::NoFontProvider)
        .expect("bitmap rendered");
    assert_eq!(bitmap.len(), (w as usize) * (h as usize) * 4);

    let alpha_at = |x: u32, y: u32| -> u8 { bitmap[((y * w + x) * 4 + 3) as usize] };

    // TL corner of the quad — outline pixel, should be opaque.
    assert!(alpha_at(100, 100) > 0, "expected non-zero alpha at quad TL");
    // Far outside the quad — should be untouched (alpha 0).
    assert_eq!(alpha_at(10, 10), 0, "expected alpha=0 outside quad");
    assert_eq!(alpha_at(300, 300), 0, "expected alpha=0 outside quad");
}

#[test]
fn bitmap_pipeline_round_trip_under_translation() {
    // End-to-end check that the bitmap pipeline aligns with the rest
    // of the system: apply a known H to a real photo, track, recover
    // H', then verify that projecting overlay corners through H'
    // matches projecting through the known H within a small pixel
    // budget. This is the canonical pixel-correctness check for the
    // bitmap+coord stack.
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let anchor_id = engine.acquire_now(&gray, 1_000).expect("acquire");

    let canonical_quad = [
        (140.0_f32, 220.0_f32),
        (260.0_f32, 220.0_f32),
        (260.0_f32, 270.0_f32),
        (140.0_f32, 270.0_f32),
    ];
    let overlay = CanonicalOverlay {
        id: 7,
        quad: canonical_quad,
        payload: String::new(),
    };
    assert!(engine.set_overlays(anchor_id, vec![overlay]));

    let known = [1.0, 0.0, 11.0, 0.0, 1.0, -7.0, 0.0, 0.0, 1.0];
    let warped = warp_with_h(&gray, &known);
    let cmd = engine.process_frame(&warped, 2_000);
    let recovered = match cmd {
        TrackerCommand::Locked { homography, .. } => homography,
        other => panic!("expected Locked, got {:?}", other),
    };

    let projected = engine.project_overlays(anchor_id, &recovered);
    assert_eq!(projected.len(), 1);
    let p = &projected[0];
    for i in 0..4 {
        let (cx, cy) = canonical_quad[i];
        let (ex, ey) =
            translator::homography::project(&known, cx, cy).expect("known projection valid");
        let (rx, ry) = p.quad[i];
        let err = ((rx - ex).powi(2) + (ry - ey).powi(2)).sqrt();
        assert!(
            err < 2.0,
            "overlay corner {} drift: recovered=({}, {}) expected=({}, {}) err={}",
            i,
            rx,
            ry,
            ex,
            ey,
            err
        );
    }
}

fn h_rotate_about(cx: f32, cy: f32, theta: f32) -> [f32; 9] {
    let (s, c) = theta.sin_cos();
    let t_pos = [1.0, 0.0, cx, 0.0, 1.0, cy, 0.0, 0.0, 1.0];
    let t_neg = [1.0, 0.0, -cx, 0.0, 1.0, -cy, 0.0, 0.0, 1.0];
    let r = [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0];
    let m = mat3_mul_test(&r, &t_neg);
    mat3_mul_test(&t_pos, &m)
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
fn assert_anchor_id_nonzero(_id: AnchorId) {}

/// v0 chain test: force a sequence of handoffs by configuring the
/// trigger to fire on virtually any motion, pan back to the original
/// frame, and verify the engine re-locks onto the root anchor without
/// a re-acquire. Also verifies that the externally-emitted homography
/// projects overlays correctly through the chain.
#[test]
fn anchor_chain_persists_overlays_across_handoffs() {
    let mut cfg = test_engine_config();
    // Force handoff on essentially any motion: any frame with <95%
    // keypoint visibility OR <1000 inliers (always) triggers a spawn.
    cfg.handoff_min_inliers = 1000;
    cfg.handoff_min_visible_ratio = 0.95;
    cfg.handoff_cooldown_ns = 0;
    let mut engine = LivePlanarEngine::new(cfg);
    let gray = load_gray("book.jpg", 480);
    let root_id = engine.acquire_now(&gray, 1_000_000).expect("acquire");
    assert_eq!(engine.current_anchor(), Some(root_id));

    // Attach an overlay to the root that we'll project through the
    // chain to verify correctness at every hop.
    let canonical_quad = [
        (140.0_f32, 220.0),
        (260.0, 220.0),
        (260.0, 270.0),
        (140.0, 270.0),
    ];
    let overlay = CanonicalOverlay {
        id: 99,
        quad: canonical_quad,
        payload: "anchor".to_string(),
    };
    assert!(engine.set_overlays(root_id, vec![overlay]));

    // Walk through 3 successive translations. Each frame's external
    // homography should equal the applied translation (because overlays
    // live in root coords and translations are exact).
    let translations: [[f32; 9]; 3] = [
        [1.0, 0.0, 6.0, 0.0, 1.0, -3.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 14.0, 0.0, 1.0, -8.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 22.0, 0.0, 1.0, -14.0, 0.0, 0.0, 1.0],
    ];
    let mut t_ns = 2_000_000_u64;
    for (i, h_known) in translations.iter().enumerate() {
        let warped = warp_with_h(&gray, h_known);
        let cmd = engine.process_frame(&warped, t_ns);
        let (anchor_id, homography) = match cmd {
            TrackerCommand::Locked {
                anchor_id,
                homography,
                ..
            } => (anchor_id, homography),
            other => panic!("hop {} expected Locked, got {:?}", i, other),
        };
        assert_eq!(
            anchor_id, root_id,
            "hop {}: externally-emitted anchor id should stay = root",
            i
        );
        // Project the root overlay through the chain-composed H and
        // compare against the known projection. Allow a few px of
        // chain drift — RANSAC residual compounds per hop.
        let projected = engine.project_overlays(root_id, &homography);
        assert_eq!(projected.len(), 1);
        for c in 0..4 {
            let (cx, cy) = canonical_quad[c];
            let (ex, ey) =
                translator::homography::project(h_known, cx, cy).expect("known projection valid");
            let (rx, ry) = projected[0].quad[c];
            let err = ((rx - ex).powi(2) + (ry - ey).powi(2)).sqrt();
            assert!(
                err < 4.0,
                "hop {}: overlay corner {} drift {} (recovered=({},{}) expected=({},{}))",
                i,
                c,
                err,
                rx,
                ry,
                ex,
                ey
            );
        }
        t_ns += 1_000_000;
    }
    // Cache should have grown: root + at least one handoff anchor.
    assert!(
        engine.cache_len() >= 2,
        "expected chain growth, cache_len={}",
        engine.cache_len()
    );

    // Now pan all the way back to the original frame. The engine
    // should re-lock on the root anchor (potentially via fallback
    // through cached anchors). No re-acquire.
    let cmd = engine.process_frame(&gray, t_ns + 100_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id,
            homography,
            ..
        } => {
            assert_eq!(anchor_id, root_id, "come-back should re-emit root id");
            // Identity homography means we're back at root canonical coords.
            let projected = engine.project_overlays(root_id, &homography);
            for c in 0..4 {
                let (cx, cy) = canonical_quad[c];
                let (rx, ry) = projected[0].quad[c];
                let err = ((rx - cx).powi(2) + (ry - cy).powi(2)).sqrt();
                assert!(err < 4.0, "come-back: overlay corner {} drift {}", c, err);
            }
        }
        other => panic!("expected Locked on come-back, got {:?}", other),
    }
}

#[test]
fn handoff_children_inherit_root_quadrant() {
    use translator::coords::Quadrant;
    let mut cfg = test_engine_config();
    // Same recipe as `anchor_chain_persists_overlays_across_handoffs`:
    // force a handoff on essentially any frame motion.
    cfg.handoff_min_inliers = 1000;
    cfg.handoff_min_visible_ratio = 0.95;
    cfg.handoff_cooldown_ns = 0;
    let mut engine = LivePlanarEngine::new(cfg);
    let gray = load_gray("book.jpg", 480);
    let root_id = engine
        .acquire_now_with_orientation(&gray, &[], 0, 1_000_000, Some(Quadrant::R180))
        .expect("acquire with orientation");

    // Drive a couple of translated frames so a handoff fires.
    let translations: [[f32; 9]; 2] = [
        [1.0, 0.0, 6.0, 0.0, 1.0, -3.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 14.0, 0.0, 1.0, -8.0, 0.0, 0.0, 1.0],
    ];
    let mut t_ns = 2_000_000_u64;
    for h_known in translations {
        let warped = warp_with_h(&gray, &h_known);
        let cmd = engine.process_frame(&warped, t_ns);
        match cmd {
            TrackerCommand::Locked {
                anchor_id,
                canonical_rotation,
                ..
            } => {
                assert_eq!(
                    anchor_id, root_id,
                    "external anchor id stays = root through handoff",
                );
                assert_eq!(
                    canonical_rotation,
                    Quadrant::R180,
                    "child anchor inherited root quadrant",
                );
            }
            other => panic!("expected Locked, got {:?}", other),
        }
        t_ns += 1_000_000;
    }
    // Sanity: we did spawn at least one child.
    assert!(
        engine.cache_len() >= 2,
        "expected chain growth, cache_len={}",
        engine.cache_len()
    );
}

#[test]
fn acquire_with_orientation_stores_quadrant_on_root() {
    use translator::coords::Quadrant;
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    let id = engine
        .acquire_now_with_orientation(&gray, &[], 0, 1_000_000, Some(Quadrant::R270))
        .expect("acquire with orientation");
    let cmd = engine.process_frame(&gray, 2_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id,
            canonical_rotation,
            ..
        } => {
            assert_eq!(anchor_id, id);
            assert_eq!(canonical_rotation, Quadrant::R270);
        }
        other => panic!("expected Locked, got {:?}", other),
    }
}

#[test]
fn no_estimator_consensus_falls_back_to_default() {
    use translator::coords::Quadrant;
    // Engine configured with a non-R0 default (e.g. phone-portrait).
    let mut cfg = test_engine_config();
    cfg.default_canonical_quadrant = Quadrant::R270;
    let mut engine = LivePlanarEngine::new(cfg);
    let gray = load_gray("book.jpg", 480);
    let id = engine
        .acquire_now_with_orientation(&gray, &[], 0, 1_000_000, None)
        .expect("acquire without consensus");
    let cmd = engine.process_frame(&gray, 2_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id,
            canonical_rotation,
            ..
        } => {
            assert_eq!(anchor_id, id);
            assert_eq!(canonical_rotation, Quadrant::R270);
        }
        other => panic!("expected Locked, got {:?}", other),
    }
}

#[test]
fn no_consensus_falls_back_to_previous_known() {
    use translator::coords::Quadrant;
    let mut engine = LivePlanarEngine::new(test_engine_config());
    let gray = load_gray("book.jpg", 480);
    // First acquire establishes R90 as last_known.
    let _first = engine
        .acquire_now_with_orientation(&gray, &[], 0, 1_000_000, Some(Quadrant::R90))
        .expect("first acquire");
    assert_eq!(engine.last_known_quadrant(), Quadrant::R90);
    // Second acquire with no consensus inherits R90 (not the default R0).
    let second = engine
        .acquire_now_with_orientation(&gray, &[], 0, 2_000_000, None)
        .expect("second acquire");
    let cmd = engine.process_frame(&gray, 3_000_000);
    match cmd {
        TrackerCommand::Locked {
            anchor_id,
            canonical_rotation,
            ..
        } => {
            assert_eq!(anchor_id, second);
            assert_eq!(canonical_rotation, Quadrant::R90);
        }
        other => panic!("expected Locked, got {:?}", other),
    }
}

fn ekf_engine_config() -> EngineConfig {
    let mut cfg = test_engine_config();
    cfg.use_h_ekf = true;
    cfg
}

/// First Locked frame after acquire emits the raw RANSAC homography
/// unchanged — the EKF has no history yet and must initialise from
/// the current measurement. Anything else would mean the filter is
/// silently biasing the first frame toward an arbitrary default.
#[test]
fn h_ekf_first_frame_after_acquire_is_passthrough() {
    let mut ekf_cfg = ekf_engine_config();
    ekf_cfg.use_h_ekf = false;
    let mut baseline = LivePlanarEngine::new(ekf_cfg);
    let mut ekf_engine = LivePlanarEngine::new(ekf_engine_config());
    let gray = load_gray("book.jpg", 480);
    let translated = warp_with_h(&gray, &[1.0, 0.0, 7.0, 0.0, 1.0, -3.0, 0.0, 0.0, 1.0]);

    baseline
        .acquire_now(&gray, 1_000_000)
        .expect("baseline acquire");
    ekf_engine
        .acquire_now(&gray, 1_000_000)
        .expect("ekf acquire");
    let baseline_cmd = baseline.process_frame(&translated, 2_000_000);
    let ekf_cmd = ekf_engine.process_frame(&translated, 2_000_000);
    let baseline_h = match baseline_cmd {
        TrackerCommand::Locked { homography, .. } => homography,
        other => panic!("baseline expected Locked, got {:?}", other),
    };
    let ekf_h = match ekf_cmd {
        TrackerCommand::Locked { homography, .. } => homography,
        other => panic!("ekf expected Locked, got {:?}", other),
    };
    for i in 0..9 {
        assert!(
            (baseline_h[i] - ekf_h[i]).abs() < 1e-6,
            "first frame h[{}] differs: baseline={} ekf={}",
            i,
            baseline_h[i],
            ekf_h[i]
        );
    }
}

/// With the EKF active, a sequence of identical frames against the
/// same anchor must keep the emitted homography arbitrarily close to
/// the per-frame fit — the underlying observation is the same every
/// frame, so the filter's steady state must match it. Catches the
/// regression where the EKF silently drifts away from the measurement
/// stream (sign error in the Jacobian, broken covariance update,
/// stale process noise, etc.).
#[test]
fn h_ekf_steady_state_tracks_constant_h() {
    let mut engine = LivePlanarEngine::new(ekf_engine_config());
    let gray = load_gray("book.jpg", 480);
    let known = [1.0, 0.0, 9.0, 0.0, 1.0, -5.0, 0.0, 0.0, 1.0];
    let warped = warp_with_h(&gray, &known);
    let anchor_id = engine.acquire_now(&gray, 1_000_000).expect("acquire");
    // Attach an overlay we can project through the emitted H so the
    // assertion is in display-space pixels rather than raw matrix
    // entries (which obscures whether the drift is geometrically
    // meaningful).
    let quad = [
        (140.0_f32, 220.0),
        (260.0, 220.0),
        (260.0, 270.0),
        (140.0, 270.0),
    ];
    let overlay = CanonicalOverlay {
        id: 1,
        quad,
        payload: String::new(),
    };
    assert!(engine.set_overlays(anchor_id, vec![overlay]));

    let mut worst_after_warmup = 0.0_f32;
    for frame in 1..=20 {
        let cmd = engine.process_frame(&warped, (1 + frame) * 1_000_000);
        let h = match cmd {
            TrackerCommand::Locked { homography, .. } => homography,
            other => panic!("frame {} expected Locked, got {:?}", frame, other),
        };
        let projected = engine.project_overlays(anchor_id, &h);
        assert_eq!(projected.len(), 1);
        for i in 0..4 {
            let (cx, cy) = quad[i];
            let (ex, ey) = translator::homography::project(&known, cx, cy).unwrap();
            let (rx, ry) = projected[0].quad[i];
            let err = ((rx - ex).powi(2) + (ry - ey).powi(2)).sqrt();
            // Warm-up: the EKF's covariance starts at `EKF_P0_DEFAULT`
            // and shrinks over the first handful of measurements. The
            // first frame is the initialisation (raw fit, no filtering)
            // and frames 2-3 may still have appreciable disagreement
            // between the filter and the measurement; from frame 4
            // onward the steady state should be tight.
            if frame >= 4 && err > worst_after_warmup {
                worst_after_warmup = err;
            }
        }
    }
    assert!(
        worst_after_warmup < 2.0,
        "steady-state overlay corner error {} px should be < 2",
        worst_after_warmup
    );
}

/// EKF state is per-active-anchor. When the engine switches anchors
/// (force_acquire ⇒ new root id), the EKF must re-initialise rather
/// than feed the new anchor's measurements into the previous
/// anchor's covariance (which is in a different canonical frame).
#[test]
fn h_ekf_resets_on_anchor_change() {
    let mut engine = LivePlanarEngine::new(ekf_engine_config());
    let gray = load_gray("book.jpg", 480);
    let translated = warp_with_h(&gray, &[1.0, 0.0, 6.0, 0.0, 1.0, -2.0, 0.0, 0.0, 1.0]);

    let root_a = engine.acquire_now(&gray, 1_000_000).expect("acquire A");
    for frame in 1..=6 {
        let _ = engine.process_frame(&translated, (1 + frame) * 1_000_000);
    }
    // Force a fresh acquire — anchor changes.
    let root_b = engine.acquire_now(&gray, 50_000_000).expect("acquire B");
    assert_ne!(root_a, root_b, "force-acquire must produce a new anchor id");
    // First frame on new anchor: EKF must re-init, not blend with
    // anchor A's filter state.
    let cmd_a = engine.process_frame(&translated, 60_000_000);
    let h_a = match cmd_a {
        TrackerCommand::Locked {
            homography,
            anchor_id,
            ..
        } => {
            assert_eq!(anchor_id, root_b);
            homography
        }
        other => panic!("expected Locked on B, got {:?}", other),
    };
    // Same engine with EKF disabled would emit the raw fit on the
    // same first frame. The two should be ~equal because the EKF
    // initialises from the raw fit on first frame after anchor
    // change (the passthrough behaviour the per-frame-after-acquire
    // test asserts).
    let mut baseline_cfg = ekf_engine_config();
    baseline_cfg.use_h_ekf = false;
    let mut baseline = LivePlanarEngine::new(baseline_cfg);
    baseline.acquire_now(&gray, 1_000_000).expect("baseline A");
    for frame in 1..=6 {
        let _ = baseline.process_frame(&translated, (1 + frame) * 1_000_000);
    }
    baseline.acquire_now(&gray, 50_000_000).expect("baseline B");
    let cmd_b = baseline.process_frame(&translated, 60_000_000);
    let h_b = match cmd_b {
        TrackerCommand::Locked { homography, .. } => homography,
        other => panic!("baseline expected Locked, got {:?}", other),
    };
    for i in 0..9 {
        assert!(
            (h_a[i] - h_b[i]).abs() < 1e-5,
            "post-reset h[{}] differs from baseline: ekf={} baseline={}",
            i,
            h_a[i],
            h_b[i]
        );
    }
}
