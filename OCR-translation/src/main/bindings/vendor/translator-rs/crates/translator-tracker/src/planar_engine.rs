//! Lifecycle state machine + anchor LRU cache for the planar tracker.
//!
//! `planar_tracker` does pure per-frame detection / matching / fitting.
//! This module owns:
//!   - the Acquiring → Locked → Lost state machine (Phase E)
//!   - what counts as "scene change" / "refresh needed" (E + F)
//!   - the LRU cache of recently-seen anchors with their translated
//!     overlay sets, so flipping back to a known scene is instant (G)
//!
//! See `FUTURE_PLANAR_TRACKER.md` for the design rationale.

use std::sync::Arc;

use image::GrayImage;

use crate::coarse_tracker::{Correction, Lifecycle};
use crate::planar_tracker::{
    FrameFeatures, SceneAnchor, TrackResult, TrackerConfig, build_anchor, build_anchor_in_regions,
    compute_frame_features, track_against_anchor_with_features,
};
use translator_core::coords::Quadrant;
use translator_core::homography::{invert, mat3_mul, project};
use translator_core::homography_ekf::{EKF_Q_DEFAULT, EKF_R_DEFAULT, HomographyEkf};

#[cfg(feature = "image-render")]
use translator_core::ocr::{
    OrientedRect, OverlayLayoutHints, OverlayLayoutMode, PreparedImageOverlay, PreparedTextBlock,
    PreparedTextLine, Rect,
};
#[cfg(feature = "image-render")]
use translator_render::font_provider::FontProvider;
#[cfg(feature = "image-render")]
use translator_render::image_render::{RenderOptions, render_overlay};

/// Stable identifier for a captured scene. Increases monotonically; we
/// never reuse an id even after an anchor is evicted from the LRU
/// cache, so downstream caches can safely key on it.
pub type AnchorId = u64;

/// One overlay's geometry in *canonical-frame coordinates* (the frame
/// at which the anchor was acquired). The engine doesn't interpret
/// `payload` — it's opaque to Rust. Kotlin uses it to carry whatever
/// it needs (translated text, font hints, source bbox id).
#[derive(Clone, Debug)]
pub struct CanonicalOverlay {
    pub id: u64,
    /// Four corners (top-left, top-right, bottom-right, bottom-left)
    /// in canonical-frame pixel coordinates.
    pub quad: [(f32, f32); 4],
    pub payload: String,
}

/// One overlay's geometry in *current-frame coordinates* — what the
/// renderer actually draws. Output of `project_overlays`.
#[derive(Clone, Debug)]
pub struct OverlayProjection {
    pub id: u64,
    pub quad: [(f32, f32); 4],
    pub payload: String,
}

/// What the engine wants Kotlin to do this frame.
#[derive(Clone, Debug)]
pub enum TrackerCommand {
    /// No anchor; nothing to draw. Wait for `stable_required_ns` to
    /// elapse, then call `acquire_now`.
    Idle,
    /// We're inside the stable-frame quiet window. Same render hint as
    /// `Idle` (no overlays to project yet); useful to display a
    /// "Looking…" indicator.
    Acquiring,
    /// Locked on `anchor_id`. Project the anchor's canonical overlays
    /// through `homography` and render them. `is_new` is true the
    /// first frame after a fresh acquisition (Kotlin should run OCR
    /// then); false on subsequent frames or when we've snapped back to
    /// a cached anchor (skip OCR — Phase F caching benefit).
    /// `canonical_rotation` is the anchor's stored reading-direction
    /// quadrant (resolved at acquire-time by the textline-orientation
    /// estimator, then inherited by handoffs from their root).
    Locked {
        anchor_id: AnchorId,
        homography: [f32; 9],
        is_new: bool,
        inliers: usize,
        canonical_rotation: Quadrant,
    },
    /// We had `last_anchor_id` locked recently but lost the track.
    /// Kotlin can briefly hide overlays or extrapolate via IMU until a
    /// later frame re-locks or we time out and go back to `Idle`.
    Lost { last_anchor_id: AnchorId },
}

/// Engine tuning knobs not covered by `TrackerConfig`.
#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    pub tracker: TrackerConfig,
    /// LRU capacity for cached anchors.
    pub anchor_cache_size: usize,
    /// Min time between successive acquires (nanoseconds). Prevents
    /// thrashing when the user is still settling on a scene.
    pub acquire_cooldown_ns: u64,
    /// After this many consecutive `track == None` frames, transition
    /// Locked → Lost.
    pub lost_after_frames: u32,
    /// After this many additional frames in Lost without a re-lock,
    /// transition Lost → Idle and clear active anchor.
    pub give_up_after_frames: u32,
    /// Quiet-IMU period required before an Idle auto-acquire fires
    /// (nanoseconds).
    pub stable_required_ns: u64,
    /// Refresh trigger: if a Locked anchor is older than this and the
    /// scene is still locked, `should_refresh` will return true so
    /// Kotlin can re-run OCR to absorb new text.
    pub anchor_refresh_age_ns: u64,
    /// Spawn a new (handoff) anchor when the active anchor's per-frame
    /// RANSAC inlier count falls below this. Picked well above the
    /// wobble floor so the handoff homography fit is also stable.
    pub handoff_min_inliers: usize,
    /// Spawn a new (handoff) anchor when this fraction of the active
    /// anchor's keypoints fail to project inside the viewport — i.e.
    /// most features are about to leave the frame.
    pub handoff_min_visible_ratio: f32,
    /// Spawn a new (handoff) anchor when `|ln(approx_scale(H_anchor→view))|`
    /// exceeds this threshold. BRIEF descriptors are scale-variant; once
    /// the view's effective scale diverges past ~1.35× from acquire
    /// (`ln(1.35) ≈ 0.30`) per-keypoint Hamming matching starts losing
    /// inliers from descriptor drift before the inlier-count or
    /// visible-ratio gates trip. Catching the scale change earlier lets
    /// the handoff fit on still-co-visible features rather than at the
    /// bottom of an inlier cliff.
    pub handoff_scale_log_threshold: f32,
    /// Cooldown between successive handoffs. Prevents churn when
    /// inliers oscillate near the trigger threshold.
    pub handoff_cooldown_ns: u64,
    /// Inlier-discontinuity sanity gate: reject a new RANSAC fit when
    /// its inlier count drops below `sanity_gate_drop_ratio * EMA`. A
    /// drop this severe in one frame is almost always a wrong-basin
    /// fit or a descriptor-collapse event, not a real world change.
    pub sanity_gate_drop_ratio: f32,
    /// Only apply the sanity gate when the running EMA is at least
    /// this high. Below this, the lock is already marginal and a
    /// "discontinuity" carries less signal than noise.
    pub sanity_gate_min_ema: f32,
    /// Maximum number of consecutive frames for which the sanity gate
    /// will substitute a predicted H. After this, fall through and
    /// let the bad fit advance `frames_lost` so we can transition to
    /// Lost on sustained failure.
    pub sanity_gate_max_consecutive: u32,
    /// EMA smoothing factor for accepted inlier counts. Higher = more
    /// responsive to recent inlier counts; lower = more stable.
    pub inlier_ema_alpha: f32,
    /// Inlier count below which an accepted fit is considered
    /// "degraded" — the anchor's BRIEF descriptors are losing
    /// correspondences as perspective drifts, and RANSAC will start
    /// over-fitting to whichever pocket of features still matches.
    pub degraded_inlier_threshold: usize,
    /// After this many consecutive degraded frames (descriptor-only inliers
    /// below `degraded_inlier_threshold` while the engine is still accepting
    /// fits), force the engine back to Idle to re-acquire. With KLT-prepend
    /// supplying the per-frame fit's perspective constraint, descriptor
    /// inliers can dip well below the threshold transiently without the actual
    /// fit degrading — so this number is generous on purpose, only bounding
    /// genuine sustained matcher collapse.
    pub degraded_max_frames: u32,
    /// Quadrant used for a fresh anchor when the textline-orientation
    /// estimator can't reach consensus AND no previous anchor has ever
    /// produced one in this engine. Caller (Android, sim, tests) sets
    /// this per camera mount: phones held portrait with rear camera
    /// typically want `R270`; desktop sim with a synthetic flat image
    /// wants `R0`.
    pub default_canonical_quadrant: Quadrant,
    /// When the leaf active anchor switches (handoff or cached snap-
    /// back) the chain-composed `H_root→view` jumps because the new
    /// anchor's canonical alignment was fitted on a different frame.
    /// Blend the emitted H from the prior frame's value toward the
    /// natural new-anchor value over this many frames to mask the
    /// discontinuity. Zero disables. Real motion within a stable
    /// anchor still emits naturally — the blend is only triggered on
    /// anchor switches.
    pub anchor_switch_blend_frames: u32,
    /// Minimum corner-projection delta (on a 1000×1000 reference
    /// square) between the prior emitted H and the new anchor's
    /// natural H before a blend is started. Below this, the switch
    /// is small enough that smoothing isn't worth the extra latency.
    pub anchor_switch_blend_threshold_px: f32,
    /// Maximum chain depth allowed before handoff is refused and the
    /// engine falls back to Idle/re-acquire instead. Each handoff
    /// multiplies a small RANSAC fit error into the chain's
    /// `H_root→canonical` composition; on motion-heavy clips this
    /// accumulates into a visibly-drifting overlay after a few
    /// handoffs in a row. Capping the chain forces a fresh root
    /// before drift becomes structural.
    pub max_chain_depth: u32,
    /// Acceptance gate on the spawn-frame's H fit *quality* before
    /// allowing a handoff. The handoff bakes the spawn-frame H into
    /// `H_root→canonical_new`; if that fit is noisy, every downstream
    /// frame inherits the bias. Block the handoff when the spawn
    /// frame's median per-inlier residual exceeds this value.
    pub handoff_max_median_residual_px: f32,
    /// Acceptance gate on inlier *ratio* (descriptor_inliers /
    /// matches) at the spawn frame. A handoff fitted with low inlier
    /// ratio is likely in a wrong basin even if the absolute inlier
    /// count is healthy — block it.
    pub handoff_min_inlier_ratio: f32,
    /// Extended Kalman Filter on `H_anchor→view`. With it, the per-frame
    /// RANSAC fit is treated as a noisy observation: the EKF carries per-DoF
    /// covariance across frames so well-determined DoFs (translation under
    /// spread inliers) respond promptly while under-determined ones (the
    /// perspective entries `h6`/`h7`, which descriptor-only fits jitter on
    /// text scenes) stay anchored to the prior. Filtered only on the steady-
    /// state Locked→Locked branch; anchor-switch frames (acquire, handoff,
    /// cached-snap) emit the raw RANSAC fit because the EKF state is anchor-
    /// relative and the covariance from the previous anchor doesn't transfer
    /// through the chain. See `analysis.md` § "EKF on H".
    pub use_h_ekf: bool,
    /// Per-inlier measurement variance (σ² in px²) for the EKF. Default
    /// matches the RANSAC inlier residual gate interpreted as ≈ 2σ.
    pub h_ekf_r_var: f64,
    /// Enable chain-composition refinement after a handoff. The
    /// spawn frame's `h_root_to_canonical_new` is fitted from one
    /// frame of RANSAC; on subsequent frames the engine re-tracks
    /// the parent anchor against the current view and averages the
    /// resulting `h_root_to_canonical_new` candidates. Each candidate
    /// is frame-invariant on a static scene, so the running mean's
    /// variance drops as 1/√N. See `cleanup_plan.md` § 3.
    pub use_chain_refine: bool,
    /// Number of post-spawn frames over which the chain matrix is
    /// refined. After this many frames the running mean is frozen
    /// into `CachedAnchor.h_root_to_canonical`.
    pub chain_refine_frames: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            tracker: TrackerConfig::default(),
            // Capacity 1: drop the "page-flip" matching that would
            // re-lock onto an old anchor whenever its subset of
            // features still showed up in the new scene (e.g. half-page
            // anchor matching a full-page view). The brief-loss
            // recovery within a single anchor still works because the
            // current anchor lives in the cache.
            // v0 surface-map persistence-on-pan: keep up to 5 anchors
            // (1 root + up to 4 handoff anchors, or multiple distinct
            // acquired roots in the page-flip case). Per-anchor cost is
            // ~20 KB; the per-frame fallback (try other anchors when the
            // active one fails) is bounded by cache size × ~5 ms/anchor
            // = ~20 ms worst case at 5.
            anchor_cache_size: 5,
            acquire_cooldown_ns: 250_000_000, // 250 ms — fast re-acquire after loss
            // Drop to Lost quickly under bad fits so the overlay
            // hides as soon as the matcher loses confidence —
            // matches Google Translate's "shake → overlay
            // disappears → re-acquire when still" UX. Paired with
            // the H-delta cap that converts wrong-basin fits into
            // None returns, so violent shake reliably triggers a
            // Lost transition in ~150 ms instead of riding a
            // degenerate fit for half a second.
            lost_after_frames: 5, // ~150 ms @ 30 fps
            // Recovery budget: 30 frames (~1 s) before we go Idle so a
            // fresh acquire can fire. Bigger values make stuck-Lost
            // feel glacial when the user has clearly aimed at something
            // new; smaller values waste re-OCR on momentary blur.
            give_up_after_frames: 30,
            stable_required_ns: 200_000_000,       // 200 ms
            anchor_refresh_age_ns: 30_000_000_000, // 30 s
            handoff_min_inliers: 50,
            handoff_min_visible_ratio: 0.6,
            // Disabled (A/B): set absurdly high so `scale_changed`
            // never fires. Original 0.30 threshold (≈ 1.35× scale
            // change) was theoretically perspective-invariant via
            // `sqrt(|det|)`, but on real-world tilts where the
            // camera isn't perfectly centred on the surface the
            // affine det can still move 10-20% and spuriously fire
            // a handoff that creates a new full-frame anchor with
            // mismatched feature density. Bring back once we have a
            // perspective-stable scale measure (e.g. min singular
            // value of the upper-left 2×2 + bounds on max).
            handoff_scale_log_threshold: f32::INFINITY,
            handoff_cooldown_ns: 500_000_000, // 500 ms
            sanity_gate_drop_ratio: 0.3,
            sanity_gate_min_ema: 60.0,
            sanity_gate_max_consecutive: 3,
            inlier_ema_alpha: 0.2,
            degraded_inlier_threshold: 75,
            degraded_max_frames: 5,
            default_canonical_quadrant: Quadrant::R0,
            anchor_switch_blend_frames: 5,
            anchor_switch_blend_threshold_px: 15.0,
            max_chain_depth: 3,
            // Quality gates on the spawn-frame H fit. The handoff
            // bakes that fit into `H_root→canonical_new`; if it's
            // noisy the new chain inherits the bias permanently.
            // 1.5 px median residual and 0.4 inlier ratio are above
            // typical clean-fit values (median residual ~0.5-1 px;
            // ratio ~0.6-0.9 on stable scenes) but below the
            // wrong-basin / degraded regimes (residual ~2-3 px,
            // ratio ~0.2-0.3) we saw in earlier traces.
            handoff_max_median_residual_px: 1.5,
            handoff_min_inlier_ratio: 0.4,
            // Re-enabled after the prior-aware RANSAC scoring + short-circuit
            // collapsed the cadence-N ABAB basin-flip (~30 px corner swing) to
            // the residual per-tick refit noise (~2 px corner swing from
            // descriptor / KLT sub-pixel jitter on the same basin). The EKF's
            // per-DoF Q (slow h6/h7, fast translation) is the right shape for
            // attenuating that residual without lagging real motion.
            use_h_ekf: true,
            h_ekf_r_var: EKF_R_DEFAULT,
            use_chain_refine: true,
            chain_refine_frames: 10,
        }
    }
}

