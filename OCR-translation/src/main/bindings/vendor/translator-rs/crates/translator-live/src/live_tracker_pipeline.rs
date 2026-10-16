//! End-to-end orchestration for the live-camera planar OCR path.
//!
//! Owns the planar tracker engine, the smoothed homography, the
//! resident overlay store, the matted-strip cache, and an internal
//! worker thread that runs the heavy acquire / refresh pipelines off
//! the per-frame critical path. Public surface is intentionally tiny:
//!
//!   * [`LiveTrackerPipeline::process_frame`] — synchronous per-frame
//!     entry. Runs the tracker step, composites directly into a
//!     caller-supplied RGBA slice, and (if needed) materializes the
//!     frame bytes + dispatches an async acquire/refresh job.
//!   * [`LiveTrackerPipeline::set_languages`] — update the language
//!     config used by future async jobs.
//!   * [`LiveTrackerPipeline::set_target_mode`] — toggle between
//!     `Active` (normal operation) and `Suppressed` (e.g. during AF
//!     scans — every frame resets engine state and bumps generation).
//!   * [`LiveTrackerPipeline::reset`] — bump generation, clear engine
//!     + smoothed H + session state. Any in-flight worker job will
//!     observe the new generation and bail.
//!   * [`LiveTrackerPipeline::last_acquire_telemetry`] — pull the most
//!     recent async-job outcome for debug telemetry.
//!
//! Per-frame fast path returns a small `ProcessFrameResult`; the bulk
//! of acquire telemetry lives in `last_acquire_telemetry` because most
//! frames don't trigger an acquire and the per-frame caller doesn't
//! need to allocate / marshal a big struct.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::live_worker::SlotWorker;
use std::time::{Duration, Instant};

use crate::live_session::{
    LiveOcrHost, LiveSession, PostDetectInput, PostDetectOutcome, h_view_to_surface_from,
    project_oriented_rect, viewport_surface_aabb,
};
use translator_core::api::LanguageCode;
use translator_core::api::TranslatorError;
use translator_core::coords::Quadrant;
use translator_core::homography;
use translator_core::ocr::{DetectedTextBox, OrientedRect, Rect};
use translator_raster::color_matting::MattedStrip;
use translator_raster::live_frame::LiveFrame;
use translator_render::font_provider::FontProvider;
use translator_tracker::coarse_tracker::{CoarseTracker, Correction, Lifecycle};
use translator_tracker::planar_engine::{EngineConfig, LivePlanarEngine};

/// Tracker state surfaced to the caller. Mirrors the engine command
/// shape but flattened for FFI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanarTrackerState {
    Idle,
    Acquiring,
    Locked,
    Lost,
}

/// Operating mode for the pipeline. `Suppressed` mirrors the
/// pre-refactor behaviour where every frame during a camera AF scan
/// bumped the generation + cleared engine state to defeat any
/// mid-scan acquire while still letting the compositor paint
/// camera-only frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMode {
    Active,
    Suppressed,
}

#[derive(Debug, Clone)]
struct PipelineConfig {
    from_lang: String,
    to_lang: String,
    is_auto_source: bool,
    target_mode: TargetMode,
    /// Detector pixel budget — passed to `OrientedImage::build*` for
    /// downsampling. Both OCR (visible region) and tracker (full
    /// display) caches use the same cap.
    det_max_pixels: u32,
    /// Padding around each detected text box when building an anchor
    /// (see `acquire_now_in_regions`).
    anchor_padding_px: u32,
    /// Per-batch translate size during the rec → translate fan-out.
    rec_batch_size: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            from_lang: String::new(),
            to_lang: String::new(),
            is_auto_source: false,
            target_mode: TargetMode::Active,
            det_max_pixels: 750_000,
            anchor_padding_px: 60,
            rec_batch_size: 4,
        }
    }
}

/// Telemetry returned by an acquire or refresh job. Surfaced to
/// callers via [`LiveTrackerPipeline::last_acquire_telemetry`] for the
/// debug status pill; not on the per-frame fast path.
#[derive(Debug, Clone, Default)]
pub struct AcquireTelemetry {
    pub anchor_id: u64,
    pub detected_count: u32,
    pub rec_ok_count: u32,
    pub rec_empty_count: u32,
    pub cache_hits: u32,
    pub rec_called_count: u32,
    pub total_ms: f64,
    pub canceled: bool,
    pub error: Option<String>,
    /// `true` if this telemetry came from a refresh job, `false` for
    /// an acquire. Debug pills can label accordingly.
    pub is_refresh: bool,
}

/// Result of one synchronous [`LiveTrackerPipeline::process_frame`] call.
#[derive(Debug, Clone)]
pub struct ProcessFrameResult {
    pub state: PlanarTrackerState,
    pub anchor_id: u64,
    pub inliers: u32,
    /// Approximate linear magnification of the tracked plane relative
    /// to the chain root's acquire frame (`approx_scale` of the emitted
    /// root→view homography). `1.0` at acquire, grows as the camera
    /// nears the plane. `0.0` when not Locked. The camera layer turns
    /// this into a focus-distance estimate so it can drive
    /// `LENS_FOCUS_DISTANCE` without running autofocus.
    pub scale: f32,
    /// Number of bytes presented for this frame, set by the GPU present in the
    /// shell. `0` means nothing was drawn (no external camera, dims zero).
    pub composite_bytes: u32,
    /// Homography to warp the overlay by at present time (`H_surface→view` for the
    /// active anchor), or `None` when there's nothing locked to overlay. The shell
    /// bakes the overlay on content change and warps it over the camera by this.
    pub compose_h: Option<[f32; 9]>,
    /// `true` when this call spawned an async acquire pipeline job.
    pub started_acquire: bool,
    /// `true` when this call spawned an async refresh pipeline job.
    pub started_refresh: bool,
    /// Set on a gray-only frame when the tracker wants to acquire/refresh but
    /// the per-frame frame carries no RGBA. The caller reads back a full-res
    /// RGBA frame and hands both back via
    /// [`LiveTrackerPipeline::provide_acquire_rgb`]; nothing was dispatched.
    pub rgb_request: Option<AcquireRequest>,
}

/// Which heavy pipeline a deferred RGB request will run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncKind {
    Acquire,
    Refresh,
}

/// Opaque token returned by `process_frame` on a gray-only frame when an
/// acquire/refresh is due. The caller supplies the matching RGBA frame to
/// [`LiveTrackerPipeline::provide_acquire_rgb`]; carries the per-frame context
/// (crop, config, timestamp, generation) so the dispatch matches what the
/// tracker decided.
#[derive(Debug, Clone)]
pub struct AcquireRequest {
    kind: AsyncKind,
    display_crop: Rect,
    config: PipelineConfig,
    timestamp_ns: u64,
    generation: u64,
}

impl AcquireRequest {
    /// The crop the acquire will detect/recognize in — the GPU bridge stamps
    /// this onto the split [`OrientedImage`] so the cache key matches.
    pub fn display_crop(&self) -> Rect {
        self.display_crop
    }
    /// Detector pixel budget for sizing the GPU det-gray readback.
    pub fn det_max_pixels(&self) -> u32 {
        self.config.det_max_pixels
    }
}

/// Per-anchor record of the last emitted homography. Used so the
/// `LOSS_HIDE_AFTER_FRAMES` grace period during Lost can keep
/// projecting overlays through the last-good H before they hide.
#[derive(Default)]
struct LastEmittedH {
    h: Option<[f32; 9]>,
    anchor_id: u64,
    consecutive_lost: u32,
}

const LOSS_HIDE_AFTER_FRAMES: u32 = 4;

/// Dispatch the Relocalizer (`engine.relocalize`) to the async worker every
/// Nth frame. The CoarseTracker tracks every frame; the worker runs
/// fire-and-forget — its Correction arrives ~1 wall frame later and the weave
/// composes the absolute fix with the view-space KLT motion since the frame
/// the worker consumed (non-identity, unlike the synchronous step-2 weave
/// which reduced to a snap). The engine's frame-counted gates are rescaled by
/// this value at pipeline init so wall-time hysteresis matches cadence-1.
const RELOCALIZER_CADENCE: u64 = 2;
const RELOCK_OVERLAP_THRESHOLD: f32 = 0.65;
/// Inflation around the viewport AABB when asking the session
/// "is this view already covered?". A small pad swallows the
/// per-frame tracker jitter without admitting genuinely-new
/// surface area as already-covered. Surface coords are anchor-
/// resolution, so 24 px = ~2-3% of typical viewport extent.
const COVERAGE_PAD_PX: f32 = 24.0;
const ENABLE_COLOR_MATTING: bool = true;

/// Defaults divided by [`RELOCALIZER_CADENCE`] so wall-time hysteresis matches
/// step 1. The engine counts in frames *it sees*; at cadence N each engine tick
/// is N wall frames, so a 5-frame Lost gate would otherwise become 10 wall
/// frames. `handoff_cooldown_ns` and the loss-hide grace are ns- or wall-frame-
/// counted and untouched.
fn rescaled_engine_config() -> EngineConfig {
    let mut cfg = EngineConfig::default();
    let n = RELOCALIZER_CADENCE.max(1) as u32;
    let div_ceil = |t: u32| t.div_ceil(n);
    cfg.lost_after_frames = div_ceil(cfg.lost_after_frames);
    cfg.give_up_after_frames = div_ceil(cfg.give_up_after_frames);
    cfg.degraded_max_frames = div_ceil(cfg.degraded_max_frames);
    // `anchor_switch_blend_frames` is engine-side emit-smoothing; it ticks at
    // engine rate too, so rescale until the blend moves to CoarseTracker.
    cfg.anchor_switch_blend_frames = div_ceil(cfg.anchor_switch_blend_frames);
    cfg
}

/// What kind of async job the per-frame fast path needs to dispatch.
#[derive(Clone)]
enum PendingJob {
    Acquire {
        frame: Arc<LiveFrame>,
        display_crop: Rect,
        config: PipelineConfig,
        timestamp_ns: u64,
        generation: u64,
    },
    Refresh {
        frame: Arc<LiveFrame>,
        display_crop: Rect,
        config: PipelineConfig,
        generation: u64,
    },
}

