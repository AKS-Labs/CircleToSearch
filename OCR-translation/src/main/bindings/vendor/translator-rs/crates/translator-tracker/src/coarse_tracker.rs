//! Per-frame coarse pose (the fast half of the async-H split) and the
//! `Correction` it consumes from the Relocalizer. See `async_h_design.md`.
//!
//! The CoarseTracker owns the cheap, every-frame KLT pose in **root→view**
//! coords: it LK-tracks the previous frame's seed correspondences forward and
//! fits `H_root→view`, drifting between corrections. The Relocalizer (the engine,
//! run intermittently) hands back an absolute `Correction`; the tracker weaves
//! it in (same root) or snaps to it (root changed). It never sees leaf anchors
//! or the chain — the Relocalizer maps everything to root coords first.

use std::collections::VecDeque;

use image::GrayImage;

use crate::klt::{DEFAULT_PYRAMID_LEVELS, Pyramid};
use crate::planar_tracker::{TrackerConfig, klt_forward_fit};
use translator_core::coords::{AnchorId, Quadrant};
use translator_core::homography::{invert, mat3_mul, project};

/// Lifecycle verdict from the Relocalizer (which owns the state machine).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Locked,
    Lost,
    ReAcquire,
}

/// Absolute re-alignment from the Relocalizer for frame `frame_idx`. The
/// `refinement_h`/`seeds`/`root_id`/`canonical_rotation` fields are meaningful
/// only when `lifecycle == Locked`.
#[derive(Clone, Debug)]
pub struct Correction {
    pub frame_idx: u64,
    pub lifecycle: Lifecycle,
    /// `H_root→view` at `frame_idx`, authoritative.
    pub refinement_h: [f32; 9],
    /// `(root_x, root_y, view_x, view_y)` inliers at `frame_idx`.
    pub seeds: Vec<(f32, f32, f32, f32)>,
    pub root_id: AnchorId,
    pub canonical_rotation: Quadrant,
    pub inliers: usize,
}

/// What the CoarseTracker emits for compositing on a frame it can place.
#[derive(Clone, Copy, Debug)]
pub struct CoarsePose {
    pub h_root_to_view: [f32; 9],
    pub root_id: AnchorId,
    pub canonical_rotation: Quadrant,
    pub inliers: usize,
}

/// Recent `(frame_idx, H_root→view)` retained so a Correction for an older
/// frame can be woven onto the current pose.
const RING: usize = 16;
/// Minimum KLT inliers to emit a coarse pose. Below this the frame is treated
/// as un-placeable (→ caller's loss-hide grace). Tuned in step 1.
const COARSE_MIN_INLIERS: usize = 8;
/// Cap on seeds carried frame to frame (mirror of the engine's `KLT_MAX_SEEDS`).
const MAX_SEEDS: usize = 80;
/// Element-wise EMA mix toward the latest `current_h` for the compose pose.
/// `compose_h = α · current_h + (1-α) · prev_compose_h`. Smooths the residual
/// per-frame perspective wobble from KLT sub-pixel jitter + descriptor refit
/// noise on the same basin.
///
/// At 30 fps, α = 0.2 puts the first-order low-pass corner near 1 Hz: per-frame
/// (15 Hz) noise is attenuated ~25 dB (≈18×). The async-seam reorder removed
/// the cadence-locked basin flip that drove the visible perspective AB, so the
/// residual to suppress here is the small per-tick refit jitter, and a
/// stronger LPF is fine — hand-held perspective change is well below 1 Hz on
/// device repro.
const COMPOSE_EMA_ALPHA: f32 = 1.0;

pub struct CoarseTracker {
    cfg: TrackerConfig,
    prev_pyramid: Option<Pyramid>,
    /// `(root, view)` correspondences from the previous frame.
    seeds: Vec<(f32, f32, f32, f32)>,
    current_h: Option<[f32; 9]>,
    /// EMA-smoothed companion of `current_h`, exposed to callers as the
    /// compose pose. Updated element-wise toward `current_h` after each fit,
    /// snap, or weave. Kept separate from `current_h` so the Relocalizer's
    /// guided-match prior still sees fresh data (smoothing the prior would
    /// hide real motion from the matcher).
    compose_h: Option<[f32; 9]>,
    root_id: Option<AnchorId>,
    canonical_rotation: Quadrant,
    ring: VecDeque<(u64, [f32; 9])>,
}

