//! Static screen-translate pipeline: the no-tracker counterpart to
//! [`crate::live_tracker_pipeline::LiveTrackerPipeline`].
//!
//! A MediaProjection-style screen capture is a flat, fronto-parallel surface
//! fixed in the capture frame, so there is no homography to track — the
//! transform from detected text to overlay is identity. This pipeline drops
//! the engine / coarse-tracker / async-weave machinery entirely and just runs
//! the shared acquire core (detect → orient → rec/translate) on a timestamp
//! cadence, then composites the resident overlays at identity every frame.
//!
//! The detect→rec→translate→overlay-build and the composite are the exact same
//! functions the tracked camera path uses
//! ([`crate::live_tracker_pipeline::acquire_detect`] etc.), so there is no
//! duplicated OCR/overlay logic — only the orchestration differs.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::live_session::{LiveOcrHost, LiveSession};
use crate::live_tracker_pipeline::{acquire_detect, acquire_rec_translate};
use crate::live_worker::SlotWorker;
use translator_core::ocr::{OrientedRect, Rect};
use translator_ocr::orientation::dominant_axis_quadrant;
use translator_raster::live_frame::LiveFrame;
use translator_render::font_provider::FontProvider;
use translator_tracker::screen_monitor::{
    FrameClassification, Lattice, MonitorConfig, Rgb, ScreenMonitor, channel_delta,
};

/// The one anchor the screen pipeline owns; everything composites against it.
const SCREEN_ANCHOR_ID: u64 = 1;

/// Pinhole lattice pitch (canonical px) for the v2 under-pill change detector.
/// Holes are punched here in the overlay and the recovery shader samples here.
/// 2.5 canonical ≈ 5 display px/pinhole: the present's 2× upscale turns this into
/// an integer 5px display pitch (1px hole + 4px content), which with the 2px
/// per-row stagger pushes the lattice's moiré off-axis. Fractional canonical is
/// fine — the punch/recovery snap each hole to the mirror texel grid.
const SCREEN_LATTICE_SPACING: f32 = 2.0;
/// Pill colour the screen overlay draws (opaque black), as a 0..1 luma — the
/// recovery's `pill` term.
const SCREEN_PILL_LUMA: f32 = 0.0;
/// Engineered effective screen fraction at a hole (`1 − effective overlay alpha`):
/// the hole reads 70% screen / 30% pill. Higher is cleaner recovery — the shader
/// inverts `rec = raw / frac`, so a larger fraction amplifies capture noise less
/// (×1.43 here vs ×2 at 0.5) — which the crisp 1px present-time holes can afford
/// since they're barely visible. The hole's baked alpha is chosen so the captured
/// blend lands here, keeping the recovery shader fixed.
const SCREEN_HOLE_FRAC: f32 = 0.7;
/// Hole dot radius (canonical px). The holes are punched at present time, per
/// display pixel (see the present fragment shader), centred on the same lattice +
/// 0.25 texel the recovery samples — so the sampled texel sits at distance 0 and is
/// always inside the hole, while the radius just sets how many *display* pixels
/// around it also open up. 0.3 canonical ≈ 0.6 display px, which resolves to a
/// single crisp pixel: enough to guarantee the read texel (needs ≥ 0.25), small
/// enough to stay one dot. No fat disc is needed now that there's no LINEAR-upscale
/// smear to fight (that was the old baked-hole problem).
const SCREEN_HOLE_RADIUS: f32 = 0.3;
/// After a trip drops blocks, ignore further trips for this long so a noisy
/// recovery can't storm removals.
const V2_TRIP_COOLDOWN_NS: i64 = 500_000_000;
/// While settled, run a masked additive acquire at least this often to pick up
/// new text that appeared in the gaps.
const V2_PERIODIC_NS: i64 = 1_500_000_000;
/// After dropping/clearing pills, hold the re-acquire off for this long so the
/// dropped pills have actually left the captured mirror before OCR runs on it.
/// MediaProjection composites the overlay window into the mirror a frame or two
/// after `eglSwapBuffers` returns, so re-OCRing immediately recognises our own
/// just-removed labels as "new" text. A fixed settle is deterministic where the
/// old present-timestamp fence raced the compositor and opened ~1-2 frames early.
/// Also rides out a content transition (e.g. a subtitle crossfade): a re-acquire
/// fired mid-fade OCRs half-faded glyphs and gets rejected on confidence, then
/// has to wait out the ~1.5s periodic scan — so the settle is the cheaper place
/// to absorb the transition than adding a retry loop.
const V2_SETTLE_NS: i64 = 300_000_000;
/// Per-lattice-point inter-frame luma delta that counts a gap point as "moved"
/// for the global-motion signal (both the scroll trigger and the settle gate).
const V2_MOTION_THR: u32 = 30;
/// Frames a lattice point stays "recently moved" after an inter-frame jump, for
/// the per-box dynamic-content test. Spans a detection (~a few frames) so a box
/// detected over a region that moved any time in this window is rejected.
const V2_MOTION_WINDOW: u16 = 8;
/// Fraction of a freshly-detected box's lattice points that must be "recently
/// moved" for the box to count as over dynamic content (video / game / scroll)
/// and be dropped before commit. Static text scores ~0; a moving region ~all.
const V2_DYNAMIC_BOX_FRAC: f32 = 0.4;
/// Fraction of gap (non-pill) lattice points moving frame-to-frame that reads as
/// a wholesale change (scroll / navigation / app-switch) → drop all + re-acquire.
/// This is the fast, box-independent path: it fires the same frame the screen
/// starts moving (before the per-box detectors confirm) and clears even boxes the
/// per-box monitor can't track (too few holes), which would otherwise persist. A
/// localized video moves only its own region, staying below this.
const V2_SCROLL_MOTION_FRAC: f32 = 0.30;
/// Below this fraction of moving gap points the screen is settled enough to run
/// the post-drop re-acquire — OCRing mid-scroll just produces garbage that trips
/// again. The band between this and [`V2_SCROLL_MOTION_FRAC`] is hysteresis.
const V2_SETTLE_MOTION_FRAC: f32 = 0.15;
/// Companion settle gate over the recent-motion accumulator across *all* lattice
/// points (gaps and under-pill recovery alike). `motion_frac` only sees the bare
/// gap points, so a densely-pilled page reads as settled mid-scroll and the
/// re-acquire grabs transient lines; this catches that, since the now-faithful
/// under-pill recovery makes `recent_motion` trustworthy everywhere. Kept as a
/// separate, higher threshold than the gap gate because `recent_motion` lingers
/// for [`V2_MOTION_WINDOW`] frames after each jump. Both gates must read settled.
const V2_SETTLE_CONTENT_FRAC: f32 = 0.20;
/// Minimum number of gap (non-pill) lattice points needed to trust the motion
/// fraction. When pills cover almost everything there aren't enough bare screen
/// samples, so the gate treats the frame as settled.
const V2_MOTION_MIN_POINTS: usize = 20;
/// A per-box change touching at least this fraction of the tracked boxes is a
/// whole-screen change (scroll / navigation): drop everything and re-acquire from
/// scratch rather than blinking pills off one by one.
const SCROLL_CHANGED_FRAC: f32 = 0.75;
/// A box needs at least this many lattice holes to be monitored.
const SCREEN_MIN_BOX_HOLES: usize = 4;