/// One per-frame engine job handed to the [`TrackerCompute`] thread. Owns
/// the cloned tracker gray so the caller can run the camera upload (which
/// reads the same frame's RGBA) concurrently without sharing the buffer.
struct TrackerRequest {
    gray: image::GrayImage,
    timestamp_ns: u64,
    /// CoarseTracker's current `H_root→view` (det-res), used as the engine's
    /// guided-match prior — replaces the engine's `last_homography` fallback so
    /// the descriptor matcher sees this frame's KLT pose, not the previous fit.
    coarse_prior: Option<[f32; 9]>,
    /// Monotonic frame index — tags the resulting Correction so the
    /// CoarseTracker's ringbuffer can locate the matching `H_then` for the weave.
    frame_idx: u64,
    /// `(root_x, root_y, view_x, view_y)` seeds from the CoarseTracker at
    /// `frame_idx`. The Relocalizer transforms them into the active leaf's
    /// canonical frame and prepends to RANSAC, anchoring h6/h7 against the
    /// clustered-inlier perspective wobble descriptor-only fits exhibit.
    coarse_seeds: Vec<(f32, f32, f32, f32)>,
}

/// What [`LiveTrackerPipeline::run_engine`] produces: the engine's correction
/// (det-coord refinement_h + root-coord seeds + lifecycle) plus its sub-step
/// timings. The pipeline scales `refinement_h` by `det_to_full` before applying.
type TrackerComputeResult =
    Result<(Correction, translator_tracker::planar_engine::StepTimings), TranslatorError>;

/// Long-lived worker thread that runs `engine.relocalize` asynchronously
/// (async-H step 3). The present thread no longer blocks on it: it
/// `try_dispatch`es a fresh job at cadence boundaries (gated by a single
/// `in_flight` slot — backpressure drops duplicate dispatches rather than
/// queueing) and `try_take_result`s any completed Correction at the top of
/// each frame. Corrections arrive ~1 wall frame after dispatch on this
/// device, and the CoarseTracker's weave composes the absolute fix with the
/// view-space KLT motion since the frame the worker consumed.
///
/// Shutdown is implicit: the channel ends live on the pipeline, so when the
/// pipeline is dropped `req_rx.recv()` returns `Err` and the thread exits.
/// The worker holds only a `Weak` ref, upgraded per frame and dropped before
/// it blocks on `recv`, so it never keeps the pipeline alive.
struct TrackerCompute {
    req_tx: std::sync::mpsc::Sender<TrackerRequest>,
    resp_rx: std::sync::mpsc::Receiver<TrackerComputeResult>,
    /// `true` between a successful `try_dispatch` and the matching
    /// `try_take_result`. Caps the worker queue at one outstanding job so a
    /// slow tick can't pile up behind a fast camera cadence.
    in_flight: AtomicBool,
}

impl TrackerCompute {
    fn spawn(pipeline: Arc<LiveTrackerPipeline>) -> Self {
        let (req_tx, req_rx) = std::sync::mpsc::channel::<TrackerRequest>();
        let (resp_tx, resp_rx) = std::sync::mpsc::channel::<TrackerComputeResult>();
        let pipeline_weak = Arc::downgrade(&pipeline);
        std::thread::Builder::new()
            .name("LiveTrackerCompute".into())
            .spawn(move || {
                while let Ok(req) = req_rx.recv() {
                    let Some(pipeline) = pipeline_weak.upgrade() else {
                        return;
                    };
                    let result = pipeline.run_engine(&req);
                    // Drop the upgraded Arc before blocking on the next
                    // `recv`, so a concurrent pipeline drop isn't deadlocked
                    // behind this thread holding the last reference.
                    drop(pipeline);
                    if resp_tx.send(result).is_err() {
                        return;
                    }
                }
            })
            .expect("failed to spawn LiveTrackerCompute");
        TrackerCompute {
            req_tx,
            resp_rx,
            in_flight: AtomicBool::new(false),
        }
    }

    /// Fire-and-forget dispatch. Returns `true` when the job was sent. Returns
    /// `false` when a previous job is still in flight — the caller drops this
    /// tick rather than queueing, so a slow worker can't pile up behind a fast
    /// camera cadence. The caller MUST drain pending results via
    /// [`try_take_result`](Self::try_take_result) before dispatching, otherwise
    /// the in-flight slot stays set until the next poll.
    fn try_dispatch(&self, req: TrackerRequest) -> bool {
        if self.in_flight.swap(true, Ordering::Acquire) {
            return false;
        }
        if let Err(e) = self.req_tx.send(req) {
            // Worker died — unset so the caller can stop trying.
            self.in_flight.store(false, Ordering::Release);
            log::error!("tracker compute thread died on dispatch: {e:?}");
            return false;
        }
        true
    }

    /// Non-blocking poll. Returns `Some` when the most recently-dispatched job
    /// has finished, clearing the in-flight slot so a fresh dispatch can fire.
    fn try_take_result(&self) -> Option<TrackerComputeResult> {
        match self.resp_rx.try_recv() {
            Ok(r) => {
                self.in_flight.store(false, Ordering::Release);
                Some(r)
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.in_flight.store(false, Ordering::Release);
                log::error!("tracker compute thread disconnected");
                None
            }
        }
    }
}

/// Rolling per-frame timing for `process_frame`. Reset every
/// `TIMING_WINDOW_FRAMES` calls; emits one log line per window so the
/// caller can spot regressions or pauses without per-frame chatter.
///
/// `tracker_ms_sum` is the engine's wall time; the `*_sub_sum` fields
/// break it down into the same sub-steps the engine reports via
/// `LivePlanarEngine::last_step_timings`. `composite_overlay_count_sum`
/// lets the consumer see overlay count alongside warp time.
struct TimingStats {
    window_count: u32,
    total_ms_sum: f64,
    /// CPU crop+downscale+RGBA→luma for the tracker gray
    /// (`prepare_tracker_gray`). Measured *outside* `tracker_ms_sum` (it
    /// runs before the engine step), so it's logged as a sibling of
    /// `tracker`/`composite` — this is the full-res CPU pass that Deferred B
    /// (zero-copy GPU camera) would move onto the GPU, so it's the number
    /// that justifies or kills that change.
    gray_build_ms_sum: f64,
    tracker_ms_sum: f64,
    tracker_features_ms_sum: f64,
    tracker_track_ms_sum: f64,
    tracker_chain_refine_ms_sum: f64,
    tracker_cached_ms_sum: f64,
    composite_ms_sum: f64,
    composite_overlay_count_sum: u64,
    window_start: Instant,
}

impl Default for TimingStats {
    fn default() -> Self {
        Self {
            window_count: 0,
            total_ms_sum: 0.0,
            gray_build_ms_sum: 0.0,
            tracker_ms_sum: 0.0,
            tracker_features_ms_sum: 0.0,
            tracker_track_ms_sum: 0.0,
            tracker_chain_refine_ms_sum: 0.0,
            tracker_cached_ms_sum: 0.0,
            composite_ms_sum: 0.0,
            composite_overlay_count_sum: 0,
            window_start: Instant::now(),
        }
    }
}

const TIMING_WINDOW_FRAMES: u32 = 10;

/// The pipeline itself.
/// What the pipeline carries across non-engine frames at cadence > 1. The
/// CoarseTracker holds the geometry (incl. canonical rotation); this carries
/// only what downstream non-coarse code reads from the engine's last verdict so
/// the pipeline keeps emitting the right `PlanarTrackerState` + `anchor_id`
/// between Relocalizer ticks.
#[derive(Clone, Copy)]
struct LastVerdict {
    lifecycle: Lifecycle,
    root_id: u64,
    inliers: u32,
}

impl Default for LastVerdict {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::Lost,
            root_id: 0,
            inliers: 0,
        }
    }
}

/// Backoff for acquire attempts after empty detections. Without it an empty
/// scene (covered camera, blank wall) re-runs det back-to-back forever,
/// pinning the MNN threads at full duty. Armed by an empty detect, doubled per
/// consecutive empty, cleared by a non-empty detect, a user reset, or a scene
/// change (the block-luma signature moving away from what the armed scene
/// looked like).
struct AcquireBackoff {
    until: Option<Instant>,
    delay: Duration,
    scene_sig: Option<[u8; 64]>,
}

impl Default for AcquireBackoff {
    fn default() -> Self {
        Self {
            until: None,
            delay: Self::INITIAL_DELAY,
            scene_sig: None,
        }
    }
}

impl AcquireBackoff {
    const INITIAL_DELAY: Duration = Duration::from_millis(250);
    const MAX_DELAY: Duration = Duration::from_secs(2);
    /// Mean abs block-luma delta (0-255) above which the scene counts as
    /// changed and a fresh detect is worth running immediately.
    const SCENE_DELTA: f32 = 6.0;

    fn active(&self) -> bool {
        self.until.is_some_and(|t| Instant::now() < t)
    }

    fn arm(&mut self) {
        self.delay = if self.until.is_some() {
            (self.delay * 2).min(Self::MAX_DELAY)
        } else {
            Self::INITIAL_DELAY
        };
        self.until = Some(Instant::now() + self.delay);
        // Re-sampled by the next `observe_scene`; the scene is static while
        // the backoff holds, so the one-frame lag doesn't matter.
        self.scene_sig = None;
    }

    fn clear(&mut self) {
        self.until = None;
        self.scene_sig = None;
    }

    /// Track the scene signature while armed; clears the backoff when the
    /// view changes enough that the next acquire may find something new.
    fn observe_scene(&mut self, gray: &image::GrayImage) {
        if !self.active() {
            return;
        }
        let sig = block_luma_signature(gray);
        match self.scene_sig {
            None => self.scene_sig = Some(sig),
            Some(prev) => {
                let delta = prev
                    .iter()
                    .zip(sig.iter())
                    .map(|(a, b)| (*a as f32 - *b as f32).abs())
                    .sum::<f32>()
                    / 64.0;
                if delta > Self::SCENE_DELTA {
                    self.clear();
                }
            }
        }
    }
}

/// 8×8 grid of block-mean lumas, subsampled every 4th pixel in each axis.
fn block_luma_signature(gray: &image::GrayImage) -> [u8; 64] {
    let (w, h) = (gray.width().max(8), gray.height().max(8));
    let mut sums = [0u32; 64];
    let mut counts = [0u32; 64];
    let mut y = 0;
    while y < gray.height() {
        let by = (y * 8 / h).min(7) as usize;
        let mut x = 0;
        while x < gray.width() {
            let bx = (x * 8 / w).min(7) as usize;
            let i = by * 8 + bx;
            sums[i] += gray.get_pixel(x, y)[0] as u32;
            counts[i] += 1;
            x += 4;
        }
        y += 4;
    }
    let mut sig = [0u8; 64];
    for (i, out) in sig.iter_mut().enumerate() {
        *out = (sums[i] / counts[i].max(1)) as u8;
    }
    sig
}