/// The full live-OCR tracker engine.
pub struct LivePlanarEngine {
    config: EngineConfig,
    cache: AnchorCache,
    state: EngineState,
    next_anchor_id: AnchorId,
    last_acquire_ns: u64,
    /// First timestamp at which IMU went quiet (None when moving).
    stable_since_ns: Option<u64>,
    /// Timestamp of the last handoff (anchor spawn from an existing
    /// chain). Used to throttle handoffs via `handoff_cooldown_ns`.
    last_spawn_ns: u64,
    /// History of accepted H + inlier EMA for the active anchor.
    /// Drives the inlier-discontinuity sanity gate.
    track_quality: TrackQualityState,
    /// Most recent quadrant the estimator confirmed (across all
    /// acquires, not just the active anchor). Used as fallback when a
    /// later acquire fails to reach consensus. Seeded from
    /// `EngineConfig.default_canonical_quadrant`.
    last_known_quadrant: Quadrant,
    /// Most recent TrackResult produced by the per-frame fit (after
    /// the sanity gate), paired with the leaf anchor its `inlier_pairs`
    /// are expressed in. The leaf id is captured here because
    /// `self.state.anchor_id` can flip to a new leaf via handoff
    /// *after* the fit lands, but the inlier pairs stay in the old
    /// leaf's canonical frame; `root_coord_seeds` must project through
    /// the source leaf's `h_root_to_canonical`, not the post-handoff
    /// active leaf's.
    last_track_result: Option<(AnchorId, TrackResult)>,
    /// Per-emission state used to blend `H_root→view` across leaf-
    /// anchor switches. Cleared on Lost/Idle so a re-acquire after
    /// loss doesn't lerp from a stale pre-loss H.
    emit_smooth: EmitSmoothState,
    /// EKF on `H_anchor→view` for the active anchor. Re-initialised on every
    /// anchor change (handoff / cached-snap / Lost / Idle) since the state is
    /// anchor-relative and the covariance from the previous anchor doesn't
    /// transfer through the chain. See [`EngineConfig::use_h_ekf`].
    h_ekf: Option<HomographyEkfTracker>,
    /// Running average of `h_root_to_canonical` for a freshly-spawned
    /// handoff anchor, populated from re-tracking the parent against
    /// the current view across the first N post-spawn frames. See
    /// [`EngineConfig::use_chain_refine`].
    chain_refine: Option<ChainRefineState>,
    /// Discriminant of the last `TrackerCommand` returned by
    /// `process_frame`. Used only to emit a debug line per change-of-
    /// kind — without it, debug logging in the Acquiring or Locked
    /// branches would fire every frame.
    last_cmd_kind: Option<&'static str>,
    /// Wall-clock-ish timestamp (frame timestamp) of the last
    /// successful `process_frame` call. Lets the debug log surface
    /// long gaps between calls (e.g. Kotlin pausing the pipeline)
    /// alongside engine-internal state changes.
    last_process_ns: Option<u64>,
    /// Cumulative counters for the inlier-discontinuity sanity gate
    /// and the corner-jump delta cap. Used to measure whether these
    /// gates still fire meaningfully now that the EKF provides
    /// temporal continuity at the measurement level. See
    /// `cleanup_plan.md` § 2.
    gate_counters: GateCounters,
    /// Sub-step timings from the most recent `process_frame` call.
    /// Reset at the start of each call; populated for the Locked
    /// branch (other branches are cheap and leave it at zeros).
    last_step_timings: StepTimings,
    /// Fixed-size pool the per-frame tracker work runs on. The whole
    /// `process_frame_inner` is wrapped in `tracker_pool.install`, so
    /// every `par_iter` it reaches (FAST detect, BRIEF describe,
    /// descriptor matching) fans out over these threads. Sized small on
    /// purpose: the steady-state tracking loop wants a couple of big
    /// cores, not the whole machine (the async OCR worker and the rest
    /// of the device need headroom).
    tracker_pool: Arc<rayon::ThreadPool>,
}

/// Sub-step timings from the most recent `process_frame` call.
/// Diagnostic surface for the prod per-window timing log — lets the
/// caller break a 20 ms tracker step down into the pieces (FAST/BRIEF,
/// match+RANSAC, KLT pyramid build, chain refinement) so regressions
/// can be attributed.
#[derive(Clone, Copy, Debug, Default)]
pub struct StepTimings {
    /// `compute_frame_features`: FAST detect + BRIEF describe.
    pub features_ms: f64,
    /// `track_against_anchor_with_features`: match + RANSAC + refit.
    pub track_ms: f64,
    /// `step_chain_refine`: re-track parent for chain-mean averaging.
    pub chain_refine_ms: f64,
    /// `try_cached_anchors`: FAST+BRIEF + descriptor match against cached
    /// anchors on non-Locked frames (Idle/Lost/handoff-alt). Lives outside
    /// the Locked-path timers above, so without this a frame spent here
    /// shows `tracker=` wall time with every sub-part at zero.
    pub cached_match_ms: f64,
}

/// Read-only snapshot of the engine's bandaid-gate fire counts.
/// Diagnostic surface for smoke runs.
#[derive(Clone, Copy, Debug, Default)]
pub struct GateCounters {
    /// Frames where the sanity gate substituted the previous H for
    /// the raw RANSAC fit (path 1: suspicious drop, budget available).
    pub sanity_gate_freeze: u32,
    /// Frames where the sanity gate rejected the fit outright (path 2:
    /// suspicious drop, no history or budget exhausted).
    pub sanity_gate_reject: u32,
    /// Brute fits dropped by `homography_delta_is_sane` (300 px
    /// corner-jump cap vs the prior accepted H).
    pub delta_cap_reject: u32,
    /// Brute fits dropped by `homography_is_sane` (out-of-frame /
    /// degenerate projected viewport).
    pub h_sanity_reject: u32,
}

/// Running mean of `h_root_to_canonical` for one freshly-spawned
/// handoff anchor. The chain matrix is static (a property of the
/// anchor's coordinate frame, not the current camera pose), so any
/// per-frame estimate is a noisy observation of the same underlying
/// quantity — averaging across N frames drops the variance as 1/√N.
///
/// Per frame the candidate is computed as
/// `h_root_to_new = inv(H_new_to_view) · H_parent_to_view ·
/// h_root_to_parent`, which is frame-invariant in the absence of
/// RANSAC noise.
struct HomographyEkfTracker {
    anchor_id: AnchorId,
    ekf: HomographyEkf,
}

struct ChainRefineState {
    new_anchor_id: AnchorId,
    parent_id: AnchorId,
    /// Snapshot of the parent's chain matrix at spawn time. Static
    /// for the duration of refinement even if the parent's matrix
    /// is itself being refined elsewhere (would only matter for
    /// chain depths > 1; with `max_chain_depth = 1` this is just
    /// the root's identity).
    parent_h_root_to_canonical: [f32; 9],
    frames_remaining: u32,
    /// Element-wise sum of canonicalised (`h22 = 1`) candidates,
    /// including the spawn-frame's seed value at index 0.
    chain_sum: [f64; 9],
    /// Number of candidates folded into `chain_sum`.
    count: u32,
}

#[derive(Clone, Debug, Default)]
struct EmitSmoothState {
    last_active_id: Option<AnchorId>,
    last_emitted_h: Option<[f32; 9]>,
    blend: Option<BlendState>,
}

#[derive(Clone, Debug)]
struct BlendState {
    base_h_at_switch: [f32; 9],
    target_active_id: AnchorId,
    total_frames: u32,
    /// 0-based, incremented after each emission within the blend.
    elapsed_frames: u32,
}

/// Worker threads in the per-frame tracker pool (FAST/BRIEF + matching).
/// Two is enough to roughly halve the parallel sub-steps without
/// starving the async OCR worker or the rest of the device.
const TRACKER_POOL_THREADS: usize = 4;

/// Single-step history of accepted `H_anchor→view` for the active anchor,
/// plus a running EMA of accepted inlier counts. Drives the inlier-
/// discontinuity sanity gate (rejects catastrophic single-frame drops
/// that are usually wrong-basin fits or descriptor collapse) and
/// supplies the "freeze" baseline (`h_prev`) the gate substitutes when
/// it suppresses a suspicious fit.
#[derive(Clone, Debug)]
struct TrackQualityState {
    h_prev: Option<[f32; 9]>,
    anchor_id: Option<AnchorId>,
    inlier_ema: Option<f32>,
    suspicious_frames: u32,
    /// Consecutive accepted frames whose inlier count fell below
    /// `degraded_inlier_threshold`. When this exceeds
    /// `degraded_max_frames`, the engine forces a re-acquire rather
    /// than continuing to track a bleeding anchor.
    degraded_frames: u32,
    /// Consecutive frames the sanity gate accepted normally (path 3).
    /// Reset on freeze (path 1) and reject (path 2). Used to defer
    /// handoff right after a freeze sequence: the first post-freeze
    /// accepted fit is often wrong-basin, and we don't want to bake
    /// that into a new anchor's canonical alignment.
    consecutive_clean_frames: u32,
}

impl TrackQualityState {
    fn new() -> Self {
        Self {
            h_prev: None,
            anchor_id: None,
            inlier_ema: None,
            suspicious_frames: 0,
            degraded_frames: 0,
            consecutive_clean_frames: 0,
        }
    }
    fn reset(&mut self) {
        *self = Self::new();
    }
    /// Drops history when the anchor changes — the H coordinate system
    /// is anchor-relative.
    fn ensure_anchor(&mut self, anchor_id: AnchorId) {
        if self.anchor_id != Some(anchor_id) {
            *self = Self {
                anchor_id: Some(anchor_id),
                ..Self::new()
            };
        }
    }
    fn push_h(&mut self, h: [f32; 9]) {
        self.h_prev = Some(h);
    }
    fn update_ema(&mut self, inliers: f32, alpha: f32) {
        self.inlier_ema = Some(match self.inlier_ema {
            Some(prev) => prev * (1.0 - alpha) + inliers * alpha,
            None => inliers,
        });
    }
    /// Reset the EMA and substitution budget after the sanity gate
    /// rejects a suspicious fit, but **keep `h_prev`** so the next
    /// suspicious frame can still freeze to the last accepted H.
    /// Clearing EMA prevents a stale high baseline from making every
    /// subsequent fit look suspicious (which produced multi-frame
    /// reject cascades).
    fn reset_ema_and_budget(&mut self) {
        self.inlier_ema = None;
        self.suspicious_frames = 0;
    }
}

#[derive(Clone, Debug)]
enum EngineState {
    Idle,
    Locked {
        anchor_id: AnchorId,
        frames_lost: u32,
        last_homography: [f32; 9],
    },
    Lost {
        last_anchor_id: AnchorId,
        frames_lost: u32,
    },
}

struct CachedAnchor {
    anchor: SceneAnchor,
    /// Populated only on root anchors (those created by `acquire_now`).
    /// Handoff anchors carry empty overlays — their root holds them.
    overlays: Vec<CanonicalOverlay>,
    /// The root this anchor projects through. `root_id == self_id` for
    /// roots; for handoff descendants, points at the root in whose
    /// canonical frame overlays live.
    root_id: AnchorId,
    /// Homography mapping root canonical → this anchor's canonical
    /// frame. Identity for roots. `H_root_to_canonical · root_point`
    /// gives the equivalent point in this anchor's canonical coords.
    h_root_to_canonical: [f32; 9],
    created_at_ns: u64,
    last_locked_ns: u64,
    /// Reading-direction quadrant in the camera frame at acquire time.
    /// Roots get this from the estimator (with fallback to
    /// `last_known_quadrant`). Handoff children inherit from their
    /// root.
    canonical_rotation: Quadrant,
    /// Number of handoffs between this anchor and its chain root.
    /// 0 for roots; +1 per handoff spawn. Used to cap chain length —
    /// each handoff multiplies a small fit-error into `h_root_to_canonical`
    /// and deep chains visibly drift on motion-heavy clips. When the
    /// active anchor's depth reaches `max_chain_depth`, the engine
    /// stops handing off and falls back to Idle/re-acquire instead.
    chain_depth: u32,
}