/// A capture region in normalized `[0,1]` coordinates (origin top-left), so it
/// survives capture-resolution and rotation changes unchanged. Mapped to pixels
/// at acquire time against the live canonical dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NormRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl NormRect {
    /// Map to a pixel [`Rect`] within a `w×h` frame: clamped in-bounds, edges
    /// ordered, and at least one pixel wide/tall.
    pub fn to_px(&self, w: u32, h: u32) -> Rect {
        let c01 = |v: f32| v.clamp(0.0, 1.0);
        let (l, r) = (
            c01(self.left.min(self.right)),
            c01(self.left.max(self.right)),
        );
        let (t, b) = (
            c01(self.top.min(self.bottom)),
            c01(self.top.max(self.bottom)),
        );
        let left = ((l * w as f32).round() as u32).min(w.saturating_sub(1));
        let top = ((t * h as f32).round() as u32).min(h.saturating_sub(1));
        let right = ((r * w as f32).round() as u32).clamp(left + 1, w);
        let bottom = ((b * h as f32).round() as u32).clamp(top + 1, h);
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }
}

#[derive(Clone)]
struct ScreenConfig {
    from_lang: String,
    to_lang: String,
    is_auto_source: bool,
    det_max_pixels: u32,
    rec_batch_size: usize,
    /// Restrict OCR (detect + rec/translate) to this region; `None` = full screen.
    region: Option<NormRect>,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        Self {
            from_lang: String::new(),
            to_lang: String::new(),
            is_auto_source: true,
            det_max_pixels: 650_000,
            rec_batch_size: 4,
            region: None,
        }
    }
}

/// Per-frame result, packed by the JNI layer for the debug pill.
#[derive(Debug, Clone, Default)]
pub struct ScreenFrameResult {
    pub overlay_count: u32,
    pub did_detect: bool,
    pub detected_count: u32,
    pub rec_ok_count: u32,
}

/// What the monitor wants the GL worker to do after a captured frame / tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorAction {
    /// Screen unchanged enough — leave the overlay as-is.
    None,
    /// Movement started — hide the overlay so the user sees real content.
    Hide,
    /// Settled — run a clear → clean-capture → detect/rec/translate → show cycle.
    Acquire,
}

/// One acquire job: the GPU-rendered OCR frame + the generation it was dispatched
/// at (so a later `abort`/reset can cancel it mid-flight).
struct ScreenJob {
    frame: Arc<LiveFrame>,
    generation: u64,
}

/// Screen-tuned config for the per-box (v2) monitor. Thresholds are in recovered
/// luma units (0..255); the recovery doubles a black-pill hole, so a stroke
/// change is a large delta.
fn screen_monitor_config() -> MonitorConfig {
    MonitorConfig {
        warmup_frames: 6,
        // Per-channel RGB delta counted as a hard swing (cf. playtranslate's per-channel
        // SPLATTER_THRESHOLD=60). Above the recovery noise floor (~20) and benign drift
        // (~50), under a real stroke↔gap / colour swing (150+).
        hard_threshold: 60,
        // Fraction of a box's holes that must swing hard to trip. 0.10 (vs playtranslate's
        // 0.05) is the floor our recovery's noise allows for now — sparse-text scroll moves
        // ~10–25% of holes, which the old 0.25 left stranded.
        hard_frac: 0.10,
        scroll_frac: 0.7,
        scroll_min_boxes: 2,
    }
}

/// The v2 per-box monitor plus the canonical dims its lattice was built for and
/// the overlay content version its boxes were last (re)based at.
struct MonitorV2State {
    monitor: ScreenMonitor,
    cw: u32,
    ch: u32,
    last_populated: u64,
    /// block_id → the `content_hash` its box was baselined at, so an unchanged
    /// block keeps its baseline across unrelated acquires.
    baselined: HashMap<u64, u64>,
    /// Observe-driven trips are ignored until this time (warmup after a re-base,
    /// cooldown after a drop).
    suppress_trips_until_ns: i64,
    /// Next time to dispatch the periodic new-text scan (masked additive acquire
    /// for text that appeared in the gaps). Frame-driven.
    next_acquire_ns: i64,
    /// A re-acquire may not run before this time: armed to `now + V2_SETTLE_NS`
    /// whenever pills are dropped/cleared, so the dropped pills have left the
    /// captured mirror before OCR runs (replaces the racy present-generation
    /// fence). `0` = no settle pending.
    reacquire_not_before_ns: i64,
    /// A re-acquire is owed (bootstrap, or after a drop/clear) and must fire even
    /// if no frames arrive — drives `wants_tick` so the GL loop keeps ticking. The
    /// periodic new-text scan is frame-driven only, so a static screen with a
    /// stable overlay doesn't tick forever. Cleared once dispatch is *confirmed*
    /// (the worker goes busy), not optimistically when the action is returned.
    pending_reacquire: bool,
    /// Previous frame's recovered samples, for the global inter-frame motion
    /// (scroll/navigation) signal.
    prev_samples: Option<Vec<Rgb>>,
    /// Previous frame's per-point pill-coverage mask. Motion is compared only at
    /// points that were a gap last frame *and* this frame, so a pill appearing /
    /// leaving over a point isn't mistaken for screen motion.
    prev_covered: Option<Vec<bool>>,
    /// Currently in a global-motion (scroll/navigation) episode — the overlay has
    /// been cleared and re-acquire is deferred until motion settles.
    scrolling: bool,
    /// Per-lattice-point "moved in the last few frames" accumulator (all points,
    /// not just gaps): bumped to [`V2_MOTION_WINDOW`] when a point's inter-frame
    /// luma delta exceeds [`V2_MOTION_THR`], decays by 1 otherwise. A freshly
    /// detected box whose region scores high here is over dynamic content (video /
    /// game sprite / mid-scroll) and is dropped before commit, so a pill never
    /// pins to moving content. Empty until sized to the lattice.
    recent_motion: Vec<u16>,
    /// Per-lattice-point "inside the OCR region" mask, in `points()` order. Empty
    /// when there's no region (full screen). The motion signals only sample points
    /// where this is set, so movement outside the crop can't trip a clear/re-acquire.
    in_region: Vec<bool>,
    /// Diagnostic frame counter for rate-limited logging.
    frames: u64,
}