pub struct LiveTrackerPipeline {
    /// Async-H fast half: per-frame KLT pose owner. The present thread runs
    /// `track` to produce the compose pose + the prior the engine relocalizes
    /// against; `apply` weaves the engine's Correction back in.
    coarse: Mutex<CoarseTracker>,
    /// Monotonic per-frame counter — tags each Correction so the weave can find
    /// the matching ringbuffer entry.
    frame_counter: AtomicU64,
    /// Last engine verdict — carried across non-engine frames at cadence > 1 so
    /// the pipeline keeps emitting the right state between Relocalizer ticks.
    last_verdict: Mutex<LastVerdict>,
    engine: Mutex<LivePlanarEngine>,
    session: Arc<LiveSession>,
    last_emitted_h: Mutex<LastEmittedH>,
    last_root_to_view: Mutex<Option<(u64, [f32; 9])>>,
    pending_refresh_target: Mutex<Option<(u64, [f32; 9])>>,
    pending_compose: Mutex<Option<(u64, [f32; 9])>>,
    matted_strips: Mutex<HashMap<u64, Vec<Option<MattedStrip>>>>,
    /// Per-anchor ink text-metrics from the matting stage (indexed parallel to
    /// the anchor's detections), consumed by `run_post_detect` to re-fit each
    /// line's grouping box. Lives alongside `matted_strips` because both are
    /// produced from the same per-frame ink mattes.
    line_metrics: Mutex<HashMap<u64, Vec<Option<translator_raster::text_metrics::LineMetrics>>>>,
    /// Per-anchor per-box ink bold column profiles, indexed parallel to the anchor's
    /// detections. Pooled per word against CTC firings in `run_post_detect`. `None` per box
    /// when the model has no bold channel; populated alongside `line_metrics`.
    bold_profiles: Mutex<HashMap<u64, Vec<Option<translator_raster::text_metrics::BoldProfile>>>>,
    generation: AtomicU64,
    config: Mutex<PipelineConfig>,
    host: Arc<dyn LiveOcrHost>,
    font_provider: Arc<dyn FontProvider + Send + Sync>,
    last_telemetry: Mutex<Option<AcquireTelemetry>>,
    acquire_backoff: Mutex<AcquireBackoff>,
    worker: Mutex<Option<SlotWorker<PendingJob>>>,
    /// Long-lived engine-step thread. `Option` because, like `worker`, it's
    /// spawned after the `Arc<Self>` exists. Driven only by `process_frame`,
    /// so the outer mutex is uncontended.
    tracker_compute: Mutex<Option<TrackerCompute>>,
    timing: Mutex<TimingStats>,
}

impl LiveTrackerPipeline {
    /// Construct a new pipeline. The internal worker thread is
    /// lazily spawned on the first frame so callers that build a
    /// pipeline but never feed frames don't pay for it.
    pub fn new(
        host: Arc<dyn LiveOcrHost>,
        font_provider: Arc<dyn FontProvider + Send + Sync>,
    ) -> Arc<Self> {
        let engine_cfg = rescaled_engine_config();
        let coarse = CoarseTracker::new(engine_cfg.tracker.clone());
        let pipeline = Arc::new(Self {
            coarse: Mutex::new(coarse),
            frame_counter: AtomicU64::new(0),
            last_verdict: Mutex::new(LastVerdict::default()),
            engine: Mutex::new(LivePlanarEngine::new(engine_cfg)),
            session: Arc::new(LiveSession::new()),
            last_emitted_h: Mutex::new(LastEmittedH::default()),
            last_root_to_view: Mutex::new(None),
            pending_refresh_target: Mutex::new(None),
            pending_compose: Mutex::new(None),
            matted_strips: Mutex::new(HashMap::new()),
            line_metrics: Mutex::new(HashMap::new()),
            bold_profiles: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
            config: Mutex::new(PipelineConfig::default()),
            host,
            font_provider,
            last_telemetry: Mutex::new(None),
            acquire_backoff: Mutex::new(AcquireBackoff::default()),
            worker: Mutex::new(None),
            tracker_compute: Mutex::new(None),
            timing: Mutex::new(TimingStats::default()),
        });
        let worker = {
            let pipeline_weak = Arc::downgrade(&pipeline);
            SlotWorker::spawn("LiveTrackerPipelineWorker", move |job: PendingJob| {
                let Some(pipeline) = pipeline_weak.upgrade() else {
                    return;
                };
                let telemetry = match job {
                    PendingJob::Acquire {
                        frame,
                        display_crop,
                        config,
                        timestamp_ns,
                        generation,
                    } => pipeline.run_acquire_inner(
                        &frame,
                        display_crop,
                        &config,
                        timestamp_ns,
                        generation,
                    ),
                    PendingJob::Refresh {
                        frame,
                        display_crop,
                        config,
                        generation,
                    } => pipeline.run_refresh_inner(&frame, display_crop, &config, generation),
                };
                if let Ok(mut slot) = pipeline.last_telemetry.lock() {
                    *slot = Some(telemetry);
                }
            })
        };
        *pipeline.worker.lock().expect("worker slot poisoned") = Some(worker);
        let tracker_compute = TrackerCompute::spawn(Arc::clone(&pipeline));
        *pipeline
            .tracker_compute
            .lock()
            .expect("tracker compute slot poisoned") = Some(tracker_compute);
        pipeline
    }

