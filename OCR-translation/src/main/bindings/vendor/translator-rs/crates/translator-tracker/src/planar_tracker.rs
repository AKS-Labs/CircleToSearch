//! Planar surface tracker built on FAST-9 corners + BRIEF-256 descriptors.
//!
//! Replaces the per-region SAD tracker in `live_tracking.rs`. Instead of
//! tracking each text bbox independently, we fit a single homography
//! per frame mapping the original "canonical" frame (the moment we
//! acquired the surface) to the current frame. All overlays are then
//! projected through that one homography so they move rigidly together.
//!
//! See `FUTURE_PLANAR_TRACKER.md` at the repo root for the design and
//! Phase A benchmark results (which is why we use FAST+BRIEF rather
//! than AKAZE).

use image::GrayImage;
use rayon::prelude::*;

use crate::klt::{KltConfig, Pyramid, track_points};
use translator_core::homography::{
    fit_affine, fit_homography, fit_similarity, invert, mat3_mul, project,
};

/// Master toggle for per-frame tracker timing logs (target
/// `planar_timing`). Flip to `false` to silence the per-frame
/// detect/describe/match/ransac line — useful when something other
/// than the planar pipeline is being investigated and the timing
/// chatter is in the way. The bindings crate has its own equivalent
/// gate on the `process_and_composite` outer line.
pub const PER_FRAME_TIMING_LOG: bool = false;

/// BRIEF descriptor length in bits. 256 is the original BRIEF default;
/// 32 bytes per keypoint, Hamming distance fits in a u32.
pub const BRIEF_BITS: usize = 256;
/// Byte width of one descriptor.
pub const DESCRIPTOR_BYTES: usize = BRIEF_BITS / 8;
/// Patch around each keypoint sampled by BRIEF: a `(2 * R + 1)`-square
/// area in pixels. 15 → 31x31 patch.
pub const BRIEF_PATCH_RADIUS: i32 = 15;
/// Distance from image edge required for a keypoint to be describable.
/// Sample offsets are within `BRIEF_PATCH_RADIUS` of the centre, but
/// after rotating the BRIEF pattern by an arbitrary angle, the worst
/// case is a corner sample at `(R, R)` rotating to `(0, R√2)` — so we
/// need at least `ceil(R·√2)` plus the 3x3 box-blur neighbourhood.
/// R=15 → ceil(15·√2)=22 → +1 box blur = 23. Round up to 24 for slack.
pub const KEYPOINT_BORDER: i32 = 24;

/// FAST-9 corner. Score is the sum-of-absolute-differences between the
/// 16 circle pixels and the center; used only for non-max suppression.
#[derive(Clone, Copy, Debug)]
pub struct KeyPoint {
    pub x: f32,
    pub y: f32,
    pub score: i32,
}

/// 256-bit BRIEF descriptor. Hamming distance fits in a u32.
#[derive(Clone, Copy, Debug)]
pub struct Descriptor(pub [u8; DESCRIPTOR_BYTES]);

impl Descriptor {
    pub fn hamming(&self, other: &Self) -> u32 {
        let mut d = 0u32;
        for i in 0..DESCRIPTOR_BYTES {
            d += (self.0[i] ^ other.0[i]).count_ones();
        }
        d
    }
}

/// Reference scene captured at acquire time. Each keypoint slot has a
/// position (in this frame's pixel coords — the "canonical" frame for
/// the current acquisition) and a descriptor.
#[derive(Clone, Debug)]
pub struct SceneAnchor {
    pub descriptors: Vec<Descriptor>,
    pub positions: Vec<(f32, f32)>,
    pub image_dims: (u32, u32),
    pub created_at_ns: u64,
}

impl SceneAnchor {
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}

/// Result of a successful per-frame fit.
#[derive(Clone, Debug)]
pub struct TrackResult {
    /// Row-major 3x3 mapping canonical-frame pixel coords → current-frame
    /// pixel coords. See `crate::homography::project`.
    pub homography: [f32; 9],
    /// Number of correspondences that survived RANSAC + final LS fit.
    pub inliers: usize,
    /// Total matched correspondences (before RANSAC).
    pub matches: usize,
    /// Median re-projection residual in pixels (over inliers).
    pub median_residual_px: f32,
    /// The actual inlier correspondences `(anchor_x, anchor_y, view_x,
    /// view_y)` — populated by RANSAC for callers that need to feed
    /// them back next frame (e.g. the KLT propagator).
    pub inlier_pairs: Vec<(f32, f32, f32, f32)>,
    /// Subset of `inliers` that came from the descriptor matcher
    /// (i.e. not from externally-injected KLT correspondences). The
    /// engine uses this to drive handoff decisions on descriptor-
    /// matcher health rather than the KLT-padded total, which would
    /// otherwise mask sustained descriptor degradation (e.g. scale
    /// drift past BRIEF's invariance) and starve handoffs.
    pub descriptor_inliers: usize,
}

/// Public-facing tracker state. Wraps an optional anchor and the
/// detection / matching tuning knobs.
pub struct LivePlanarTracker {
    pub anchor: Option<SceneAnchor>,
    pub config: TrackerConfig,
}

#[derive(Clone, Copy, Debug)]
pub struct TrackerConfig {
    /// FAST corner threshold (0..255). Higher → fewer, stronger corners.
    pub fast_threshold: u8,
    /// Fallback FAST threshold for the Locked-state matcher: if a
    /// first-pass detect at `fast_threshold` returns fewer than
    /// `fast_min_keypoints`, we redetect at this looser threshold to
    /// catch corners that motion blur has softened below the strict
    /// score cutoff. Used only on the per-frame match path; acquire and
    /// re-lock keep `fast_threshold` so anchor quality stays high.
    pub fast_threshold_fallback: u8,
    /// Minimum first-pass keypoint count below which the Locked matcher
    /// triggers the `fast_threshold_fallback` redetect.
    pub fast_min_keypoints: usize,
    /// Cap on keypoints returned by detection — strongest survive.
    pub max_features: usize,
    /// Lowe ratio test cutoff (best/second-best Hamming) used by
    /// fresh-acquire and re-lock paths. Lower = stricter, fewer false
    /// matches, fewer total matches; safe default for cold starts where
    /// we have no geometric prior to filter the surviving pool.
    pub lowe_ratio: f32,
    /// Lowe ratio used when the tracker is *already* Locked. Relaxed
    /// (default 0.9) so that descriptors degraded by motion blur — best
    /// vs second-best ratio creeps from 0.7 toward 1.0 as the patch
    /// smears — still get through to RANSAC. The previous-frame H seed
    /// and the engine's sanity gate (corner-jump cap, inlier-bbox
    /// coverage) filter the resulting false matches; the safety net
    /// only triggers when guess-quality has actually collapsed.
    pub lowe_ratio_locked: f32,
    /// RANSAC inlier residual threshold in pixels.
    pub ransac_residual_px: f32,
    /// RANSAC iterations.
    pub ransac_iters: usize,
    /// Minimum inliers to *acquire* a successful track (called from a
    /// cold start or from Lost-recovery). Higher = stricter, but
    /// avoids locking onto a noisy match.
    pub min_inliers: usize,
    /// Minimum inliers to *keep* a currently-Locked track. Lower than
    /// `min_inliers` to give hysteresis — a Locked track that drops a
    /// frame or two near the threshold shouldn't flicker between
    /// Locked and Lost on every camera frame.
    pub min_inliers_keep_locked: usize,
    /// Non-max suppression radius for FAST corners (in pixels).
    pub nms_radius: i32,
    /// Search-window radius for guided descriptor matching, in
    /// pixel-coords of the gray the matcher operates on. The
    /// per-frame Locked matcher uses this when an `H_anchor→view`
    /// prior is provided (= we're tracking, not re-acquiring). Set to
    /// roughly cover the worst-case feature displacement between two
    /// adjacent frames: at 30 fps with handheld slow pan that's
    /// ~10-20 px; we leave headroom for moderate motion.
    pub guided_search_radius_px: f32,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            fast_threshold: 15,
            fast_threshold_fallback: 7,
            fast_min_keypoints: 200,
            max_features: 500,
            lowe_ratio: 0.8,
            lowe_ratio_locked: 0.85,
            ransac_residual_px: 4.0,
            ransac_iters: 400,
            min_inliers: 25,
            // Hysteresis floor for keeping a Locked track. Set to 10
            // so the cascading-model refinement in
            // `ransac_homography_with_prior` can fall through to the
            // 4-DoF similarity fit at low inlier counts — earlier we
            // pinned this at 18 (mid-affine band), which cut RANSAC
            // off before the similarity path ever engaged. Similarity
            // is over-flat on tilted planes (no perspective), but at
            // sub-15-inlier counts the 8-DoF homography is so under-
            // constrained that it produces worse drift than the flat
            // similarity does; better a stable 4-DoF lock through a
            // brief blur than a wild 8-DoF wrong-basin fit. The
            // per-frame H-delta cap + sanity gate spread-coverage
            // check still catch egregiously bad similarity fits.
            min_inliers_keep_locked: 25,
            nms_radius: 3,
            guided_search_radius_px: 30.0,
        }
    }
}