/// Axis-aligned-bounds overlap of two oriented rects. Used to drop detections
/// that fall on an existing pill (our own re-captured overlay).
fn rects_overlap(a: &OrientedRect, b: &OrientedRect) -> bool {
    let aabb = |r: &OrientedRect| {
        let (c, s) = (r.angle_radians.cos().abs(), r.angle_radians.sin().abs());
        let hw = r.width * 0.5 * c + r.height * 0.5 * s;
        let hh = r.width * 0.5 * s + r.height * 0.5 * c;
        (r.cx, r.cy, hw, hh)
    };
    let (ax, ay, ahw, ahh) = aabb(a);
    let (bx, by, bhw, bhh) = aabb(b);
    (ax - bx).abs() <= ahw + bhw && (ay - by).abs() <= ahh + bhh
}

/// Reconcile the v2 boxes (keyed by `block_id`) against the resident blocks
/// *incrementally*: a box's baseline + learned stroke mask are **preserved**
/// while its block's `content_hash` is unchanged, so an unrelated acquire can't
/// reset a stale block's reference (which is what let stale pills accumulate).
/// Only new blocks and re-OCR'd blocks (changed hash) are (re)baselined off the
/// recovered `samples`; boxes for vanished blocks are dropped. `baselined` tracks
/// the hash each box was baselined at.
fn reconcile_boxes(
    monitor: &mut ScreenMonitor,
    blocks: &[(u64, u64, Vec<OrientedRect>)],
    samples: &[Rgb],
    baselined: &mut HashMap<u64, u64>,
) {
    let current: std::collections::HashSet<u64> = blocks.iter().map(|b| b.0).collect();
    for id in monitor.box_ids() {
        if !current.contains(&id) {
            monitor.remove_box(id);
            baselined.remove(&id);
        }
    }
    for (block_id, hash, strips) in blocks {
        if baselined.get(block_id) == Some(hash) {
            continue; // unchanged — keep its baseline + frozen mask
        }
        let mut holes: Vec<usize> = strips
            .iter()
            .flat_map(|s| monitor.lattice().holes_in_rect(s))
            .collect();
        holes.sort_unstable();
        holes.dedup();
        if holes.len() < SCREEN_MIN_BOX_HOLES {
            continue;
        }
        monitor.set_box(*block_id, holes, samples);
        baselined.insert(*block_id, *hash);
    }
}

pub struct LiveScreenPipeline {
    host: Arc<dyn LiveOcrHost>,
    session: Arc<LiveSession>,
    font_provider: Arc<dyn FontProvider + Send + Sync>,
    config: Mutex<ScreenConfig>,
    /// Bumped on reset / language change / abort so an in-flight rec/translate bails.
    generation: AtomicU64,
    /// Per-box under-pill change detector; `None` until the first frame establishes
    /// the canonical dims.
    monitor_v2: Mutex<Option<MonitorV2State>>,
    /// Background OCR worker; `None` only transiently during `new`.
    worker: Mutex<Option<SlotWorker<ScreenJob>>>,
}

impl LiveScreenPipeline {
    pub fn new(
        host: Arc<dyn LiveOcrHost>,
        font_provider: Arc<dyn FontProvider + Send + Sync>,
    ) -> Arc<Self> {
        let session = Arc::new(LiveSession::new());
        // Opaque pill: the screen overlay window is already alpha-clamped (~0.79)
        // for touch passthrough, so the default translucent pill (0xC8) would
        // double-dim into unreadable mush. Camera keeps the translucent default.
        session.set_overlay_bg([0x00, 0x00, 0x00, 0xFF]);
        let pipeline = Arc::new(Self {
            host,
            session,
            font_provider,
            config: Mutex::new(ScreenConfig::default()),
            generation: AtomicU64::new(0),
            monitor_v2: Mutex::new(None),
            worker: Mutex::new(None),
        });
        let worker = {
            let pipeline_weak = Arc::downgrade(&pipeline);
            SlotWorker::spawn("LiveScreenWorker", move |job: ScreenJob| {
                let Some(pipeline) = pipeline_weak.upgrade() else {
                    return;
                };
                pipeline.run_screen_acquire(job.frame, job.generation);
            })
        };
        *pipeline.worker.lock().expect("worker slot poisoned") = Some(worker);
        pipeline
    }

    fn reset_monitor(&self) {
        if let Ok(mut m) = self.monitor_v2.lock() {
            *m = None;
        }
    }

    /// Pinhole lattice pitch (canonical px) the v2 detector + the overlay holes
    /// share. The GL caller sizes its recovery readback and punches holes with it.
    pub fn lattice_spacing(&self) -> f32 {
        SCREEN_LATTICE_SPACING
    }

    /// Lattice grid `(cols, rows)` for a `cw×ch` capture — the size of the
    /// recovery readback the caller must produce for [`Self::monitor_under_pill_changed`].
    pub fn lattice_dims(&self, cw: u32, ch: u32) -> (u32, u32) {
        Lattice::dims(cw, ch, SCREEN_LATTICE_SPACING)
    }

    /// The pill colour (0..1 luma) and screen fraction the recovery shader uses.
    pub fn recovery_params(&self) -> (f32, f32) {
        (SCREEN_PILL_LUMA, SCREEN_HOLE_FRAC)
    }

    /// Baked pill alpha to use for the pinhole holes given the *effective* opaque
    /// pill alpha on screen (`present pill_alpha × overlay-window alpha`): chosen
    /// so the captured blend at a hole lands at [`SCREEN_HOLE_FRAC`], keeping the
    /// recovery fixed at that fraction. The hole radius is half the pitch.
    pub fn hole_params(&self, effective_opaque_alpha: f32) -> (f32, f32) {
        let target_overlay_alpha = 1.0 - SCREEN_HOLE_FRAC;
        let alpha = (target_overlay_alpha / effective_opaque_alpha.max(1e-3)).clamp(0.0, 1.0);
        (SCREEN_HOLE_RADIUS, alpha)
    }