    pub fn set_languages(&self, from: &str, to: &str, is_auto_source: bool) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.from_lang = from.to_string();
            cfg.to_lang = to.to_string();
            cfg.is_auto_source = is_auto_source;
        }
    }

    pub fn set_target_mode(&self, mode: TargetMode) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.target_mode = mode;
        }
    }

    /// Set the overlay-canvas oversample factor (texels per surface
    /// unit). The caller derives it from the display/canonical
    /// resolution ratio so the overlay rasterizes near display res
    /// while the OCR/tracker frame stays canonical-sized.
    pub fn set_overlay_oversample(&self, factor: f32) {
        self.session.set_overlay_oversample(factor);
    }

    /// The pipeline's overlay session — the GL shell reads its content version +
    /// draw list to bake/warp the overlay at present time.
    pub fn session(&self) -> &LiveSession {
        &self.session
    }

    /// Bump generation, clear engine state + smoothed H + session
    /// state. Any in-flight worker job will observe the new generation
    /// and bail at its next gen-check.
    pub fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut engine) = self.engine.lock() {
            engine.clear();
        }
        // CoarseTracker no longer resets on Lost/ReAcquire (it keeps KLT-
        // tracking through the engine's re-acquire window so the overlay
        // doesn't freeze) — so an explicit user reset must clear it here.
        if let Ok(mut coarse) = self.coarse.lock() {
            coarse.reset();
        }
        if let Ok(mut v) = self.last_verdict.lock() {
            *v = LastVerdict::default();
        }
        if let Ok(mut sm) = self.last_emitted_h.lock() {
            *sm = LastEmittedH::default();
        }
        if let Ok(mut slot) = self.last_root_to_view.lock() {
            *slot = None;
        }
        if let Ok(mut slot) = self.pending_refresh_target.lock() {
            *slot = None;
        }
        if let Ok(mut slot) = self.pending_compose.lock() {
            *slot = None;
        }
        if let Ok(mut backoff) = self.acquire_backoff.lock() {
            backoff.clear();
        }
        self.session.clear();
    }

    /// Drop all resident overlay items. Compositor will draw a
    /// camera-only frame after this.
    pub fn clear_overlay(&self) {
        self.session.clear_overlays();
    }

    /// Pull (and clear) the most recent async-job telemetry. The
    /// per-frame fast path doesn't return this; callers poll it
    /// separately for the debug status pill.
    pub fn last_acquire_telemetry(&self) -> Option<AcquireTelemetry> {
        self.last_telemetry.lock().ok().and_then(|mut g| g.take())
    }

    /// One-shot per-frame entry. Locks the engine + frame state internally, runs
    /// the tracker step, and (when needed) materializes the frame bytes +
    /// dispatches an async acquire/refresh job. It does NOT present — the GPU
    /// present lives in the shell, which reads `compose_h` from the result and
    /// warps the baked overlay over the camera. This keeps `process_frame` GL-free
    /// (the rendezvous test drives it without a GL context).
    ///
    /// `visible_sensor_w/h` are the visible-region dims in sensor coords (typically
    /// equal to `dst_w/h` when the SurfaceView uses FILL_CENTER on the sensor
    /// frame). `full_view_w/h` are the *full-display* dims in sensor coords, used by
    /// the relock decision (which compares the current viewport against the anchor's
    /// lock viewport in full coords).
    #[allow(clippy::too_many_arguments)]
    pub fn process_frame(
        &self,
        frame: &Arc<LiveFrame>,
        display_crop: Rect,
        full_view_w: u32,
        full_view_h: u32,
        timestamp_ns: u64,
    ) -> Result<ProcessFrameResult, TranslatorError> {
        let cfg = self.config.lock().map(|c| c.clone()).unwrap_or_default();
        // AF-scan or other "suppressed" intent: bump generation every
        // frame so any in-flight acquire bails at its next check; clear
        // engine + smoothed state so the stable-window restarts on
        // every frame and the overlay disappears.
        if matches!(cfg.target_mode, TargetMode::Suppressed) {
            self.generation.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut engine) = self.engine.lock() {
                engine.clear();
            }
            if let Ok(mut coarse) = self.coarse.lock() {
                coarse.reset();
            }
            if let Ok(mut v) = self.last_verdict.lock() {
                *v = LastVerdict::default();
            }
            if let Ok(mut sm) = self.last_emitted_h.lock() {
                *sm = LastEmittedH::default();
            }
        }

        let t_frame = Instant::now();

        // Build the small downscaled tracker gray under the frame lock, then
        // drop the lock so the heavy engine step can run lock-free. This is
        // what lets the camera upload (which also reads the frame's RGBA)
        // proceed concurrently with the tracker on this thread.
        let t_gray = Instant::now();
        let (tracker_gray, det_to_full) = self.prepare_tracker_gray(frame, cfg.det_max_pixels)?;
        let gray_build_ms = t_gray.elapsed().as_secs_f64() * 1000.0;

        // No-op unless an empty-detect backoff is armed; then it watches for
        // the scene changing so the next acquire can fire immediately.
        if let Ok(mut backoff) = self.acquire_backoff.lock() {
            backoff.observe_scene(&tracker_gray);
        }

        // Async-H step 3: the Relocalizer runs on the worker fire-and-forget.
        // Order matters at the async seam — **apply → track → dispatch**:
        //
        // 1. Apply any completed Correction first. This updates `current_h`
        //    and `seeds` to reflect the engine's latest verdict for an older
        //    frame.
        // 2. Then run `track()`, so KLT consumes the post-apply seeds and
        //    `ring[frame_idx]` is pushed *after* the weave. The ring entry is
        //    now identical to whatever the dispatch a few lines below would
        //    see as `current_h`.
        // 3. Then dispatch the new engine job carrying that same post-track
        //    state. Whatever Correction comes back for this frame later, its
        //    `weave` will compute `motion = h_now · inv(ring[frame_idx])`
        //    against the matching temporal base — not a stale pre-apply KLT
        //    pose. Without this order the ring entry at a cadence frame where
        //    apply *and* dispatch fire is pre-apply while the dispatched
        //    prior is post-apply, and the eventual weave mixes incompatible
        //    bases (the previous correction's jump leaks into the next
        //    correction's motion factor, creating a deterministic period-2
        //    "KLT basin ↔ relocalizer basin" loop visible as cadence-locked
        //    perspective wobble).
        let frame_idx = self.frame_counter.fetch_add(1, Ordering::Relaxed);
        let t_tracker = Instant::now();
        // Poll the worker for any completed Correction. This is the *only*
        // place anchor/lifecycle state advances; everything else is the
        // CoarseTracker's frame-by-frame KLT.
        let polled = self
            .tracker_compute
            .lock()
            .map_err(|_| poisoned())?
            .as_ref()
            .and_then(|tc| tc.try_take_result());
        let (applied_correction, step_timings) = match polled {
            Some(Ok((correction, st))) => {
                let verdict = LastVerdict {
                    lifecycle: correction.lifecycle,
                    root_id: correction.root_id,
                    inliers: correction.inliers as u32,
                };
                let lifecycle_for_dispatch = correction.lifecycle;
                self.coarse
                    .lock()
                    .map_err(|_| poisoned())?
                    .apply(correction);
                if let Ok(mut slot) = self.last_verdict.lock() {
                    *slot = verdict;
                }
                (Some(lifecycle_for_dispatch), st)
            }
            Some(Err(e)) => {
                log::warn!("engine relocalize failed: {e:?}");
                (
                    None,
                    translator_tracker::planar_engine::StepTimings::default(),
                )
            }
            None => (
                None,
                translator_tracker::planar_engine::StepTimings::default(),
            ),
        };

        // Track on the post-apply state. KLT picks up whatever seeds the
        // Correction (if any) just installed.
        let _ = self
            .coarse
            .lock()
            .map_err(|_| poisoned())?
            .track(&tracker_gray, frame_idx);

        // Dispatch a fresh engine job at cadence boundaries, regardless of
        // whether `track()` succeeded this frame. If KLT failed the eventual
        // Correction lands via `snap` (no ring[frame_idx] entry → `weave`
        // falls through to `snap`), which is the right recovery — the
        // alternative is letting the engine sit blind to a scene change and
        // never declare Lost.
        let engine_frame = frame_idx % RELOCALIZER_CADENCE == 0;
        let dispatch_ok = if engine_frame {
            let (coarse_prior, coarse_seeds) = {
                let c = self.coarse.lock().map_err(|_| poisoned())?;
                (c.current_h(), c.seeds_snapshot())
            };
            self.tracker_compute
                .lock()
                .map_err(|_| poisoned())?
                .as_ref()
                .map(|tc| {
                    tc.try_dispatch(TrackerRequest {
                        gray: tracker_gray,
                        timestamp_ns,
                        coarse_prior,
                        frame_idx,
                        coarse_seeds,
                    })
                })
                .unwrap_or(false)
        } else {
            // tracker_gray drops here unused — coarse already borrowed it.
            false
        };
        let _ = dispatch_ok;

        let v = self.last_verdict.lock().map(|g| *g).unwrap_or_default();
        let (lifecycle, tracker_anchor, tracker_inliers) = (v.lifecycle, v.root_id, v.inliers);

        // Compose pose from the CoarseTracker's `current_h` (post-track and,
        // when applied this frame, post-weave). Gated by the engine's
        // lifecycle: Locked → emit; Lost/ReAcquire → tracker_h = None and let
        // `select_compose_h`'s loss-hide grace handle the brief gap until the
        // next Locked Correction snaps in. Coarse is reset by `apply` on
        // Lost/ReAcquire, so `current_h` is already None then anyway.
        // Read the EMA-smoothed compose pose (per-frame low-pass on top of the
        // raw KLT+weave path), not the raw `current_h`. The engine's prior
        // continues to read `current_h` further up so the matcher isn't fed
        // delayed data.
        let coarse_h = self.coarse.lock().map_err(|_| poisoned())?.compose_h();
        let tracker_h = match lifecycle {
            Lifecycle::Locked => coarse_h.map(|h| {
                if det_to_full != 1.0 {
                    scale_homography(&h, det_to_full)
                } else {
                    h
                }
            }),
            _ => None,
        };
        let tracker_state = match lifecycle {
            Lifecycle::Locked => PlanarTrackerState::Locked,
            Lifecycle::ReAcquire => PlanarTrackerState::Acquiring,
            Lifecycle::Lost => PlanarTrackerState::Lost,
        };
        let tracker_scale = tracker_h
            .map(|h| (h[0] * h[4] - h[1] * h[3]).abs().sqrt())
            .unwrap_or(0.0);
        let tracker_ms = t_tracker.elapsed().as_secs_f64() * 1000.0;
        let frame_state_dims = {
            let state = frame.state().lock().map_err(|_| poisoned())?;
            (state.width, state.height, state.rotation_degrees)
        };

        let h_for_compose = self.select_compose_h(tracker_state, tracker_anchor, tracker_h);
        if let Ok(mut slot) = self.pending_compose.lock() {
            *slot = h_for_compose.map(|h| (tracker_anchor, h));
        }

        // Compositing moved to the GPU present in the shell (`live_gpu_tick`):
        // `process_frame` only decides *what* to present. `compose_h` carries the
        // homography; the shell bakes the overlay (on content change) and warps it
        // over the camera passthrough. `composite_bytes` is filled in by the shell
        // after the present.
        let overlay_count = 0u32;
        let composite_ms = 0.0f64;

        // Detect-on-tracking refresh trigger. Gated on the engine's lifecycle
        // (not `tracker_state`, which is now Locked-when-coarse-has-a-pose for
        // compositing); refresh only makes sense when the engine itself is
        // locked.
        let should_refresh = matches!(lifecycle, Lifecycle::Locked)
            && self.update_refresh_trigger(
                tracker_anchor,
                tracker_h,
                frame_state_dims,
                full_view_w,
                full_view_h,
            );

        // Mode decision off the engine's lifecycle, NOT the compositing
        // `tracker_state` — otherwise the "coarse keeps tracking through
        // ReAcquire" change suppresses the rgb_request that drives det/rec.
        // Dispatch only on frames where the worker actually delivered a fresh
        // Correction so a sustained ReAcquire window doesn't spam the gray-only
        // rgb_request path every frame.
        let async_kind =
            if applied_correction.is_none() || matches!(cfg.target_mode, TargetMode::Suppressed) {
                None
            } else {
                match lifecycle {
                    Lifecycle::ReAcquire => {
                        let backed_off = self
                            .acquire_backoff
                            .lock()
                            .map(|b| b.active())
                            .unwrap_or(false);
                        if backed_off {
                            None
                        } else {
                            Some(AsyncKind::Acquire)
                        }
                    }
                    Lifecycle::Locked if should_refresh => Some(AsyncKind::Refresh),
                    _ => None,
                }
            };

        // A gray-only frame (GPU readback) carries no RGBA, so the heavy
        // det/rec can't run from it. Instead of dispatching, hand the caller an
        // `rgb_request`; it reads back a full-res RGBA frame and feeds it to
        // `provide_acquire_rgb`. Frames that carry RGBA (Android) dispatch
        // inline as before. Both gated by worker backpressure.
        let (started_acquire, started_refresh, rgb_request) = match async_kind {
            None => (false, false, None),
            Some(kind) if frame.is_gray_only() => (
                false,
                false,
                Some(AcquireRequest {
                    kind,
                    display_crop,
                    config: cfg.clone(),
                    timestamp_ns,
                    generation: self.generation.load(Ordering::SeqCst),
                }),
            ),
            Some(kind) => {
                let (a, r) = self.dispatch_async(
                    frame,
                    kind,
                    display_crop,
                    &cfg,
                    timestamp_ns,
                    self.generation.load(Ordering::SeqCst),
                );
                (a, r, None)
            }
        };

        // Materialize bytes if any async work was dispatched, else
        // drop the borrow. Cheap (~3 ms memcpy at 1.2 MP) and avoided
        // entirely on the common pure-tracking frame.
        {
            let mut state = frame.state().lock().map_err(|_| poisoned())?;
            if started_acquire || started_refresh {
                state.materialize_owned();
            } else {
                state.clear_external();
            }
        }

        let total_ms = t_frame.elapsed().as_secs_f64() * 1000.0;
        if let Ok(mut t) = self.timing.lock() {
            t.window_count += 1;
            t.total_ms_sum += total_ms;
            t.gray_build_ms_sum += gray_build_ms;
            t.tracker_ms_sum += tracker_ms;
            t.tracker_features_ms_sum += step_timings.features_ms;
            t.tracker_track_ms_sum += step_timings.track_ms;
            t.tracker_chain_refine_ms_sum += step_timings.chain_refine_ms;
            t.tracker_cached_ms_sum += step_timings.cached_match_ms;
            t.composite_ms_sum += composite_ms;
            t.composite_overlay_count_sum += overlay_count as u64;
            if t.window_count >= TIMING_WINDOW_FRAMES {
                let n = t.window_count as f64;
                let wall_s = t.window_start.elapsed().as_secs_f64();
                let fps = if wall_s > 1e-6 { n / wall_s } else { 0.0 };
                log::info!(
                    "[lt] {} frames fps={:.1} total={:.1}ms gray={:.1}ms tracker={:.1}ms (feat={:.1} match={:.1} chain={:.1} cached={:.1}) composite={:.1}ms (overlays={:.1})",
                    t.window_count,
                    fps,
                    t.total_ms_sum / n,
                    t.gray_build_ms_sum / n,
                    t.tracker_ms_sum / n,
                    t.tracker_features_ms_sum / n,
                    t.tracker_track_ms_sum / n,
                    t.tracker_chain_refine_ms_sum / n,
                    t.tracker_cached_ms_sum / n,
                    t.composite_ms_sum / n,
                    t.composite_overlay_count_sum as f64 / n,
                );
                *t = TimingStats::default();
            }
        }

        Ok(ProcessFrameResult {
            state: tracker_state,
            anchor_id: tracker_anchor,
            inliers: tracker_inliers,
            scale: tracker_scale,
            composite_bytes: 0,
            compose_h: h_for_compose,
            started_acquire,
            started_refresh,
            rgb_request,
        })
    }

    // ---- internal helpers ----

    /// Build (or reuse) the downscaled tracker gray and clone it out, so the
    /// caller can drop the `frame.state` lock before the heavy engine step.
    /// The clone is small (~300 KB) — far cheaper than holding the frame
    /// mutex across `engine.process_frame` (which would serialize the
    /// concurrent camera upload against the tracker).
    fn prepare_tracker_gray(
        &self,
        frame: &Arc<LiveFrame>,
        det_max_pixels: u32,
    ) -> Result<(image::GrayImage, f32), TranslatorError> {
        let mut state = frame.state().lock().map_err(|_| poisoned())?;
        state.ensure_tracker_oriented(det_max_pixels)?;
        let oriented = state
            .cached_tracker
            .as_ref()
            .expect("ensure_tracker filled cache");
        // Tracker oriented image is isotropic (CPU `build`), so `.0 == .1`.
        Ok((oriented.gray.clone(), oriented.det_to_full.0))
    }

    /// Run the engine on the pre-built gray. Takes only the engine lock, so
    /// it can run on a scoped thread while the camera upload proceeds on the
    /// render thread.
    fn run_engine(&self, req: &TrackerRequest) -> TrackerComputeResult {
        let mut engine = self.engine.lock().map_err(|_| poisoned())?;
        let correction = engine.relocalize(
            &req.gray,
            req.coarse_prior,
            &req.coarse_seeds,
            req.frame_idx,
            req.timestamp_ns,
        );
        let step_timings = engine.last_step_timings();
        Ok((correction, step_timings))
    }

    fn select_compose_h(
        &self,
        state: PlanarTrackerState,
        anchor_id: u64,
        incoming: Option<[f32; 9]>,
    ) -> Option<[f32; 9]> {
        let mut sm = self.last_emitted_h.lock().ok()?;
        match state {
            PlanarTrackerState::Locked => {
                sm.consecutive_lost = 0;
                let incoming = incoming?;
                sm.h = Some(incoming);
                sm.anchor_id = anchor_id;
                Some(incoming)
            }
            PlanarTrackerState::Lost => {
                sm.consecutive_lost = sm.consecutive_lost.saturating_add(1);
                if sm.consecutive_lost < LOSS_HIDE_AFTER_FRAMES {
                    sm.h
                } else {
                    None
                }
            }
            PlanarTrackerState::Idle | PlanarTrackerState::Acquiring => {
                sm.consecutive_lost = 0;
                None
            }
        }
    }

    fn update_refresh_trigger(
        &self,
        anchor_id: u64,
        tracker_h: Option<[f32; 9]>,
        frame_state_dims: (u32, u32, i32),
        _full_view_w_hint: u32,
        _full_view_h_hint: u32,
    ) -> bool {
        let Some(h) = tracker_h else { return false };
        if let Ok(mut slot) = self.last_root_to_view.lock() {
            *slot = Some((anchor_id, h));
        }
        if !self.session.has_last_lock_h(anchor_id) {
            self.session.set_last_lock_h(anchor_id, h);
            return false;
        }
        let r = ((frame_state_dims.2 % 360) + 360) % 360;
        let (full_view_w, full_view_h) = if r == 90 || r == 270 {
            (frame_state_dims.1 as f32, frame_state_dims.0 as f32)
        } else {
            (frame_state_dims.0 as f32, frame_state_dims.1 as f32)
        };
        if self.session.should_relock_by_view(
            anchor_id,
            &h,
            full_view_w,
            full_view_h,
            RELOCK_OVERLAP_THRESHOLD,
        ) {
            // Quadrilateral area changed enough to *consider* a
            // refresh, but a pure tilt / perspective change can flip
            // this trigger without actually revealing new content.
            // Gate on coverage: if the new viewport AABB is still
            // inside the surface area we've already detected text in,
            // every visible line is already in the surface map and a
            // detect+OCR pass would be wasted work — at best a no-op,
            // at worst (if tracker drift exceeds the merge tolerance)
            // it produces stacked duplicate overlays.
            let Some(h_view_to_surface) = h_view_to_surface_from(&h) else {
                return false;
            };
            let viewport = match viewport_surface_aabb(&h_view_to_surface, full_view_w, full_view_h)
            {
                Some(v) => v,
                None => return false,
            };
            if self
                .session
                .viewport_contained_in_coverage(anchor_id, &viewport, COVERAGE_PAD_PX)
            {
                return false;
            }
            log::debug!(
                "[refresh] firing: viewport AABB extends beyond covered surface region (anchor={anchor_id})",
            );
            self.session.clear_last_lock_h(anchor_id);
            if let Ok(mut slot) = self.pending_refresh_target.lock() {
                *slot = Some((anchor_id, h));
            }
            true
        } else {
            false
        }
    }

    /// Dispatch the heavy acquire/refresh job for `kind` against `frame` (which
    /// must carry RGBA — either an Android per-frame frame or the RGBA frame the
    /// caller supplied via [`provide_acquire_rgb`]). Returns
    /// `(started_acquire, started_refresh)`; gated by worker backpressure.
    fn dispatch_async(
        &self,
        frame: &Arc<LiveFrame>,
        kind: AsyncKind,
        display_crop: Rect,
        cfg: &PipelineConfig,
        timestamp_ns: u64,
        generation: u64,
    ) -> (bool, bool) {
        let worker_guard = self.worker.lock().ok();
        let Some(worker) = worker_guard.as_ref().and_then(|w| w.as_ref()) else {
            return (false, false);
        };
        match kind {
            AsyncKind::Acquire => {
                let started = worker.try_dispatch(PendingJob::Acquire {
                    frame: Arc::clone(frame),
                    display_crop,
                    config: cfg.clone(),
                    timestamp_ns,
                    generation,
                });
                (started, false)
            }
            AsyncKind::Refresh => {
                let started = worker.try_dispatch(PendingJob::Refresh {
                    frame: Arc::clone(frame),
                    display_crop,
                    config: cfg.clone(),
                    generation,
                });
                (false, started)
            }
        }
    }

    /// Satisfy an [`AcquireRequest`] from a gray-only `process_frame`: dispatch
    /// the heavy det/rec against the caller-supplied full-res RGBA `frame`.
    /// Returns whether a job was started (worker backpressure may drop it).
    pub fn provide_acquire_rgb(&self, req: AcquireRequest, frame: &Arc<LiveFrame>) -> bool {
        let (a, r) = self.dispatch_async(
            frame,
            req.kind,
            req.display_crop,
            &req.config,
            req.timestamp_ns,
            req.generation,
        );
        a || r
    }

    // ---- async stages (run on the worker thread) ----

    /// Top-level acquire orchestrator. Sequences the named stages
    /// below; each stage owns its own lock acquisitions, error paths,
    /// and side effects. Cancellation (`gen_check`) is checked
    /// between stages so a navigate-away mid-acquire drops the work
    /// at the next stage boundary instead of having to thread a
    /// cancel signal into each stage. The Locked → bbox-overlay
    /// → orient → rec/translate ordering encodes the user-visible
    /// contract: the engine flips Locked after stage 2, the
    /// provisional bbox canvas is up after stage 3, and only stage 6
    /// pays the multi-hundred-ms rec/translate cost.
    fn run_acquire_inner(
        &self,
        frame: &Arc<LiveFrame>,
        display_crop: Rect,
        cfg: &PipelineConfig,
        timestamp_ns: u64,
        generation: u64,
    ) -> AcquireTelemetry {
        let gen_check = || self.generation.load(Ordering::SeqCst) == generation;
        let t_overall = Instant::now();
        let elapsed_ms = || t_overall.elapsed().as_secs_f64() * 1000.0;

        if !gen_check() {
            return canceled_telemetry(0, 0.0);
        }

        // 1. Detect text boxes in the visible region.
        let detected = match self.acquire_stage_detect(frame, display_crop, cfg) {
            Ok(d) => d,
            Err(msg) => return error_telemetry(msg),
        };
        if !gen_check() {
            return canceled_telemetry(0, elapsed_ms());
        }
        if detected.is_empty() {
            if let Ok(mut backoff) = self.acquire_backoff.lock() {
                backoff.arm();
            }
            return AcquireTelemetry {
                total_ms: elapsed_ms(),
                ..Default::default()
            };
        }
        if let Ok(mut backoff) = self.acquire_backoff.lock() {
            backoff.clear();
        }

        // 2. Register the anchor — engine flips Locked here. Doing
        //    this before orient-rec is what cuts the user-perceived
        //    "I see text" latency from ~870 ms to ~400 ms.
        let (anchor_id, h_view_to_sensor) =
            match self.acquire_stage_register(frame, cfg, &detected, timestamp_ns) {
                Ok(r) => r,
                Err(msg) => return error_telemetry(msg),
            };
        self.session.reset_anchor_state(anchor_id);

        // 3. Provisional bbox-only overlay — paints translucent
        //    pills under each detection so the user has immediate
        //    feedback that detection landed. Best-effort; canvas
        //    rebuild folds these in atomically.
        self.acquire_stage_provisional(anchor_id, &detected, &h_view_to_sensor);
        if !gen_check() {
            return canceled_telemetry(anchor_id, elapsed_ms());
        }

        // 4. Orient-rec — picks reading direction (R0/R180/…).
        //    Failure here is fatal: rec downstream needs the
        //    quadrant for angle snap + block grouping.
        let estimated_quadrant = match self.acquire_stage_orient(frame, cfg, &detected) {
            Ok(q) => q,
            Err(msg) => return error_telemetry(msg),
        };
        self.acquire_stage_apply_orientation(anchor_id, estimated_quadrant);
        if !gen_check() {
            return canceled_telemetry(anchor_id, elapsed_ms());
        }

        // 5. Color matting (disabled by default). Inline because
        //    it's a const-gated side effect, not a stage.
        if ENABLE_COLOR_MATTING {
            if let Err(msg) = self.acquire_stage_color_matting(frame, anchor_id, &detected) {
                return error_telemetry(msg);
            }
        }

        // 6. Rec + translate. Wraps `run_post_detect`, which fills
        //    the canvas with the final translated blocks.
        let total = detected.len();
        let outcome = match self.acquire_stage_rec_translate(
            frame,
            display_crop,
            cfg,
            &detected,
            anchor_id,
            generation,
        ) {
            Ok(o) => o,
            Err(msg) => return error_telemetry(msg),
        };
        self.session.on_acquire();
        if outcome.canceled {
            return canceled_telemetry(anchor_id, elapsed_ms());
        }

        // 7. Finalize: drop the anchor if rec found nothing,
        //    sync session state to the engine's LRU.
        self.acquire_stage_finalize(anchor_id, &outcome, total);

        let rec_ok = outcome.rec_ok_count as usize;
        let rec_empty = outcome.rec_empty_count as usize;
        AcquireTelemetry {
            anchor_id,
            detected_count: total as u32,
            rec_ok_count: rec_ok as u32,
            rec_empty_count: rec_empty as u32,
            cache_hits: outcome.cache_hits,
            rec_called_count: outcome.rec_called_count,
            total_ms: elapsed_ms(),
            canceled: false,
            error: None,
            is_refresh: false,
        }
    }

    /// Stage 1 of [`Self::run_acquire_inner`]. Ensures the oriented
    /// RGB+gray cache is built for `display_crop`, then runs the
    /// PaddleOCR detector. Returns the detected boxes already
    /// scaled back to full-resolution pixel coords.
    fn acquire_stage_detect(
        &self,
        frame: &Arc<LiveFrame>,
        display_crop: Rect,
        cfg: &PipelineConfig,
    ) -> Result<Vec<DetectedTextBox>, &'static str> {
        acquire_detect(&*self.host, frame, display_crop, cfg.det_max_pixels)
    }

    /// Stage 2: register the anchor and flip the engine to Locked.
    /// Computes the tracker-resolution regions for
    /// `acquire_now_in_regions` and the `view → sensor` translation
    /// the provisional overlay (stage 3) and `run_post_detect`
    /// (stage 6) reuse.
    fn acquire_stage_register(
        &self,
        frame: &Arc<LiveFrame>,
        cfg: &PipelineConfig,
        detected: &[DetectedTextBox],
        timestamp_ns: u64,
    ) -> Result<(u64, [f32; 9]), &'static str> {
        let mut state = frame.state().lock().map_err(|_| "frame.state poisoned")?;
        state
            .ensure_tracker_oriented(cfg.det_max_pixels)
            .map_err(|_| "ensure_tracker failed")?;
        let tracker_oriented = state
            .cached_tracker
            .as_ref()
            .expect("ensure_tracker filled cache");
        let cached_sensor_crop = state
            .cached
            .as_ref()
            .map(|oi| oi.sensor_crop)
            .unwrap_or(Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            });
        let h_view_to_sensor = view_to_sensor_h(&cached_sensor_crop);
        let scale_down = if tracker_oriented.det_to_full.0 > 0.0 {
            1.0 / tracker_oriented.det_to_full.0
        } else {
            1.0
        };
        let regions: Vec<(u32, u32, u32, u32)> = detected
            .iter()
            .map(|d| {
                let scale_u32 = |v: u32| ((v as f32) * scale_down).round() as u32;
                (
                    scale_u32(d.rect.left + cached_sensor_crop.left),
                    scale_u32(d.rect.top + cached_sensor_crop.top),
                    scale_u32(d.rect.right + cached_sensor_crop.left),
                    scale_u32(d.rect.bottom + cached_sensor_crop.top),
                )
            })
            .collect();
        let mut engine = self.engine.lock().map_err(|_| "engine poisoned")?;
        let id = engine
            .acquire_now_in_regions(
                &tracker_oriented.gray,
                &regions,
                cfg.anchor_padding_px,
                timestamp_ns,
            )
            .unwrap_or(0);
        if id == 0 {
            return Err("acquire_now returned 0");
        }
        Ok((id, h_view_to_sensor))
    }

    /// Stage 3: publish the provisional bbox-only canvas. Pure side
    /// effect; failures from inside `upsert_provisional_overlay`
    /// (e.g. session lock poisoned) are swallowed because losing the
    /// provisional layer is a UX downgrade, not a pipeline failure.
    fn acquire_stage_provisional(
        &self,
        anchor_id: u64,
        detected: &[DetectedTextBox],
        h_view_to_sensor: &[f32; 9],
    ) {
        let surface_strips: Vec<OrientedRect> = detected
            .iter()
            .map(|d| {
                project_oriented_rect(&d.tight_box, h_view_to_sensor)
                    .unwrap_or_else(|| d.tight_box.clone())
            })
            .collect();
        if !surface_strips.is_empty() {
            self.session
                .upsert_provisional_overlay(anchor_id, surface_strips);
        }
    }

    /// Stage 4: estimate the canonical reading-direction quadrant by
    /// running the recognizer over a sample of detections at R0 and
    /// R180 and picking the higher-confidence axis. Returns
    /// `Ok(None)` when the estimator can't reach consensus — the
    /// rec path then falls back to `last_known_quadrant`.
    fn acquire_stage_orient(
        &self,
        frame: &Arc<LiveFrame>,
        cfg: &PipelineConfig,
        detected: &[DetectedTextBox],
    ) -> Result<Option<Quadrant>, &'static str> {
        acquire_orient(
            &*self.host,
            frame,
            &cfg.from_lang,
            cfg.is_auto_source,
            detected,
        )
    }

    /// Stage 5: write the resolved quadrant back onto the cached
    /// anchor and drop the provisional canvas. Called after stage 4
    /// even when `quadrant` is `None` so the provisional layer is
    /// cleared before stage 6 paints the real blocks on top.
    fn acquire_stage_apply_orientation(&self, anchor_id: u64, quadrant: Option<Quadrant>) {
        if let Some(q) = quadrant {
            if let Ok(mut engine) = self.engine.lock() {
                engine.set_canonical_rotation(anchor_id, q);
            }
        }
        self.session.drop_provisional_overlay(anchor_id);
    }

    /// Optional stage between orient and rec/translate: compute per-
    /// detection background mats. Off by default (`ENABLE_COLOR_MATTING`).
    fn acquire_stage_color_matting(
        &self,
        frame: &Arc<LiveFrame>,
        anchor_id: u64,
        detected: &[DetectedTextBox],
    ) -> Result<(), &'static str> {
        // Same resolved quadrant the recognizer used, so the ink strip's contour dewarp
        // matches the rec strip and the bold channel lines up with the CTC firings.
        let canonical_quadrant = self
            .engine
            .lock()
            .ok()
            .and_then(|e| e.canonical_rotation_for(anchor_id));
        let matted: Vec<Option<MattedStrip>> = {
            let state = frame.state().lock().map_err(|_| "frame.state poisoned")?;
            let oriented = state.cached.as_ref().expect("oriented still cached");
            let rec_scale = oriented.rec_scale;
            // Crop colours from the (possibly rec-res) rgb with rec-scaled boxes,
            // then lift the strips' canonical geometry back to canonical coords.
            let scaled = oriented.rec_scaled_boxes(detected);
            let rgb = oriented.rgb.as_ref().expect("with_rgb path");
            let ink_strips = self
                .host
                .ppocr_engine()
                .map(|ppocr| {
                    ppocr.ink_strips(
                        rgb,
                        &scaled,
                        canonical_quadrant,
                        translator_ocr::ppocr::InkChannelOrder::Rgb,
                    )
                })
                .unwrap_or_default();
            // Bold column profile per box from the model's ch1, parallel to `detected`;
            // pooled per word against CTC firings at rec time. `None` per box when the model
            // has no bold channel.
            let bold_profiles: Vec<Option<translator_raster::text_metrics::BoldProfile>> =
                ink_strips
                    .iter()
                    .map(|s| s.as_ref().and_then(|s| s.bold_profile()))
                    .collect();
            let ink_masks: Vec<Option<image::GrayImage>> = ink_strips
                .iter()
                .map(|s| s.as_ref().map(|s| s.matte.clone()))
                .collect();
            let ink_src_maps: Vec<Option<Vec<(f32, f32)>>> = ink_strips
                .iter()
                .map(|s| s.as_ref().and_then(|s| s.src_map.clone()))
                .collect();
            // Text-metrics off the same mattes. Measured against the *canonical*
            // box dims (not the rec-scaled `scaled`), so the recovered x-height /
            // width / offsets come out in canonical coords like `detected`.
            let metrics: Vec<Option<translator_raster::text_metrics::LineMetrics>> = detected
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    let mask = ink_masks.get(i)?.as_ref()?;
                    translator_raster::text_metrics::measure_line(
                        mask,
                        d.oriented_box.width,
                        d.oriented_box.height,
                    )
                })
                .collect();
            if let Ok(mut store) = self.line_metrics.lock() {
                store.insert(anchor_id, metrics);
            }
            if let Ok(mut store) = self.bold_profiles.lock() {
                store.insert(anchor_id, bold_profiles);
            }
            let model_colors: Vec<Option<translator_core::ocr::InkColor>> = ink_strips
                .iter()
                .map(|s| s.as_ref().and_then(|s| s.pooled_color()))
                .collect();
            let strips = translator_raster::color_matting::mat_detections(
                &rgb.to_rgba8(),
                &scaled,
                &ink_masks,
                &ink_src_maps,
                &model_colors,
            );
            // `mat_detections` returns only the boxes that matted (keyed by
            // `box_index`); the block grouping downstream indexes strips
            // positionally against `detected`, so re-expand to that shape.
            let mut matted: Vec<Option<MattedStrip>> = vec![None; detected.len()];
            for mut s in strips {
                if rec_scale != 1.0 {
                    let inv = 1.0 / rec_scale;
                    s.canonical_cx *= inv;
                    s.canonical_cy *= inv;
                    s.canonical_width *= inv;
                    s.canonical_height *= inv;
                }
                let idx = s.box_index;
                matted[idx] = Some(s);
            }
            matted
        };
        if let Ok(mut store) = self.matted_strips.lock() {
            store.insert(anchor_id, matted);
        }
        Ok(())
    }

    /// Stage 6: rec + translate. Looks up the canonical quadrant
    /// (now set by stage 5 if orient-rec succeeded), reads the
    /// matting cache (empty if stage 5 skipped), and hands
    /// everything to `run_post_detect` which observes the boxes
    /// into the surface map, groups them, runs rec on the boxes that
    /// need it, translates, and upserts the real blocks.
    fn acquire_stage_rec_translate(
        &self,
        frame: &Arc<LiveFrame>,
        display_crop: Rect,
        cfg: &PipelineConfig,
        detected: &[DetectedTextBox],
        anchor_id: u64,
        generation: u64,
    ) -> Result<PostDetectOutcome, &'static str> {
        let matted_strips: Vec<Option<MattedStrip>> = match self.matted_strips.lock() {
            Ok(g) => g.get(&anchor_id).cloned().unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let line_metrics: Vec<Option<translator_raster::text_metrics::LineMetrics>> =
            match self.line_metrics.lock() {
                Ok(g) => g.get(&anchor_id).cloned().unwrap_or_default(),
                Err(_) => Vec::new(),
            };
        let bold_profiles: Vec<Option<translator_raster::text_metrics::BoldProfile>> =
            match self.bold_profiles.lock() {
                Ok(g) => g.get(&anchor_id).cloned().unwrap_or_default(),
                Err(_) => Vec::new(),
            };
        let canonical_quadrant = self
            .engine
            .lock()
            .ok()
            .and_then(|e| e.canonical_rotation_for(anchor_id));
        let cancel = || self.generation.load(Ordering::SeqCst) != generation;
        acquire_rec_translate(
            &*self.host,
            &self.session,
            &*self.font_provider,
            frame,
            display_crop,
            &cfg.from_lang,
            &cfg.to_lang,
            cfg.is_auto_source,
            cfg.rec_batch_size,
            detected,
            anchor_id,
            canonical_quadrant,
            &matted_strips,
            &line_metrics,
            &bold_profiles,
            &cancel,
        )
    }

    /// Stage 7: post-rec cleanup. Drops the anchor entirely if rec
    /// produced nothing usable (every detection came back empty),
    /// then syncs session per-anchor state to the engine's LRU set
    /// so evicted anchors don't keep stale overlays around.
    fn acquire_stage_finalize(&self, _anchor_id: u64, outcome: &PostDetectOutcome, total: usize) {
        let rec_ok = outcome.rec_ok_count as usize;
        let rec_empty = outcome.rec_empty_count as usize;
        if rec_ok == 0 && rec_empty + rec_ok == total {
            if let Ok(mut engine) = self.engine.lock() {
                engine.clear();
            }
            self.session.clear_overlays();
        }
        if let Ok(engine) = self.engine.lock() {
            let keep = engine.cached_root_ids();
            drop(engine);
            self.session.retain_anchors(&keep);
        }
    }

    fn run_refresh_inner(
        &self,
        frame: &Arc<LiveFrame>,
        display_crop: Rect,
        cfg: &PipelineConfig,
        generation: u64,
    ) -> AcquireTelemetry {
        let gen_check = || self.generation.load(Ordering::SeqCst) == generation;
        if !gen_check() {
            return AcquireTelemetry {
                canceled: true,
                is_refresh: true,
                ..Default::default()
            };
        }
        let t_overall = Instant::now();
        let (anchor_id, h_root_to_view) = match self.pending_refresh_target.lock() {
            Ok(mut g) => match g.take() {
                Some((id, h)) => (id, h),
                None => {
                    return AcquireTelemetry {
                        error: Some("refresh without armed trigger".into()),
                        is_refresh: true,
                        ..Default::default()
                    };
                }
            },
            Err(_) => {
                return AcquireTelemetry {
                    error: Some("pending_refresh_target poisoned".into()),
                    is_refresh: true,
                    ..Default::default()
                };
            }
        };
        let h_sensor_view_to_surface = match homography::invert(&h_root_to_view) {
            Some(h) => h,
            None => {
                return AcquireTelemetry {
                    error: Some("H_root→view not invertible".into()),
                    is_refresh: true,
                    ..Default::default()
                };
            }
        };

        let detected: Vec<DetectedTextBox> = {
            let mut state = match frame.state().lock() {
                Ok(s) => s,
                Err(_) => {
                    return AcquireTelemetry {
                        error: Some("frame.state poisoned".into()),
                        is_refresh: true,
                        ..Default::default()
                    };
                }
            };
            if state
                .ensure_oriented_with_rgb(display_crop, cfg.det_max_pixels)
                .is_err()
            {
                return AcquireTelemetry {
                    error: Some("ensure_oriented failed".into()),
                    is_refresh: true,
                    ..Default::default()
                };
            }
            let oriented = state.cached.as_ref().expect("ensure_oriented filled cache");
            let raw = match self.host.ppocr_engine().and_then(|ppocr| {
                ppocr
                    .detect_only_image(
                        oriented
                            .rgb_det
                            .as_ref()
                            .expect("detect path requires build_with_rgb"),
                        translator_ocr::ppocr::PpocrProfile::Live,
                    )
                    .map_err(|e| format!("{e:?}"))
            }) {
                Ok(r) => r,
                Err(e) => {
                    log::warn!("[refresh] detect failed: {e}");
                    return AcquireTelemetry {
                        error: Some("detect failed".into()),
                        is_refresh: true,
                        ..Default::default()
                    };
                }
            };
            let (sx, sy) = oriented.det_to_full;
            let rgb = oriented.rgb.as_ref().expect("with_rgb path");
            let max_w = (rgb.width() as f32 / oriented.rec_scale) as u32;
            let max_h = (rgb.height() as f32 / oriented.rec_scale) as u32;
            raw.into_iter()
                .map(|b| scale_detected_box(b, sx, sy, max_w, max_h))
                .collect()
        };
        if !gen_check() {
            return AcquireTelemetry {
                canceled: true,
                is_refresh: true,
                ..Default::default()
            };
        }
        if detected.is_empty() {
            self.clear_engine_and_overlays();
            return AcquireTelemetry {
                anchor_id,
                total_ms: t_overall.elapsed().as_secs_f64() * 1000.0,
                is_refresh: true,
                ..Default::default()
            };
        }

        let available_codes: Vec<LanguageCode> = self.host.available_language_codes();

        let state = match frame.state().lock() {
            Ok(s) => s,
            Err(_) => {
                return AcquireTelemetry {
                    error: Some("frame.state poisoned".into()),
                    is_refresh: true,
                    ..Default::default()
                };
            }
        };
        let oriented = match state
            .cached
            .as_ref()
            .filter(|oi| oi.display_crop == display_crop)
        {
            Some(o) => o,
            None => {
                return AcquireTelemetry {
                    error: Some("oriented cache miss".into()),
                    is_refresh: true,
                    ..Default::default()
                };
            }
        };
        let cancel = || self.generation.load(Ordering::SeqCst) != generation;
        let h_view_to_sensor = view_to_sensor_h(&oriented.sensor_crop);
        let h_view_to_surface_composed =
            homography::mat3_mul(&h_sensor_view_to_surface, &h_view_to_sensor);
        let host: &dyn LiveOcrHost = &*self.host;
        self.session.clear_anchor_state_for_relock(anchor_id);
        let canonical_quadrant = self
            .engine
            .lock()
            .ok()
            .and_then(|e| e.canonical_rotation_for(anchor_id));
        let outcome = self.session.run_post_detect(
            PostDetectInput {
                detections: &detected,
                oriented,
                h_view_to_surface: Some(h_view_to_surface_composed),
                anchor_id,
                from_lang: &cfg.from_lang,
                to_lang: &cfg.to_lang,
                is_auto_source: cfg.is_auto_source,
                available_codes: &available_codes,
                font_provider: &*self.font_provider,
                matted_strips: &[],
                line_metrics: &[],
                bold_profiles: &[],
                rec_batch_size: cfg.rec_batch_size,
                canonical_quadrant,
            },
            host,
            host,
            &cancel,
        );
        drop(state);

        if outcome.canceled {
            return AcquireTelemetry {
                canceled: true,
                anchor_id,
                is_refresh: true,
                ..Default::default()
            };
        }
        // Nothing kept (all rec results empty, no cache hits). Demote the
        // engine to Idle so the next process_frame falls through to acquire
        // instead of holding the Locked overlay against a scene that no
        // longer has text.
        if outcome.rec_ok_count == 0 && outcome.cache_hits == 0 {
            self.clear_engine_and_overlays();
        }
        AcquireTelemetry {
            anchor_id,
            detected_count: outcome.detected_count,
            rec_ok_count: outcome.rec_ok_count,
            rec_empty_count: outcome.rec_empty_count,
            cache_hits: outcome.cache_hits,
            rec_called_count: outcome.rec_called_count,
            total_ms: t_overall.elapsed().as_secs_f64() * 1000.0,
            canceled: false,
            error: None,
            is_refresh: true,
        }
    }

    fn clear_engine_and_overlays(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            engine.clear();
        }
        self.session.clear_overlays();
    }
}