impl LivePlanarTracker {
    pub fn new() -> Self {
        Self::with_config(TrackerConfig::default())
    }

    pub fn with_config(config: TrackerConfig) -> Self {
        Self {
            anchor: None,
            config,
        }
    }

    /// Capture `gray` as the new reference scene. Replaces any existing
    /// anchor. Returns `true` if enough features were found to be useful.
    pub fn acquire(&mut self, gray: &GrayImage, created_at_ns: u64) -> bool {
        let anchor = build_anchor(gray, &self.config, created_at_ns);
        let ok = anchor
            .as_ref()
            .is_some_and(|a| a.len() >= self.config.min_inliers);
        self.anchor = anchor;
        ok
    }

    /// Attempt to fit a homography from the current anchor's canonical
    /// frame to `gray`. Returns `None` if there is no anchor or if the
    /// inlier count falls below `config.min_inliers`.
    pub fn track(&self, gray: &GrayImage) -> Option<TrackResult> {
        let anchor = self.anchor.as_ref()?;
        track_against_anchor(anchor, gray, &self.config)
    }

    pub fn clear(&mut self) {
        self.anchor = None;
    }
}

impl Default for LivePlanarTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level scene-build helper exposed for tests and benches.
pub fn build_anchor(
    gray: &GrayImage,
    cfg: &TrackerConfig,
    created_at_ns: u64,
) -> Option<SceneAnchor> {
    build_anchor_filtered(gray, cfg, created_at_ns, |_x, _y| true)
}

/// Like [`build_anchor`] but only keeps keypoints inside any of the
/// given axis-aligned regions (padded by `pad_px` on each side).
///
/// Why: when the camera is looking at a moving object against a richly
/// textured background (e.g. a book on a duvet), naive whole-frame
/// feature extraction picks up many more background corners than object
/// corners. RANSAC then locks onto the background, and the overlays
/// stop following the object when it moves but the phone doesn't.
///
/// Restricting anchor features to the detected text regions keeps the
/// tracker focused on the surface that actually carries the OCR
/// content.
pub fn build_anchor_in_regions(
    gray: &GrayImage,
    cfg: &TrackerConfig,
    regions: &[(u32, u32, u32, u32)],
    pad_px: u32,
    created_at_ns: u64,
) -> Option<SceneAnchor> {
    if regions.is_empty() {
        return build_anchor(gray, cfg, created_at_ns);
    }
    let (img_w, img_h) = gray.dimensions();
    let padded: Vec<(f32, f32, f32, f32)> = regions
        .iter()
        .map(|&(l, t, r, b)| {
            let lf = (l.saturating_sub(pad_px)) as f32;
            let tf = (t.saturating_sub(pad_px)) as f32;
            let rf = (r.saturating_add(pad_px).min(img_w)) as f32;
            let bf = (b.saturating_add(pad_px).min(img_h)) as f32;
            (lf, tf, rf, bf)
        })
        .collect();
    build_anchor_filtered(gray, cfg, created_at_ns, move |x, y| {
        padded
            .iter()
            .any(|&(l, t, r, b)| x >= l && x < r && y >= t && y < b)
    })
}

fn build_anchor_filtered<F: Fn(f32, f32) -> bool>(
    gray: &GrayImage,
    cfg: &TrackerConfig,
    created_at_ns: u64,
    accept: F,
) -> Option<SceneAnchor> {
    let (w, h) = gray.dimensions();
    let mut kps = detect_fast(gray, cfg.fast_threshold, cfg.max_features, cfg.nms_radius);
    kps.retain(|k| accept(k.x, k.y));
    if kps.is_empty() {
        return None;
    }
    let (kept_kps, descs) = describe_brief(gray, &kps);
    if kept_kps.is_empty() {
        return None;
    }
    let positions: Vec<(f32, f32)> = kept_kps.iter().map(|k| (k.x, k.y)).collect();
    Some(SceneAnchor {
        descriptors: descs,
        positions,
        image_dims: (w, h),
        created_at_ns,
    })
}

/// Single-frame fit. Used internally by `track` and exposed for tests.
pub fn track_against_anchor(
    anchor: &SceneAnchor,
    gray: &GrayImage,
    cfg: &TrackerConfig,
) -> Option<TrackResult> {
    track_against_anchor_with_min(anchor, gray, cfg, cfg.min_inliers)
}

/// Like [`track_against_anchor`] but lets the caller override the
/// inlier threshold. Used by the engine to apply hysteresis: a higher
/// bar for *acquiring* a Locked state vs. a lower bar for *keeping*
/// one we already have. Without this split the per-frame inlier
/// count flickers across `min_inliers` and the state machine
/// thrashes Locked ↔ Lost every frame.
pub fn track_against_anchor_with_min(
    anchor: &SceneAnchor,
    gray: &GrayImage,
    cfg: &TrackerConfig,
    min_inliers: usize,
) -> Option<TrackResult> {
    track_against_anchor_with_prior(anchor, gray, cfg, min_inliers, None)
}

/// Like [`track_against_anchor_with_min`] but seeded with a `prior`
/// homography. The prior is evaluated as the first hypothesis in
/// RANSAC; if it already has many inliers, RANSAC's random iterations
/// rarely beat it, so we converge to a clean fit immediately. Use this
/// with an IMU-derived prediction when fast pan / rotation would
/// otherwise make random sampling miss the answer.
pub fn track_against_anchor_with_prior(
    anchor: &SceneAnchor,
    gray: &GrayImage,
    cfg: &TrackerConfig,
    min_inliers: usize,
    prior: Option<[f32; 9]>,
) -> Option<TrackResult> {
    track_against_anchor_with_prior_and_extra(anchor, gray, cfg, min_inliers, prior, &[])
}

/// Per-frame keypoint + descriptor cache. Reuse one set of features
/// across multiple anchor-match attempts (Lost-state cached-anchor
/// retry loop) instead of redetecting/redescribing per anchor.
#[derive(Clone, Debug)]
pub struct FrameFeatures {
    pub frame_kps: Vec<KeyPoint>,
    pub frame_descs: Vec<Descriptor>,
    /// Raw FAST keypoint count before describe_brief filtered the
    /// near-edge ones. Carried for diagnostics / timing logs only.
    pub raw_kp_count: usize,
}

/// FAST + BRIEF on a single frame, with the same blur-fallback
/// behaviour used inside `track_against_anchor_with_prior_and_extra`.
/// Returns `None` if no usable keypoints survive.
pub fn compute_frame_features(gray: &GrayImage, cfg: &TrackerConfig) -> Option<FrameFeatures> {
    let mut kps = detect_fast(gray, cfg.fast_threshold, cfg.max_features, cfg.nms_radius);
    if kps.len() < cfg.fast_min_keypoints && cfg.fast_threshold_fallback < cfg.fast_threshold {
        let retry = detect_fast(
            gray,
            cfg.fast_threshold_fallback,
            cfg.max_features,
            cfg.nms_radius,
        );
        if retry.len() > kps.len() {
            kps = retry;
        }
    }
    let raw_kp_count = kps.len();
    if kps.is_empty() {
        return None;
    }
    let (frame_kps, frame_descs) = describe_brief(gray, &kps);
    if frame_kps.is_empty() {
        return None;
    }
    Some(FrameFeatures {
        frame_kps,
        frame_descs,
        raw_kp_count,
    })
}