    /// Overlay pill footprints as canonical AABBs `(cx, cy, half_w, half_h)` for
    /// the recovery shader's per-point pill test — the per-block strip rects (the
    /// actual holed pills), so recovery only inverts the blend where a hole is.
    pub fn monitor_pill_aabbs(&self) -> Vec<(f32, f32, f32, f32)> {
        // One AABB per BLOCK (union of its strips), not per strip: the recovery
        // shader caps at `REC_MAX_PILLS`, and a multi-line block emits many strips,
        // so per-strip easily blows past the cap on a dense screen — blocks beyond
        // the cap then never get their under-pill blend inverted and read a dimmed
        // near-constant (they can never trip → permanent labels). Per block keeps
        // the count at ≤ #blocks, comfortably under the cap.
        self.session
            .overlay_blocks(SCREEN_ANCHOR_ID)
            .iter()
            .filter_map(|(_, _, strips)| {
                let mut min_x = f32::MAX;
                let mut min_y = f32::MAX;
                let mut max_x = f32::MIN;
                let mut max_y = f32::MIN;
                for r in strips {
                    let (c, s) = (r.angle_radians.cos().abs(), r.angle_radians.sin().abs());
                    let hw = r.width * 0.5 * c + r.height * 0.5 * s;
                    let hh = r.width * 0.5 * s + r.height * 0.5 * c;
                    min_x = min_x.min(r.cx - hw);
                    min_y = min_y.min(r.cy - hh);
                    max_x = max_x.max(r.cx + hw);
                    max_y = max_y.max(r.cy + hh);
                }
                if min_x > max_x {
                    return None;
                }
                Some((
                    (min_x + max_x) * 0.5,
                    (min_y + max_y) * 0.5,
                    (max_x - min_x) * 0.5,
                    (max_y - min_y) * 0.5,
                ))
            })
            .collect()
    }