fn poisoned() -> TranslatorError {
    TranslatorError::new(
        translator_core::api::TranslatorErrorKind::Internal,
        "live pipeline mutex poisoned",
    )
}

/// Conjugate a homography `H` by an isotropic scale `s` so an H built
/// in downsampled coords maps the equivalent points in full coords.
fn scale_homography(h: &[f32; 9], s: f32) -> [f32; 9] {
    let inv_s = 1.0 / s;
    [
        h[0],
        h[1],
        h[2] * s,
        h[3],
        h[4],
        h[5] * s,
        h[6] * inv_s,
        h[7] * inv_s,
        h[8],
    ]
}

/// Build the early-return telemetry the orchestrator emits when a
/// `gen_check()` between stages comes back false. `anchor_id` is the
/// in-flight id (zero before stage 2 lands), `total_ms` is the
/// elapsed time so far. Mirrors the pre-split inline blocks so
/// callers see the same fields they used to.
fn canceled_telemetry(anchor_id: u64, total_ms: f64) -> AcquireTelemetry {
    AcquireTelemetry {
        anchor_id,
        canceled: true,
        total_ms,
        ..Default::default()
    }
}

/// Build the early-return telemetry the orchestrator emits when a
/// stage returns `Err(&'static str)`. Mirrors the pre-split inline
/// blocks (no anchor_id, no total_ms) for backwards-compatible
/// telemetry shape.
fn error_telemetry(msg: &'static str) -> AcquireTelemetry {
    AcquireTelemetry {
        error: Some(msg.into()),
        ..Default::default()
    }
}