impl CoarseTracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Self {
            cfg,
            prev_pyramid: None,
            seeds: Vec::new(),
            current_h: None,
            compose_h: None,
            root_id: None,
            canonical_rotation: Quadrant::default(),
            ring: VecDeque::with_capacity(RING),
        }
    }

    /// Current `H_root→view` — the prior the Relocalizer uses to guide matching.
    /// Raw (un-smoothed) so the matcher sees fresh data; use [`compose_h`] for
    /// display.
    pub fn current_h(&self) -> Option<[f32; 9]> {
        self.current_h
    }

    /// Temporally-smoothed `H_root→view` for compositing. First-order EMA of
    /// `current_h` with mix [`COMPOSE_EMA_ALPHA`]. Equals `current_h` on the
    /// first frame after a reset/snap so a fresh lock isn't lerped from None.
    pub fn compose_h(&self) -> Option<[f32; 9]> {
        self.compose_h
    }

    pub fn current_root(&self) -> Option<AnchorId> {
        self.root_id
    }

    /// `(root_x, root_y, view_x, view_y)` correspondences as of the most
    /// recent `track`. Cloned so the caller can hand them to the worker
    /// without holding the lock. Empty until the first Correction snaps in.
    /// Used by the Relocalizer as KLT-prepend pairs: spatially-distributed
    /// sub-pixel correspondences that anchor RANSAC's perspective DoFs
    /// (which descriptor-only fits get under-determined on under perspective
    /// change).
    pub fn seeds_snapshot(&self) -> Vec<(f32, f32, f32, f32)> {
        self.seeds.clone()
    }

    /// Forget all tracking state (overlay hides until the next Locked
    /// Correction re-seeds). Called on Lost/ReAcquire.
    pub fn reset(&mut self) {
        self.seeds.clear();
        self.current_h = None;
        self.compose_h = None;
        self.root_id = None;
        self.ring.clear();
        // prev_pyramid is left; it's overwritten next `track` and only used
        // when seeds exist.
    }

    /// One per-frame step: build the pyramid, LK-forward the seeds, fit
    /// `H_root→view`. Returns the pose to composite, or `None` when there are no
    /// seeds yet or the fit collapsed (caller applies its loss-hide grace).
    pub fn track(&mut self, gray: &GrayImage, frame_idx: u64) -> Option<CoarsePose> {
        let cur = Pyramid::build(gray, DEFAULT_PYRAMID_LEVELS);
        let pose = self.fit(&cur, frame_idx);
        self.prev_pyramid = Some(cur);
        pose
    }

    fn fit(&mut self, cur: &Pyramid, frame_idx: u64) -> Option<CoarsePose> {
        let prev = self.prev_pyramid.as_ref()?;
        let root_id = self.root_id?;
        let r = klt_forward_fit(
            prev,
            cur,
            &self.seeds,
            &self.cfg,
            COARSE_MIN_INLIERS,
            self.current_h,
        )?;
        self.current_h = Some(r.homography);
        self.compose_h = Some(ema_step(self.compose_h, &r.homography));
        self.seeds = r.inlier_pairs.iter().take(MAX_SEEDS).copied().collect();
        self.ring.push_back((frame_idx, r.homography));
        while self.ring.len() > RING {
            self.ring.pop_front();
        }
        Some(CoarsePose {
            h_root_to_view: r.homography,
            root_id,
            canonical_rotation: self.canonical_rotation,
            inliers: r.inliers,
        })
    }

    /// Fold in a Relocalizer `Correction`. Single-writer: the present thread.
    pub fn apply(&mut self, c: Correction) {
        match c.lifecycle {
            // Drop tracking state — the engine has given up on this anchor.
            // Keeping KLT alive with stale seeds drifts unboundedly under
            // perspective change. The pipeline's loss-hide grace covers the
            // brief gap until the next Locked Correction snaps to a fresh
            // anchor; the right way to avoid frequent re-acquires is to keep
            // the engine in Locked longer (see `degraded_max_frames`), not to
            // mask their effects here.
            Lifecycle::Lost | Lifecycle::ReAcquire => self.reset(),
            Lifecycle::Locked => {
                let root_changed = self.root_id != Some(c.root_id);
                if root_changed {
                    self.snap(c);
                } else {
                    self.weave(c);
                }
            }
        }
    }

    /// Different root (fresh acquire / cross-root relock): adopt the absolute
    /// pose wholesale. Brief staleness for one frame, then KLT catches up.
    fn snap(&mut self, c: Correction) {
        let h = canonicalize(&c.refinement_h);
        self.current_h = Some(h);
        // Reset compose smoothing: a fresh root has no prior trajectory to
        // average with, and lerping from the previous root's pose would drag
        // the new anchor's overlay across the screen for the EMA's settling
        // time.
        self.compose_h = Some(h);
        // Cap at MAX_SEEDS to match `weave` and `fit`. `c.seeds` is the
        // engine's RANSAC inlier set projected to root coords; it's bounded
        // only by `extras + descriptor_matches` upstream and can carry
        // several hundred entries. Without the cap, a snap installs the full
        // set as `self.seeds`, the next dispatch sends them back as extras,
        // the engine RANSAC is dominated by the extras, descriptor_inliers
        // crashes (the fit no longer aligns the descriptor matches), the
        // degraded gate trips, and re-acquire fires → fresh snap with even
        // more seeds. Cap keeps the contract symmetric across all three
        // mutating paths.
        self.seeds = c.seeds.into_iter().take(MAX_SEEDS).collect();
        self.root_id = Some(c.root_id);
        self.canonical_rotation = c.canonical_rotation;
        self.ring.clear();
        self.ring.push_back((c.frame_idx, h));
    }

    /// Same root: correct in place by composing the absolute fix at `frame_idx`
    /// with the view-space motion the coarse path tracked since then.
    /// `H_now := (H_now · inv(H_then)) · refinement_h`. Falls back to snap if the
    /// `frame_idx` pose was evicted from the ring.
    fn weave(&mut self, c: Correction) {
        // Engine-side grace: the engine had no fresh fit + no cached relock on
        // this frame, but it's internally still Locked, so it emits its
        // `last_homography` as the refinement and leaves `seeds` empty. That
        // refinement is stale by design; weaving it in would override the
        // KLT-tracked motion this coarse path has accumulated, freezing the
        // overlay at the last fresh fit until a new one lands (visible as
        // "stuck overlay → sudden jump"). Treat empty-seeds as "no new
        // information": skip the weave entirely, the per-frame KLT in
        // `track()` already advanced `current_h` smoothly.
        if c.seeds.is_empty() {
            self.canonical_rotation = c.canonical_rotation;
            return;
        }
        let (Some(h_now), Some(h_then)) = (self.current_h, self.ring_get(c.frame_idx)) else {
            self.snap(c);
            return;
        };
        let Some(h_then_inv) = invert(&h_then) else {
            self.snap(c);
            return;
        };
        let motion = mat3_mul(&h_now, &h_then_inv); // view_then → view_now
        let woven = canonicalize(&mat3_mul(&motion, &c.refinement_h));
        self.current_h = Some(woven);
        self.compose_h = Some(ema_step(self.compose_h, &woven));
        self.canonical_rotation = c.canonical_rotation;
        // Re-seed from the correction's fresh inliers, projecting their view
        // side forward by the same motion so they line up with the current view.
        self.seeds = c
            .seeds
            .iter()
            .map(|&(rx, ry, vx, vy)| {
                let (vx2, vy2) = project(&motion, vx, vy).unwrap_or((vx, vy));
                (rx, ry, vx2, vy2)
            })
            .take(MAX_SEEDS)
            .collect();
    }

    fn ring_get(&self, frame_idx: u64) -> Option<[f32; 9]> {
        self.ring
            .iter()
            .find(|(idx, _)| *idx == frame_idx)
            .map(|(_, h)| *h)
    }
}

/// Element-wise EMA step toward `target`. On the first call (no previous
/// compose pose) the target is adopted verbatim — no settling from `None`.
/// Caller is expected to canonicalise (`h22 = 1`) before mixing; both source
/// and target should already share that convention.
fn ema_step(prev: Option<[f32; 9]>, target: &[f32; 9]) -> [f32; 9] {
    let Some(p) = prev else {
        return *target;
    };
    let mut out = [0.0_f32; 9];
    for i in 0..9 {
        out[i] = COMPOSE_EMA_ALPHA * target[i] + (1.0 - COMPOSE_EMA_ALPHA) * p[i];
    }
    out
}

/// Divide all nine elements by `h22` so downstream consumers see `h22 = 1`
/// (matches `planar_engine::canonicalize_h`). `project` tolerates `h22 != 1`,
/// but keeping the convention avoids surprises in scale decomposition.
fn canonicalize(h: &[f32; 9]) -> [f32; 9] {
    let d = h[8];
    if d.abs() < 1e-12 {
        return *h;
    }
    let mut out = [0.0; 9];
    for i in 0..9 {
        out[i] = h[i] / d;
    }
    out
}