    /// The per-box screen monitor — sole authority for the screen path. Feed
    /// recovered screen_est `samples` (one byte per lattice point, in
    /// `Lattice::points()` order, sized to [`Self::lattice_dims`]); returns what
    /// the GL worker should do:
    ///   * `Acquire` — run a masked additive acquire (a box's text changed and was
    ///     dropped here so the acquire re-grabs it; the periodic new-text scan is
    ///     due; or bootstrap).
    ///   * `Hide` — a whole-screen change (scroll / navigation: at least
    ///     [`SCROLL_CHANGED_FRAC`] of the boxes changed at once): the resident
    ///     overlay is cleared and a re-acquire scheduled after a settle; the GL
    ///     worker clears the rendered output and aborts any in-flight acquire.
    ///   * `None` — nothing to do (background motion with stable text is ignored).
    pub fn monitor_screen_v2(
        &self,
        samples: &[Rgb],
        cw: u32,
        ch: u32,
        now_ns: i64,
    ) -> MonitorAction {
        let mut guard = self.monitor_v2.lock().expect("monitor_v2 lock");
        let stale = guard.as_ref().is_none_or(|s| s.cw != cw || s.ch != ch);
        if stale {
            let lattice = Lattice::build(cw, ch, SCREEN_LATTICE_SPACING);
            // Mask the motion signals to the OCR region (if any): a point counts
            // only when it sits inside the crop, so a video / animation outside the
            // region can't trip the scroll/settle gates.
            let in_region = match self.config.lock().ok().and_then(|c| c.region) {
                Some(r) => {
                    let px = r.to_px(cw, ch);
                    lattice
                        .points()
                        .iter()
                        .map(|p| {
                            p.x >= px.left as f32
                                && p.x < px.right as f32
                                && p.y >= px.top as f32
                                && p.y < px.bottom as f32
                        })
                        .collect()
                }
                None => Vec::new(),
            };
            *guard = Some(MonitorV2State {
                monitor: ScreenMonitor::new(lattice, screen_monitor_config()),
                cw,
                ch,
                last_populated: u64::MAX,
                baselined: HashMap::new(),
                suppress_trips_until_ns: 0,
                next_acquire_ns: now_ns,
                reacquire_not_before_ns: 0,
                pending_reacquire: true,
                prev_samples: None,
                prev_covered: None,
                scrolling: false,
                recent_motion: Vec::new(),
                in_region,
                frames: 0,
            });
        }
        let st = guard.as_mut().expect("monitor_v2 state");
        if samples.len() != st.monitor.lattice().len() {
            return MonitorAction::None;
        }
        let version = self.session.content_version();
        // Inter-frame motion over lattice points NOT under any pill. Two uses: the
        // fast wholesale-change trigger (> [`V2_SCROLL_MOTION_FRAC`] → drop all,
        // the same frame the screen starts moving) and the settle gate (the
        // post-drop re-acquire waits until it falls below
        // [`V2_SETTLE_MOTION_FRAC`] so OCR runs on a still frame, not mid-scroll).
        // Gap points sample the raw screen. Points whose coverage changed since
        // last frame are skipped (a pill just appeared / left).
        let mut covered = vec![false; samples.len()];
        st.monitor.fill_covered(&mut covered);
        let motion_frac = match (&st.prev_samples, &st.prev_covered) {
            (Some(prev), Some(prev_cov))
                if prev.len() == samples.len() && prev_cov.len() == samples.len() =>
            {
                let mut moved = 0usize;
                let mut eligible = 0usize;
                for i in 0..samples.len() {
                    if covered[i] || prev_cov[i] {
                        continue;
                    }
                    if !st.in_region.is_empty() && !st.in_region[i] {
                        continue;
                    }
                    eligible += 1;
                    if channel_delta(samples[i], prev[i]) > V2_MOTION_THR {
                        moved += 1;
                    }
                }
                if eligible >= V2_MOTION_MIN_POINTS {
                    moved as f32 / eligible as f32
                } else {
                    0.0
                }
            }
            _ => 0.0,
        };
        // Per-point recent-motion accumulator over ALL points (incl. under pills, so
        // a box's own region is measured): bump points that jumped this frame, decay
        // the rest. `run_screen_acquire` reads this to drop a detection over a region
        // that's actively moving (video / game / mid-scroll) before it commits a pill.
        if st.recent_motion.len() != samples.len() {
            st.recent_motion = vec![0u16; samples.len()];
        }
        if let Some(prev) = &st.prev_samples {
            if prev.len() == samples.len() {
                let masked = !st.in_region.is_empty();
                for i in 0..samples.len() {
                    if masked && !st.in_region[i] {
                        st.recent_motion[i] = 0;
                        continue;
                    }
                    if channel_delta(samples[i], prev[i]) > V2_MOTION_THR {
                        st.recent_motion[i] = V2_MOTION_WINDOW;
                    } else {
                        st.recent_motion[i] = st.recent_motion[i].saturating_sub(1);
                    }
                }
            }
        }
        // Fraction of lattice points (gaps + under-pill) that moved within the
        // recent-motion window — the dense-page settle signal `motion_frac` can't see.
        // Out-of-region points are forced to 0 above, so the denominator is the
        // in-region point count when a crop is active.
        let content_motion_frac = if st.recent_motion.is_empty() {
            0.0
        } else if st.in_region.is_empty() {
            st.recent_motion.iter().filter(|&&m| m > 0).count() as f32
                / st.recent_motion.len() as f32
        } else {
            let total = st.in_region.iter().filter(|&&b| b).count().max(1);
            st.recent_motion.iter().filter(|&&m| m > 0).count() as f32 / total as f32
        };
        st.prev_samples = Some(samples.to_vec());
        st.prev_covered = Some(covered);

        st.frames += 1;
        if st.frames % 60 == 0 {
            let luma = |p: Rgb| (p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8;
            let lo = samples.iter().map(|&s| luma(s)).min().unwrap_or(0);
            let hi = samples.iter().map(|&s| luma(s)).max().unwrap_or(0);
            let mean = samples.iter().map(|&s| luma(s)).sum::<u32>() / samples.len().max(1) as u32;
            let (judged, changed, max_frac) = st.monitor.debug_last_stats();
            let (tid, _, tfrac) = st.monitor.debug_top_box().unwrap_or((0, 0, 0));
            log::info!(
                "[screen-monitor-v2] screen_est min={lo} max={hi} mean={mean} motion={motion_frac:.2} \
                 cmotion={content_motion_frac:.2} \
                 boxes={} judged={judged} changed={changed} max_frac={max_frac}% \
                 top_box={tid}(frac={tfrac}%) ver={}",
                st.baselined.len(),
                self.session.content_version(),
            );
            // Per-box keep-reason joined with each label's text: hard% = holes whose
            // per-channel RGB delta crossed the threshold; mean = mean per-channel delta.
            let texts: HashMap<u64, String> = self
                .session
                .block_display_texts(SCREEN_ANCHOR_ID)
                .into_iter()
                .collect();
            for (id, holes, frac, max_delta, mean, changed) in st.monitor.debug_boxes() {
                let txt: String = texts
                    .get(id)
                    .map(|s| s.chars().take(20).collect())
                    .unwrap_or_default();
                log::info!(
                    "[screen-box] id={id} holes={holes} hard={frac}% maxd={max_delta} mean={mean}{} \"{txt}\"",
                    if *changed { " CHANGED" } else { "" },
                );
            }
        }

        // Reconcile boxes against the resident blocks before observing, preserving
        // the baseline of unchanged blocks (so a stale block can still detect its
        // own change). Don't touch `next_acquire_ns`: the periodic schedule and the
        // settle delay are independent of an acquire landing.
        if st.last_populated != version {
            let blocks = self.session.overlay_blocks(SCREEN_ANCHOR_ID);
            reconcile_boxes(&mut st.monitor, &blocks, samples, &mut st.baselined);
            st.last_populated = version;
        }
        // Always observe so the per-hole variance keeps learning, even while a trip
        // is suppressed.
        let classification = st.monitor.observe(samples);

        // Confirm dispatch (review item C): once the worker is busy, the owed
        // re-acquire is being served, so the flag can drop. Clearing it only here —
        // not when the action was returned — means a dispatch that never landed
        // (worker busy / capture failed) keeps the re-acquire owed.
        let busy = self.acquire_busy();
        if busy {
            st.pending_reacquire = false;
        }

        // Two routes to a drop, feeding one drop-all path (not a separate state
        // machine):
        //   * fast wholesale change — global gap-motion (scroll / navigation /
        //     app-switch). Fires the frame the screen starts moving, and clears
        //     even boxes the per-box monitor can't track (too few holes), which
        //     would otherwise persist forever.
        //   * per-box change — a single box diverging from what it's translating
        //     (e.g. a subtitle line), dropped + re-acquired in place. A large
        //     fraction of boxes changing together collapses onto drop-all too.
        // Act only when settled — NOT while an acquire is in flight: mid-acquire the
        // capture carries our own provisional pills (which the recovery doesn't
        // invert), so per-box deviation is contaminated. Acting then aborts the
        // acquire before it commits, re-presents the provisional pills, and re-trips
        // on them — an endless self-flash. The cooldown + busy gate stop a storm.
        let act = !busy && now_ns >= st.suppress_trips_until_ns;
        let total_boxes = st.baselined.len();
        let mut full_clear = false;
        let mut changed: Vec<u64> = Vec::new();
        if act {
            if motion_frac > V2_SCROLL_MOTION_FRAC {
                full_clear = true;
            }
            match &classification {
                FrameClassification::BoxesChanged(ids) => changed.extend_from_slice(ids),
                FrameClassification::Scroll => full_clear = true,
                FrameClassification::Quiet => {}
            }
            // A large fraction of several boxes changing together is a scroll /
            // navigation, not a per-pill edit: collapse it onto the drop-all path.
            // Below 3 boxes, keep the smooth per-box swap (e.g. one subtitle line).
            if total_boxes >= 3 && changed.len() as f32 >= SCROLL_CHANGED_FRAC * total_boxes as f32
            {
                full_clear = true;
            }
        }

        if full_clear {
            if !st.scrolling {
                log::info!(
                    "[screen-monitor-v2] whole-screen change (motion={:.2}, {}/{} boxes) → drop all + re-acquire",
                    motion_frac,
                    changed.len(),
                    total_boxes,
                );
                // reset_anchor_state (not clear_overlays) so the SurfaceMap OCR cache
                // is dropped too — otherwise the next detection at the same (x,y) is a
                // MergedUnchanged cache hit and re-shows the *old* text.
                self.session.reset_anchor_state(SCREEN_ANCHOR_ID);
                st.monitor.clear_boxes();
                st.baselined.clear();
                st.scrolling = true;
            }
            st.suppress_trips_until_ns = now_ns + V2_TRIP_COOLDOWN_NS;
            // Hold the re-acquire until the cleared pills have left the captured
            // mirror (settle), and until motion stops (the gate below).
            st.reacquire_not_before_ns = now_ns + V2_SETTLE_NS;
            // Pull the periodic deadline forward to the settle so an owed re-acquire
            // on a now-static screen fires right after the settle (the tick path ANDs
            // `next_acquire_ns`) instead of waiting out the leftover periodic window.
            st.next_acquire_ns = st.reacquire_not_before_ns;
            st.pending_reacquire = true;
            return MonitorAction::Hide;
        }
        st.scrolling = false;

        if !changed.is_empty() {
            changed.sort_unstable();
            changed.dedup();
            log::info!(
                "[screen-monitor-v2] {} block(s) changed under pills → drop + re-acquire",
                changed.len()
            );
            let set: std::collections::HashSet<u64> = changed.iter().copied().collect();
            let strips: Vec<OrientedRect> = self
                .session
                .overlay_blocks(SCREEN_ANCHOR_ID)
                .into_iter()
                .filter(|(id, _, _)| set.contains(id))
                .flat_map(|(_, _, s)| s)
                .collect();
            self.session
                .invalidate_surface_region(SCREEN_ANCHOR_ID, &strips);
            self.session.remove_blocks(SCREEN_ANCHOR_ID, &changed);
            st.suppress_trips_until_ns = now_ns + V2_TRIP_COOLDOWN_NS;
            // Hold the re-acquire until the dropped pills have left the mirror, so
            // the re-OCR of the now-exposed region can't re-read our own old label.
            st.reacquire_not_before_ns = now_ns + V2_SETTLE_NS;
            // Pull the periodic deadline forward so the owed re-acquire fires right
            // after the settle on a now-static screen (the tick path ANDs
            // `next_acquire_ns`), not at the leftover periodic slot up to ~1.5s out.
            st.next_acquire_ns = st.reacquire_not_before_ns;
            st.pending_reacquire = true;
        }

        // Fire a masked additive acquire once any post-drop settle has elapsed, the
        // screen has stopped moving, and nothing is already in flight. The settle (a
        // fixed delay after a drop/clear) guarantees the captured frame no longer
        // shows the dropped pills — deterministic where the old present-generation
        // fence raced the compositor. The motion gate keeps OCR off a mid-scroll
        // frame: `motion_frac` (gap points) catches motion before there's any text,
        // and `content_motion_frac` (recent motion over all points incl. under-pill
        // recovery) catches it once pills cover the page, where `motion_frac` goes
        // blind. Both must read settled.
        let settled =
            motion_frac < V2_SETTLE_MOTION_FRAC && content_motion_frac < V2_SETTLE_CONTENT_FRAC;
        if now_ns >= st.reacquire_not_before_ns
            && settled
            && !self.acquire_busy()
            && (st.pending_reacquire || now_ns >= st.next_acquire_ns)
        {
            st.next_acquire_ns = now_ns + V2_PERIODIC_NS;
            st.reacquire_not_before_ns = 0;
            return MonitorAction::Acquire;
        }
        MonitorAction::None
    }

    /// Timed tick (no new frame): fire an *owed* re-acquire (bootstrap, or the
    /// scheduled re-grab after a drop/clear) so it still happens when the screen
    /// goes static and stops emitting frames. The periodic new-text scan is not
    /// fired here — it's frame-driven, so a stable static overlay won't churn.
    pub fn monitor_screen_v2_tick(&self, now_ns: i64) -> MonitorAction {
        let mut guard = self.monitor_v2.lock().expect("monitor_v2 lock");
        let Some(st) = guard.as_mut() else {
            return MonitorAction::None;
        };
        // No captured frame here, so no motion gate — a tick only happens when the
        // mirror stopped emitting frames, i.e. the screen is static (settled). Fire
        // the owed re-acquire once its settle delay has elapsed (bootstrap, or the
        // scheduled re-grab after a drop/clear on a now-static screen).
        if st.pending_reacquire
            && now_ns >= st.reacquire_not_before_ns
            && !self.acquire_busy()
            && now_ns >= st.next_acquire_ns
        {
            st.next_acquire_ns = now_ns + V2_PERIODIC_NS;
            st.reacquire_not_before_ns = 0;
            return MonitorAction::Acquire;
        }
        MonitorAction::None
    }

    /// Whether the GL worker should poll on a timer: a settle deadline is armed,
    /// or an acquire is in flight (so it picks up the worker's provisional/full
    /// overlays even though the static screen emits no frames).
    pub fn wants_tick(&self) -> bool {
        // Keep the GL loop ticking while an acquire is in flight (so streamed
        // overlays present even on a static screen) or while a re-acquire is owed
        // (bootstrap / post-drop / post-clear).
        if self.acquire_busy() {
            return true;
        }
        self.monitor_v2
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.pending_reacquire))
            .unwrap_or(false)
    }

    pub fn set_languages(&self, from: &str, to: &str, is_auto_source: bool) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.from_lang = from.to_string();
            cfg.to_lang = to.to_string();
            cfg.is_auto_source = is_auto_source;
        }
        // Drop stale overlays; the next detect (gated by the GL worker)
        // repopulates them.
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.session.clear_overlays();
        self.reset_monitor();
    }

    /// Restrict OCR + the motion monitor to `region` (normalized), or `None` for
    /// the full screen. Like a language change, drops stale overlays and resets
    /// the monitor so the next acquire rebuilds against the new bounds.
    pub fn set_region(&self, region: Option<NormRect>) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.region = region;
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.session.clear_overlays();
        self.reset_monitor();
    }

    pub fn region(&self) -> Option<NormRect> {
        self.config.lock().ok().and_then(|c| c.region)
    }

    pub fn set_overlay_oversample(&self, factor: f32) {
        self.session.set_overlay_oversample(factor);
    }

    /// Detector pixel budget — the JNI layer uses it to size the GPU det-gray
    /// readback ([`translator_raster::live_frame::aligned_det_dims`]).
    pub fn det_max_pixels(&self) -> u32 {
        self.config
            .lock()
            .map(|c| c.det_max_pixels)
            .unwrap_or(650_000)
    }

    pub fn clear_overlay(&self) {
        self.session.clear_overlays();
    }

    pub fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.session.clear();
        self.reset_monitor();
    }

    /// Dispatch an acquire (detect/rec/translate) to the background worker for
    /// the GPU-rendered `frame`. Non-blocking; returns whether the job was
    /// queued (the monitor serializes acquires, so this should always succeed).
    pub fn dispatch_acquire(&self, frame: Arc<LiveFrame>) -> bool {
        let generation = self.generation.load(Ordering::SeqCst);
        let job = ScreenJob { frame, generation };
        let dispatched = self
            .worker
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.try_dispatch(job)))
            .unwrap_or(false);
        dispatched
    }

    /// Abort an in-flight acquire (the screen moved). Bumps the generation so the
    /// worker's `cancel` closure trips at the next rec batch.
    pub fn abort_acquire(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    /// Whether the worker is busy (an acquire is in flight or queued).
    pub fn acquire_busy(&self) -> bool {
        self.worker
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|w| w.busy()))
            .unwrap_or(false)
    }

    /// Monotonically increasing each time overlay *content* changes (the
    /// provisional pills, then each rec batch's translated block, then the
    /// post-retain cleanup). The GL thread re-presents whenever this changes —
    /// and the actual canvas raster happens in [`Self::composite_current`] on
    /// that thread, so blocks appear incrementally without stalling the worker.
    pub fn overlay_version(&self) -> u64 {
        self.session.content_version()
    }

    /// Build the GPU overlay draw list (pills + per-block text tiles) from the
    /// resident screen overlay content. The GL thread bakes this into a texture
    /// and presents it — no CPU canvas raster. `None` when there's nothing to
    /// show. Runs on the GL thread.
    pub fn overlay_draw_list(&self) -> Option<crate::live_session::OverlayDrawList> {
        self.session.overlay_draw_list(SCREEN_ANCHOR_ID)
    }

    /// Fraction of `rect`'s lattice points that moved in the last few frames — the
    /// per-box dynamic-content test for [`Self::run_screen_acquire`]. High → the
    /// region is video / a game sprite / mid-scroll, so a detection there must not
    /// be committed. `0` when the monitor isn't up yet (nothing known → don't drop).
    fn region_motion_frac(&self, rect: &OrientedRect) -> f32 {
        let guard = self.monitor_v2.lock().expect("monitor_v2 lock");
        let Some(st) = guard.as_ref() else {
            return 0.0;
        };
        if st.recent_motion.len() != st.monitor.lattice().len() {
            return 0.0;
        }
        let holes = st.monitor.lattice().holes_in_rect(rect);
        if holes.is_empty() {
            return 0.0;
        }
        let moved = holes.iter().filter(|&&h| st.recent_motion[h] > 0).count();
        moved as f32 / holes.len() as f32
    }

    /// Worker-thread body: detect → provisional overlay → rec/translate → full
    /// overlay, bumping [`overlay_version`](Self::overlay_version) after each
    /// upsert so the GL thread presents provisional pills immediately and the
    /// translated text when ready. Bails at any stage if `generation` moved
    /// (language change / reset / abort-on-movement).
    fn run_screen_acquire(&self, frame: Arc<LiveFrame>, generation: u64) {
        let cfg = self.config.lock().map(|c| c.clone()).unwrap_or_default();
        let cancel = || self.generation.load(Ordering::SeqCst) != generation;
        // The crop the frame was rendered with: full-screen, or the region sub-rect
        // (in full-frame canonical coords) the GPU readback already cropped to. The
        // worker is agnostic — detection runs on whatever the cached oriented holds,
        // and `view_to_sensor_h(sensor_crop)` re-offsets surviving boxes to their
        // full-frame screen position.
        let crop = {
            let s = frame.state().lock().expect("frame state poisoned");
            s.cached.as_ref().map(|oi| oi.display_crop).unwrap_or(Rect {
                left: 0,
                top: 0,
                right: s.width,
                bottom: s.height,
            })
        };
        // The detector ran on the (possibly cropped) region, so `detected` boxes are
        // in region-local canonical coords. The resident overlay blocks, the motion
        // lattice and the provisional overlay all live in full-frame surface coords,
        // so lift a box into surface space (a no-op for a full-screen crop) before
        // using it in any of those. Recognition keeps the region-local boxes — they
        // index the region-sized rgb, and `run_post_detect` applies this same offset
        // via `view_to_sensor_h` when it places the committed blocks.
        let to_surface = |b: &OrientedRect| OrientedRect {
            cx: b.cx + crop.left as f32,
            cy: b.cy + crop.top as f32,
            ..*b
        };
        // Additive + masked acquire: we run with the overlay UP and keep the
        // resident blocks. Detections overlapping an existing pill are *our own*
        // translated overlay re-captured in the mirror — drop them so we never
        // re-OCR ourselves. What survives is new text in the gaps (and any region
        // whose pill was blinked off by the v2 under-pill detector). A scroll
        // clears the overlay first (see the Hide path), so this same pass runs
        // "full" then. `run_post_detect` keys blocks by stable id and never drops
        // others, so re-detecting unchanged text just upserts it in place.
        // Use the resident block strips (updated immediately by remove_blocks),
        // NOT gpu_painted_pills (only refreshed on the GL thread's next bake) — a
        // just-dropped block must not still mask its now-exposed region.
        let existing_pills: Vec<OrientedRect> = self
            .session
            .overlay_blocks(SCREEN_ANCHOR_ID)
            .into_iter()
            .flat_map(|(_, _, strips)| strips)
            .collect();
        let t_det = std::time::Instant::now();
        let detected = match acquire_detect(&*self.host, &frame, crop, cfg.det_max_pixels) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("[screen] detect failed: {e}");
                return;
            }
        };
        let det_ms = t_det.elapsed().as_secs_f64() * 1000.0;
        if cancel() {
            return;
        }
        let detected: Vec<_> = detected
            .into_iter()
            .filter(|d| {
                let s = to_surface(&d.tight_box);
                !existing_pills.iter().any(|p| rects_overlap(&s, p))
            })
            .collect();
        // Drop detections over actively-moving content (video / game sprite /
        // mid-scroll): committing a pill there would pin it to content that's about
        // to change (a label OCR'd mid-animation, then stuck). Measured per-box from
        // the monitor's recent-motion accumulator — so static text (a HUD, caption,
        // or menu) on an otherwise-animated screen is still kept, unlike a global
        // motion gate which would reject everything while anything on screen moves.
        let n_before = detected.len();
        let detected: Vec<_> = detected
            .into_iter()
            .filter(|d| self.region_motion_frac(&to_surface(&d.tight_box)) <= V2_DYNAMIC_BOX_FRAC)
            .collect();
        let n_dynamic = n_before - detected.len();
        if detected.is_empty() {
            // Nothing new under the gaps (or all over moving content) — keep resident.
            log::info!(
                "[screen] detect={det_ms:.0}ms new=0 dropped_moving={n_dynamic} (kept resident)"
            );
            return;
        }
        // Provisional bbox-only pills the instant detection lands, lifted to surface
        // coords (the screen anchor composites at identity, so a region-local box
        // would otherwise flash at the screen's top-left instead of inside the crop).
        // The canvas rebuild bumps the session version, so the GL thread presents
        // these before rec/translate finishes.
        let strips: Vec<OrientedRect> = detected.iter().map(|d| to_surface(&d.tight_box)).collect();
        self.session
            .upsert_provisional_overlay(SCREEN_ANCHOR_ID, strips);
        // The upsert bumped the content version; the GL present thread builds the
        // provisional draw list + bakes it on its next poll. No CPU canvas raster
        // on the worker (the GPU compositor replaced it).
        if cancel() {
            // Cancellation (movement / scroll clear) after we already upserted the
            // provisional pills must drop them, or they orphan as a stuck overlay
            // the clear was meant to remove (review item F).
            self.session.drop_provisional_overlay(SCREEN_ANCHOR_ID);
            return;
        }
        // Geometric 90° quadrant (supports landscape); skip the rec-based 180°
        // disambiguation since a captured screen is world-up.
        let quadrant = Some(dominant_axis_quadrant(&detected));
        let t_rec = std::time::Instant::now();
        let rec_result = acquire_rec_translate(
            &*self.host,
            &self.session,
            &*self.font_provider,
            &frame,
            crop,
            &cfg.from_lang,
            &cfg.to_lang,
            cfg.is_auto_source,
            cfg.rec_batch_size,
            &detected,
            SCREEN_ANCHOR_ID,
            quadrant,
            &[],
            &[],
            &[],
            &cancel,
        );
        let rec_ms = t_rec.elapsed().as_secs_f64() * 1000.0;
        let mut rec_ok = 0;
        match rec_result {
            Ok(outcome) => {
                rec_ok = outcome.rec_ok_count;
                // Additive: do NOT `retain_blocks` to this pass's survivors —
                // that would evict the resident blocks we deliberately masked out
                // of detection. `run_post_detect` already evicts *this run's*
                // rec-failed blocks by stable id, so only the provisional bbox
                // pills for non-surviving new detections need clearing.
                self.session.drop_provisional_overlay(SCREEN_ANCHOR_ID);
            }
            Err(e) => log::warn!("[screen] rec/translate failed: {e}"),
        }
        log::info!(
            "[screen] detect={det_ms:.0}ms rec+translate={rec_ms:.0}ms boxes={} rec_ok={rec_ok}",
            detected.len(),
        );
    }
}