/// Scale a `DetectedTextBox` from detector-image coords up to
/// full-crop coords, clamping inside the destination dimensions.
// ---------------------------------------------------------------------------
// Shared acquire-core ops. Called by both `LiveTrackerPipeline` (tracked camera
// path) and `LiveScreenPipeline` (static screen-capture path) so the
// detect → orient → rec/translate → composite work has a single definition.
// The engine/anchor-lifecycle bits (register, write-quadrant-to-engine,
// finalize/LRU) stay in `LiveTrackerPipeline`; these take only what they need.
// ---------------------------------------------------------------------------

/// `H_view→sensor`: the pure translation by the oriented frame's `sensor_crop`
/// origin (the view is the cropped region inside the sensor frame). Shared by anchor
/// registration, refresh, and `run_post_detect`.
fn view_to_sensor_h(crop: &Rect) -> [f32; 9] {
    [
        1.0,
        0.0,
        crop.left as f32,
        0.0,
        1.0,
        crop.top as f32,
        0.0,
        0.0,
        1.0,
    ]
}

/// Ensure the oriented RGB+gray cache for `display_crop`, run the detector, and
/// return boxes in full-resolution pixel coords. (Body of
/// `LiveTrackerPipeline::acquire_stage_detect`.)
pub(crate) fn acquire_detect(
    host: &dyn LiveOcrHost,
    frame: &Arc<LiveFrame>,
    display_crop: Rect,
    det_max_pixels: u32,
) -> Result<Vec<DetectedTextBox>, &'static str> {
    let mut state = frame.state().lock().map_err(|_| "frame.state poisoned")?;
    state
        .ensure_oriented_with_rgb(display_crop, det_max_pixels)
        .map_err(|_| "ensure_oriented failed")?;
    let oriented = state.cached.as_ref().expect("ensure_oriented filled cache");
    let raw = host
        .ppocr_engine()
        .and_then(|ppocr| {
            ppocr
                .detect_only_image(
                    oriented
                        .rgb_det
                        .as_ref()
                        .expect("detect path requires build_with_rgb"),
                    translator_ocr::ppocr::PpocrProfile::Live,
                )
                .map_err(|e| format!("{e:?}"))
        })
        .map_err(|e| {
            log::warn!("detect failed: {e}");
            "detect failed"
        })?;
    let (sx, sy) = oriented.det_to_full;
    let rgb = oriented.rgb.as_ref().expect("with_rgb path");
    // Boxes land in canonical coords; `rgb` may be rec-res (rec_scale<1), so the
    // canonical clamp is rgb dims / rec_scale (== rgb dims on the CPU paths).
    let max_w = (rgb.width() as f32 / oriented.rec_scale) as u32;
    let max_h = (rgb.height() as f32 / oriented.rec_scale) as u32;
    Ok(raw
        .into_iter()
        .map(|b| scale_detected_box(b, sx, sy, max_w, max_h))
        .collect())
}