/// Like [`track_against_anchor_with_prior_and_extra`] but with the
/// per-frame FAST + BRIEF already computed by
/// [`compute_frame_features`]. Use when matching the same frame
/// against multiple candidate anchors (e.g. the Lost-state cached-
/// anchor retry loop).
pub fn track_against_anchor_with_features(
    anchor: &SceneAnchor,
    features: &FrameFeatures,
    cfg: &TrackerConfig,
    min_inliers: usize,
    prior: Option<[f32; 9]>,
    extra_pairs: &[(f32, f32, f32, f32)],
) -> Option<TrackResult> {
    let frame_kps = &features.frame_kps;
    let frame_descs = &features.frame_descs;
    let t2 = std::time::Instant::now();
    // Guided when we have a prior (= Locked, tracking through the
    // scene): restrict each anchor's match candidates to a small
    // window around its predicted view position. The narrow window
    // lets blur-degraded descriptors pass Lowe ratio because they
    // compete only against the ~handful of in-window candidates
    // instead of the global pool, and the resulting inlier set is
    // spread across the anchor (every anchor keypoint with a frame
    // feature near its prediction can match), eliminating the
    // clustered-inlier drift seen with brute matching. Brute path
    // remains for re-acquire (no prior) where the previous-frame
    // location is meaningless.
    let mut matches = match prior {
        Some(h_prior) => match_descriptors_guided(
            &anchor.descriptors,
            &anchor.positions,
            frame_kps,
            frame_descs,
            cfg.lowe_ratio_locked,
            &h_prior,
            cfg.guided_search_radius_px,
        ),
        None => match_descriptors(&anchor.descriptors, frame_descs, cfg.lowe_ratio_locked),
    };
    // Brute fallback. When the guided matcher returns few matches the prior
    // is likely stale (perspective drift between engine ticks pushes real
    // descriptor positions outside the 30 px window). Brute considers every
    // frame keypoint and finds what the windowed search missed. The fit
    // produced from these recovered matches propagates back to the prior
    // via the coarse weave, so this fallback fires only on the frames where
    // the prior went stale — steady state stays in guided.
    const GUIDED_BRUTE_FALLBACK_THRESHOLD: usize = 75;
    if prior.is_some() && matches.len() < GUIDED_BRUTE_FALLBACK_THRESHOLD {
        let brute = match_descriptors(&anchor.descriptors, frame_descs, cfg.lowe_ratio_locked);
        if brute.len() > matches.len() {
            matches = brute;
        }
    }
    matches.sort_unstable_by_key(|m| m.distance);
    let t_match = t2.elapsed();
    let n_matches = matches.len();
    if matches.len() < 4 {
        if PER_FRAME_TIMING_LOG {
            log::info!(
                target: "planar_timing",
                "match-only: match={:.1}ms (matches={}) → bail",
                t_match.as_secs_f64() * 1000.0,
                n_matches,
            );
        }
        return None;
    }
    let mut pairs: Vec<(f32, f32, f32, f32)> =
        Vec::with_capacity(extra_pairs.len() + matches.len());
    pairs.extend_from_slice(extra_pairs);
    pairs.extend(matches.iter().map(|m| {
        let (ax, ay) = anchor.positions[m.anchor_idx];
        let fk = &frame_kps[m.frame_idx];
        (ax, ay, fk.x, fk.y)
    }));
    let t3 = std::time::Instant::now();
    let mut out = ransac_homography_with_prior(&pairs, cfg, min_inliers, prior);
    let t_ransac = t3.elapsed();
    if let Some(r) = out.as_mut() {
        let r_thresh_sq = cfg.ransac_residual_px * cfg.ransac_residual_px;
        let n_klt = extra_pairs.len();
        let descriptor_pairs = &pairs[n_klt..];
        r.descriptor_inliers = descriptor_pairs
            .iter()
            .filter(|&&(px, py, qx, qy)| {
                match translator_core::homography::project(&r.homography, px, py) {
                    Some((px2, py2)) => {
                        let dx = px2 - qx;
                        let dy = py2 - qy;
                        dx * dx + dy * dy <= r_thresh_sq
                    }
                    None => false,
                }
            })
            .count();
        // Temporal-prior post-refit was tried here and reverted:
        // `homography::fit_homography_with_temporal_prior` with
        // `lambda = max(0, 50 - desc_inliers)` (sparse-only) helped
        // book (M2 19.3 → 13.9) but regressed magn (M2 6.4 → 19.4)
        // and increased Lost frames on park/gintonic. The
        // fundamental tension: λ strong enough to break wrong-basin
        // ties is strong enough to lag during real motion, and
        // descriptor inlier count alone doesn't distinguish
        // "sparse because under-determined" from "sparse because
        // fast motion / transitional frame". A proper Kalman-style
        // per-DoF covariance tracking would solve this; the simple
        // damped DLT cannot. The implementation
        // `fit_homography_with_temporal_prior` is kept in
        // `homography.rs` for future use. See `analysis.md` § "EKF
        // path" for the principled-fit roadmap.
    }
    if PER_FRAME_TIMING_LOG {
        log::info!(
            target: "planar_timing",
            "match-only: match={:.1}ms ransac={:.1}ms (matches={} inliers={:?} desc_inliers={:?})",
            t_match.as_secs_f64() * 1000.0,
            t_ransac.as_secs_f64() * 1000.0,
            n_matches,
            out.as_ref().map(|r| r.inliers),
            out.as_ref().map(|r| r.descriptor_inliers),
        );
    }
    out
}

/// Like [`track_against_anchor_with_prior`] but accepts pre-computed
/// `(anchor_x, anchor_y, view_x, view_y)` correspondences from outside
/// the descriptor matcher (e.g. KLT-tracked inliers from the previous
/// frame). Extras are prepended to the descriptor-matched pairs so
/// PROSAC's high-quality early phases sample them first.
pub fn track_against_anchor_with_prior_and_extra(
    anchor: &SceneAnchor,
    gray: &GrayImage,
    cfg: &TrackerConfig,
    min_inliers: usize,
    prior: Option<[f32; 9]>,
    extra_pairs: &[(f32, f32, f32, f32)],
) -> Option<TrackResult> {
    let features = compute_frame_features(gray, cfg)?;
    track_against_anchor_with_features(anchor, &features, cfg, min_inliers, prior, extra_pairs)
}

/// One match between an anchor descriptor and a current-frame descriptor.
#[derive(Clone, Copy, Debug)]
pub struct Match {
    pub anchor_idx: usize,
    pub frame_idx: usize,
    pub distance: u32,
}