#[cfg(test)]
mod monitor_tests {
    use super::*;

    #[test]
    fn norm_rect_maps_to_pixels_clamped_and_ordered() {
        // Half-width centred region on a 1000×500 frame.
        let r = NormRect {
            left: 0.25,
            top: 0.2,
            right: 0.75,
            bottom: 0.8,
        };
        assert_eq!(
            r.to_px(1000, 500),
            Rect {
                left: 250,
                top: 100,
                right: 750,
                bottom: 400
            }
        );
        // Swapped/out-of-range edges are ordered and clamped in-bounds.
        let inv = NormRect {
            left: 1.5,
            top: 0.9,
            right: -0.5,
            bottom: 0.1,
        };
        let px = inv.to_px(200, 200);
        assert_eq!(
            px,
            Rect {
                left: 0,
                top: 20,
                right: 200,
                bottom: 180
            }
        );
        // A degenerate region still yields a ≥1px rect.
        let dot = NormRect {
            left: 0.5,
            top: 0.5,
            right: 0.5,
            bottom: 0.5,
        };
        let px = dot.to_px(100, 100);
        assert!(px.right > px.left && px.bottom > px.top);
    }

    fn band(cx: f32, cy: f32, w: f32, h: f32) -> OrientedRect {
        OrientedRect {
            cx,
            cy,
            width: w,
            height: h,
            angle_radians: 0.0,
        }
    }