/// Estimate the canonical reading-direction quadrant. (Body of
/// `LiveTrackerPipeline::acquire_stage_orient`.) `None` = no consensus.
pub(crate) fn acquire_orient(
    host: &dyn LiveOcrHost,
    frame: &Arc<LiveFrame>,
    from_lang: &str,
    is_auto_source: bool,
    detected: &[DetectedTextBox],
) -> Result<Option<Quadrant>, &'static str> {
    let forced_script = host.orient_script(from_lang, is_auto_source);
    let Ok(ppocr) = host.ppocr_engine() else {
        return Ok(None);
    };
    let state = frame.state().lock().map_err(|_| "frame.state poisoned")?;
    let oriented = state.cached.as_ref().ok_or("oriented cache miss")?;
    Ok(if let Some(script) = forced_script {
        translator_ocr::orientation::estimate_canonical_via_rec(&ppocr, oriented, detected, script)
    } else {
        translator_ocr::orientation::estimate_canonical_quadrant_with(&ppocr, oriented, detected)
    })
}

/// Recognize + translate the detections and upsert the resident overlay blocks
/// for `anchor_id` via [`LiveSession::run_post_detect`]. (Body of
/// `LiveTrackerPipeline::acquire_stage_rec_translate`, minus the engine
/// quadrant lookup — pass `canonical_quadrant` directly.)
#[allow(clippy::too_many_arguments)]
pub(crate) fn acquire_rec_translate(
    host: &dyn LiveOcrHost,
    session: &LiveSession,
    font_provider: &dyn FontProvider,
    frame: &Arc<LiveFrame>,
    display_crop: Rect,
    from_lang: &str,
    to_lang: &str,
    is_auto_source: bool,
    rec_batch_size: usize,
    detected: &[DetectedTextBox],
    anchor_id: u64,
    canonical_quadrant: Option<Quadrant>,
    matted_strips: &[Option<MattedStrip>],
    line_metrics: &[Option<translator_raster::text_metrics::LineMetrics>],
    bold_profiles: &[Option<translator_raster::text_metrics::BoldProfile>],
    cancel: &dyn Fn() -> bool,
) -> Result<PostDetectOutcome, &'static str> {
    let available_codes: Vec<LanguageCode> = host.available_language_codes();
    let state = frame.state().lock().map_err(|_| "frame.state poisoned")?;
    let oriented = state
        .cached
        .as_ref()
        .filter(|oi| oi.display_crop == display_crop)
        .ok_or("oriented cache miss")?;
    let h_view_to_sensor = view_to_sensor_h(&oriented.sensor_crop);
    let outcome = session.run_post_detect(
        PostDetectInput {
            detections: detected,
            oriented,
            h_view_to_surface: Some(h_view_to_sensor),
            anchor_id,
            from_lang,
            to_lang,
            is_auto_source,
            available_codes: &available_codes,
            font_provider,
            matted_strips,
            line_metrics,
            bold_profiles,
            rec_batch_size,
            canonical_quadrant,
        },
        host,
        host,
        cancel,
    );
    drop(state);
    Ok(outcome)
}