/// Brute-force Hamming matcher with Lowe ratio test. For every anchor
/// descriptor, find the two closest frame descriptors and accept iff
/// `best.distance < lowe_ratio * second.distance`.
pub fn match_descriptors(
    anchor: &[Descriptor],
    frame: &[Descriptor],
    lowe_ratio: f32,
) -> Vec<Match> {
    if frame.len() < 2 {
        return Vec::new();
    }
    // Each anchor descriptor's best/second-best search is independent,
    // so fan out over the active pool. Indexed par-iter preserves anchor
    // order, so the result matches the serial scan before the caller's
    // distance sort.
    anchor
        .par_iter()
        .enumerate()
        .filter_map(|(a_idx, a_desc)| {
            let mut best = u32::MAX;
            let mut best_idx = 0usize;
            let mut second = u32::MAX;
            for (f_idx, f_desc) in frame.iter().enumerate() {
                let d = a_desc.hamming(f_desc);
                if d < best {
                    second = best;
                    best = d;
                    best_idx = f_idx;
                } else if d < second {
                    second = d;
                }
            }
            if (best as f32) < lowe_ratio * (second as f32) {
                Some(Match {
                    anchor_idx: a_idx,
                    frame_idx: best_idx,
                    distance: best,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Guided Hamming matcher. Uses `h_prior` (typically the last-frame
/// `H_anchor→view`) to project each anchor keypoint into expected view
/// coords; only frame keypoints inside a `search_radius_px`-pixel disk
/// around that prediction are candidates. The Lowe ratio test is
/// applied within that restricted set, so a blur-degraded descriptor —
/// whose "best" candidate in brute matching is barely better than the
/// "second best" amongst the global ~500 frame keypoints — competes
/// only against the handful of candidates in its predicted neighbourhood
/// and usually wins decisively. This is what lets the matcher *survive*
/// motion blur and produce spread inliers when brute matching would
/// collapse to clustered noise.
///
/// Returns brute-style matches; downstream RANSAC + sanity gate are
/// unchanged.
pub fn match_descriptors_guided(
    anchor_descs: &[Descriptor],
    anchor_positions: &[(f32, f32)],
    frame_kps: &[KeyPoint],
    frame_descs: &[Descriptor],
    lowe_ratio: f32,
    h_prior: &[f32; 9],
    search_radius_px: f32,
) -> Vec<Match> {
    if frame_kps.len() < 2 {
        return Vec::new();
    }
    let r_sq = search_radius_px * search_radius_px;
    // Per-anchor windowed search is independent; fan out over the pool.
    // Indexed par-iter preserves anchor order, matching the serial scan
    // before the caller's distance sort.
    (0..anchor_descs.len())
        .into_par_iter()
        .filter_map(|a_idx| {
            let a_desc = &anchor_descs[a_idx];
            let a_pos = anchor_positions[a_idx];
            let predicted = project(h_prior, a_pos.0, a_pos.1)?;
            let mut best = u32::MAX;
            let mut best_idx = 0usize;
            let mut second = u32::MAX;
            let mut window_count = 0usize;
            for (f_idx, fkp) in frame_kps.iter().enumerate() {
                let dx = fkp.x - predicted.0;
                let dy = fkp.y - predicted.1;
                if dx * dx + dy * dy > r_sq {
                    continue;
                }
                window_count += 1;
                let d = a_desc.hamming(&frame_descs[f_idx]);
                if d < best {
                    second = best;
                    best = d;
                    best_idx = f_idx;
                } else if d < second {
                    second = d;
                }
            }
            // The Lowe ratio test needs a second-best to measure ambiguity
            // against. With exactly one in-window candidate there's nothing
            // to compare to — but the geometric prior + tight guided window
            // already vouch for the candidate's position, so fall back to an
            // absolute Hamming threshold. Without this fallback, sparse FAST
            // output or blur-degraded patches lose stable single-candidate
            // matches that the prior trusts. Tight threshold to avoid
            // admitting random matches: BRIEF-256 random distance is ~128
            // bits, "clearly same patch" is typically < 40, so 60 bits
            // (~23 % of the descriptor) is well inside the safe zone.
            const SINGLE_CANDIDATE_HAMMING_THRESHOLD: u32 = 60;
            if window_count == 0 {
                return None;
            }
            if window_count == 1 {
                return if best < SINGLE_CANDIDATE_HAMMING_THRESHOLD {
                    Some(Match {
                        anchor_idx: a_idx,
                        frame_idx: best_idx,
                        distance: best,
                    })
                } else {
                    None
                };
            }
            if (best as f32) < lowe_ratio * (second as f32) {
                Some(Match {
                    anchor_idx: a_idx,
                    frame_idx: best_idx,
                    distance: best,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Inlier penalty per pixel of corner-projection delta from the prior,
/// applied beyond [`PRIOR_PENALTY_DEAD_PX`]. Saturates at
/// [`PRIOR_PENALTY_MAX_INLIERS`] so a far-off but genuinely-supported basin
/// can still win when its inlier margin over the prior is large enough.
const PRIOR_PENALTY_PER_PX: f32 = 1.0;
/// Corner-projection delta from the prior below which no penalty is applied.
/// Sized for the legitimate per-tick motion of a hand-held camera (~5–8 px
/// at this resolution) so real movement isn't discouraged.
const PRIOR_PENALTY_DEAD_PX: f32 = 8.0;
/// Saturation cap on the penalty in units of inliers. Two basins disagree
/// over the far corner by up to ~30 px in the ABAB repro; capping at 30 means
/// the prior wins ties within that radius but a new basin with ≥30 more
/// inliers (a clear win) overrides it within one tick.
const PRIOR_PENALTY_MAX_INLIERS: f32 = 30.0;
/// If the prior's inlier count is at least this fraction of the best PROSAC
/// sample's, refit on the prior's inliers instead. Locks per-tick refit
/// determinism on still scenes; on a real scene change the new basin's
/// inlier count exceeds 1/FRAC × the prior's, so this branch doesn't fire.
const PRIOR_SHORT_CIRCUIT_FRAC: f32 = 0.9;

/// Max corner-projection delta in pixels between two homographies,
/// evaluated at the four `corners`. Used by the RANSAC prior-aware penalty
/// to express "how different is this candidate's geometry from the prior's
/// over the data region".
fn max_corner_delta_px(h_a: &[f32; 9], h_b: &[f32; 9], corners: &[(f32, f32); 4]) -> f32 {
    let mut max = 0.0_f32;
    for &(x, y) in corners {
        let (Some((ax, ay)), Some((bx, by))) = (project(h_a, x, y), project(h_b, x, y)) else {
            continue;
        };
        let dx = ax - bx;
        let dy = ay - by;
        let d = (dx * dx + dy * dy).sqrt();
        if d > max {
            max = d;
        }
    }
    max
}

/// Axis-aligned bounding box of the anchor side of `pairs`, as four corners.
/// Falls back to a zero-extent quad when `pairs` is empty.
fn pairs_anchor_aabb_corners(pairs: &[(f32, f32, f32, f32)]) -> [(f32, f32); 4] {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for &(x, y, _, _) in pairs {
        if x < min_x {
            min_x = x;
        }
        if y < min_y {
            min_y = y;
        }
        if x > max_x {
            max_x = x;
        }
        if y > max_y {
            max_y = y;
        }
    }
    if !min_x.is_finite() || !max_x.is_finite() {
        return [(0.0, 0.0); 4];
    }
    [
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
        (min_x, max_y),
    ]
}

/// RANSAC scoring with a soft pull toward the prior basin. Equals raw inlier
/// count when no prior is supplied or when the candidate is within
/// [`PRIOR_PENALTY_DEAD_PX`] of the prior at every reference corner.
fn adjusted_inlier_score(
    inliers: usize,
    h: &[f32; 9],
    prior: Option<&[f32; 9]>,
    reference_corners: &[(f32, f32); 4],
) -> f32 {
    let raw = inliers as f32;
    let Some(p) = prior else {
        return raw;
    };
    let delta = max_corner_delta_px(h, p, reference_corners);
    let above_dead = (delta - PRIOR_PENALTY_DEAD_PX).max(0.0);
    let penalty = (PRIOR_PENALTY_PER_PX * above_dead).min(PRIOR_PENALTY_MAX_INLIERS);
    raw - penalty
}

/// RANSAC over `(canonical_x, canonical_y, frame_x, frame_y)` pairs.
/// Samples 4 random correspondences per iteration, fits a homography,
/// counts inliers (residual under `cfg.ransac_residual_px`), keeps the
/// best, then re-fits on all inliers.
pub fn ransac_homography(
    pairs: &[(f32, f32, f32, f32)],
    cfg: &TrackerConfig,
) -> Option<TrackResult> {
    ransac_homography_with_min(pairs, cfg, cfg.min_inliers)
}

pub fn ransac_homography_with_min(
    pairs: &[(f32, f32, f32, f32)],
    cfg: &TrackerConfig,
    min_inliers: usize,
) -> Option<TrackResult> {
    ransac_homography_with_prior(pairs, cfg, min_inliers, None)
}

pub fn ransac_homography_with_prior(
    pairs: &[(f32, f32, f32, f32)],
    cfg: &TrackerConfig,
    min_inliers: usize,
    prior: Option<[f32; 9]>,
) -> Option<TrackResult> {
    if pairs.len() < 4 {
        return None;
    }
    let mut rng = SmallRng::from_seed(0xA5A5_5A5A_3C3C_C3C3);
    let mut best_h: Option<[f32; 9]> = None;
    let mut best_inliers_idx: Vec<usize> = Vec::new();
    let mut best_score: f32 = f32::NEG_INFINITY;
    let r_thresh_sq = cfg.ransac_residual_px * cfg.ransac_residual_px;
    let n = pairs.len();
    // Only trust the prior if it's *still* well-supported by the
    // current frame's correspondences. Threshold = half of min_inliers
    // — if fewer than that many current matches fit the prior, the
    // prior is stale (the scene has moved enough that it no longer
    // explains the data) and biasing RANSAC toward it would be
    // counter-productive. This prevents the "stuck prior" failure
    // where a fast camera move makes the previous H useless but the
    // prior's barely-passing inlier count blocks random samples from
    // displacing it.
    let prior_min_inliers = (min_inliers / 2).max(1);
    // Reference corners for the prior-aware penalty: AABB of the anchor side
    // of the pairs. Penalty measures basin discontinuity over the actual data
    // region (no separate dims parameter required, and the magnitude is in
    // view pixels at the resolution the fit is being computed in).
    let reference_corners = pairs_anchor_aabb_corners(pairs);
    // Track the prior's inlier set separately from the running winner so the
    // post-PROSAC short-circuit can revert to it even when an adjusted-score
    // PROSAC sample temporarily displaced it.
    let mut prior_inliers_idx: Option<Vec<usize>> = None;
    if let Some(h) = prior {
        let mut inliers_idx = Vec::with_capacity(n);
        for (i, &(px, py, qx, qy)) in pairs.iter().enumerate() {
            if let Some((px2, py2)) = project(&h, px, py) {
                let dx = px2 - qx;
                let dy = py2 - qy;
                if dx * dx + dy * dy <= r_thresh_sq {
                    inliers_idx.push(i);
                }
            }
        }
        if inliers_idx.len() >= prior_min_inliers {
            // Prior's corner-delta against itself is 0, so its adjusted score
            // equals its raw inlier count. PROSAC samples have to beat this
            // either on raw inliers (by more than their corner-delta penalty)
            // or on prior-agreement (small delta means small penalty).
            best_score = inliers_idx.len() as f32;
            prior_inliers_idx = Some(inliers_idx.clone());
            best_inliers_idx = inliers_idx;
            best_h = Some(h);
        }
    }
    // Only apply the prior-aware penalty when the prior actually had enough
    // support to seed the search. A bad/stale prior would otherwise distort
    // candidate scoring away from genuinely-better basins for no reason.
    let prior_for_scoring: Option<[f32; 9]> = prior.filter(|_| prior_inliers_idx.is_some());
    // PROSAC schedule: assumes `pairs` is pre-sorted by match quality
    // (best first). Early iters sample from a small top-quality
    // prefix; the prefix grows in four phases over the iteration
    // budget so that bad-prior tops get a chance to lose to broader
    // samples later. Quarters were picked over a smooth curve for
    // implementation simplicity — the gain over uniform random is
    // primarily from phase 0, not the exact growth law.
    let quarter = (cfg.ransac_iters / 4).max(1);
    for t in 0..cfg.ransac_iters {
        let phase = (t / quarter).min(3);
        let growth_cap = match phase {
            0 => (n / 4).max(8).min(n),
            1 => (n / 2).max(8).min(n),
            2 => (n * 3 / 4).max(8).min(n),
            _ => n,
        };
        let mut sample = [(0.0f32, 0.0f32, 0.0f32, 0.0f32); 4];
        let mut idxs = [0usize; 4];
        for k in 0..4 {
            loop {
                let candidate = (rng.next_u32() as usize) % growth_cap;
                if idxs[..k].iter().all(|&p| p != candidate) {
                    idxs[k] = candidate;
                    sample[k] = pairs[candidate];
                    break;
                }
            }
        }
        let h = match fit_homography(&sample) {
            Some(h) => h,
            None => continue,
        };
        let mut inliers_idx = Vec::with_capacity(n);
        for (i, &(px, py, qx, qy)) in pairs.iter().enumerate() {
            let Some((px2, py2)) = project(&h, px, py) else {
                continue;
            };
            let dx = px2 - qx;
            let dy = py2 - qy;
            if dx * dx + dy * dy <= r_thresh_sq {
                inliers_idx.push(i);
            }
        }
        if inliers_idx.is_empty() {
            continue;
        }
        // Prior-aware scoring. On the same scene the descriptor matcher can
        // yield two near-equal-inlier basins differing in the perspective
        // DoFs; without a tiebreak, PROSAC sample order picks one at random
        // per tick (visible as the cadence-N ABAB pivot wobble at the far
        // corner from the inlier centroid). Subtract a saturating penalty
        // proportional to the corner-projection delta from the prior — on
        // basin-flip this discounts the non-prior basin enough for the prior
        // to keep winning, while real perspective change still transitions
        // because the new basin out-inliers the prior beyond the penalty cap.
        let score = adjusted_inlier_score(
            inliers_idx.len(),
            &h,
            prior_for_scoring.as_ref(),
            &reference_corners,
        );
        if score > best_score {
            best_score = score;
            best_inliers_idx = inliers_idx;
            best_h = Some(h);
        }
    }
    // Prior-as-seed short-circuit. If the prior already explains ≥ this
    // fraction of what the best PROSAC sample found, refit on the prior's
    // inlier set. Two effects: (a) per-tick determinism — same input + same
    // prior → same refit set → same H (no sampling-order wobble), and (b) on
    // a still scene the prior is the canonical basin, so we shouldn't trade
    // it for a slightly-better-on-paper PROSAC sample that's likely just a
    // sampling artefact. A clear-winner basin (>1/SHORT_CIRCUIT_FRAC times the
    // prior's inliers) still takes over because it falls below the threshold.
    if let (Some(prior_idx), Some(p_h)) = (prior_inliers_idx.as_ref(), prior) {
        let winner_raw = best_inliers_idx.len();
        if winner_raw > 0 {
            let frac = prior_idx.len() as f32 / winner_raw as f32;
            if frac >= PRIOR_SHORT_CIRCUIT_FRAC {
                best_inliers_idx = prior_idx.clone();
                best_h = Some(p_h);
            }
        }
    }
    if best_inliers_idx.len() < min_inliers {
        return None;
    }
    let _ = best_h?;
    let inlier_pairs: Vec<(f32, f32, f32, f32)> =
        best_inliers_idx.iter().map(|&i| pairs[i]).collect();
    // Adaptive model: pick fitter complexity by inlier count. At sparse
    // inlier counts the 8-DoF homography fit is under-constrained — the
    // perspective entries (h6/h7) and the implicit rotation get driven
    // by noise, producing frame-to-frame jitter (sub-30 inliers) or
    // sudden 30°+ rotation jumps (<20 inliers). Affine drops to 6 DoF
    // (no perspective); similarity drops to 4 DoF (uniform scale +
    // explicit rotation parameter), both stable at the sparse end.
    // Above ~30 inliers the full homography wins back its real
    // perspective expressiveness on actually-tilted surfaces.
    let refined = if inlier_pairs.len() >= 30 {
        fit_homography(&inlier_pairs)?
    } else if inlier_pairs.len() >= 15 {
        fit_affine(&inlier_pairs)?
    } else {
        fit_similarity(&inlier_pairs)?
    };
    let mut residuals: Vec<f32> = Vec::with_capacity(inlier_pairs.len());
    for &(px, py, qx, qy) in &inlier_pairs {
        if let Some((px2, py2)) = project(&refined, px, py) {
            let dx = px2 - qx;
            let dy = py2 - qy;
            residuals.push((dx * dx + dy * dy).sqrt());
        }
    }
    residuals.sort_by(|a, b| a.total_cmp(b));
    let median = if residuals.is_empty() {
        f32::INFINITY
    } else {
        residuals[residuals.len() / 2]
    };
    Some(TrackResult {
        homography: refined,
        inliers: inlier_pairs.len(),
        matches: pairs.len(),
        median_residual_px: median,
        inlier_pairs,
        descriptor_inliers: 0,
    })
}

/// Coarse per-frame pose: LK-track each seed's previous view-point from
/// `prev_pyr` into `cur_pyr`, then update the prior by composing with a
/// frame-to-frame similarity DELTA fit from `(prev_view, cur_view)`.
///
/// Why a delta and not an absolute root→view refit: the engine's adaptive
/// model (similarity for 8-14 inliers, affine for 15-29, homography for 30+)
/// is the right shape for descriptor relocalization — there the H is fit
/// against fresh feature correspondences with no presumed prior. For the
/// coarse, the *prior already encodes the correct perspective*; a low-inlier
/// frame falling through to similarity-on-(root, cur_view) would replace
/// the prior's 8-DoF estimate with a 4-DoF fit and erase the perspective
/// entries (visible as transient "flatten" wobble during real motion).
/// Composing a similarity delta preserves the prior's perspective and only
/// updates translation/rotation/uniform-scale frame-to-frame; new perspective
/// is introduced exclusively by the Relocalizer via `Correction` snap/weave.
///
/// `prior` is required for this path. If absent, falls back to the absolute
/// adaptive fit (used only on the very first coarse fit before any anchor's
/// `h_root_to_canonical` is known — which on the current pipeline doesn't
/// arise, because the CoarseTracker only fits after a Correction snap has set
/// `current_h`).
pub fn klt_forward_fit(
    prev_pyr: &Pyramid,
    cur_pyr: &Pyramid,
    seeds: &[(f32, f32, f32, f32)],
    cfg: &TrackerConfig,
    min_inliers: usize,
    prior: Option<[f32; 9]>,
) -> Option<TrackResult> {
    if seeds.is_empty() || prev_pyr.levels[0].dimensions() != cur_pyr.levels[0].dimensions() {
        return None;
    }
    let prev_view_pts: Vec<(f32, f32)> = seeds.iter().map(|&(_, _, vx, vy)| (vx, vy)).collect();
    let tracked = track_points(prev_pyr, cur_pyr, &prev_view_pts, &KltConfig::default());

    let Some(prior_h) = prior else {
        let mut pairs: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(seeds.len());
        for (seed, t) in seeds.iter().zip(tracked.iter()) {
            if t.success {
                pairs.push((seed.0, seed.1, t.x, t.y));
            }
        }
        return ransac_homography_with_prior(&pairs, cfg, min_inliers, None);
    };

    // Build the two parallel pair lists for the delta fit:
    //   view_pairs:  (prev_view, cur_view)   — input to the similarity RANSAC
    //   root_pairs:  (root,      cur_view)   — used for the returned TrackResult
    let mut view_pairs: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(seeds.len());
    let mut root_pairs: Vec<(f32, f32, f32, f32)> = Vec::with_capacity(seeds.len());
    for (seed, t) in seeds.iter().zip(tracked.iter()) {
        if t.success {
            view_pairs.push((seed.2, seed.3, t.x, t.y));
            root_pairs.push((seed.0, seed.1, t.x, t.y));
        }
    }
    if view_pairs.len() < min_inliers {
        return None;
    }
    let delta = ransac_similarity_with_min(&view_pairs, cfg, min_inliers)?;
    let new_h = mat3_mul(&delta, &prior_h);

    // Re-score the composed H over (root, cur_view) for `inliers` /
    // `median_residual_px` / `inlier_pairs`. These drive downstream gates
    // (engine's degraded / handoff thresholds, the dispatch gate this caller
    // uses to decide whether to enqueue a Relocalizer job).
    let r_thresh_sq = cfg.ransac_residual_px * cfg.ransac_residual_px;
    let mut inlier_pairs: Vec<(f32, f32, f32, f32)> = Vec::new();
    let mut residuals: Vec<f32> = Vec::new();
    for &(rx, ry, vx, vy) in &root_pairs {
        let Some((px, py)) = project(&new_h, rx, ry) else {
            continue;
        };
        let dx = px - vx;
        let dy = py - vy;
        let r2 = dx * dx + dy * dy;
        if r2 <= r_thresh_sq {
            inlier_pairs.push((rx, ry, vx, vy));
            residuals.push(r2.sqrt());
        }
    }
    if inlier_pairs.len() < min_inliers {
        return None;
    }
    residuals.sort_by(|a, b| a.total_cmp(b));
    let median = if residuals.is_empty() {
        f32::INFINITY
    } else {
        residuals[residuals.len() / 2]
    };
    Some(TrackResult {
        homography: new_h,
        inliers: inlier_pairs.len(),
        matches: view_pairs.len(),
        median_residual_px: median,
        inlier_pairs,
        descriptor_inliers: 0,
    })
}

/// RANSAC fit of a 4-DoF similarity (rotation + uniform scale + translation)
/// over `(a_x, a_y, b_x, b_y)` pairs. Two-point minimal sample, residual gate
/// from `cfg.ransac_residual_px`. Used by the CoarseTracker's per-frame
/// delta fit. Strict `>` tie-break so deterministic samples don't flip the
/// winning model on a tied inlier count.
fn ransac_similarity_with_min(
    pairs: &[(f32, f32, f32, f32)],
    cfg: &TrackerConfig,
    min_inliers: usize,
) -> Option<[f32; 9]> {
    if pairs.len() < 2 {
        return None;
    }
    let mut rng = SmallRng::from_seed(0xA5A5_5A5A_3C3C_C3C3);
    let r_thresh_sq = cfg.ransac_residual_px * cfg.ransac_residual_px;
    let n = pairs.len();
    let mut best_inliers_idx: Vec<usize> = Vec::new();
    for _ in 0..cfg.ransac_iters {
        let i0 = (rng.next_u32() as usize) % n;
        let mut i1 = (rng.next_u32() as usize) % n;
        while i1 == i0 {
            i1 = (rng.next_u32() as usize) % n;
        }
        let sample = [pairs[i0], pairs[i1]];
        let h = match fit_similarity(&sample) {
            Some(h) => h,
            None => continue,
        };
        let mut inliers_idx: Vec<usize> = Vec::with_capacity(n);
        for (i, &(ax, ay, bx, by)) in pairs.iter().enumerate() {
            let Some((px, py)) = project(&h, ax, ay) else {
                continue;
            };
            let dx = px - bx;
            let dy = py - by;
            if dx * dx + dy * dy <= r_thresh_sq {
                inliers_idx.push(i);
            }
        }
        if inliers_idx.len() > best_inliers_idx.len() {
            best_inliers_idx = inliers_idx;
        }
    }
    if best_inliers_idx.len() < min_inliers {
        return None;
    }
    let inlier_pairs: Vec<(f32, f32, f32, f32)> =
        best_inliers_idx.iter().map(|&i| pairs[i]).collect();
    fit_similarity(&inlier_pairs)
}

/// FAST-9 corner detector with non-maximum suppression. Scans every
/// interior pixel; tests if 9 contiguous pixels of the Bresenham-3
/// circle are all brighter than `c + t` or all darker than `c - t`.
/// Returns the top `max_features` corners by score, after suppressing
/// any corner that has a higher-scoring neighbour within `nms_radius`.
pub fn detect_fast(
    gray: &GrayImage,
    threshold: u8,
    max_features: usize,
    nms_radius: i32,
) -> Vec<KeyPoint> {
    let (w, h) = gray.dimensions();
    let w_i = w as i32;
    let h_i = h as i32;
    if w_i < 2 * KEYPOINT_BORDER + 1 || h_i < 2 * KEYPOINT_BORDER + 1 {
        return Vec::new();
    }
    let buf = gray.as_raw();
    let t = threshold as i16;
    // Per-row FAST scan is independent across rows, so fan the row
    // sweep out over the active rayon pool (the engine installs a
    // fixed-size pool around the per-frame feature call). `flat_map_iter`
    // preserves row order, so the resulting `raw` is identical to the
    // serial scan and NMS stays deterministic.
    let raw: Vec<KeyPoint> = (KEYPOINT_BORDER..(h_i - KEYPOINT_BORDER))
        .into_par_iter()
        .flat_map_iter(|y| {
            let row = (y as u32 * w) as usize;
            let mut row_kps = Vec::new();
            for x in KEYPOINT_BORDER..(w_i - KEYPOINT_BORDER) {
                let c = buf[row + x as usize] as i16;
                let hi = c + t;
                let lo = c - t;
                let p = |dx: i32, dy: i32| {
                    buf[((y + dy) as u32 * w) as usize + (x + dx) as usize] as i16
                };
                // Bresenham circle of radius 3 (clockwise from top).
                let p1 = p(0, -3);
                let p5 = p(3, 0);
                let p9 = p(0, 3);
                let p13 = p(-3, 0);
                let n_hi = (p1 > hi) as u8 + (p5 > hi) as u8 + (p9 > hi) as u8 + (p13 > hi) as u8;
                let n_lo = (p1 < lo) as u8 + (p5 < lo) as u8 + (p9 < lo) as u8 + (p13 < lo) as u8;
                if n_hi < 3 && n_lo < 3 {
                    continue;
                }
                let pix = [
                    p1,
                    p(1, -3),
                    p(2, -2),
                    p(3, -1),
                    p5,
                    p(3, 1),
                    p(2, 2),
                    p(1, 3),
                    p9,
                    p(-1, 3),
                    p(-2, 2),
                    p(-3, 1),
                    p13,
                    p(-3, -1),
                    p(-2, -2),
                    p(-1, -3),
                ];
                let (is_corner, score) = fast9_test(&pix, c, t);
                if is_corner {
                    row_kps.push(KeyPoint {
                        x: x as f32,
                        y: y as f32,
                        score,
                    });
                }
            }
            row_kps
        })
        .collect();
    let mut kps = nms_filter(raw, nms_radius, max_features);
    // Sub-pixel refinement: each FAST keypoint comes out at an
    // integer pixel because the corner test runs at integer pixel
    // positions. The actual visual corner lies somewhere within
    // that pixel's footprint — typically ±0.5 px. Fitting a parabola
    // to the SAD score response across the 4 cardinal neighbours
    // and interpolating to its peak recovers ~0.1-0.2 px localisation
    // accuracy, which propagates directly to a tighter RANSAC fit
    // (per-keypoint position error is the source noise the H fit
    // distributes across its DoFs). See `analysis.md` § "Source-
    // side fixes". Cost: ~32 k extra ops per frame on 500 kps,
    // negligible vs detect itself.
    for kp in kps.iter_mut() {
        refine_keypoint_subpixel(buf, w as usize, w_i, h_i, kp);
    }
    kps
}

/// Compute the FAST SAD score at an arbitrary pixel — sum of
/// |circle_pixel − centre| over the 16 Bresenham-radius-3 circle
/// pixels. Unlike `fast9_test`, this returns a score regardless of
/// whether the pixel actually passes the FAST-9 corner test;
/// callers use it to evaluate the score *surface* around a confirmed
/// corner for parabolic peak fitting.
#[inline]
fn fast_sad_at(buf: &[u8], w: usize, x: i32, y: i32) -> i32 {
    let c = buf[(y as usize) * w + (x as usize)] as i32;
    let p = |dx: i32, dy: i32| -> i32 { buf[((y + dy) as usize) * w + ((x + dx) as usize)] as i32 };
    let offsets: [(i32, i32); 16] = [
        (0, -3),
        (1, -3),
        (2, -2),
        (3, -1),
        (3, 0),
        (3, 1),
        (2, 2),
        (1, 3),
        (0, 3),
        (-1, 3),
        (-2, 2),
        (-3, 1),
        (-3, 0),
        (-3, -1),
        (-2, -2),
        (-1, -3),
    ];
    let mut s = 0i32;
    for &(dx, dy) in &offsets {
        s += (p(dx, dy) - c).abs();
    }
    s
}

/// Refine a FAST keypoint's (x, y) to sub-pixel position by fitting
/// a 1D parabola in x and y separately to the SAD response at the
/// integer-pixel centre and its 4 cardinal neighbours. The keypoint's
/// `score` field is unchanged (still the integer-pixel SAD); only
/// `x` and `y` are refined.
///
/// Parabola through `(-1, S_l), (0, S_c), (1, S_r)` peaks at
/// `x_peak = (S_l − S_r) / (2 · (S_l + S_r − 2·S_c))`. Only valid
/// when `S_c` is a local max in that axis (denom < 0). If the
/// integer pixel isn't a local max along an axis (because the SAD
/// surface around this corner is degenerate or saddle-like), no
/// refinement is applied on that axis. The result is clamped to
/// `[-0.5, +0.5]` — parabolic fit is only trustworthy for offsets
/// smaller than the sample spacing.
fn refine_keypoint_subpixel(buf: &[u8], w: usize, w_i: i32, h_i: i32, kp: &mut KeyPoint) {
    let kx = kp.x as i32;
    let ky = kp.y as i32;
    // SAD evaluation at (kx±1, ky) and (kx, ky±1) needs the radius-3
    // circle around those neighbours to be in-frame: i.e. positions
    // (kx±4, ky) and (kx, ky±4) must be valid pixels. The detector's
    // KEYPOINT_BORDER (24) already guarantees a much wider margin
    // for all kept keypoints, so this is just a defensive check.
    if kx < 4 || kx >= w_i - 4 || ky < 4 || ky >= h_i - 4 {
        return;
    }
    let s_c = kp.score;
    let s_l = fast_sad_at(buf, w, kx - 1, ky);
    let s_r = fast_sad_at(buf, w, kx + 1, ky);
    let s_u = fast_sad_at(buf, w, kx, ky - 1);
    let s_d = fast_sad_at(buf, w, kx, ky + 1);
    let denom_x = (s_l + s_r - 2 * s_c) as f32;
    let denom_y = (s_u + s_d - 2 * s_c) as f32;
    let dx = if denom_x < -1e-3 {
        (0.5 * (s_l - s_r) as f32 / denom_x).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let dy = if denom_y < -1e-3 {
        (0.5 * (s_u - s_d) as f32 / denom_y).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    kp.x += dx;
    kp.y += dy;
}

/// Check FAST-9 condition on 16 circle pixels: 9 contiguous brighter
/// than `c + t` OR 9 contiguous darker than `c - t`. The score is the
/// sum-of-absolute-differences of all circle pixels vs the center —
/// fine for non-max suppression even though it's not the "standard"
/// FAST score function.
fn fast9_test(pix: &[i16; 16], c: i16, t: i16) -> (bool, i32) {
    let hi = c + t;
    let lo = c - t;
    let bright = [
        pix[0] > hi,
        pix[1] > hi,
        pix[2] > hi,
        pix[3] > hi,
        pix[4] > hi,
        pix[5] > hi,
        pix[6] > hi,
        pix[7] > hi,
        pix[8] > hi,
        pix[9] > hi,
        pix[10] > hi,
        pix[11] > hi,
        pix[12] > hi,
        pix[13] > hi,
        pix[14] > hi,
        pix[15] > hi,
    ];
    let dark = [
        pix[0] < lo,
        pix[1] < lo,
        pix[2] < lo,
        pix[3] < lo,
        pix[4] < lo,
        pix[5] < lo,
        pix[6] < lo,
        pix[7] < lo,
        pix[8] < lo,
        pix[9] < lo,
        pix[10] < lo,
        pix[11] < lo,
        pix[12] < lo,
        pix[13] < lo,
        pix[14] < lo,
        pix[15] < lo,
    ];
    let mut score = 0i32;
    for v in pix {
        score += (*v as i32 - c as i32).abs();
    }
    (has_run(&bright, 9) || has_run(&dark, 9), score)
}

/// Does the cyclic boolean array contain a run of at least `n` trues?
fn has_run(arr: &[bool; 16], n: usize) -> bool {
    let mut run = 0;
    let mut total = 0;
    for &b in arr.iter().chain(arr.iter().take(n - 1)) {
        if b {
            run += 1;
            total = total.max(run);
            if total >= n {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// Greedy non-maximum suppression: sort by score descending, then keep
/// only points whose `(nms_radius)` neighbourhood contains no
/// already-kept higher-scoring point. Returns at most `max_features`.
fn nms_filter(mut kps: Vec<KeyPoint>, nms_radius: i32, max_features: usize) -> Vec<KeyPoint> {
    kps.sort_unstable_by(|a, b| b.score.cmp(&a.score));
    let r2 = (nms_radius * nms_radius) as f32;
    let mut kept: Vec<KeyPoint> = Vec::with_capacity(max_features.min(kps.len()));
    for kp in kps {
        let too_close = kept.iter().any(|other| {
            let dx = other.x - kp.x;
            let dy = other.y - kp.y;
            dx * dx + dy * dy <= r2
        });
        if too_close {
            continue;
        }
        kept.push(kp);
        if kept.len() >= max_features {
            break;
        }
    }
    kept
}

/// Compute oriented BRIEF-256 descriptors for each keypoint. The ORB-
/// style intensity centroid gives a dominant orientation per keypoint;
/// the BRIEF sample pattern is rotated by that angle before sampling.
/// This makes descriptors rotation-invariant — without it, rotating the
/// camera (or the scene) by more than ~30° drops the match rate to
/// near-zero.
///
/// Drops keypoints too close to the edge; the returned `keypoints`
/// vector is filtered in lock-step with `descriptors`.
pub fn describe_brief(gray: &GrayImage, kps: &[KeyPoint]) -> (Vec<KeyPoint>, Vec<Descriptor>) {
    let (w, h) = gray.dimensions();
    let w_i = w as i32;
    let h_i = h as i32;
    let buf = gray.as_raw();
    let pattern = &BRIEF_PATTERN;
    // Each keypoint's descriptor is independent, so describe in parallel
    // on the active rayon pool. `par_iter().filter_map().collect()`
    // preserves keypoint order, so the kept/descriptor pairing is
    // identical to the serial path.
    // Sample pairs at distance ≤ R from the keypoint, plus the 3x3 box-blur
    // neighbourhood; KEYPOINT_BORDER already accounts for both.
    let pairs: Vec<(KeyPoint, Descriptor)> = kps
        .par_iter()
        .filter_map(|kp| {
            let cx = kp.x.round() as i32;
            let cy = kp.y.round() as i32;
            if cx < KEYPOINT_BORDER
                || cy < KEYPOINT_BORDER
                || cx >= w_i - KEYPOINT_BORDER
                || cy >= h_i - KEYPOINT_BORDER
            {
                return None;
            }
            let angle = patch_orientation(buf, w, cx, cy);
            let (sin_a, cos_a) = angle.sin_cos();
            // 3x3 box-blur of the gray patch at integer offsets — matches the
            // smoothing the original BRIEF paper prescribes. Bounds are
            // guaranteed by KEYPOINT_BORDER + cap on rotated radius.
            let sample = |dx: i32, dy: i32| -> u8 {
                let mut acc = 0u32;
                for oy in -1..=1 {
                    let row = ((cy + dy + oy) as u32 * w) as usize;
                    for ox in -1..=1 {
                        acc += buf[row + (cx + dx + ox) as usize] as u32;
                    }
                }
                (acc / 9) as u8
            };
            let mut bytes = [0u8; DESCRIPTOR_BYTES];
            for (i, &(ax, ay, bx, by)) in pattern.iter().enumerate() {
                // Rotate the sample offsets by the keypoint's dominant angle.
                // Rotation magnitudes are bounded by BRIEF_PATCH_RADIUS, well
                // inside KEYPOINT_BORDER.
                let rax = (cos_a * ax as f32 - sin_a * ay as f32).round() as i32;
                let ray = (sin_a * ax as f32 + cos_a * ay as f32).round() as i32;
                let rbx = (cos_a * bx as f32 - sin_a * by as f32).round() as i32;
                let rby = (sin_a * bx as f32 + cos_a * by as f32).round() as i32;
                let a_val = sample(rax, ray);
                let b_val = sample(rbx, rby);
                if a_val < b_val {
                    bytes[i / 8] |= 1 << (i % 8);
                }
            }
            Some((*kp, Descriptor(bytes)))
        })
        .collect();
    pairs.into_iter().unzip()
}

/// ORB-style intensity centroid. Compute first-order moments of a
/// circular patch of radius `BRIEF_PATCH_RADIUS` around (cx, cy); the
/// angle from the patch centre to the intensity-weighted centroid is
/// the dominant orientation. Bounds-safe: the caller has already
/// checked `KEYPOINT_BORDER`.
fn patch_orientation(buf: &[u8], w: u32, cx: i32, cy: i32) -> f32 {
    let r = BRIEF_PATCH_RADIUS;
    let r_sq = r * r;
    let mut m10: i32 = 0;
    let mut m01: i32 = 0;
    for dy in -r..=r {
        let row = ((cy + dy) as u32 * w) as usize;
        for dx in -r..=r {
            if dx * dx + dy * dy > r_sq {
                continue;
            }
            let v = buf[row + (cx + dx) as usize] as i32;
            m10 += dx * v;
            m01 += dy * v;
        }
    }
    if m10 == 0 && m01 == 0 {
        return 0.0;
    }
    (m01 as f32).atan2(m10 as f32)
}

/// The BRIEF sampling pattern. 256 (ax, ay, bx, by) tuples uniformly
/// drawn within the patch via a deterministic xorshift PRNG, computed at
/// compile time. No runtime initialization, no rand-crate dep, and the
/// pattern is identical on every platform.
const BRIEF_PATTERN: [(i8, i8, i8, i8); BRIEF_BITS] = generate_brief_pattern();

const fn generate_brief_pattern() -> [(i8, i8, i8, i8); BRIEF_BITS] {
    let mut out = [(0i8, 0i8, 0i8, 0i8); BRIEF_BITS];
    let mut state: u64 = 0x1234_5678_9ABC_DEF0;
    let r = BRIEF_PATCH_RADIUS as i32;
    let span = (2 * r + 1) as u32;
    let mut i = 0;
    while i < BRIEF_BITS {
        let (s1, ax) = const_uniform(state, span, r);
        let (s2, ay) = const_uniform(s1, span, r);
        let (s3, bx) = const_uniform(s2, span, r);
        let (s4, by) = const_uniform(s3, span, r);
        state = s4;
        out[i] = (ax, ay, bx, by);
        i += 1;
    }
    out
}

const fn const_uniform(state: u64, span: u32, radius: i32) -> (u64, i8) {
    let next = xorshift64(state);
    let v = (next as u32) % span;
    (next, (v as i32 - radius) as i8)
}

const fn xorshift64(mut x: u64) -> u64 {
    if x == 0 {
        x = 0xDEAD_BEEF_CAFE_BABE;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Tiny xorshift64 PRNG. We deliberately don't depend on `rand` — keeps
/// the dep tree small and the determinism guarantees explicit.
pub struct SmallRng(u64);

impl SmallRng {
    pub fn from_seed(seed: u64) -> Self {
        Self(if seed == 0 {
            0xDEAD_BEEF_CAFE_BABE
        } else {
            seed
        })
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = xorshift64(self.0);
        self.0 as u32
    }
}

/// Compose two homographies: `out = a * b` (matrix multiplication on
/// row-major 3x3). Convenience wrapper over `crate::homography::mat3_mul`.
pub fn compose(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    mat3_mul(a, b)
}

/// `crate::homography::invert` re-exported for tests.
pub fn invert_h(h: &[f32; 9]) -> Option<[f32; 9]> {
    invert(h)
}