    /// Full-lattice samples: background everywhere; inside each rect, alternate
    /// holes between `stroke` (ink) and a light value so the box has real contrast.
    /// Changing a rect's `stroke` simulates the text under that pill changing.
    fn samples_two(
        lat: &Lattice,
        a: &OrientedRect,
        a_stroke: u8,
        b: &OrientedRect,
        b_stroke: u8,
    ) -> Vec<Rgb> {
        let mut v = vec![[120u8; 3]; lat.len()];
        for (i, &h) in lat.holes_in_rect(a).iter().enumerate() {
            v[h] = if i % 2 == 0 { [a_stroke; 3] } else { [220; 3] };
        }
        for (i, &h) in lat.holes_in_rect(b).iter().enumerate() {
            v[h] = if i % 2 == 0 { [b_stroke; 3] } else { [220; 3] };
        }
        v
    }

    #[test]
    fn reconcile_keeps_baseline_so_a_stale_block_still_trips() {
        // The accumulation bug: an unrelated acquire used to rebuild every box and
        // reset its baseline, so a stale block never noticed its own change.
        // Reconcile preserves an unchanged block's baseline, so it still trips even
        // after another acquire reconciled against the *changed* frame.
        let lat = Lattice::build(100, 100, SCREEN_LATTICE_SPACING);
        let a = band(50.0, 25.0, 80.0, 20.0);
        let b = band(50.0, 75.0, 80.0, 20.0);
        let mut mon = ScreenMonitor::new(
            Lattice::build(100, 100, SCREEN_LATTICE_SPACING),
            screen_monitor_config(),
        );
        let mut baselined = HashMap::new();

        let base = samples_two(&lat, &a, 30, &b, 60);
        reconcile_boxes(
            &mut mon,
            &[(1, 100, vec![a.clone()])],
            &base,
            &mut baselined,
        );
        assert!(baselined.contains_key(&1), "block A baselined");
        // Warm up the variance mask on the stable base so A's holes freeze as glyph.
        for _ in 0..screen_monitor_config().warmup_frames + 1 {
            mon.observe(&base);
        }

        // A's text changes on screen, and an *unrelated* acquire adds block B and
        // reconciles against the changed frame. A's hash is unchanged → keep its
        // baseline; B is new → baselined to the changed frame.
        let changed = samples_two(&lat, &a, 220, &b, 60);
        reconcile_boxes(
            &mut mon,
            &[(1, 100, vec![a]), (2, 200, vec![b])],
            &changed,
            &mut baselined,
        );
        assert!(baselined.contains_key(&2), "new block B added");

        match mon.observe(&changed) {
            FrameClassification::BoxesChanged(ids) => {
                assert!(ids.contains(&1), "stale block A must still trip: {ids:?}");
                assert!(
                    !ids.contains(&2),
                    "freshly-baselined B must not trip: {ids:?}"
                );
            }
            other => panic!("expected A to trip, got {other:?}"),
        }
    }