/// Hand-rolled LRU. Capacity is small (≤5 in production) so a Vec keyed
/// in MRU-first order is fine; not worth pulling in a crate. Insertions
/// at the front, evictions from the back.
struct AnchorCache {
    capacity: usize,
    entries: Vec<(AnchorId, CachedAnchor)>,
}

impl AnchorCache {
    fn new(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            capacity: cap,
            entries: Vec::with_capacity(cap),
        }
    }

    fn insert(&mut self, id: AnchorId, entry: CachedAnchor) {
        self.entries.insert(0, (id, entry));
        while self.entries.len() > self.capacity {
            self.entries.pop();
        }
    }

    fn touch(&mut self, id: AnchorId) {
        let idx = match self.entries.iter().position(|(k, _)| *k == id) {
            Some(i) => i,
            None => return,
        };
        if idx == 0 {
            return;
        }
        let entry = self.entries.remove(idx);
        self.entries.insert(0, entry);
    }

    fn get(&self, id: AnchorId) -> Option<&CachedAnchor> {
        self.entries
            .iter()
            .find_map(|(k, v)| if *k == id { Some(v) } else { None })
    }

    fn get_mut(&mut self, id: AnchorId) -> Option<&mut CachedAnchor> {
        self.entries
            .iter_mut()
            .find_map(|(k, v)| if *k == id { Some(v) } else { None })
    }

    fn ids_mru(&self) -> Vec<AnchorId> {
        self.entries.iter().map(|(k, _)| *k).collect()
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl LivePlanarEngine {
    pub fn new(config: EngineConfig) -> Self {
        let cache_size = config.anchor_cache_size;
        let default_quadrant = config.default_canonical_quadrant;
        let tracker_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(TRACKER_POOL_THREADS)
                .thread_name(|i| format!("planar-trk-{i}"))
                .build()
                .expect("failed to build planar tracker thread pool"),
        );
        Self {
            config,
            cache: AnchorCache::new(cache_size),
            state: EngineState::Idle,
            next_anchor_id: 1,
            last_acquire_ns: 0,
            stable_since_ns: None,
            last_spawn_ns: 0,
            track_quality: TrackQualityState::new(),
            last_known_quadrant: default_quadrant,
            last_track_result: None,
            emit_smooth: EmitSmoothState::default(),
            h_ekf: None,
            chain_refine: None,
            last_cmd_kind: None,
            last_process_ns: None,
            gate_counters: GateCounters::default(),
            last_step_timings: StepTimings::default(),
            tracker_pool,
        }
    }

    /// Cumulative counts of bandaid-gate fire events since engine
    /// construction. Diagnostic surface — exposed for smoke-trace
    /// emission, never read by the engine itself.
    pub fn gate_counters(&self) -> GateCounters {
        self.gate_counters
    }

    /// Sub-step timings from the most recent `process_frame` call.
    /// Diagnostic surface for the per-window prod timing log.
    pub fn last_step_timings(&self) -> StepTimings {
        self.last_step_timings
    }

    /// Last accepted TrackResult from the per-frame fit, if any. Lets
    /// diagnostics & smoke harnesses log `matches`, `median_residual_px`
    /// alongside the inlier count already exposed via `TrackerCommand`.
    pub fn last_track_result(&self) -> Option<&TrackResult> {
        self.last_track_result.as_ref().map(|(_, r)| r)
    }

    /// Coarse rotation the engine last committed to (most recent
    /// successful acquire-time estimate, or `default_canonical_quadrant`
    /// if the estimator has never confirmed one).
    pub fn last_known_quadrant(&self) -> Quadrant {
        self.last_known_quadrant
    }

    /// Reading-direction quadrant stored on an anchor's chain root.
    /// Returns `None` if the anchor isn't cached anymore (LRU-evicted).
    /// Use this on the refresh path to populate
    /// `PostDetectInput.canonical_quadrant` for the active anchor.
    pub fn canonical_rotation_for(&self, anchor_id: AnchorId) -> Option<Quadrant> {
        let root_id = self.root_of(anchor_id);
        self.cache.get(root_id).map(|a| a.canonical_rotation)
    }

    /// Process one camera frame. Decides whether to track against the
    /// current anchor, fall back to a cached anchor, declare loss, or
    /// stay idle waiting for an acquire.
    pub fn process_frame(&mut self, gray: &GrayImage, timestamp_ns: u64) -> TrackerCommand {
        self.run_inner(gray, timestamp_ns, None, &[])
    }

    /// Async-H entry: run the per-frame relocalization with the CoarseTracker's
    /// pose as the guided-match prior + KLT seeds as RANSAC-anchoring extra
    /// correspondences (in root coords), and return a [`Correction`]
    /// (root→view pose + root-coord seeds + lifecycle) for the CoarseTracker
    /// to weave/snap. `frame_idx` tags the correction so the weave can compose
    /// it onto the coarse pose for the frame it was computed from. See
    /// `async_h_design.md`.
    pub fn relocalize(
        &mut self,
        gray: &GrayImage,
        coarse_prior: Option<[f32; 9]>,
        coarse_seeds: &[(f32, f32, f32, f32)],
        frame_idx: u64,
        timestamp_ns: u64,
    ) -> Correction {
        let cmd = self.run_inner(gray, timestamp_ns, coarse_prior, coarse_seeds);
        self.command_to_correction(cmd, frame_idx)
    }

    /// `(root_x, root_y, view_x, view_y)` for the last fit's inliers: the fit's
    /// anchor side is in the *source* leaf's canonical coords (the leaf the fit
    /// was computed against, captured when `last_track_result` was stored —
    /// not necessarily the currently-active leaf, which may have moved on via
    /// handoff). Map through that leaf's `inv(h_root_to_canonical)` so the
    /// CoarseTracker (which works purely in root coords) can track them
    /// forward.
    fn root_coord_seeds(&self) -> Vec<(f32, f32, f32, f32)> {
        let Some((source_leaf, r)) = self.last_track_result.as_ref() else {
            return Vec::new();
        };
        if !matches!(self.state, EngineState::Locked { .. }) {
            return Vec::new();
        }
        let h_rtc = self
            .cache
            .get(*source_leaf)
            .map(|a| a.h_root_to_canonical)
            .unwrap_or(IDENTITY);
        let inv = invert(&h_rtc).unwrap_or(IDENTITY);
        r.inlier_pairs
            .iter()
            .map(|&(lx, ly, vx, vy)| {
                let (rx, ry) = project(&inv, lx, ly).unwrap_or((lx, ly));
                (rx, ry, vx, vy)
            })
            .collect()
    }

    fn command_to_correction(&self, cmd: TrackerCommand, frame_idx: u64) -> Correction {
        let base = Correction {
            frame_idx,
            lifecycle: Lifecycle::Lost,
            refinement_h: IDENTITY,
            seeds: Vec::new(),
            root_id: 0,
            canonical_rotation: Quadrant::default(),
            inliers: 0,
        };
        match cmd {
            TrackerCommand::Locked {
                anchor_id: root_id,
                homography,
                inliers,
                canonical_rotation,
                ..
            } => Correction {
                lifecycle: Lifecycle::Locked,
                refinement_h: homography,
                seeds: self.root_coord_seeds(),
                root_id,
                canonical_rotation,
                inliers,
                ..base
            },
            // Acquiring is the engine asking for a fresh detect/recognize; the
            // pipeline satisfies it via the rgb-request seam.
            TrackerCommand::Acquiring => Correction {
                lifecycle: Lifecycle::ReAcquire,
                ..base
            },
            // Lost during the loss-hide grace still needs the chain root so the
            // compositor can find the anchor's overlay canvas while the pipeline
            // projects through `sm.h` — otherwise `overlay_count` drops to 0 and
            // the overlay vanishes for the Lost frame even though grace returns
            // a valid H.
            TrackerCommand::Lost { last_anchor_id } => Correction {
                lifecycle: Lifecycle::Lost,
                root_id: last_anchor_id,
                ..base
            },
            // Idle: engine gave up; grace will exhaust, no need to preserve an
            // anchor reference here.
            TrackerCommand::Idle => base,
        }
    }

    fn run_inner(
        &mut self,
        gray: &GrayImage,
        timestamp_ns: u64,
        prior: Option<[f32; 9]>,
        coarse_seeds: &[(f32, f32, f32, f32)],
    ) -> TrackerCommand {
        // Surface long gaps between calls — the engine being silent
        // for many frames usually means Kotlin paused the pipeline
        // (AF scan, surface lifecycle, etc.) and is the most likely
        // cause of UI "Locked → IDLE → Locked" flickers that have no
        // corresponding state transition in the engine's own logs.
        if let Some(prev_ns) = self.last_process_ns {
            let gap_ns = timestamp_ns.saturating_sub(prev_ns);
            if gap_ns > 200_000_000 {
                log::debug!(
                    "[engine] process_frame: {:.0} ms gap since last call (Kotlin pause? AF scan?)",
                    gap_ns as f64 / 1_000_000.0,
                );
            }
        }
        self.last_process_ns = Some(timestamp_ns);
        // Run the whole per-frame pipeline inside the fixed-size pool so
        // every `par_iter` it reaches (detect/describe/match) fans out
        // over the tracker threads rather than the global rayon pool.
        let pool = Arc::clone(&self.tracker_pool);
        let cmd =
            pool.install(|| self.process_frame_inner(gray, timestamp_ns, prior, coarse_seeds));
        let kind = match &cmd {
            TrackerCommand::Idle => "Idle",
            TrackerCommand::Acquiring => "Acquiring",
            TrackerCommand::Locked { .. } => "Locked",
            TrackerCommand::Lost { .. } => "Lost",
        };
        if self.last_cmd_kind != Some(kind) {
            let from = self.last_cmd_kind.unwrap_or("(start)");
            log::debug!("[engine] emit {from} → {kind}");
            self.last_cmd_kind = Some(kind);
        }
        if matches!(cmd, TrackerCommand::Idle) {
            self.track_quality.reset();
        }
        cmd
    }

    fn process_frame_inner(
        &mut self,
        gray: &GrayImage,
        timestamp_ns: u64,
        prior: Option<[f32; 9]>,
        coarse_seeds: &[(f32, f32, f32, f32)],
    ) -> TrackerCommand {
        if self.stable_since_ns.is_none() {
            self.stable_since_ns = Some(timestamp_ns);
        }
        self.last_track_result = None;
        self.last_step_timings = StepTimings::default();
        match self.state.clone() {
            EngineState::Idle => {
                // Even when "Idle", we still try matching against cached
                // anchors. If the user picked up a previously-seen scene,
                // we want to snap back to it without forcing a new acquire.
                if !self.cache.is_empty() {
                    if let Some((id, result)) = self.try_cached_anchors_timed(gray, None) {
                        self.last_track_result = Some((id, result.clone()));
                        log::debug!(
                            "[engine] Idle → Locked via cached anchor leaf={id} (inliers={} desc={})",
                            result.inliers,
                            result.descriptor_inliers,
                        );
                        self.transition_to_locked(id, &result, timestamp_ns, false);
                        let (root_id, h_root_to_view) =
                            self.chain_homography(id, &result.homography);
                        let canonical_rotation = self.quadrant_for_active(id);
                        return self.emit_locked(
                            root_id,
                            id,
                            h_root_to_view,
                            false,
                            result.inliers,
                            canonical_rotation,
                        );
                    }
                }
                self.reset_emit_smooth();
                if self.is_stable_enough(timestamp_ns) {
                    TrackerCommand::Acquiring
                } else {
                    TrackerCommand::Idle
                }
            }
            EngineState::Locked {
                anchor_id,
                frames_lost,
                last_homography,
            } => {
                self.track_quality.ensure_anchor(anchor_id);
                // In Locked state we apply hysteresis: a lower inlier
                // bar to *keep* the lock than to acquire it. Avoids
                // per-frame Locked↔Lost flicker when inliers wander
                // around the acquire threshold. Pair with the
                // IMU-derived prior (if any) to short-circuit RANSAC.
                //
                // Fallback prior: when no external `prior` is provided
                // (e.g. IMU bypassed), seed RANSAC with the previous
                // frame's `H_anchor→view`. Without a seed, RANSAC
                // starts from random 4-point samples; on scenes with
                // repetitive features (e.g. pages of text), ~20% of
                // those random seeds suggest a *flipped* homography
                // and RANSAC can then confirm that flip from other
                // wrong correspondences in the same basin — visible
                // as "UI floating inverted in the middle of the air"
                // with healthy inlier counts (107+) even on a static
                // scene. The previous-frame H is the most informed
                // seed we can give RANSAC at zero extra cost; it's
                // essentially what the IMU prior reduces to when
                // rotation is small.
                let keep_min = self.config.tracker.min_inliers_keep_locked;
                let seed_prior = prior.or(Some(last_homography));
                // Per-frame KLT lives in the CoarseTracker (async-H split). The
                // engine is now descriptor-only, guided by the CoarseTracker's
                // pose as `seed_prior`. Compute features once and reuse them
                // for both the main track and the chain-refinement re-track.
                let t_features = std::time::Instant::now();
                let features = compute_frame_features(gray, &self.config.tracker);
                self.last_step_timings.features_ms = t_features.elapsed().as_secs_f64() * 1000.0;
                let t_track = std::time::Instant::now();
                let (brute_result, dims) = match (self.cache.get(anchor_id), features.as_ref()) {
                    (Some(a), Some(features)) => {
                        let dims = a.anchor.image_dims;
                        // KLT-prepend: the CoarseTracker's seeds are
                        // `(root_pt, view_pt)`; transform the model side
                        // through this leaf's chain matrix into leaf-canonical
                        // coords so they line up with the anchor's
                        // descriptors. Spatial spread (KLT tracks the previous
                        // anchor inliers, which were already spread by
                        // construction) anchors h6/h7 against the clustered
                        // descriptor matches under perspective change.
                        let extras: Vec<(f32, f32, f32, f32)> = coarse_seeds
                            .iter()
                            .filter_map(|&(rx, ry, vx, vy)| {
                                project(&a.h_root_to_canonical, rx, ry)
                                    .map(|(lx, ly)| (lx, ly, vx, vy))
                            })
                            .collect();
                        let r = track_against_anchor_with_features(
                            &a.anchor,
                            features,
                            &self.config.tracker,
                            keep_min,
                            seed_prior,
                            &extras,
                        );
                        (r, dims)
                    }
                    (Some(a), None) => (None, a.anchor.image_dims),
                    (None, _) => (None, (0, 0)),
                };
                self.last_step_timings.track_ms = t_track.elapsed().as_secs_f64() * 1000.0;
                let result = match brute_result {
                    None => {
                        log::debug!("[engine] brute force returned None (matcher failed)");
                        None
                    }
                    Some(t) => {
                        let raw_inliers = t.inliers;
                        let h_ok = homography_is_sane(&t.homography, dims.0, dims.1);
                        let delta_ok = homography_delta_is_sane(
                            &t.homography,
                            &last_homography,
                            dims.0,
                            dims.1,
                        );
                        if !h_ok {
                            self.gate_counters.h_sanity_reject =
                                self.gate_counters.h_sanity_reject.saturating_add(1);
                        }
                        if h_ok && !delta_ok {
                            self.gate_counters.delta_cap_reject =
                                self.gate_counters.delta_cap_reject.saturating_add(1);
                        }
                        if !h_ok || !delta_ok {
                            log::debug!(
                                "[engine] brute force fit rejected by validate() (insane H or large delta vs last_homography); raw inliers={}",
                                raw_inliers
                            );
                            None
                        } else {
                            Some(t)
                        }
                    }
                };
                // Capture the raw H pre-gate so we can detect a sanity-gate
                // freeze (gate substituted `h_prev` for the suspicious fit):
                // the substituted H is not a fresh observation, so the EKF
                // should predict only — not fold the bad-fit inlier pairs in
                // as measurements.
                let pre_gate_h = result.as_ref().map(|r| r.homography);
                let result = result.and_then(|r| self.apply_sanity_gate(r));
                if let Some(mut r) = result {
                    if self.config.use_h_ekf {
                        let frozen =
                            pre_gate_h.map_or(false, |pre| !h_elementwise_eq(&pre, &r.homography));
                        let new_h = if frozen {
                            self.apply_h_ekf(anchor_id, r.homography, &[])
                        } else {
                            self.apply_h_ekf(anchor_id, r.homography, &r.inlier_pairs)
                        };
                        r.homography = new_h;
                    }
                    // Chain-composition refinement: re-track the
                    // parent anchor against the current frame and
                    // fold the implied `h_root_to_canonical_new` into
                    // the running mean stored on the cached anchor.
                    // No-op when no refinement is active. Reuses the
                    // main path's `features` instead of running
                    // FAST+BRIEF a second time.
                    let t_chain = std::time::Instant::now();
                    self.step_chain_refine(features.as_ref(), anchor_id, &r.homography);
                    self.last_step_timings.chain_refine_ms =
                        t_chain.elapsed().as_secs_f64() * 1000.0;
                    // Source leaf = `anchor_id` (the fit's anchor side is in
                    // this leaf's canonical frame). A handoff later in this
                    // branch can flip `self.state.anchor_id` to a child leaf,
                    // but the inlier_pairs stay in `anchor_id`'s frame.
                    self.last_track_result = Some((anchor_id, r.clone()));
                    // Sustained inlier decline: the anchor's
                    // descriptors are losing correspondences as
                    // perspective drifts, and RANSAC will start
                    // over-fitting to whichever pocket still matches.
                    // First try to spawn a single handoff anchor at
                    // the current view (provided the chain isn't
                    // already at its depth cap — deep chains
                    // accumulate fit error across the cascading
                    // composition). If spawn is refused or fails,
                    // fall back to Idle so the harness re-acquires
                    // on a later frame with a fresh chain root.
                    let active_depth = self
                        .cache
                        .get(anchor_id)
                        .map(|a| a.chain_depth)
                        .unwrap_or(0);
                    let chain_cap_ok = active_depth < self.config.max_chain_depth;
                    let handoff_quality_ok = self.handoff_quality_ok(&r);
                    if self.track_quality.degraded_frames >= self.config.degraded_max_frames {
                        let spawned = if chain_cap_ok && handoff_quality_ok {
                            self.spawn_handoff(gray, anchor_id, &r.homography, timestamp_ns)
                        } else {
                            if !handoff_quality_ok {
                                log::debug!(
                                    "[engine] degraded handoff blocked by quality gate: median_residual={:.2}px inl_ratio={:.2}",
                                    r.median_residual_px,
                                    if r.matches > 0 {
                                        r.descriptor_inliers as f32 / r.matches as f32
                                    } else {
                                        0.0
                                    },
                                );
                            }
                            None
                        };
                        if let Some(new_id) = spawned {
                            log::info!(
                                "[engine] anchor {anchor_id} degraded ({} consecutive frames < {} desc inliers); handoff → {new_id} (avoids Idle)",
                                self.track_quality.degraded_frames,
                                self.config.degraded_inlier_threshold,
                            );
                            self.start_chain_refine(new_id, anchor_id);
                            self.cache.touch(new_id);
                            if let Some(entry) = self.cache.get_mut(new_id) {
                                entry.last_locked_ns = timestamp_ns;
                            }
                            self.state = EngineState::Locked {
                                anchor_id: new_id,
                                frames_lost: 0,
                                last_homography: IDENTITY,
                            };
                            self.track_quality.degraded_frames = 0;
                            let (root_id, h_root_to_view) =
                                self.chain_homography(anchor_id, &r.homography);
                            let canonical_rotation = self.quadrant_for_active(new_id);
                            return self.emit_locked(
                                root_id,
                                new_id,
                                h_root_to_view,
                                false,
                                r.inliers,
                                canonical_rotation,
                            );
                        }
                        log::info!(
                            "[engine] anchor {} degraded ({} consecutive frames < {} desc inliers), chain_depth={} cap={} → forcing re-acquire",
                            anchor_id,
                            self.track_quality.degraded_frames,
                            self.config.degraded_inlier_threshold,
                            active_depth,
                            self.config.max_chain_depth,
                        );
                        self.state = EngineState::Idle;
                        self.track_quality.reset();
                        self.reset_emit_smooth();
                        return TrackerCommand::Idle;
                    }
                    self.cache.touch(anchor_id);
                    if let Some(entry) = self.cache.get_mut(anchor_id) {
                        entry.last_locked_ns = timestamp_ns;
                    }
                    // External: emit the root id and the chain-composed
                    // H_root→view. The handoff machinery is invisible
                    // to Kotlin.
                    let (root_id, h_root_to_view) = self.chain_homography(anchor_id, &r.homography);
                    // Handoff trigger: spawn a new anchor while the
                    // current one still has enough overlap to fit a
                    // stable H_active→new. Inlier and visibility checks
                    // both matter — see EngineConfig docs.
                    let cooldown_elapsed = timestamp_ns.saturating_sub(self.last_spawn_ns)
                        >= self.config.handoff_cooldown_ns;
                    let visible_ratio = self
                        .cache
                        .get(anchor_id)
                        .map(|a| {
                            visible_keypoint_ratio(
                                &a.anchor.positions,
                                &r.homography,
                                gray.width(),
                                gray.height(),
                            )
                        })
                        .unwrap_or(1.0);
                    // Scale-change trigger: H_anchor→view's local linear
                    // scale captures cumulative zoom since acquire. The
                    // anchor's canonical frame is by construction
                    // identity-mapped to itself, so the current H *is*
                    // the change. BRIEF is not scale-invariant; fire a
                    // handoff once we're past ~1.35× to rebuild
                    // descriptors at the current scale before matching
                    // collapses.
                    let scale = translator_core::coords::Homography::<
                        translator_core::coords::AnchorSpace,
                        translator_core::coords::TrackerSpace,
                    >::from_raw(r.homography)
                    .approx_scale();
                    let scale_log = if scale > 0.0 { scale.ln().abs() } else { 0.0 };
                    let scale_changed = scale_log > self.config.handoff_scale_log_threshold;
                    // Use descriptor-only inliers for the handoff
                    // trigger: KLT-propagated correspondences make the
                    // total inlier count look healthy even while the
                    // descriptor matcher is degrading (scale drift past
                    // BRIEF's invariance, blur, etc.) and the anchor
                    // genuinely needs replacement. Counting total
                    // inliers here masks that degradation and starves
                    // handoffs until the matcher cliffs entirely.
                    let needs_handoff = r.descriptor_inliers < self.config.handoff_min_inliers
                        || visible_ratio < self.config.handoff_min_visible_ratio
                        || scale_changed;
                    // Defer handoff while the matcher is still recovering
                    // from a sanity-gate freeze. The first one or two
                    // accepted fits after a freeze sequence are often
                    // wrong-basin (matcher was starved, then released
                    // into whichever pocket re-matched first). Baking
                    // such a fit into a new anchor's canonical alignment
                    // produces a visible overlay snap at handoff.
                    let recovering = self.track_quality.consecutive_clean_frames < 2;
                    if needs_handoff
                        && cooldown_elapsed
                        && !recovering
                        && chain_cap_ok
                        && !handoff_quality_ok
                    {
                        log::debug!(
                            "[engine] handoff blocked by quality gate: median_residual={:.2}px inl_ratio={:.2}",
                            r.median_residual_px,
                            if r.matches > 0 {
                                r.descriptor_inliers as f32 / r.matches as f32
                            } else {
                                0.0
                            },
                        );
                    }
                    let new_active = if cooldown_elapsed
                        && needs_handoff
                        && !recovering
                        && chain_cap_ok
                        && handoff_quality_ok
                    {
                        let spawned = self
                            .spawn_handoff(gray, anchor_id, &r.homography, timestamp_ns)
                            .unwrap_or(anchor_id);
                        if spawned != anchor_id {
                            let reason = if r.descriptor_inliers < self.config.handoff_min_inliers {
                                "low_desc_inliers"
                            } else if visible_ratio < self.config.handoff_min_visible_ratio {
                                "low_visibility"
                            } else {
                                "scale_drift"
                            };
                            log::debug!(
                                "[engine] handoff {anchor_id} → {spawned} reason={reason} desc_inl={} vis={:.2} scale_log={:.2}",
                                r.descriptor_inliers,
                                visible_ratio,
                                scale_log,
                            );
                            self.start_chain_refine(spawned, anchor_id);
                        }
                        spawned
                    } else {
                        anchor_id
                    };
                    // If we handed off, the new anchor's canonical frame
                    // IS this view, so its `last_homography` is identity.
                    // Otherwise we stay on the old anchor with its
                    // tracked H.
                    let (new_state_h, new_state_id) = if new_active == anchor_id {
                        (r.homography, anchor_id)
                    } else {
                        (IDENTITY, new_active)
                    };
                    self.state = EngineState::Locked {
                        anchor_id: new_state_id,
                        frames_lost: 0,
                        last_homography: new_state_h,
                    };
                    let canonical_rotation = self.quadrant_for_active(new_state_id);
                    return self.emit_locked(
                        root_id,
                        new_state_id,
                        h_root_to_view,
                        false,
                        r.inliers,
                        canonical_rotation,
                    );
                }
                // Current anchor lost the frame — try cached siblings.
                if let Some((id, alt)) = self.try_cached_anchors_timed(gray, Some(anchor_id)) {
                    self.last_track_result = Some((id, alt.clone()));
                    log::debug!(
                        "[engine] Locked active={anchor_id} produced no fit → cached sibling leaf={id} (inl={} desc={})",
                        alt.inliers,
                        alt.descriptor_inliers,
                    );
                    self.transition_to_locked(id, &alt, timestamp_ns, false);
                    let (root_id, h_root_to_view) = self.chain_homography(id, &alt.homography);
                    let canonical_rotation = self.quadrant_for_active(id);
                    return self.emit_locked(
                        root_id,
                        id,
                        h_root_to_view,
                        false,
                        alt.inliers,
                        canonical_rotation,
                    );
                }
                // Matcher failed AND no cached sibling caught the frame.
                let new_frames_lost = frames_lost + 1;
                let root = self.root_of(anchor_id);
                log::info!(
                    "[engine] Locked frame produced no usable fit; frames_lost {} -> {} (gate ema={:?} susp={})",
                    frames_lost,
                    new_frames_lost,
                    self.track_quality.inlier_ema,
                    self.track_quality.suspicious_frames,
                );
                if new_frames_lost >= self.config.lost_after_frames {
                    log::debug!(
                        "[engine] Locked active={anchor_id} → Lost after {} consecutive bad frames",
                        new_frames_lost,
                    );
                    self.state = EngineState::Lost {
                        last_anchor_id: anchor_id,
                        frames_lost: 0,
                    };
                    self.reset_emit_smooth();
                    TrackerCommand::Lost {
                        last_anchor_id: root,
                    }
                } else {
                    // Engine-side grace: we're internally still Locked
                    // (frames_lost < threshold) on a transient no-fit + no-
                    // cached frame. Emit Locked with the previous good H
                    // rather than thrashing the pipeline through Lost (which
                    // also resets `emit_smooth` and floods state-transition
                    // logs at every other-frame matcher hiccup). No fresh fit
                    // → no fresh inliers → the Correction's seeds come out
                    // empty, and the CoarseTracker keeps its existing seeds
                    // tracking on the woven H.
                    self.state = EngineState::Locked {
                        anchor_id,
                        frames_lost: new_frames_lost,
                        last_homography,
                    };
                    let (root_id, h_root_to_view) =
                        self.chain_homography(anchor_id, &last_homography);
                    let canonical_rotation = self.quadrant_for_active(anchor_id);
                    self.emit_locked(
                        root_id,
                        anchor_id,
                        h_root_to_view,
                        false,
                        0,
                        canonical_rotation,
                    )
                }
            }
            EngineState::Lost {
                last_anchor_id,
                frames_lost,
            } => {
                if let Some((id, result)) = self.try_cached_anchors_timed(gray, None) {
                    self.last_track_result = Some((id, result.clone()));
                    log::debug!(
                        "[engine] Lost (last_active={last_anchor_id}) → Locked via cached leaf={id} (inl={} desc={})",
                        result.inliers,
                        result.descriptor_inliers,
                    );
                    self.transition_to_locked(id, &result, timestamp_ns, false);
                    let (root_id, h_root_to_view) = self.chain_homography(id, &result.homography);
                    let canonical_rotation = self.quadrant_for_active(id);
                    return self.emit_locked(
                        root_id,
                        id,
                        h_root_to_view,
                        false,
                        result.inliers,
                        canonical_rotation,
                    );
                }
                let new_frames_lost = frames_lost + 1;
                let root = self.root_of(last_anchor_id);
                if new_frames_lost >= self.config.give_up_after_frames {
                    log::debug!(
                        "[engine] Lost (last_active={last_anchor_id}) → Idle (gave up after {} frames without re-lock)",
                        new_frames_lost,
                    );
                    self.state = EngineState::Idle;
                    self.reset_emit_smooth();
                    TrackerCommand::Idle
                } else {
                    self.state = EngineState::Lost {
                        last_anchor_id,
                        frames_lost: new_frames_lost,
                    };
                    self.reset_emit_smooth();
                    TrackerCommand::Lost {
                        last_anchor_id: root,
                    }
                }
            }
        }
    }

    /// Build a `TrackerCommand::Locked` whose `homography` has been
    /// smoothed across leaf-anchor switches. When the engine swaps
    /// between two anchors that disagree on `H_root→view` by more
    /// than `anchor_switch_blend_threshold_px`, the emitted H is
    /// lerped from the prior frame's emitted H toward the natural
    /// new-anchor H over `anchor_switch_blend_frames` frames. Real
    /// motion inside a single anchor is emitted naturally.
    fn emit_locked(
        &mut self,
        root_id: AnchorId,
        active_id: AnchorId,
        h_root_to_view: [f32; 9],
        is_new: bool,
        inliers: usize,
        canonical_rotation: Quadrant,
    ) -> TrackerCommand {
        let smoothed = self.smooth_emit_h(active_id, h_root_to_view);
        TrackerCommand::Locked {
            anchor_id: root_id,
            homography: smoothed,
            is_new,
            inliers,
            canonical_rotation,
        }
    }

    fn smooth_emit_h(&mut self, active_id: AnchorId, h_natural: [f32; 9]) -> [f32; 9] {
        let blend_frames = self.config.anchor_switch_blend_frames;
        let threshold_px = self.config.anchor_switch_blend_threshold_px;
        if blend_frames == 0 {
            self.emit_smooth.last_active_id = Some(active_id);
            self.emit_smooth.last_emitted_h = Some(h_natural);
            self.emit_smooth.blend = None;
            return h_natural;
        }
        let switched = matches!(self.emit_smooth.last_active_id, Some(prev) if prev != active_id);
        if switched {
            if let Some(prior_h) = self.emit_smooth.last_emitted_h {
                let delta = approx_corner_delta(&prior_h, &h_natural);
                let prev_leaf = self.emit_smooth.last_active_id;
                if delta > threshold_px {
                    log::debug!(
                        "[emit_smooth] switch leaf={:?} → {active_id} delta={:.1}px > {:.1}px, blending {blend_frames} frames",
                        prev_leaf,
                        delta,
                        threshold_px,
                    );
                    self.emit_smooth.blend = Some(BlendState {
                        base_h_at_switch: prior_h,
                        target_active_id: active_id,
                        total_frames: blend_frames,
                        elapsed_frames: 0,
                    });
                } else {
                    log::debug!(
                        "[emit_smooth] switch leaf={:?} → {active_id} delta={:.1}px below threshold, no blend",
                        prev_leaf,
                        delta,
                    );
                    self.emit_smooth.blend = None;
                }
            }
        }
        let emit = match self.emit_smooth.blend.as_mut() {
            Some(blend) if blend.target_active_id == active_id => {
                blend.elapsed_frames = blend.elapsed_frames.saturating_add(1);
                let t = (blend.elapsed_frames as f32) / (blend.total_frames as f32);
                if t >= 1.0 {
                    self.emit_smooth.blend = None;
                    h_natural
                } else {
                    lerp_h(&blend.base_h_at_switch, &h_natural, t)
                }
            }
            Some(_) => {
                // Target moved to a different active anchor mid-blend
                // (rapid second switch). Abandon the previous blend
                // and start fresh on the next switch detection.
                self.emit_smooth.blend = None;
                h_natural
            }
            None => h_natural,
        };
        self.emit_smooth.last_active_id = Some(active_id);
        self.emit_smooth.last_emitted_h = Some(emit);
        emit
    }

    fn reset_emit_smooth(&mut self) {
        self.emit_smooth = EmitSmoothState::default();
        // EKF state is anchor-relative; drop whenever we leave the active
        // anchor (Idle/Lost). It will re-initialise from the next accepted fit.
        self.h_ekf = None;
        // Chain refinement is tied to the currently-active handoff
        // child. If we go Idle/Lost, that anchor's chain matrix is
        // frozen at whatever the running mean has accumulated so far.
        self.chain_refine = None;
    }

    /// Drive the per-anchor EKF on `H_anchor→view` and return the filtered
    /// homography. Re-initialises from `raw_h` when the active anchor changes
    /// or the EKF is empty (first frame after acquire / handoff / cached-snap).
    /// Empty `pairs` skips the measurement update (predict only) — used on a
    /// sanity-gate freeze where the upstream H is the substituted previous fit
    /// rather than a fresh observation.
    fn apply_h_ekf(
        &mut self,
        anchor_id: AnchorId,
        raw_h: [f32; 9],
        pairs: &[(f32, f32, f32, f32)],
    ) -> [f32; 9] {
        let same_anchor = self
            .h_ekf
            .as_ref()
            .map_or(false, |s| s.anchor_id == anchor_id);
        if !same_anchor {
            match HomographyEkf::new(raw_h) {
                Some(ekf) => {
                    self.h_ekf = Some(HomographyEkfTracker { anchor_id, ekf });
                }
                None => {
                    self.h_ekf = None;
                }
            }
            return raw_h;
        }
        let tracker = self
            .h_ekf
            .as_mut()
            .expect("same_anchor implies h_ekf is Some");
        tracker.ekf.predict(&EKF_Q_DEFAULT);
        tracker.ekf.update_pairs(pairs, self.config.h_ekf_r_var);
        tracker.ekf.homography()
    }

    /// Seed chain-composition refinement for a newly-spawned handoff
    /// anchor. The cached anchor's `h_root_to_canonical` was computed
    /// from a single RANSAC fit at the spawn frame; the refinement
    /// state averages additional same-quantity estimates across the
    /// next `chain_refine_frames` frames.
    fn start_chain_refine(&mut self, new_anchor_id: AnchorId, parent_id: AnchorId) {
        if !self.config.use_chain_refine || self.config.chain_refine_frames == 0 {
            return;
        }
        let initial = match self.cache.get(new_anchor_id) {
            Some(a) => canonicalize_h(&a.h_root_to_canonical),
            None => return,
        };
        let parent_h_root_to_canonical = match self.cache.get(parent_id) {
            Some(a) => a.h_root_to_canonical,
            None => return,
        };
        let chain_sum: [f64; 9] = std::array::from_fn(|i| initial[i] as f64);
        log::debug!(
            "[chain_refine] start: anchor {} parent {} frames {}",
            new_anchor_id,
            parent_id,
            self.config.chain_refine_frames,
        );
        self.chain_refine = Some(ChainRefineState {
            new_anchor_id,
            parent_id,
            parent_h_root_to_canonical,
            frames_remaining: self.config.chain_refine_frames,
            chain_sum,
            count: 1,
        });
    }

    /// One step of chain-composition refinement. Re-tracks the parent
    /// anchor against the current frame and folds the implied
    /// `h_root_to_canonical_new` into the running mean, which is
    /// written back into the cached anchor's chain matrix each frame.
    /// No-op when the refinement is inactive, has expired, or the
    /// active anchor doesn't match the one being refined (cached
    /// snap, additional handoff, etc.).
    ///
    /// `features` is the main-path's FAST+BRIEF for this frame —
    /// reused to avoid a second pass over `gray`. Pass `None` when
    /// the main path failed to extract features (the refinement
    /// still ticks down a frame, just without an observation).
    fn step_chain_refine(
        &mut self,
        features: Option<&FrameFeatures>,
        current_active: AnchorId,
        h_new_to_view: &[f32; 9],
    ) {
        let in_progress = self.chain_refine.as_ref().map_or(false, |s| {
            s.new_anchor_id == current_active && s.frames_remaining > 0
        });
        if !in_progress {
            // If the refinement is still set but for a different
            // active anchor, drop it — we lost the context that made
            // its observations meaningful.
            if let Some(s) = self.chain_refine.as_ref() {
                if s.new_anchor_id != current_active {
                    log::debug!(
                        "[chain_refine] drop: active={} but refining={}",
                        current_active,
                        s.new_anchor_id,
                    );
                    self.chain_refine = None;
                }
            }
            return;
        }
        let parent_r = {
            let s = self
                .chain_refine
                .as_ref()
                .expect("in_progress implies chain_refine is Some");
            let parent_id = s.parent_id;
            let Some(parent) = self.cache.get(parent_id) else {
                self.chain_refine = None;
                return;
            };
            // Use the same inlier floor as the regular tracker. A
            // lower threshold here makes more parent re-tracks
            // succeed, but those low-inlier fits are usually biased
            // (the parent's descriptors are post-degraded at handoff,
            // and degrade further over subsequent frames) — averaging
            // biased candidates pulls the running mean away from the
            // spawn-frame value rather than toward truth.
            features.and_then(|features| {
                track_against_anchor_with_features(
                    &parent.anchor,
                    features,
                    &self.config.tracker,
                    self.config.tracker.min_inliers_keep_locked,
                    None,
                    &[],
                )
            })
        };
        let s = self
            .chain_refine
            .as_mut()
            .expect("in_progress implies chain_refine is Some");
        s.frames_remaining = s.frames_remaining.saturating_sub(1);
        let Some(parent_r) = parent_r else {
            // No usable observation this frame. Just decrement and
            // wait for the next; the running mean is unchanged.
            if s.frames_remaining == 0 {
                log::debug!(
                    "[chain_refine] anchor {} finished with {} obs",
                    s.new_anchor_id,
                    s.count,
                );
                self.chain_refine = None;
            }
            return;
        };
        let h_inv = match invert(h_new_to_view) {
            Some(h) => h,
            None => {
                if s.frames_remaining == 0 {
                    self.chain_refine = None;
                }
                return;
            }
        };
        let candidate_raw = mat3_mul(
            &h_inv,
            &mat3_mul(&parent_r.homography, &s.parent_h_root_to_canonical),
        );
        let candidate = canonicalize_h(&candidate_raw);
        if !candidate.iter().all(|v| v.is_finite()) {
            if s.frames_remaining == 0 {
                self.chain_refine = None;
            }
            return;
        }
        // Reject candidates that disagree with the current running
        // mean by more than a few pixels of corner-projection delta.
        // Post-handoff frames frequently produce a parent fit that's
        // biased (descriptor drift past BRIEF's invariance pulls the
        // fit into a wrong basin); folding biased candidates into the
        // mean pulls the chain matrix away from truth rather than
        // toward it. With the gate, biased candidates are silently
        // dropped and the seed value persists — which is the original
        // pre-refinement behaviour, i.e. no regression.
        const CHAIN_REFINE_MAX_DELTA_PX: f32 = 5.0;
        let mean_so_far: [f32; 9] =
            std::array::from_fn(|i| (s.chain_sum[i] / s.count as f64) as f32);
        let delta = approx_corner_delta(&mean_so_far, &candidate);
        if !delta.is_finite() || delta > CHAIN_REFINE_MAX_DELTA_PX {
            log::debug!(
                "[chain_refine] anchor {} dropped candidate: delta={:.1}px > {:.1}px",
                s.new_anchor_id,
                delta,
                CHAIN_REFINE_MAX_DELTA_PX,
            );
            if s.frames_remaining == 0 {
                log::debug!(
                    "[chain_refine] anchor {} finished with {} obs",
                    s.new_anchor_id,
                    s.count,
                );
                self.chain_refine = None;
            }
            return;
        }
        for i in 0..9 {
            s.chain_sum[i] += candidate[i] as f64;
        }
        s.count += 1;
        let count = s.count;
        let new_anchor_id = s.new_anchor_id;
        let mean: [f32; 9] = std::array::from_fn(|i| (s.chain_sum[i] / count as f64) as f32);
        if let Some(new_anchor) = self.cache.get_mut(new_anchor_id) {
            new_anchor.h_root_to_canonical = mean;
        }
        if self
            .chain_refine
            .as_ref()
            .map_or(false, |s| s.frames_remaining == 0)
        {
            log::debug!(
                "[chain_refine] anchor {} finished with {} obs",
                new_anchor_id,
                count,
            );
            self.chain_refine = None;
        }
    }

    /// Acceptance gate on a TrackResult considered as the spawn frame
    /// for a handoff. Returns true iff the fit is clean enough that
    /// baking it into `H_root→canonical_new` won't introduce a
    /// visible-permanently bias into the chain. Checks two
    /// independent quality signals from RANSAC:
    /// - median per-inlier residual (absolute fit error)
    /// - descriptor inlier ratio (fraction of matches RANSAC kept)
    ///
    /// Both must clear their respective `EngineConfig` thresholds.
    /// Either failing means the current frame's fit is in a noisy
    /// region (low overlap with anchor, wrong-basin, or sanity-gate
    /// adjacent) and the handoff is deferred. The matcher will
    /// retry next frame; if the user is in a sustained noisy regime
    /// the degraded path eventually falls back to Idle.
    fn handoff_quality_ok(&self, r: &TrackResult) -> bool {
        if r.median_residual_px > self.config.handoff_max_median_residual_px {
            return false;
        }
        if r.matches == 0 {
            return false;
        }
        let inl_ratio = r.descriptor_inliers as f32 / r.matches as f32;
        inl_ratio >= self.config.handoff_min_inlier_ratio
    }

    /// Force-acquire a new scene anchor from `gray`. Use this when
    /// Kotlin has already run OCR on the frame and wants to lock the
    /// surface in place. Returns the new anchor id, or `None` if the
    /// frame had insufficient features.
    pub fn acquire_now(&mut self, gray: &GrayImage, timestamp_ns: u64) -> Option<AnchorId> {
        self.acquire_inner(gray, &[], 0, timestamp_ns, None)
    }

    /// Like [`acquire_now`] but restricts anchor features to those
    /// inside any of the given axis-aligned regions (padded by
    /// `pad_px`). Use this when you know which surface in the frame
    /// you care about (e.g. the union of OCR-detected text bboxes) so
    /// the tracker doesn't lock onto background clutter.
    pub fn acquire_now_in_regions(
        &mut self,
        gray: &GrayImage,
        regions: &[(u32, u32, u32, u32)],
        pad_px: u32,
        timestamp_ns: u64,
    ) -> Option<AnchorId> {
        self.acquire_inner(gray, regions, pad_px, timestamp_ns, None)
    }

    /// Acquire variant that carries a pre-computed reading-direction
    /// quadrant. Passing `None` for `estimated_quadrant` means the
    /// orientation estimator either couldn't run or didn't reach
    /// consensus; the engine then falls back to `last_known_quadrant`
    /// (which itself starts as `EngineConfig.default_canonical_quadrant`).
    pub fn acquire_now_with_orientation(
        &mut self,
        gray: &GrayImage,
        regions: &[(u32, u32, u32, u32)],
        pad_px: u32,
        timestamp_ns: u64,
        estimated_quadrant: Option<Quadrant>,
    ) -> Option<AnchorId> {
        self.acquire_inner(gray, regions, pad_px, timestamp_ns, estimated_quadrant)
    }

    fn acquire_inner(
        &mut self,
        gray: &GrayImage,
        regions: &[(u32, u32, u32, u32)],
        pad_px: u32,
        timestamp_ns: u64,
        estimated_quadrant: Option<Quadrant>,
    ) -> Option<AnchorId> {
        if timestamp_ns < self.last_acquire_ns
            || timestamp_ns - self.last_acquire_ns < self.config.acquire_cooldown_ns
        {
            if self.last_acquire_ns != 0 {
                return None;
            }
        }
        let anchor = if regions.is_empty() {
            build_anchor(gray, &self.config.tracker, timestamp_ns)?
        } else {
            build_anchor_in_regions(gray, &self.config.tracker, regions, pad_px, timestamp_ns)?
        };
        if anchor.len() < self.config.tracker.min_inliers {
            return None;
        }
        let id = self.next_anchor_id;
        self.next_anchor_id += 1;
        let canonical_rotation = match estimated_quadrant {
            Some(q) => {
                self.last_known_quadrant = q;
                q
            }
            None => self.last_known_quadrant,
        };
        self.cache.insert(
            id,
            CachedAnchor {
                anchor,
                overlays: Vec::new(),
                // `acquire_now` always creates a fresh root: this anchor
                // owns its own overlays and starts a new chain.
                root_id: id,
                h_root_to_canonical: IDENTITY,
                created_at_ns: timestamp_ns,
                last_locked_ns: timestamp_ns,
                canonical_rotation,
                chain_depth: 0,
            },
        );
        self.state = EngineState::Locked {
            anchor_id: id,
            frames_lost: 0,
            last_homography: IDENTITY,
        };
        self.last_acquire_ns = timestamp_ns;
        self.track_quality.reset();
        // Reset spawn cooldown — a fresh root means there's no chain
        // yet to throttle.
        self.last_spawn_ns = timestamp_ns;
        Some(id)
    }

    /// Late-bind the reading-direction quadrant on an anchor's chain
    /// root. Used by the acquire path when orient-rec finishes after
    /// `acquire_inner` — we acquire first (so the engine flips to
    /// Locked while detect alone is enough to draw bbox overlays) and
    /// only later learn the canonical quadrant. Also updates
    /// `last_known_quadrant` so a subsequent fresh acquire seeds from
    /// the most recent observation.
    /// Returns false if the anchor isn't cached anymore.
    pub fn set_canonical_rotation(&mut self, anchor_id: AnchorId, q: Quadrant) -> bool {
        let root_id = self.root_of(anchor_id);
        match self.cache.get_mut(root_id) {
            Some(entry) => {
                entry.canonical_rotation = q;
                self.last_known_quadrant = q;
                true
            }
            None => false,
        }
    }

    /// Attach (or replace) the canonical overlay set for an anchor.
    /// Returns false if the anchor isn't cached anymore.
    pub fn set_overlays(&mut self, anchor_id: AnchorId, overlays: Vec<CanonicalOverlay>) -> bool {
        match self.cache.get_mut(anchor_id) {
            Some(entry) => {
                entry.overlays = overlays;
                true
            }
            None => false,
        }
    }

    /// Read the overlays for an anchor.
    pub fn overlays(&self, anchor_id: AnchorId) -> &[CanonicalOverlay] {
        match self.cache.get(anchor_id) {
            Some(entry) => &entry.overlays,
            None => &[],
        }
    }

    /// Backwards-compatible shim — delegates to the free
    /// [`render_text_overlay_bitmap`] function so existing callers that
    /// already hold an `&LivePlanarEngine` keep working. New callers
    /// (uniffi `prepare_overlay_for_composite`) should call the free
    /// function directly so they don't need to lock the engine mutex
    /// for the duration of the raster — which is what was blocking the
    /// detector thread (and therefore the display pipeline) during the
    /// 50–100 ms it takes to rasterize a dense page.
    #[cfg(feature = "image-render")]
    pub fn render_text_overlay_bitmap(
        &self,
        frame_width: u32,
        frame_height: u32,
        items: &[TextRenderItem],
        fonts: &dyn FontProvider,
    ) -> Option<Vec<u8>> {
        render_text_overlay_bitmap(frame_width, frame_height, items, fonts)
    }

    /// Project all overlays of `anchor_id` through `homography` into
    /// current-frame coordinates. Degenerate projections are skipped.
    pub fn project_overlays(
        &self,
        anchor_id: AnchorId,
        homography: &[f32; 9],
    ) -> Vec<OverlayProjection> {
        let entry = match self.cache.get(anchor_id) {
            Some(e) => e,
            None => return Vec::new(),
        };
        entry
            .overlays
            .iter()
            .filter_map(|ov| {
                let mut quad = [(0.0, 0.0); 4];
                for (i, &(x, y)) in ov.quad.iter().enumerate() {
                    let (px, py) = translator_core::homography::project(homography, x, y)?;
                    quad[i] = (px, py);
                }
                Some(OverlayProjection {
                    id: ov.id,
                    quad,
                    payload: ov.payload.clone(),
                })
            })
            .collect()
    }

    /// True if the anchor's overlays should be re-derived from a new
    /// OCR pass: anchor older than `anchor_refresh_age_ns` and still
    /// being actively locked recently.
    pub fn should_refresh(&self, anchor_id: AnchorId, now_ns: u64) -> bool {
        match self.cache.get(anchor_id) {
            Some(entry) => {
                let age = now_ns.saturating_sub(entry.created_at_ns);
                age >= self.config.anchor_refresh_age_ns
            }
            None => false,
        }
    }

    pub fn current_anchor(&self) -> Option<AnchorId> {
        match self.state {
            EngineState::Locked { anchor_id, .. } => Some(self.root_of(anchor_id)),
            EngineState::Lost { last_anchor_id, .. } => Some(self.root_of(last_anchor_id)),
            EngineState::Idle => None,
        }
    }

    /// Internal cache handles, MRU-first. **These name specific cache
    /// entries, not root coordinate frames.** A single root (one
    /// physical surface) may have several handles after handoffs.
    /// `pub(crate)` so the handle id space never escapes
    /// `planar_engine` — confusing it with the root id space (the
    /// id externally-emitted by `TrackerCommand::Locked`,
    /// `acquire_now`, etc.) was the source of "surface-map state
    /// evaporates after panning" bugs. External callers want
    /// [`Self::cached_root_ids`].
    #[allow(dead_code)]
    pub(crate) fn cached_handle_ids(&self) -> Vec<AnchorId> {
        self.cache.ids_mru()
    }

    /// Unique root anchor ids currently represented in the cache, in
    /// MRU-of-any-descendant order. One root per physical surface;
    /// the right id space for session-state retention because the
    /// emitted `TrackerCommand::Locked.anchor_id` is always a root.
    ///
    /// A root survives in this list as long as **any** of its
    /// descendants is still cached — handoff chains preserve the
    /// root's coord frame even when the original root anchor itself
    /// gets LRU-evicted from `AnchorCache`.
    pub fn cached_root_ids(&self) -> Vec<AnchorId> {
        let mut seen = std::collections::HashSet::new();
        let mut roots = Vec::new();
        for id in self.cache.ids_mru() {
            if let Some(entry) = self.cache.get(id) {
                if seen.insert(entry.root_id) {
                    roots.push(entry.root_id);
                }
            }
        }
        roots
    }

    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    pub fn clear(&mut self) {
        self.cache = AnchorCache::new(self.config.anchor_cache_size);
        self.state = EngineState::Idle;
        self.next_anchor_id = 1;
        self.last_acquire_ns = 0;
        self.stable_since_ns = None;
        self.last_spawn_ns = 0;
        self.track_quality.reset();
    }

    // -- internal helpers --------------------------------------------------

    /// Map an active-anchor id to its chain root. External APIs (the
    /// `TrackerCommand` enum, `current_anchor`, etc.) speak in root
    /// ids so Kotlin doesn't see the handoff machinery.
    fn root_of(&self, active_id: AnchorId) -> AnchorId {
        self.cache
            .get(active_id)
            .map(|a| a.root_id)
            .unwrap_or(active_id)
    }

    /// Resolve the canonical reading-direction quadrant for an active
    /// anchor: look up its root, then read the root's stored quadrant.
    /// Falls back to `last_known_quadrant` if either lookup fails (e.g.
    /// the root has been evicted from the cache).
    fn quadrant_for_active(&self, active_id: AnchorId) -> Quadrant {
        let root_id = self.root_of(active_id);
        self.cache
            .get(root_id)
            .map(|a| a.canonical_rotation)
            .unwrap_or(self.last_known_quadrant)
    }

    /// Inlier-discontinuity sanity gate.
    ///
    /// Three outcomes per call:
    ///   - *Accept*: inlier count is consistent with the running EMA.
    ///     Push to history, update EMA, return the fit.
    ///   - *Freeze*: count dropped catastrophically (wrong-basin fit
    ///     or descriptor-collapse) and we have a last-accepted H
    ///     within budget. Substitute the frozen H instead of the
    ///     suspicious fit; don't advance history or EMA.
    ///   - *Reject*: drop happened but we have no history or have
    ///     burned the substitution budget. Return None so the caller
    ///     treats this like a matcher miss; `frames_lost` advances
    ///     toward Lost via the normal path.
    fn apply_sanity_gate(&mut self, r: TrackResult) -> Option<TrackResult> {
        let cfg = &self.config;
        let new_inliers_f = r.inliers as f32;
        let ema = self.track_quality.inlier_ema.unwrap_or(new_inliers_f);
        let freeze_h = self.track_quality.h_prev;
        let suspicious =
            ema >= cfg.sanity_gate_min_ema && new_inliers_f < cfg.sanity_gate_drop_ratio * ema;
        let can_freeze = self.track_quality.suspicious_frames < cfg.sanity_gate_max_consecutive
            && freeze_h.is_some();
        if suspicious && can_freeze {
            self.track_quality.suspicious_frames += 1;
            self.track_quality.consecutive_clean_frames = 0;
            self.gate_counters.sanity_gate_freeze =
                self.gate_counters.sanity_gate_freeze.saturating_add(1);
            let hf = freeze_h.expect("freeze_h is Some when can_freeze");
            log::info!(
                "[sanity_gate] freeze (substitute with last accepted H): inliers={} ema={:.1} run={}",
                r.inliers,
                ema,
                self.track_quality.suspicious_frames,
            );
            Some(TrackResult {
                homography: hf,
                ..r
            })
        } else if suspicious {
            let reason = if freeze_h.is_none() {
                "no_history(h_prev=false)".to_string()
            } else {
                format!(
                    "budget_exhausted(run={})",
                    self.track_quality.suspicious_frames
                )
            };
            log::info!(
                "[sanity_gate] reject suspicious fit: inliers={} ema={:.1} reason={}",
                r.inliers,
                ema,
                reason,
            );
            self.gate_counters.sanity_gate_reject =
                self.gate_counters.sanity_gate_reject.saturating_add(1);
            self.track_quality.reset_ema_and_budget();
            self.track_quality.consecutive_clean_frames = 0;
            None
        } else {
            if self.track_quality.suspicious_frames > 0 {
                log::debug!(
                    "[sanity_gate] resumed: cleared {} suspicious frame(s); accepted fit inliers={} ema={:.1}",
                    self.track_quality.suspicious_frames,
                    r.inliers,
                    ema,
                );
            }
            self.track_quality.suspicious_frames = 0;
            self.track_quality.consecutive_clean_frames = self
                .track_quality
                .consecutive_clean_frames
                .saturating_add(1);
            self.track_quality
                .update_ema(new_inliers_f, cfg.inlier_ema_alpha);
            self.track_quality.push_h(r.homography);
            if r.descriptor_inliers < cfg.degraded_inlier_threshold {
                self.track_quality.degraded_frames += 1;
            } else {
                self.track_quality.degraded_frames = 0;
            }
            Some(r)
        }
    }

    /// return `(root_id, H_root→view)` — the pair the engine should
    /// emit externally. The chain compose lets Kotlin keep treating
    /// the engine as single-anchor: it sees the root id and a single
    /// homography per frame, never the handoff anchors in between.
    /// Falls back to `(active_id, h_active_to_view)` if the active
    /// anchor isn't in the cache (shouldn't happen during normal flow).
    fn chain_homography(
        &self,
        active_id: AnchorId,
        h_active_to_view: &[f32; 9],
    ) -> (AnchorId, [f32; 9]) {
        match self.cache.get(active_id) {
            Some(a) => {
                // Canonicalize the composed H (divide all 9 elements
                // by h22) so the downstream similarity/perspective
                // decomposition (used by the P-EMA smoother) sees a
                // matrix with h22 = 1. `mat3_mul` of two h22 = 1
                // matrices generally yields h22 ≈ 1 + small cross-
                // term contributions; without this step the
                // decomposition's S would absorb the scale offset
                // and P would be skewed, defeating the EMA's job.
                let composed = mat3_mul(h_active_to_view, &a.h_root_to_canonical);
                let h_root_to_view = canonicalize_h(&composed);
                (a.root_id, h_root_to_view)
            }
            None => (active_id, canonicalize_h(h_active_to_view)),
        }
    }

    /// Spawn a handoff anchor from the current frame as a descendant
    /// of the currently-active chain. Returns the new anchor id on
    /// success, `None` if the new anchor doesn't carry enough features
    /// to be useful. The new anchor's canonical frame *is* this view,
    /// so `H_active→newCanonical = h_active_to_view`, and
    /// `H_root→newCanonical = h_active_to_view · h_root_to_active`.
    fn spawn_handoff(
        &mut self,
        gray: &GrayImage,
        active_id: AnchorId,
        h_active_to_view: &[f32; 9],
        timestamp_ns: u64,
    ) -> Option<AnchorId> {
        let (root_id, h_root_to_active, parent_depth) = {
            let active = self.cache.get(active_id)?;
            (
                active.root_id,
                active.h_root_to_canonical,
                active.chain_depth,
            )
        };
        // Inherit canonical rotation from the chain's root, not the
        // immediate parent. Keeps orientation pinned to the original
        // acquire even across multiple handoffs.
        let inherited_rotation = self
            .cache
            .get(root_id)
            .map(|a| a.canonical_rotation)
            .unwrap_or(self.last_known_quadrant);
        let new_anchor = build_anchor(gray, &self.config.tracker, timestamp_ns)?;
        if new_anchor.len() < self.config.tracker.min_inliers {
            return None;
        }
        let h_root_to_new = mat3_mul(h_active_to_view, &h_root_to_active);
        let new_id = self.next_anchor_id;
        self.next_anchor_id += 1;
        self.cache.insert(
            new_id,
            CachedAnchor {
                anchor: new_anchor,
                overlays: Vec::new(),
                root_id,
                h_root_to_canonical: h_root_to_new,
                created_at_ns: timestamp_ns,
                last_locked_ns: timestamp_ns,
                canonical_rotation: inherited_rotation,
                chain_depth: parent_depth + 1,
            },
        );
        self.last_spawn_ns = timestamp_ns;
        Some(new_id)
    }

    fn is_stable_enough(&self, now_ns: u64) -> bool {
        match self.stable_since_ns {
            Some(t) => now_ns.saturating_sub(t) >= self.config.stable_required_ns,
            None => false,
        }
    }

    /// Timed wrapper so the FAST+BRIEF + cached-anchor match cost on
    /// non-Locked frames lands in `last_step_timings.cached_match_ms`
    /// instead of being invisible in the per-step breakdown.
    fn try_cached_anchors_timed(
        &mut self,
        gray: &GrayImage,
        skip_id: Option<AnchorId>,
    ) -> Option<(AnchorId, TrackResult)> {
        let t = std::time::Instant::now();
        let r = self.try_cached_anchors(gray, skip_id);
        self.last_step_timings.cached_match_ms = t.elapsed().as_secs_f64() * 1000.0;
        r
    }

    fn try_cached_anchors(
        &self,
        gray: &GrayImage,
        skip_id: Option<AnchorId>,
    ) -> Option<(AnchorId, TrackResult)> {
        let ids: Vec<AnchorId> = self
            .cache
            .ids_mru()
            .into_iter()
            .filter(|id| Some(*id) != skip_id)
            .collect();
        if ids.is_empty() {
            return None;
        }
        // FAST + BRIEF once per frame. Without this, the cached-anchor
        // retry loop on Lost-state frames runs the full detect+describe
        // pipeline against the same `gray` for every cached anchor
        // (O(cached_anchors × per_frame_matcher_cost)). On park.mp4
        // this dominates: tracker time was ~50 ms/frame in Lost vs
        // ~10 ms in steady-state Locked.
        let features = match compute_frame_features(gray, &self.config.tracker) {
            Some(f) => f,
            None => return None,
        };
        let min = self.config.tracker.min_inliers;
        let mut best: Option<(AnchorId, TrackResult)> = None;
        for id in ids {
            let entry = match self.cache.get(id) {
                Some(e) => e,
                None => continue,
            };
            if let Some(r) = track_against_anchor_with_features(
                &entry.anchor,
                &features,
                &self.config.tracker,
                min,
                None,
                &[],
            ) {
                let beats_current = match &best {
                    Some((_, b)) => r.inliers > b.inliers,
                    None => true,
                };
                if beats_current {
                    best = Some((id, r));
                }
            }
        }
        best
    }

    fn transition_to_locked(
        &mut self,
        anchor_id: AnchorId,
        result: &TrackResult,
        timestamp_ns: u64,
        is_new: bool,
    ) {
        let _ = is_new;
        self.cache.touch(anchor_id);
        if let Some(entry) = self.cache.get_mut(anchor_id) {
            entry.last_locked_ns = timestamp_ns;
        }
        // Stale velocity + EMA from before the Lost period would
        // mislead the sanity gate.
        self.track_quality.reset();
        self.state = EngineState::Locked {
            anchor_id,
            frames_lost: 0,
            last_homography: result.homography,
        };
    }
}