/// Scale a detected box by a per-axis `(sx, sy)`, clamped to `max_w×max_h`.
/// Per-axis (not a single scalar) because the GPU detector input is rendered at
/// a 32-aligned size whose x/y scales to canonical differ slightly; the CPU
/// paths pass `sx == sy`. Even contour points scale by x, odd by y. Angle is
/// left unchanged — the anisotropy here is ≈1.04 vs 1.01, sub-degree.
fn scale_detected_box(
    b: DetectedTextBox,
    sx: f32,
    sy: f32,
    max_w: u32,
    max_h: u32,
) -> DetectedTextBox {
    let left = ((b.rect.left as f32) * sx).max(0.0) as u32;
    let top = ((b.rect.top as f32) * sy).max(0.0) as u32;
    let right = ((b.rect.right as f32) * sx).min(max_w as f32) as u32;
    let bottom = ((b.rect.bottom as f32) * sy).min(max_h as f32) as u32;
    let rect = Rect {
        left: left.min(right.saturating_sub(1)),
        top: top.min(bottom.saturating_sub(1)),
        right: right.max(left + 1),
        bottom: bottom.max(top + 1),
    };
    let oriented = OrientedRect {
        cx: b.oriented_box.cx * sx,
        cy: b.oriented_box.cy * sy,
        width: b.oriented_box.width * sx,
        height: b.oriented_box.height * sy,
        angle_radians: b.oriented_box.angle_radians,
    };
    let tight = OrientedRect {
        cx: b.tight_box.cx * sx,
        cy: b.tight_box.cy * sy,
        width: b.tight_box.width * sx,
        height: b.tight_box.height * sy,
        angle_radians: b.tight_box.angle_radians,
    };
    let mut contour = Vec::with_capacity(b.contour.len());
    for (i, v) in b.contour.iter().enumerate() {
        contour.push(if i % 2 == 0 { v * sx } else { v * sy });
    }
    DetectedTextBox {
        rect,
        oriented_box: oriented,
        tight_box: tight,
        contour,
        score: b.score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use translator_core::ocr::Rect;
    use translator_raster::live_frame::LiveFrame;
    use translator_render::font_provider::{FontHandle, FontProvider, FontRequest};

    struct NoFonts;
    impl FontProvider for NoFonts {
        fn locate(&self, _request: &FontRequest) -> Vec<FontHandle> {
            Vec::new()
        }
    }

    /// Host that never resolves an engine. The per-frame tracker path never
    /// touches the host (only async acquire/refresh do), and the test runs in
    /// `Suppressed` mode which dispatches neither, so the mock is enough to
    /// stand the pipeline up.
    struct MockHost;
    impl crate::live_session::LiveRecognizer for MockHost {
        fn recognize(
            &self,
            _oriented: &translator_raster::live_frame::OrientedImage,
            _boxes: &[DetectedTextBox],
            _source_selection: &translator_core::ocr::OcrSourceSelection,
            _canonical_quadrant: Option<Quadrant>,
        ) -> Result<Vec<translator_core::ocr::RecognizedTextLine>, String> {
            Ok(Vec::new())
        }
    }
    impl crate::live_session::LiveTranslator for MockHost {
        fn translate_mixed_texts_with_alignment(
            &self,
            _inputs: &[String],
            _forced_source_code: Option<&str>,
            _target_code: &str,
            _available_language_codes: &[LanguageCode],
        ) -> Result<Vec<translator_translate::translate::TranslationWithAlignment>, String>
        {
            Ok(Vec::new())
        }
    }
    impl crate::live_session::LiveOcrHost for MockHost {
        fn ppocr_engine(&self) -> Result<Arc<translator_ocr::ppocr::PpocrEngine>, String> {
            Err("mock host has no engine".into())
        }
        fn orient_script(
            &self,
            _from_lang: &str,
            _is_auto_source: bool,
        ) -> Option<translator_core::catalog::PpocrScript> {
            None
        }
        fn available_language_codes(&self) -> Vec<LanguageCode> {
            Vec::new()
        }
    }

    /// Drives a few frames through the real pipeline and then drops it, all
    /// on a worker thread, while the test thread enforces a deadline. This
    /// guards the concurrency contract introduced by the persistent compute
    /// thread:
    ///   * the submit→engine→`wait` rendezvous must round-trip every frame
    ///     (a broken handoff hangs `wait`);
    ///   * dropping the pipeline must let the compute thread exit (a botched
    ///     `Weak`/channel lifecycle hangs the drop).
    /// Either failure manifests as the deadline elapsing — a deterministic
    /// `panic`, not a silently hung suite. `Suppressed` mode keeps the
    /// engine cleared each frame, so the verdict is a deterministic `Lost`
    /// with no composed homography and no async dispatch.
    #[test]
    fn process_frame_rendezvous_and_clean_shutdown() {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let pipeline = LiveTrackerPipeline::new(Arc::new(MockHost), Arc::new(NoFonts));
            pipeline.set_target_mode(TargetMode::Suppressed);

            let (w, h) = (128u32, 96u32);
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            // A mild gradient so the CV path has non-degenerate input.
            for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
                let v = (i % 251) as u8;
                px.copy_from_slice(&[v, v, v, 255]);
            }
            let frame = Arc::new(LiveFrame::new((w * h * 4) as usize));
            let crop = Rect {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            };

            for i in 0..3u64 {
                frame.reset_owned(rgba.clone(), w, h, 0);
                // No GL context here: process_frame is GL-free (the present happens
                // in the shell), so the test drives the tracker step directly.
                let r = pipeline
                    .process_frame(&frame, crop, w, h, (i + 1) * 1_000_000)
                    .expect("process_frame ok");
                assert_eq!(r.state, PlanarTrackerState::Lost);
                assert!(r.compose_h.is_none());
                assert!(!r.started_acquire && !r.started_refresh);
            }
            // Implicitly shuts down the compute thread via channel close.
            drop(pipeline);
            done_tx.send(()).expect("done channel send");
        });

        match done_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(()) => worker.join().expect("worker thread panicked"),
            Err(_) => panic!("pipeline rendezvous/shutdown did not finish in 15s (deadlock?)"),
        }
    }
}