    #[test]
    fn reconcile_rebaselines_a_reocrd_block_so_it_stays_quiet() {
        // When a block is re-OCR'd (content_hash changes), reconcile rebaselines it
        // to the new content, so it doesn't immediately re-trip (no drop/re-acquire
        // loop).
        let lat = Lattice::build(100, 100, SCREEN_LATTICE_SPACING);
        let a = band(50.0, 25.0, 80.0, 20.0);
        let b = band(50.0, 75.0, 80.0, 20.0);
        let mut mon = ScreenMonitor::new(
            Lattice::build(100, 100, SCREEN_LATTICE_SPACING),
            screen_monitor_config(),
        );
        let mut baselined = HashMap::new();

        let base = samples_two(&lat, &a, 30, &b, 60);
        reconcile_boxes(
            &mut mon,
            &[(1, 100, vec![a.clone()])],
            &base,
            &mut baselined,
        );
        for _ in 0..screen_monitor_config().warmup_frames + 1 {
            mon.observe(&base);
        }

        // Re-OCR: same block id, new content_hash → rebaselined to the new frame.
        let changed = samples_two(&lat, &a, 220, &b, 60);
        reconcile_boxes(&mut mon, &[(1, 101, vec![a])], &changed, &mut baselined);
        assert_eq!(baselined.get(&1), Some(&101), "rebaselined to the new hash");

        // Observing that same (new) frame must not trip — the baseline moved with it.
        assert_eq!(
            mon.observe(&changed),
            FrameClassification::Quiet,
            "re-OCR'd block should be quiet at its new baseline"
        );
    }
}