const IDENTITY: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Free-function form of the overlay rasterizer (extracted from
/// `LivePlanarEngine::render_text_overlay_bitmap`). Doesn't touch any
/// engine state — pure on `items` + `fonts`. Pulled out so the uniffi
/// `prepare_overlay_for_composite` call can rasterize without locking
/// the engine mutex; otherwise the rec-batch worker's raster (50–100
/// ms on a dense page) starves the detector thread that's trying to
/// run `process_frame_with_imu`, which freezes the SurfaceView at the
/// rec-batch rate instead of the camera rate.
#[cfg(feature = "image-render")]
pub fn render_text_overlay_bitmap(
    frame_width: u32,
    frame_height: u32,
    items: &[TextRenderItem],
    fonts: &dyn FontProvider,
) -> Option<Vec<u8>> {
    if frame_width == 0 || frame_height == 0 {
        return None;
    }
    let pixels = (frame_width as usize) * (frame_height as usize);
    let mut rgba = vec![0u8; pixels * 4];
    for it in items {
        let oriented = oriented_rect_from_corners(&it.quad);
        fill_oriented_rect_blended(
            &mut rgba,
            frame_width,
            frame_height,
            &oriented,
            argb_u32_to_rgba8(it.bg_argb),
        );
    }
    let blocks: Vec<PreparedTextBlock> = items
        .iter()
        .filter(|it| !it.translated_text.trim().is_empty())
        .map(|it| {
            let bg_box = oriented_rect_from_corners(&it.quad);
            // Inset the text box horizontally so the left-aligned text
            // starts inside the bg's rounded edge instead of flush
            // against it. Matches the Kotlin `HORIZONTAL_PAD_PX`.
            let text_box = OrientedRect {
                cx: bg_box.cx,
                cy: bg_box.cy,
                width: (bg_box.width - 2.0 * OVERLAY_TEXT_HORIZONTAL_INSET_PX).max(1.0),
                height: bg_box.height,
                angle_radians: bg_box.angle_radians,
            };
            let aabb = text_box.to_aabb();
            let bbox = Rect {
                left: aabb.left.min(frame_width.saturating_sub(1)),
                top: aabb.top.min(frame_height.saturating_sub(1)),
                right: aabb.right.min(frame_width),
                bottom: aabb.bottom.min(frame_height),
            };
            let line = PreparedTextLine {
                text: it.translated_text.clone(),
                bounding_box: bbox.clone(),
                oriented_box: text_box,
                word_rects: vec![bbox.clone()],
                background_argb: it.bg_argb,
                foreground: vec![translator_core::ocr::LineColorStop {
                    at: 0.0,
                    argb: it.fg_argb,
                }],
            };
            PreparedTextBlock {
                source_text: it.source_text.clone(),
                translated_text: it.translated_text.clone(),
                bounding_box: bbox,
                lines: vec![line],
                layout_hints: OverlayLayoutHints {
                    layout_mode: OverlayLayoutMode::PerLine,
                    suggested_font_size_px: it.suggested_font_px.max(6.0),
                },
                style_spans: translator_core::ocr::style_spans_from_styles(
                    it.translated_text.len(),
                    &[],
                ),
                source_style_spans: Vec::new(),
            }
        })
        .collect();
    if blocks.is_empty() {
        return Some(rgba);
    }
    let prepared = PreparedImageOverlay {
        rgba_bytes: rgba,
        width: frame_width,
        height: frame_height,
        extracted_text: String::new(),
        translated_text: String::new(),
        blocks,
        source_words: Vec::new(),
        translated_words: Vec::new(),
    };
    let opts = RenderOptions {
        language: items
            .first()
            .map(|i| i.language.clone())
            .unwrap_or_default(),
        min_font_size_px: 6.0,
    };
    render_overlay(&prepared, fonts, &opts)
        .ok()
        .map(|r| r.rgba_bytes)
}

/// Horizontal text inset (canonical pixels) within the live-overlay
/// rounded-rect background. Must equal the Kotlin-side
/// `HORIZONTAL_PAD_PX` used to inflate `visualBox`; together they yield
/// left-aligned text starting `pad` px inside the bg's rounded edge,
/// with `pad` px of bg breathing room on the right of short lines.
#[cfg(feature = "image-render")]
pub const OVERLAY_TEXT_HORIZONTAL_INSET_PX: f32 = 8.0;

/// Fraction of an anchor's canonical-frame keypoints that, projected
/// through `h` (canonical → view), land inside `(0..view_w, 0..view_h)`.
/// Returns 0.0 for empty anchors and saturates at 1.0. Used by the
/// handoff trigger: when too few features will be visible next frame,
/// we want a new anchor while the current one still has enough
/// overlap to fit `H_active→new` cleanly.
fn visible_keypoint_ratio(positions: &[(f32, f32)], h: &[f32; 9], view_w: u32, view_h: u32) -> f32 {
    if positions.is_empty() {
        return 0.0;
    }
    let w = view_w as f32;
    let h_ = view_h as f32;
    let mut visible = 0usize;
    for &(x, y) in positions {
        let Some((vx, vy)) = translator_core::homography::project(h, x, y) else {
            continue;
        };
        if vx >= 0.0 && vx < w && vy >= 0.0 && vy < h_ {
            visible += 1;
        }
    }
    visible as f32 / positions.len() as f32
}

/// Reject a homography if it would produce a visually-degenerate
/// projection of the canonical frame's four corners. A "valid" matrix
/// from RANSAC can still send some bitmap pixels to infinity (when the
/// homogeneous `w` of a corner is near zero) or produce a wildly
/// skewed trapezoid; either case manifests as the "huge diagonal
/// streaks" rendering glitch. Three checks:
///   1. all 4 corner projections must succeed (finite, non-degenerate `w`)
///   2. no projected edge longer than 4× the canonical diagonal
///   3. opposite edges within a 6× length ratio of each other
/// Maximum corner displacement (in canonical pixels) we'll accept
/// between consecutive Locked frames' homographies. RANSAC on
/// repetitive content occasionally settles into a wrong basin that
/// is locally self-consistent (high inlier count, passes
/// `homography_is_sane`) but is geometrically nonsense — projecting
/// the canonical corners hundreds of pixels away from where the
/// previous frame's H put them. Real hand motion at 30 fps doesn't
/// move a corner ~150 px in one frame for typical zoom levels, so
/// anything larger is rejected as a wrong-basin fit. The reject
/// path returns None → `frames_lost++` → after
/// `lost_after_frames` consecutive rejects, transition to Lost
/// (overlay hides, scene re-acquires when matcher recovers).
const MAX_CORNER_JUMP_PX: f32 = 300.0;

/// True when the corner displacement between `h_new` and `h_prev`
/// projected over the canonical rect stays below
/// [`MAX_CORNER_JUMP_PX`]. Treats degenerate projections as a
/// failure (= reject the fit).
fn homography_delta_is_sane(
    h_new: &[f32; 9],
    h_prev: &[f32; 9],
    canonical_w: u32,
    canonical_h: u32,
) -> bool {
    let cw = canonical_w as f32;
    let ch = canonical_h as f32;
    let corners = [(0.0_f32, 0.0_f32), (cw, 0.0), (cw, ch), (0.0, ch)];
    for &(x, y) in &corners {
        let pn = translator_core::homography::project(h_new, x, y);
        let pp = translator_core::homography::project(h_prev, x, y);
        match (pn, pp) {
            (Some(pn), Some(pp)) => {
                let dx = pn.0 - pp.0;
                let dy = pn.1 - pp.1;
                let d = (dx * dx + dy * dy).sqrt();
                if !d.is_finite() || d > MAX_CORNER_JUMP_PX {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// Max corner-projection distance between two 3×3 homographies over
/// a fixed 1000×1000 reference square. Used to size the anchor-switch
/// blend trigger: real motion within an anchor produces small per-
/// frame deltas; a switch produces a one-frame jump in the tens to
/// hundreds of px here. **Large reference is deliberate** for this
/// caller — chain-composition discontinuities at switch time are
/// most visible at far corners, so amplifying them is correct.
fn approx_corner_delta(a: &[f32; 9], b: &[f32; 9]) -> f32 {
    const W: f32 = 1000.0;
    let corners = [(0.0_f32, 0.0_f32), (W, 0.0), (W, W), (0.0, W)];
    let mut max_d = 0.0_f32;
    for &(x, y) in &corners {
        let pa = translator_core::homography::project(a, x, y);
        let pb = translator_core::homography::project(b, x, y);
        if let (Some(pa), Some(pb)) = (pa, pb) {
            let dx = pa.0 - pb.0;
            let dy = pa.1 - pb.1;
            let d = (dx * dx + dy * dy).sqrt();
            if d.is_finite() && d > max_d {
                max_d = d;
            }
        }
    }
    max_d
}

/// Per-element linear interpolation between two 3×3 row-major
/// homographies. Not geometrically rigorous for projective matrices,
/// but accurate enough over the 3–5 frame blend window we use, and
/// avoids the cost of decompose/recompose. The blend's t goes 0→1.
fn lerp_h(a: &[f32; 9], b: &[f32; 9], t: f32) -> [f32; 9] {
    let s = 1.0 - t;
    let mut out = [0.0_f32; 9];
    for i in 0..9 {
        out[i] = a[i] * s + b[i] * t;
    }
    out
}

/// Divide every element by `h22` so the matrix is in the "h22 = 1"
/// canonical form. Projective transforms are scale-invariant so this
/// is a no-op for the projection itself, but the S/P decomposition
/// (used by the P-EMA smoother) assumes h22 = 1 to keep S in
/// 4-DoF similarity form. Degenerate matrices (h22 ≈ 0) are passed
/// through unchanged — they were already broken.
/// Bit-exact equality across all nine elements. Used to detect when the sanity
/// gate substituted the freeze H for the raw RANSAC fit — the gate writes the
/// substituted matrix bit-for-bit from `h_prev`, so any pre-vs-post difference
/// indicates a freeze rather than a normal accept (and the EKF should skip
/// folding the bad fit's inlier pairs in as measurements).
fn h_elementwise_eq(a: &[f32; 9], b: &[f32; 9]) -> bool {
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.to_bits() == y.to_bits())
}

fn canonicalize_h(h: &[f32; 9]) -> [f32; 9] {
    let scale = h[8];
    if scale.abs() < 1e-9 {
        return *h;
    }
    let inv = 1.0 / scale;
    let mut out = [0.0_f32; 9];
    for i in 0..9 {
        out[i] = h[i] * inv;
    }
    out
}

fn homography_is_sane(h: &[f32; 9], canonical_w: u32, canonical_h: u32) -> bool {
    let cw = canonical_w as f32;
    let ch = canonical_h as f32;
    let corners = [(0.0_f32, 0.0_f32), (cw, 0.0), (cw, ch), (0.0, ch)];
    let mut p = [(0.0f32, 0.0f32); 4];
    for (i, &(x, y)) in corners.iter().enumerate() {
        match translator_core::homography::project(h, x, y) {
            Some(q) if q.0.is_finite() && q.1.is_finite() => p[i] = q,
            _ => return false,
        }
    }
    let edge = |a: (f32, f32), b: (f32, f32)| -> f32 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    };
    let w_top = edge(p[0], p[1]);
    let w_bot = edge(p[3], p[2]);
    let h_left = edge(p[0], p[3]);
    let h_right = edge(p[1], p[2]);
    let max_edge = w_top.max(w_bot).max(h_left).max(h_right);
    let orig_diag = (cw * cw + ch * ch).sqrt();
    // Tightened from 4.0× to 2.0× canonical diagonal: catches more of
    // the moderately-bad fits that pass the looser bound but still
    // produce visible warp. A genuinely-tilted surface at 45° has
    // edges ~√2 × canonical, so 2.0 leaves headroom; anything beyond
    // that is the perspective-DoF-noise regime, not a real view.
    if !max_edge.is_finite() || max_edge > orig_diag * 2.0 {
        return false;
    }
    let safe_min = |a: f32, b: f32| (a.min(b)).max(1.0);
    let w_ratio = w_top.max(w_bot) / safe_min(w_top, w_bot);
    let h_ratio = h_left.max(h_right) / safe_min(h_left, h_right);
    // Tightened from 6.0× to 3.0×: catches lopsided trapezoids that
    // are mathematically valid homographies but indicate a fit
    // driven by an under-spread inlier cluster.
    if w_ratio > 3.0 || h_ratio > 3.0 {
        return false;
    }
    true
}

/// One translated text region to render into the Phase-2 overlay
/// bitmap. The bindings layer fills this in from the OCR + translation
/// pipeline; the engine just stitches them into a `PreparedImageOverlay`
/// and calls the shared rasterizer.
#[cfg(feature = "image-render")]
pub struct TextRenderItem {
    pub id: u64,
    /// Canonical-frame corners (TL, TR, BR, BL) in pixels.
    pub quad: [(f32, f32); 4],
    pub translated_text: String,
    pub source_text: String,
    /// BCP-47 of the target language, used as a font-fallback hint.
    pub language: String,
    pub bg_argb: u32,
    pub fg_argb: u32,
    pub suggested_font_px: f32,
}

/// Invert `OrientedRect::corners()`. Width = distance TL→TR, height =
/// distance TL→BL, angle = TL→TR direction. Robust to slight quad
/// non-rectangularity (post-perspective warps); we use the centroid.
#[cfg(feature = "image-render")]
fn oriented_rect_from_corners(quad: &[(f32, f32); 4]) -> OrientedRect {
    let (tlx, tly) = quad[0];
    let (trx, try_) = quad[1];
    let (brx, bry) = quad[2];
    let (blx, bly) = quad[3];
    let cx = (tlx + trx + brx + blx) * 0.25;
    let cy = (tly + try_ + bry + bly) * 0.25;
    let wdx = trx - tlx;
    let wdy = try_ - tly;
    let hdx = blx - tlx;
    let hdy = bly - tly;
    OrientedRect {
        cx,
        cy,
        width: (wdx * wdx + wdy * wdy).sqrt(),
        height: (hdx * hdx + hdy * hdy).sqrt(),
        angle_radians: wdy.atan2(wdx),
    }
}

// -- Phase 1 debug bitmap helpers ----------------------------------------
//
// Tiny rasterizer for Phase 1 verification. Once Phase 2 lands we route
// through `image_render::render_overlay` instead and these go away.

fn argb_u32_to_rgba8(argb: u32) -> [u8; 4] {
    let a = ((argb >> 24) & 0xff) as u8;
    let r = ((argb >> 16) & 0xff) as u8;
    let g = ((argb >> 8) & 0xff) as u8;
    let b = (argb & 0xff) as u8;
    [r, g, b, a]
}

/// Stamp a flat colour into the canvas across the oriented rect, with
/// rounded corners and a 1-pixel anti-aliased edge. Uses *max alpha*
/// rather than alpha-blending so overlapping boxes (adjacent lines that
/// share an edge) take the louder coverage instead of summing — without
/// this, every overlap would visibly darken.
///
/// All bg fills are expected to use the same RGB; callers paint into a
/// freshly-zeroed canvas before the text rasterizer runs.
pub fn fill_oriented_rect_blended(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    rect: &translator_core::ocr::OrientedRect,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    let cos = rect.angle_radians.cos();
    let sin = rect.angle_radians.sin();
    let hw = rect.width * 0.5;
    let hh = rect.height * 0.5;
    // Corner radius: 50 % of the short half-extent, capped at 12 px.
    // Visible softness without eating into glyph space on short boxes.
    let radius = (hw.min(hh) * 0.5).min(12.0).max(0.0);
    // AABB of the rect, clipped to the canvas. Rotation can make the
    // AABB bigger than width × height, so use the half-diagonal as the
    // scan window radius.
    let half_diag = (hw * hw + hh * hh).sqrt();
    let min_x = ((rect.cx - half_diag).floor() as i32).max(0) as u32;
    let min_y = ((rect.cy - half_diag).floor() as i32).max(0) as u32;
    let max_x = ((rect.cx + half_diag).ceil() as i32).max(0).min(w as i32) as u32;
    let max_y = ((rect.cy + half_diag).ceil() as i32).max(0).min(h as i32) as u32;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 + 0.5 - rect.cx;
            let dy = y as f32 + 0.5 - rect.cy;
            let lx = dx * cos + dy * sin;
            let ly = -dx * sin + dy * cos;
            // Signed distance to a rounded rect centred at the origin
            // with half-extents (hw, hh) and corner radius `radius`.
            // < 0 inside, > 0 outside.
            let qx = lx.abs() - (hw - radius);
            let qy = ly.abs() - (hh - radius);
            let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
            let inside = qx.max(qy).min(0.0);
            let sdf = outside + inside - radius;
            // 1-px feathered edge: full coverage at sdf ≤ -0.5, zero at
            // sdf ≥ 0.5, linear in between.
            let coverage = (0.5 - sdf).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }
            let new_alpha = (color[3] as f32 * coverage).round() as u8;
            if new_alpha == 0 {
                continue;
            }
            let idx = ((y * w + x) * 4) as usize;
            if new_alpha > rgba[idx + 3] {
                rgba[idx] = color[0];
                rgba[idx + 1] = color[1];
                rgba[idx + 2] = color[2];
                rgba[idx + 3] = new_alpha;
            }
        }
    }
}

/// Fill an oriented rect with a **solid** color — hard square edges, no
/// anti-aliased feather, no rounded corners, no per-pixel SDF/`sqrt`. The screen
/// overlay uses this: its pills are opaque and dimmed under the touch cap, so the
/// SDF niceties are invisible and the per-pixel rounded-rect math was the whole
/// per-present cost (~200–500 ms). Axis-aligned rects take a row-span fast path;
/// tilted ones fall back to a per-pixel hard inside-test (still no `sqrt`).
///
/// Writes unconditionally (no "only-if-greater-alpha" read): opaque-over-opaque
/// is idempotent, so overlapping pills just re-set the same color.
pub fn fill_oriented_rect_solid(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    rect: &translator_core::ocr::OrientedRect,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    fill_oriented_rect_solid_unguarded(rgba, w, h, rect, color);
}

/// Reset an oriented rect region to transparent (`0,0,0,0`), inflated by `margin`
/// px on every side. Used to erase a pill (background + the text composited over
/// it) from the persistent screen canvas when a block is removed or a provisional
/// pill is dropped — the canvas has no content underneath, so transparent is the
/// correct "uncovered" value. The margin covers sub-pixel overhang of the original
/// solid fill so no fringe is left behind.
pub fn clear_oriented_rect(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    rect: &translator_core::ocr::OrientedRect,
    margin: f32,
) {
    let grown = translator_core::ocr::OrientedRect {
        width: rect.width + 2.0 * margin,
        height: rect.height + 2.0 * margin,
        ..*rect
    };
    fill_oriented_rect_solid_unguarded(rgba, w, h, &grown, [0, 0, 0, 0]);
}

/// Body of [`fill_oriented_rect_solid`] without the `color[3] == 0` early-out, so a
/// fully-transparent clear actually writes. Kept private; callers use
/// `fill_oriented_rect_solid` (fills) or `clear_oriented_rect` (erases).
fn fill_oriented_rect_solid_unguarded(
    rgba: &mut [u8],
    w: u32,
    h: u32,
    rect: &translator_core::ocr::OrientedRect,
    color: [u8; 4],
) {
    let hw = rect.width * 0.5;
    let hh = rect.height * 0.5;
    if rect.angle_radians.abs() < 1e-3 {
        let min_x = (rect.cx - hw).round().clamp(0.0, w as f32) as usize;
        let max_x = (rect.cx + hw).round().clamp(0.0, w as f32) as usize;
        let min_y = (rect.cy - hh).round().clamp(0.0, h as f32) as usize;
        let max_y = (rect.cy + hh).round().clamp(0.0, h as f32) as usize;
        if max_x <= min_x {
            return;
        }
        let stride = w as usize;
        for y in min_y..max_y {
            let base = (y * stride + min_x) * 4;
            let end = (y * stride + max_x) * 4;
            for px in rgba[base..end].chunks_exact_mut(4) {
                px.copy_from_slice(&color);
            }
        }
        return;
    }
    let cos = rect.angle_radians.cos();
    let sin = rect.angle_radians.sin();
    let half_diag = (hw * hw + hh * hh).sqrt();
    let min_x = ((rect.cx - half_diag).floor() as i32).max(0) as u32;
    let min_y = ((rect.cy - half_diag).floor() as i32).max(0) as u32;
    let max_x = ((rect.cx + half_diag).ceil() as i32).max(0).min(w as i32) as u32;
    let max_y = ((rect.cy + half_diag).ceil() as i32).max(0).min(h as i32) as u32;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let dx = x as f32 + 0.5 - rect.cx;
            let dy = y as f32 + 0.5 - rect.cy;
            let lx = dx * cos + dy * sin;
            let ly = -dx * sin + dy * cos;
            if lx.abs() <= hw && ly.abs() <= hh {
                let idx = ((y * w + x) * 4) as usize;
                rgba[idx] = color[0];
                rgba[idx + 1] = color[1];
                rgba[idx + 2] = color[2];
                rgba[idx + 3] = color[3];
            }
        }
    }
}

#[allow(dead_code)]
fn blend_pixel(dst: &mut [u8], src: [u8; 4]) {
    let sa = src[3] as u32;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        dst[0] = src[0];
        dst[1] = src[1];
        dst[2] = src[2];
        dst[3] = 255;
        return;
    }
    let inv = 255 - sa;
    dst[0] = ((src[0] as u32 * sa + dst[0] as u32 * inv) / 255) as u8;
    dst[1] = ((src[1] as u32 * sa + dst[1] as u32 * inv) / 255) as u8;
    dst[2] = ((src[2] as u32 * sa + dst[2] as u32 * inv) / 255) as u8;
    dst[3] = (sa + dst[3] as u32 * inv / 255) as u8;
}
