use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use image::{DynamicImage, GenericImageView, GrayImage, Rgb, RgbImage, imageops::FilterType};
use imageproc::contours::find_contours;
use imageproc::point::Point;
use rayon::ThreadPool;
use rayon::prelude::*;

use crate::mnn_inference::MnnSession;
use translator_core::api::{TranslatorError, TranslatorErrorKind};
use translator_core::catalog::PpocrScript;

const REC_TARGET_HEIGHT: u32 = 48;
const REC_WIDTH_BUCKET: usize = 32;
const REC_MIN_SCORE: f32 = 0.3;
/// Per-character CTC score gate for punctuation glyphs. Kept lower than `REC_MIN_SCORE`
/// because real punctuation is small and ambiguous (period vs comma vs apostrophe), so its
/// max-prob legitimately runs lower than letter glyphs. 0.3 still rejects the single rogue
/// glyphs that texture/JPEG-ringing artifacts produce on non-text regions.
const REC_PUNCT_MIN_SCORE: f32 = 0.3;
/// Line-level confidence gate — analogous to PaddleOCR upstream's `drop_score = 0.5`. Lines
/// whose mean accepted-character CTC score is below this threshold are discarded after
/// recognition. The dominant filter for spurious detections from non-text image regions (the
/// per-pixel `DET_SCORE_THRESHOLD` and per-character `REC_MIN_SCORE` gates accept
/// individually-weak signals; this rejects whole lines that never look strong on average).
const REC_DROP_SCORE: f32 = 0.5;
const REC_WHITESPACE_SPLIT_MAX_WIDTH: u32 = 960;
/// One rec session per parallel worker, dispatched via rayon. Mirrors the demo's strategy:
/// each session runs single-threaded (intra=1) on its own crop. Inter-session parallelism beats
/// intra-session padding-and-batching on this workload because crop widths are heterogeneous
/// and the per-graph compute is small enough that ORT/MNN intra-threading scales poorly.
const REC_PARALLELISM: usize = 4;
// Hard OOM ceiling, not a perf cap. The user's `maxImageSize` setting (and
// the doc-align warp step) already determine the working resolution; we only
// step in if something pathological reaches us.
const DET_MAX_SIDE: u32 = 4096;
const DET_SCORE_THRESHOLD: f32 = 0.3;
/// Mean DB-heatmap probability inside a contour's AABB before that contour is allowed to
/// become a detection. Analogous to upstream PaddleOCR's `det_db_box_thresh` (default 0.6) in
/// "fast" `score_mode`. Real text masks score 0.7+ on average inside their box; spurious
/// blobs from texture or compression noise typically sit at 0.35–0.45 — just enough to clear
/// the per-pixel `DET_SCORE_THRESHOLD` but well below this gate.
const DET_BOX_MIN_SCORE: f32 = 0.6;
/// Minimum contour AABB area (in detector-output mask pixels) for a detection to survive.
/// Tighter than upstream's effective minimum so noise blobs from texture / JPEG ringing /
/// foliage that just clear `DET_SCORE_THRESHOLD` get dropped before they ever hit the
/// recognizer. Real text glyphs at typical OCR scale fill many hundreds of mask pixels, so
/// the floor is comfortably below legitimate text.
const DET_MIN_AREA: u32 = 64;
/// Unlike upstream's 1.5–2.0, this is calibrated against measured ink extents (letter/book
/// samples): the kernel band is ~0.55× the real ascender-to-descender ink height, so
/// `tight·(1 + ratio) + 2·DET_BOX_BORDER` at 0.8 puts the inflated box at ~1.1–1.25× the ink —
/// enough margin to erase the antialiasing fringe without swallowing neighboring lines the way
/// upstream's ratio did (~1.75× the ink).
const DET_UNCLIP_RATIO: f32 = 1.2;
const DET_BOX_BORDER: u32 = 4;
const LIVE_REC_DROP_SCORE: f32 = 0.65;
const LIVE_DET_BOX_MIN_SCORE: f32 = 0.68;
const LIVE_DET_MIN_AREA: u32 = 350;

const PPOCR_DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const PPOCR_DET_STD: [f32; 3] = [0.229, 0.224, 0.225];
const PPOCR_REC_MEAN: [f32; 3] = [0.5, 0.5, 0.5];
const PPOCR_REC_STD: [f32; 3] = [0.5, 0.5, 0.5];
/// Per-channel u8 -> normalized-f32 lookup tables for rec preprocessing.
/// The input is u8, so `(v/255 - mean)/std` has only 256 outputs per channel;
/// computing them at compile time turns the per-pixel float math into a read.
const fn rec_norm_lut(mean: f32, std: f32) -> [f32; 256] {
    let mut t = [0.0f32; 256];
    let mut v = 0usize;
    while v < 256 {
        t[v] = (v as f32 / 255.0 - mean) / std;
        v += 1;
    }
    t
}
const REC_NORM_LUT: [[f32; 256]; 3] = [
    rec_norm_lut(PPOCR_REC_MEAN[0], PPOCR_REC_STD[0]),
    rec_norm_lut(PPOCR_REC_MEAN[1], PPOCR_REC_STD[1]),
    rec_norm_lut(PPOCR_REC_MEAN[2], PPOCR_REC_STD[2]),
];
const PULC_WIDTH: u32 = 160;
const PULC_HEIGHT: u32 = 80;
const PULC_MIN_SCORE: f32 = 0.85;
/// PaddleOCR textline orientation classifier input shape and preprocessing
/// (`textline_ori_x0_25_wq8.mnn`, exported from `PP-LCNet_x0_25_textline_ori_infer`).
/// PaddleX's inference yaml specifies ImageNet normalization (NOT
/// [0.5, 0.5, 0.5]); with the wrong normalization the model collapses to
/// near-uniform "upright" predictions regardless of input.
/// Outputs 2 logits: class 0 = upright (0°), class 1 = flipped (180°).
const TEXTLINE_ORI_WIDTH: u32 = 160;
const TEXTLINE_ORI_HEIGHT: u32 = 80;
const TEXTLINE_ORI_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const TEXTLINE_ORI_STD: [f32; 3] = [0.229, 0.224, 0.225];
const TEXTLINE_ORI_CLASSES: usize = 2;
const PULC_MIN_STRIP_AREA: u32 = 768;
const PULC_MIN_STRIP_WIDTH: u32 = 24;
const PULC_MIN_STRIP_HEIGHT: u32 = 8;
const PULC_MIN_IMAGE_AREA_RATIO: f32 = 0.00030;
/// PULC class index order — matches the model's softmax output layout. PULC's `chinese_cht`
/// class is trained on both simplified and traditional Chinese, so we name it `Chinese`.
const PULC_CLASSES: [PpocrScriptClass; 10] = [
    PpocrScriptClass::Arabic,
    PpocrScriptClass::Chinese,
    PpocrScriptClass::Cyrillic,
    PpocrScriptClass::Devanagari,
    PpocrScriptClass::Japanese,
    PpocrScriptClass::Kannada,
    PpocrScriptClass::Korean,
    PpocrScriptClass::Tamil,
    PpocrScriptClass::Telugu,
    PpocrScriptClass::Latin,
];

#[derive(Debug, Clone, Copy)]
struct PpocrThresholds {
    det_score_threshold: f32,
    det_box_min_score: f32,
    det_min_area: u32,
    rec_drop_score: f32,
}

const STILL_THRESHOLDS: PpocrThresholds = PpocrThresholds {
    det_score_threshold: DET_SCORE_THRESHOLD,
    det_box_min_score: DET_BOX_MIN_SCORE,
    det_min_area: DET_MIN_AREA,
    rec_drop_score: REC_DROP_SCORE,
};

const LIVE_THRESHOLDS: PpocrThresholds = PpocrThresholds {
    det_score_threshold: DET_SCORE_THRESHOLD,
    det_box_min_score: LIVE_DET_BOX_MIN_SCORE,
    det_min_area: LIVE_DET_MIN_AREA,
    rec_drop_score: LIVE_REC_DROP_SCORE,
};

/// Detection / recognition profile. `Live` uses stricter thresholds so transient texture
/// noise from a moving camera does not become a stable tracked overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpocrProfile {
    Still,
    Live,
}

impl PpocrProfile {
    fn thresholds(self) -> PpocrThresholds {
        match self {
            PpocrProfile::Still => STILL_THRESHOLDS,
            PpocrProfile::Live => LIVE_THRESHOLDS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PpocrScriptClass {
    Arabic,
    Chinese,
    Cyrillic,
    Devanagari,
    Japanese,
    Kannada,
    Korean,
    Tamil,
    Telugu,
    Latin,
}

impl PpocrScriptClass {
    pub fn name(&self) -> &'static str {
        match self {
            PpocrScriptClass::Arabic => "arabic",
            PpocrScriptClass::Chinese => "chinese",
            PpocrScriptClass::Cyrillic => "cyrillic",
            PpocrScriptClass::Devanagari => "devanagari",
            PpocrScriptClass::Japanese => "japanese",
            PpocrScriptClass::Kannada => "kannada",
            PpocrScriptClass::Korean => "korean",
            PpocrScriptClass::Tamil => "tamil",
            PpocrScriptClass::Telugu => "telugu",
            PpocrScriptClass::Latin => "latin",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PpocrScriptPrediction {
    pub class: PpocrScriptClass,
    pub score: f32,
}

#[derive(Debug, Clone, Copy)]
struct PpocrScriptCandidate {
    class: PpocrScriptClass,
    score: f32,
}

const PUNCTUATIONS: &[char] = &[
    ',', '.', '!', '?', ';', ':', '"', '\'', '(', ')', '[', ']', '{', '}', '-', '_', '/', '\\',
    '|', '@', '#', '$', '%', '&', '*', '+', '=', '~', '，', '。', '！', '？', '；', '：', '、',
    '「', '」', '『', '』', '（', '）', '【', '】', '《', '》', '—', '…', '·', '～',
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpocrRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

impl PpocrRect {
    pub fn width(&self) -> u32 {
        self.right.saturating_sub(self.left)
    }
    pub fn height(&self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }
}

pub struct PpocrDetector {
    session: MnnSession,
    // Model-input px per probability-map px. 1.0 for heads that emit at
    // full input resolution; 2/4 for the folded low-res heads.
    output_stride: f32,
}

/// Per-strip ink matte (soft alpha coverage). Optional — loaded only when the model is
/// present in the bucket. Input is a dewarped RGB strip; output is a 0..255 mask at the
/// model's 48px height (width padded to a multiple of 8 for the 3-level pooling).
pub struct PpocrInkModel {
    // One session per rayon worker so buckets run in parallel without contention (each
    // worker indexes its own session). High-memory load dequantizes int8 weights up front
    // so the conv-only matte uses the fast Winograd/Strassen paths, not per-tile dequant.
    sessions: Vec<Mutex<MnnSession>>,
}

impl PpocrInkModel {
    const HEIGHT: u32 = 48;
    /// Input H/W must divide by 2**levels for the U-Net pooling. The shipped model is
    /// 4-level, so strips are padded to a multiple of 16 (48px height already divides).
    const POOL_MULTIPLE: u32 = 16;

    fn load(model_path: &Path) -> Result<Self, TranslatorError> {
        // `load_conv` (MemoryMode::High) dequantizes the weight-quant model at load and
        // runs the all-3x3 UNet through MNN's Winograd path — measured ~2x faster on
        // device than full-int8 + sdot GEMM (Winograd wins for these small 3x3 convs).
        // One session per rayon worker, 1 intra-thread each (intra>1 hurts tiny convs).
        let n = rayon::current_num_threads().clamp(1, 8);
        let mut sessions = Vec::with_capacity(n);
        for _ in 0..n {
            sessions.push(Mutex::new(MnnSession::load_conv(model_path, 1)?));
        }
        Ok(Self { sessions })
    }
}

/// One box's ink-model output, 0..255 at the 48px strip height. Channels are detected from the
/// model's output depth, so older models load and run unchanged:
/// - `matte` (ch0) always present;
/// - `bold` (ch1) present for a 2-channel bold model, `None` for the legacy matte-only model;
/// - `rule` (ch2) present for a 3-channel model: per-pixel under/strike/over-line coverage
///   (the type is named downstream from its vertical position vs the matte's baseline/x-height).
///   `None` for matte-only and bold models — so a user still on the shipped 2-channel bold model
///   simply gets no rule, and rule-aware code degrades to "no underline" with no version flag.
pub struct InkStrip {
    pub matte: GrayImage,
    pub bold: Option<GrayImage>,
    pub rule: Option<GrayImage>,
    /// Predicted per-pixel ink colour F (colour-head models: the trailing 6 channels are
    /// F rgb + B rgb, trained FBA-style with a compositing loss). `None` for older models.
    pub fg: Option<RgbImage>,
    /// Predicted per-pixel background colour B. `None` for older models.
    pub bg: Option<RgbImage>,
    /// Fully α-re-solved matte (min of model matte and the F–B projection at every pixel,
    /// no kill threshold) — the *colour-pooling* weight. Erase keeps the kill-switch `matte`
    /// (recall: full re-solve washes glyph cores to ~0.75 and translucent erase misses ink),
    /// while colour pooling wants precision: a spill pixel whose observed colour sits midway
    /// to background projects low here and stops dragging the pooled ink colour.
    pub color_alpha: Option<GrayImage>,
    /// Source-image `(x, y)` each matte/bold column-row sampled, row-major over
    /// the strip's own `matte.width() × matte.height()`. Lets the matting scatter
    /// the strip back into image space (the strip is a curl-straightened contour
    /// dewarp, so there is no single affine inverse). `None` for the oriented-box
    /// fallback (a box with no contour), where matting uses the affine instead.
    pub src_map: Option<Vec<(f32, f32)>>,
}

impl InkStrip {
    /// Mask of pixels to erase = matte ∪ rule (pixel max). Under/strike/over-line rules are thin
    /// structures the matte under-covers, so the dedicated rule channel (ch2) is what makes them
    /// erase. Just the matte when there's no rule channel (legacy/bold models).
    pub fn erase_mask(&self) -> GrayImage {
        let Some(rule) = self.rule.as_ref() else {
            return self.matte.clone();
        };
        let mut buf = self.matte.as_raw().clone();
        for (m, r) in buf.iter_mut().zip(rule.iter()) {
            *m = (*m).max(*r);
        }
        GrayImage::from_raw(self.matte.width(), self.matte.height(), buf)
            .unwrap_or_else(|| self.matte.clone())
    }

    /// Mean bold probability (0..1) over the strip's ink pixels — the per-line weight
    /// estimate the caller thresholds (≈0.65). `None` when there is no bold channel
    /// (legacy matte-only model) or too little ink to be reliable.
    pub fn pooled_bold(&self) -> Option<f32> {
        let bold = self.bold.as_ref()?;
        let core = translator_raster::text_metrics::stroke_core_cut(
            self.matte.iter().copied().max().unwrap_or(0),
        );
        // Exclude rule (under/strike/over) pixels: they're non-glyph ink, so their stroke-width
        // reading is meaningless and would drag the per-line bold estimate.
        let rule = self.rule.as_ref().map(|r| r.as_raw());
        // Enough-ink gate on the matte core either way — it asks "did the model find text",
        // separate from which pixels are trustworthy for pooling.
        let n_core = self.matte.iter().filter(|&&m| m >= core).count() as u64;
        if n_core < translator_raster::text_metrics::INK_BOLD_MIN_PX {
            return None;
        }
        // Colour-head models: weight by the re-solved alpha squared — the principled form of
        // the core cut. Spill projects near 0, the anti-aliased rim at its fractional
        // coverage, solid ink high, so rim/spill dilution falls out without a threshold.
        if let Some(ca) = self.color_alpha.as_ref() {
            let (mut sum, mut wsum) = (0f64, 0f64);
            for (i, (a, b)) in ca.iter().zip(bold.iter()).enumerate() {
                if rule.is_some_and(|r| r[i] > 127) {
                    continue;
                }
                let w = (*a as f64 / 255.0) * (*a as f64 / 255.0);
                sum += w * *b as f64;
                wsum += w;
            }
            return (wsum > 0.0).then(|| (sum / wsum / 255.0) as f32);
        }
        let (mut sum, mut n) = (0u64, 0u64);
        for (i, (m, b)) in self.matte.iter().zip(bold.iter()).enumerate() {
            if *m >= core && rule.map_or(true, |r| r[i] <= 127) {
                sum += *b as u64;
                n += 1;
            }
        }
        (n > 0).then(|| sum as f32 / n as f32 / 255.0)
    }

    /// Reduce the strip's bold + matte channels to a per-reading-axis-column
    /// [`translator_raster::text_metrics::BoldProfile`]. `None` when there is no bold channel (legacy
    /// matte-only model).
    pub fn bold_profile(&self) -> Option<translator_raster::text_metrics::BoldProfile> {
        translator_raster::text_metrics::BoldProfile::from_strip(self.bold.as_ref()?, &self.matte)
    }

    /// Reduce the strip's rule + matte channels to a per-reading-axis-column
    /// [`translator_raster::text_metrics::RuleProfile`] for per-word decoration recovery. `None`
    /// when there is no rule channel (legacy matte-only / bold-only models).
    pub fn rule_profile(&self) -> Option<translator_raster::text_metrics::RuleProfile> {
        translator_raster::text_metrics::RuleProfile::from_strip(self.rule.as_ref()?, &self.matte)
    }

    /// The α field colour estimation should weight by: the fully re-solved alpha when the
    /// colour head produced one, else the matte.
    pub fn color_pool_alpha(&self) -> &GrayImage {
        self.color_alpha.as_ref().unwrap_or(&self.matte)
    }

    /// Pool the predicted colour fields to one line colour: fg = α²-weighted mean of F
    /// (α² so confident stroke cores dominate over the anti-aliased rim), bg luma =
    /// (1−α)²-weighted mean luma of B. `None` without the colour head or with too little
    /// ink to trust. The colour comes from the model's F prediction, not from image pixels
    /// gated by the matte, so matte over-marking cannot drag it toward the background.
    pub fn pooled_color(&self) -> Option<translator_core::ocr::InkColor> {
        let fg = self.fg.as_ref()?;
        let bg = self.bg.as_ref()?;
        let ink_px = self
            .matte
            .iter()
            .filter(|&&m| m >= translator_raster::text_metrics::INK_CUT as u8)
            .count();
        if ink_px < 6 {
            return None;
        }
        // F pools over the fully re-solved alpha (spill pixels project low and stop
        // dragging the colour); the background term keeps the erase matte — a washed
        // glyph core must not count as background.
        let pool = self.color_pool_alpha();
        let (mut fw, mut fsum) = (0f64, [0f64; 3]);
        let (mut bw, mut bsum) = (0f64, 0f64);
        for (i, (&m, &pa)) in self.matte.iter().zip(pool.iter()).enumerate() {
            let a = m as f64 / 255.0;
            let f = fg.get_pixel(i as u32 % fg.width(), i as u32 / fg.width());
            let b = bg.get_pixel(i as u32 % bg.width(), i as u32 / bg.width());
            let wa = (pa as f64 / 255.0) * (pa as f64 / 255.0);
            fw += wa;
            for c in 0..3 {
                fsum[c] += wa * f[c] as f64;
            }
            let wb = (1.0 - a) * (1.0 - a);
            bw += wb;
            bsum += wb * (b[0] as f64 * 0.299 + b[1] as f64 * 0.587 + b[2] as f64 * 0.114);
        }
        if fw <= 0.0 || bw <= 0.0 {
            return None;
        }
        let ch = |c: usize| (fsum[c] / fw).round().clamp(0.0, 255.0) as u32;
        Some(translator_core::ocr::InkColor {
            fg_argb: 0xff00_0000 | (ch(0) << 16) | (ch(1) << 8) | ch(2),
            bg_luma: (bsum / bw).round().clamp(0.0, 255.0) as u8,
        })
    }
}

pub struct DewarpedStrip {
    pub image: RgbImage,
    /// `None` for the oriented-box fallback — only the contour warp can map pixels back.
    pub src_map: Option<Vec<(f32, f32)>>,
}

pub struct PpocrRecognizer {
    /// One MnnSession per parallel worker. Each is wrapped in a Mutex but contention is zero —
    /// the rayon worker at index `i` only touches `sessions[i]`. The Mutex is just there to
    /// satisfy Sync on MnnSession's interior mutability without making assumptions about MNN's
    /// thread-safety guarantees.
    sessions: Vec<Mutex<MnnSession>>,
    charset: Vec<char>,
    pool: ThreadPool,
}

pub struct PpocrScriptClassifier {
    session: MnnSession,
}

/// Binary 0°/180° label out of the textline orientation model. Resolving the
/// full 90° quadrant requires running the model on the strip and again on the
/// strip rotated 90° clockwise (see `live_session::estimate_canonical_quadrant`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextlineOriLabel {
    Up,
    Flipped180,
}

#[derive(Debug, Clone, Copy)]
pub struct TextlineOriCandidate {
    pub label: TextlineOriLabel,
    pub score: f32,
}

pub struct PpocrTextlineOrientationClassifier {
    session: MnnSession,
}

#[derive(Debug, Clone)]
pub struct PpocrRecognizerSpec {
    pub script: PpocrScript,
    pub model_path: PathBuf,
    pub keys_path: PathBuf,
}

struct PpocrRecognizerSlot {
    spec: PpocrRecognizerSpec,
    loaded: OnceLock<Arc<PpocrRecognizer>>,
}

/// Byte order of the RGB image handed to the ink colour head. Det/rec are channel-agnostic,
/// but the colour head predicts *channel-identified* F/B, so the ink input planes (and the α
/// re-solve's observed pixel) must be true RGB. The app's still + PDF paths carry pixels as
/// B,G,R,A (little-endian ARGB) — `build_rgb_full` copies verbatim so their "RgbImage" is
/// actually BGR — and pass [`Bgr`](Self::Bgr) to swap the planes back. True-RGB sources (the
/// live/screen GL readback, image-crate loads in viz/bins) pass [`Rgb`](Self::Rgb).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InkChannelOrder {
    Rgb,
    Bgr,
}

impl InkChannelOrder {
    /// Maps the model's true-RGB channel index to the strip's storage channel.
    fn chan(self) -> [usize; 3] {
        match self {
            InkChannelOrder::Rgb => [0, 1, 2],
            InkChannelOrder::Bgr => [2, 1, 0],
        }
    }
}

pub struct PpocrEngine {
    detector: PpocrDetector,
    classifier: Option<PpocrScriptClassifier>,
    textline_orientation: Option<PpocrTextlineOrientationClassifier>,
    recognizers: HashMap<PpocrScript, PpocrRecognizerSlot>,
    ink: Option<PpocrInkModel>,
}

impl PpocrEngine {
    pub fn load(
        det_path: &Path,
        classifier_path: Option<&Path>,
        textline_orientation_path: Option<&Path>,
        recognizer_specs: Vec<PpocrRecognizerSpec>,
        det_intra_threads: usize,
        ink_path: Option<&Path>,
    ) -> Result<Self, TranslatorError> {
        // Det is one big graph and benefits from intra-session threading. Rec models are loaded
        // lazily per script so auto mode can route strips to multiple scripts without
        // constructing every recognizer up front.
        let detector = PpocrDetector::load(det_path, det_intra_threads)?;
        let classifier = classifier_path
            .map(PpocrScriptClassifier::load)
            .transpose()?;
        let textline_orientation = textline_orientation_path
            .map(PpocrTextlineOrientationClassifier::load)
            .transpose()?;
        let recognizers = recognizer_specs
            .into_iter()
            .map(|spec| {
                (
                    spec.script,
                    PpocrRecognizerSlot {
                        spec,
                        loaded: OnceLock::new(),
                    },
                )
            })
            .collect();
        let ink = ink_path.map(PpocrInkModel::load).transpose()?;
        Ok(Self {
            detector,
            classifier,
            textline_orientation,
            recognizers,
            ink,
        })
    }

    pub fn has_classifier(&self) -> bool {
        self.classifier.is_some()
    }

    pub fn has_ink(&self) -> bool {
        self.ink.is_some()
    }

    /// Per-box ink matte, 1:1 with `boxes`. `None` for a box when no ink model is loaded
    /// or it has a degenerate oriented box. Run after detection — this is the same "keep it
    /// per detection" pattern as the heatmap / contour / tight-box outputs. The mask is at
    /// the model's 48px height (any width), in the box's *oriented-box* rectified space
    /// (tight text band, no padding) so a caller — color matting — can register it 1:1
    /// against a strip built from the same oriented box.
    pub fn ink_masks(
        &self,
        image: &DynamicImage,
        boxes: &[translator_core::ocr::DetectedTextBox],
        canonical_quadrant: Option<translator_core::coords::Quadrant>,
        order: InkChannelOrder,
    ) -> Vec<Option<GrayImage>> {
        self.ink_strips(image, boxes, canonical_quadrant, order)
            .into_iter()
            .map(|s| s.map(|s| s.matte))
            .collect()
    }

    /// Like [`ink_masks`] but also returns the per-pixel bold map (ch1) when a 2-channel
    /// bold model is loaded. The matte (ch0) is identical to `ink_masks`.
    pub fn ink_strips(
        &self,
        image: &DynamicImage,
        boxes: &[translator_core::ocr::DetectedTextBox],
        canonical_quadrant: Option<translator_core::coords::Quadrant>,
        order: InkChannelOrder,
    ) -> Vec<Option<InkStrip>> {
        if self.ink.is_none() {
            return boxes.iter().map(|_| None).collect();
        }
        let t_pre = Instant::now();
        let strips = self.dewarp_strips(image, boxes, canonical_quadrant);
        let pre_us = t_pre.elapsed().as_micros();
        self.ink_strips_from(&strips, order, pre_us)
    }

    pub fn dewarp_strips(
        &self,
        image: &DynamicImage,
        boxes: &[translator_core::ocr::DetectedTextBox],
        canonical_quadrant: Option<translator_core::coords::Quadrant>,
    ) -> Vec<Option<DewarpedStrip>> {
        let h = PpocrInkModel::HEIGHT;
        let rgb = image.to_rgb8();
        let thickness_pad = self.detector.pool_comp();
        // Dewarp each box's text band straight to a 48px strip (width a multiple of the
        // pooling factor), in parallel — the per-pixel warp dominates and the strips
        // are independent. Prefer the recognizer's curl-straightened contour dewarp with the
        // *same* params (contour, quadrant, thickness_pad), so the bold channel's columns
        // line up with the rec CTC firing fractions; keep the per-pixel source map so the
        // matting can scatter the matte back to image space. A box with no usable contour
        // falls back to the oriented-box affine (no map; no per-word bold there).
        boxes
            .par_iter()
            .map(|b| {
                let contour: Option<Vec<(f32, f32)>> = (!b.contour.is_empty()
                    && b.contour.len() % 2 == 0)
                    .then(|| b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect());
                if let Some(natural) = contour
                    .as_deref()
                    .and_then(|c| contour_strip_warp(c, canonical_quadrant, thickness_pad))
                {
                    let content_w = ((natural.width as f32 * h as f32 / natural.height as f32)
                        .round() as u32)
                        .max(PpocrInkModel::POOL_MULTIPLE)
                        .next_multiple_of(PpocrInkModel::POOL_MULTIPLE);
                    let warp = ContourStripWarp {
                        width: content_w,
                        height: h,
                        ..natural
                    };
                    let (strip, map) = render_contour_strip_rgb_with_map(&rgb, &warp);
                    return Some(DewarpedStrip {
                        image: strip,
                        src_map: Some(map),
                    });
                }
                let o = &b.oriented_box;
                if o.width <= 1.0 || o.height <= 1.0 {
                    return None;
                }
                let aw = ((o.width * h as f32 / o.height).round() as u32)
                    .max(PpocrInkModel::POOL_MULTIPLE);
                let w = aw.next_multiple_of(PpocrInkModel::POOL_MULTIPLE);
                Some(DewarpedStrip {
                    image: dewarp_oriented_to_strip_rgb(&rgb, o, w, h),
                    src_map: None,
                })
            })
            .collect()
    }

    pub fn ink_strips_from(
        &self,
        strips: &[Option<DewarpedStrip>],
        order: InkChannelOrder,
        pre_us: u128,
    ) -> Vec<Option<InkStrip>> {
        let Some(ink) = &self.ink else {
            return strips.iter().map(|_| None).collect();
        };
        let h = PpocrInkModel::HEIGHT;
        let prepared: Vec<(usize, &RgbImage, Option<&Vec<(f32, f32)>>)> = strips
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.as_ref().map(|d| (i, &d.image, d.src_map.as_ref())))
            .collect();

        // Bucket by *exact* width (same-width strips batch into one call, no padding), then
        // run buckets in parallel — one MNN session per rayon worker, no lock contention.
        // Exact width beats binning here: the matte model is compute-bound, so padding to a
        // bin width just adds wasted conv work and coarsens the parallelism granularity.
        let mut by_width: std::collections::BTreeMap<u32, Vec<usize>> = Default::default();
        for (j, (_, r, _)) in prepared.iter().enumerate() {
            by_width.entry(r.width()).or_default().push(j);
        }
        let buckets: Vec<(u32, Vec<usize>)> = by_width.into_iter().collect();
        let n_batches = buckets.len();

        let t_inf = Instant::now();
        let sigmoid_u8 = |logit: f32| {
            ((1.0 / (1.0 + (-logit).exp())) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        let groups: Vec<Vec<(usize, InkStrip)>> = buckets
            .par_iter()
            .map(|(bw, idxs)| {
                let bw = *bw;
                let plane = (h * bw) as usize;
                let nb = idxs.len();
                let chan = order.chan();
                let mut input = vec![0f32; nb * 3 * plane];
                for (slot, &j) in idxs.iter().enumerate() {
                    let base = slot * 3 * plane;
                    for (x, y, px) in prepared[j].1.enumerate_pixels() {
                        let idx = (y * bw + x) as usize;
                        input[base + idx] = px[chan[0]] as f32 / 255.0;
                        input[base + plane + idx] = px[chan[1]] as f32 / 255.0;
                        input[base + 2 * plane + idx] = px[chan[2]] as f32 / 255.0;
                    }
                }
                let slot = rayon::current_thread_index().unwrap_or(0) % ink.sessions.len();
                let run = ink.sessions[slot]
                    .lock()
                    .expect("ink session poisoned")
                    .run(&input, &[nb, 3, h as usize, bw as usize]);
                let Ok((o, _)) = run else { return Vec::new() };
                if o.len() < nb * plane {
                    return Vec::new();
                }
                // Output is [nb, chans, h, bw]: chans=1 legacy matte-only, 2 = +bold (ch1),
                // 3 = +rule (ch2, under/strike/over); a colour-head model appends 6 more
                // (F rgb + B rgb), so 8 = matte+bold+colour and 9 = matte+bold+rule+colour.
                // Each strip's planes are contiguous, so a strip's ch0 starts at
                // slot*chans*plane and chan c at +c*plane.
                let chans = o.len() / (nb * plane);
                let has_rule = chans == 3 || chans == 9;
                let has_color = chans >= 8;
                idxs.iter()
                    .enumerate()
                    .filter_map(|(slot, &j)| {
                        let (box_idx, tw) = (prepared[j].0, prepared[j].1.width());
                        let sbase = slot * chans * plane;
                        let mut matte = vec![0u8; (h * tw) as usize];
                        let mut bold = (chans >= 2).then(|| vec![0u8; (h * tw) as usize]);
                        let mut rule = has_rule.then(|| vec![0u8; (h * tw) as usize]);
                        let mut fg = has_color.then(|| vec![0u8; (h * tw * 3) as usize]);
                        let mut bg = has_color.then(|| vec![0u8; (h * tw * 3) as usize]);
                        let f0 = (chans - 6) * plane;
                        for y in 0..h {
                            for x in 0..tw {
                                let i = (y * bw + x) as usize;
                                let o_i = (y * tw + x) as usize;
                                matte[o_i] = sigmoid_u8(o[sbase + i]);
                                if let Some(b) = bold.as_mut() {
                                    b[o_i] = sigmoid_u8(o[sbase + plane + i]);
                                }
                                if let Some(r) = rule.as_mut() {
                                    r[o_i] = sigmoid_u8(o[sbase + 2 * plane + i]);
                                }
                                if let Some(f) = fg.as_mut() {
                                    for c in 0..3 {
                                        f[o_i * 3 + c] = sigmoid_u8(o[sbase + f0 + c * plane + i]);
                                    }
                                }
                                if let Some(bgb) = bg.as_mut() {
                                    for c in 0..3 {
                                        bgb[o_i * 3 + c] =
                                            sigmoid_u8(o[sbase + f0 + (3 + c) * plane + i]);
                                    }
                                }
                            }
                        }
                        // Closed-form α re-solve (the FBA fusion step): project the observed
                        // pixel onto the predicted F–B colour line, α = ((I−B)·(F−B))/‖F−B‖².
                        // Used strictly as a background KILL-SWITCH: a matte pixel that grabbed
                        // background has I≈B so its projection lands near 0, and only those
                        // clearly-background pixels (below RESOLVE_KILL) get their matte pulled
                        // down. Above the cut the model matte stands — real stroke interiors
                        // project at ~0.7–0.8 (observed ink is never as extreme as the predicted
                        // flat F under blur/texture), and min()-ing them washed out glyph cores.
                        // Where F≈B the projection is numerically meaningless; matte stands.
                        let mut color_alpha = None;
                        if let (Some(f), Some(bgc)) = (fg.as_ref(), bg.as_ref()) {
                            const MIN_FB_DIST2: i32 = 32 * 32;
                            const RESOLVE_KILL: u8 = 102;
                            let src = prepared[j].1;
                            let mut ca = matte.clone();
                            for y in 0..h {
                                for x in 0..tw {
                                    let o_i = (y * tw + x) as usize;
                                    let p = src.get_pixel(x, y);
                                    let (mut dot, mut n2) = (0i32, 0i32);
                                    for c in 0..3 {
                                        let d = f[o_i * 3 + c] as i32 - bgc[o_i * 3 + c] as i32;
                                        // F/B are true RGB; read the observed pixel through the
                                        // same channel map as the model input.
                                        dot += (p[chan[c]] as i32 - bgc[o_i * 3 + c] as i32) * d;
                                        n2 += d * d;
                                    }
                                    if n2 < MIN_FB_DIST2 {
                                        continue;
                                    }
                                    let a = ((dot as f32 / n2 as f32).clamp(0.0, 1.0) * 255.0)
                                        .round() as u8;
                                    ca[o_i] = ca[o_i].min(a);
                                    if a < RESOLVE_KILL {
                                        matte[o_i] = matte[o_i].min(a);
                                    }
                                }
                            }
                            color_alpha = Some(ca);
                        }
                        let matte = GrayImage::from_raw(tw, h, matte)?;
                        let bold = bold.and_then(|b| GrayImage::from_raw(tw, h, b));
                        let rule = rule.and_then(|r| GrayImage::from_raw(tw, h, r));
                        let fg = fg.and_then(|f| RgbImage::from_raw(tw, h, f));
                        let bg = bg.and_then(|b| RgbImage::from_raw(tw, h, b));
                        let color_alpha = color_alpha.and_then(|c| GrayImage::from_raw(tw, h, c));
                        let src_map = prepared[j].2.cloned();
                        Some((
                            box_idx,
                            InkStrip {
                                matte,
                                bold,
                                rule,
                                fg,
                                bg,
                                color_alpha,
                                src_map,
                            },
                        ))
                    })
                    .collect()
            })
            .collect();
        let inf_us = t_inf.elapsed().as_micros();

        let mut out: Vec<Option<InkStrip>> = strips.iter().map(|_| None).collect();
        for group in groups {
            for (box_idx, strip) in group {
                out[box_idx] = Some(strip);
            }
        }
        if !prepared.is_empty() {
            log::info!(
                "ppocr ink: {} strips in {n_batches} batches, {} sessions — pre={:.1}ms infer={:.1}ms",
                prepared.len(),
                ink.sessions.len(),
                pre_us as f32 / 1000.0,
                inf_us as f32 / 1000.0,
            );
        }
        out
    }

    pub fn has_textline_orientation(&self) -> bool {
        self.textline_orientation.is_some()
    }

    /// Batched textline orientation classification. Each crop should be a
    /// dewarped text strip in its natural box-local orientation. Output is
    /// 1:1 with input; entries that produced non-finite logits come back
    /// as `None`. Errors out if no model was loaded — callers should gate
    /// on `has_textline_orientation()`.
    pub fn textline_orientation_classify(
        &self,
        crops: &[DynamicImage],
    ) -> Result<Vec<Option<TextlineOriCandidate>>, TranslatorError> {
        let Some(classifier) = &self.textline_orientation else {
            return Err(TranslatorError::new(
                TranslatorErrorKind::MissingAsset,
                "ppocr textline orientation classifier is not available",
            ));
        };
        classifier.classify_many(crops)
    }

    pub fn installed_scripts(&self) -> impl Iterator<Item = PpocrScript> + '_ {
        self.recognizers.keys().copied()
    }

    fn recognizer(&self, script: PpocrScript) -> Result<Arc<PpocrRecognizer>, TranslatorError> {
        let slot = self.recognizers.get(&script).ok_or_else(|| {
            TranslatorError::new(
                TranslatorErrorKind::MissingAsset,
                format!(
                    "ppocr recognizer for script {} is not installed",
                    script.as_slug()
                ),
            )
        })?;
        if let Some(rec) = slot.loaded.get() {
            return Ok(Arc::clone(rec));
        }
        let fresh = Arc::new(PpocrRecognizer::load(
            &slot.spec.model_path,
            &slot.spec.keys_path,
        )?);
        Ok(Arc::clone(slot.loaded.get_or_init(|| fresh)))
    }

    /// Recognize a single straightened strip and return its per-character CTC firings,
    /// for debug visualization of where each glyph fires along the reading axis. The
    /// returned [`StripChar::at`] fractions map directly onto the strip's width.
    pub fn recognize_strip_firings(
        &self,
        strip: &DynamicImage,
        script: PpocrScript,
    ) -> Result<Vec<StripChar>, TranslatorError> {
        let recognizer = self.recognizer(script)?;
        let timings = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
        let result = recognizer.recognize_one(strip, 0, &timings)?;
        Ok(result
            .chars
            .into_iter()
            .map(|c| StripChar {
                ch: c.ch,
                score: c.score,
                at: c.at.0,
            })
            .collect())
    }

    /// Run the detector on a pre-built `DynamicImage` and return geometry only,
    /// without recognition. `profile` selects still vs live thresholds.
    pub fn detect_only_image(
        &self,
        image: &DynamicImage,
        profile: PpocrProfile,
    ) -> Result<Vec<translator_core::ocr::DetectedTextBox>, TranslatorError> {
        self.detect_only_image_with_thresholds(image, profile.thresholds())
    }

    fn detect_only_image_with_thresholds(
        &self,
        image: &DynamicImage,
        thresholds: PpocrThresholds,
    ) -> Result<Vec<translator_core::ocr::DetectedTextBox>, TranslatorError> {
        let width = image.width();
        let height = image.height();
        let boxes = self.detector.detect_with_thresholds(image, thresholds)?;

        // Per-box tilt is measured first, then reconciled against a frame-level consensus angle
        // field. A single contour can't tell a genuine baseline lean from content asymmetry
        // or a deceptively-flat short word; the scene of agreeing boxes can. The consensus also
        // replaces the old hard "→ 0°" fallback, which only made sense for world-up pages: a box
        // with no tilt of its own now adopts the scene angle *at its own position* instead of
        // snapping to horizontal — under perspective or page curl the reading direction varies
        // smoothly across the frame, so a short continuation line inherits its neighborhood's
        // lean rather than a global average dominated by far-away lines.
        struct Detected {
            aabb: translator_core::ocr::Rect,
            contour: Option<Vec<(f32, f32)>>,
            score: f32,
            tilt: TiltMeasurement,
        }

        let pool_comp = self.detector.pool_comp();
        let detected: Vec<Detected> = boxes
            .into_iter()
            .map(|tb| {
                let expanded = expand_box(&tb.rect, DET_BOX_BORDER, width, height);
                let aabb = translator_core::ocr::Rect {
                    left: expanded.left,
                    top: expanded.top,
                    right: expanded.right,
                    bottom: expanded.bottom,
                };
                let tilt = tb
                    .contour
                    .as_ref()
                    .map(|c| estimate_horizontal_tilt(c))
                    .unwrap_or_else(|| {
                        let cx = (aabb.left + aabb.right) as f32 * 0.5;
                        let cy = (aabb.top + aabb.bottom) as f32 * 0.5;
                        TiltMeasurement::none(cx, cy)
                    });
                Detected {
                    aabb,
                    contour: tb.contour,
                    score: tb.score,
                    tilt,
                }
            })
            .collect();

        // Frame consensus: each contour votes its own lean, weighted by its length confidence
        // (a long line defines the frame; a short one barely nudges it), sited at its centroid.
        let votes: Vec<TiltVote> = detected
            .iter()
            .filter_map(|d| {
                d.tilt.vote.map(|angle| TiltVote {
                    x: d.tilt.cx,
                    y: d.tilt.cy,
                    weight: translator_raster::text_metrics::length_confidence(d.tilt.aspect),
                    angle,
                })
            })
            .collect();
        let field = fit_tilt_field(&votes);

        let out: Vec<translator_core::ocr::DetectedTextBox> = detected
            .into_iter()
            .map(|d| {
                let consensus = field.as_ref().map(|f| f.at(d.tilt.cx, d.tilt.cy));
                let angle = resolve_box_angle(&d.tilt, consensus);
                let contour_boxes = d
                    .contour
                    .as_ref()
                    .and_then(|c| build_oriented_boxes(c, angle, pool_comp));
                let (oriented, tight) = match contour_boxes {
                    Some(ContourBoxes { tight, inflated }) => (inflated, tight),
                    None => {
                        let aligned = translator_core::ocr::OrientedRect::axis_aligned(d.aabb);
                        (aligned, aligned)
                    }
                };
                let contour_flat: Vec<f32> = d
                    .contour
                    .as_ref()
                    .map(|c| {
                        let mut v = Vec::with_capacity(c.len() * 2);
                        for &(x, y) in c {
                            v.push(x);
                            v.push(y);
                        }
                        v
                    })
                    .unwrap_or_default();
                translator_core::ocr::DetectedTextBox {
                    rect: d.aabb,
                    oriented_box: oriented,
                    tight_box: tight,
                    contour: contour_flat,
                    score: d.score,
                }
            })
            .collect();
        Ok(out)
    }

    /// Run the detector and return its raw probability map resampled to the
    /// original image resolution: `0` = no text, `255` = highest text
    /// probability. Unlike `detect_only_image`, this exposes the
    /// pre-threshold heatmap (no thresholding or box extraction), for
    /// visualization and debugging.
    pub fn detect_heatmap(&self, image: &DynamicImage) -> Result<GrayImage, TranslatorError> {
        self.detector.probability_map(image)
    }

    pub fn classify_text_boxes_image(
        &self,
        image: &DynamicImage,
        boxes: &[translator_core::ocr::DetectedTextBox],
        canonical_quadrant: Option<translator_core::coords::Quadrant>,
    ) -> Result<Vec<Option<PpocrScriptPrediction>>, TranslatorError> {
        let Some(classifier) = &self.classifier else {
            return Err(TranslatorError::new(
                TranslatorErrorKind::MissingAsset,
                "ppocr script classifier is not available",
            ));
        };
        let crops = crop_text_strips(image, boxes, canonical_quadrant, self.detector.pool_comp()).0;
        let image_area = (image.width() as f32) * (image.height() as f32);
        let mut predictions = vec![None; boxes.len()];
        let mut eligible_crops = Vec::new();
        let mut eligible_indices = Vec::new();
        for (idx, (box_, crop)) in boxes.iter().zip(crops.iter()).enumerate() {
            if pulc_strip_eligible(box_, crop, image_area) {
                eligible_indices.push(idx);
                eligible_crops.push(crop.clone());
            } else {
                let area = box_.rect.width().saturating_mul(box_.rect.height());
                log::debug!(
                    "ppocr pulc strip={} skipped small det_score={:.3} width={} height={} area={} area_ratio={:.6} min_area_ratio={:.6} crop={}x{}",
                    idx,
                    box_.score,
                    box_.rect.width(),
                    box_.rect.height(),
                    area,
                    area as f32 / image_area.max(1.0),
                    PULC_MIN_IMAGE_AREA_RATIO,
                    crop.width(),
                    crop.height(),
                );
            }
        }
        let classified = classifier.classify_many(&eligible_crops)?;
        for (idx, candidate) in eligible_indices.into_iter().zip(classified.into_iter()) {
            let box_ = &boxes[idx];
            let crop = &crops[idx];
            let area = box_.rect.width().saturating_mul(box_.rect.height());
            let area_ratio = area as f32 / image_area.max(1.0);
            let Some(candidate) = candidate else {
                log::debug!(
                    "ppocr pulc strip={} no_candidate det_score={:.3} width={} height={} area={} area_ratio={:.6} crop={}x{}",
                    idx,
                    box_.score,
                    box_.rect.width(),
                    box_.rect.height(),
                    area,
                    area_ratio,
                    crop.width(),
                    crop.height(),
                );
                continue;
            };
            if candidate.score >= PULC_MIN_SCORE {
                log::debug!(
                    "ppocr pulc strip={} script={} score={:.3} det_score={:.3} width={} height={} area={} area_ratio={:.6} crop={}x{}",
                    idx,
                    candidate.class.name(),
                    candidate.score,
                    box_.score,
                    box_.rect.width(),
                    box_.rect.height(),
                    area,
                    area_ratio,
                    crop.width(),
                    crop.height(),
                );
                predictions[idx] = Some(PpocrScriptPrediction {
                    class: candidate.class,
                    score: candidate.score,
                });
            } else {
                log::debug!(
                    "ppocr pulc strip={} script={} score={:.3} below_threshold={:.2} det_score={:.3} width={} height={} area={} area_ratio={:.6} crop={}x{}",
                    idx,
                    candidate.class.name(),
                    candidate.score,
                    PULC_MIN_SCORE,
                    box_.score,
                    box_.rect.width(),
                    box_.rect.height(),
                    area,
                    area_ratio,
                    crop.width(),
                    crop.height(),
                );
            }
        }
        Ok(predictions)
    }

    /// Recognize text in caller-supplied boxes. `scripts` selects the recognizer pack
    /// per box (parallel to `boxes`); strips with the same script are dispatched as a
    /// batch to a single recognizer session pool. `profile` selects still vs live
    /// thresholds. Output is 1:1 aligned with input boxes; filtered entries come back
    /// with empty text and `confidence = 0.0`.
    pub fn recognize_text_in_boxes_image(
        &self,
        image: &DynamicImage,
        boxes: &[translator_core::ocr::DetectedTextBox],
        scripts: &[PpocrScript],
        profile: PpocrProfile,
        canonical_quadrant: Option<translator_core::coords::Quadrant>,
    ) -> Result<Vec<translator_core::ocr::RecognizedTextLine>, TranslatorError> {
        if boxes.is_empty() {
            return Ok(Vec::new());
        }
        if scripts.len() != boxes.len() {
            return Err(TranslatorError::new(
                TranslatorErrorKind::InvalidInput,
                "ppocr recognition scripts length must match boxes length",
            ));
        }
        let t_crops = Instant::now();
        let (crops, dewarp_count) =
            crop_text_strips(image, boxes, canonical_quadrant, self.detector.pool_comp());
        let crops_ms = t_crops.elapsed().as_secs_f32() * 1000.0;
        self.recognize_from_crops(
            crops,
            dewarp_count,
            crops_ms,
            image.width(),
            image.height(),
            boxes,
            scripts,
            profile,
        )
    }

    pub fn recognize_from_strips(
        &self,
        strips: &[Option<DewarpedStrip>],
        boxes: &[translator_core::ocr::DetectedTextBox],
        scripts: &[PpocrScript],
        profile: PpocrProfile,
        src_w: u32,
        src_h: u32,
        dewarp_ms: f32,
    ) -> Result<Vec<translator_core::ocr::RecognizedTextLine>, TranslatorError> {
        if boxes.is_empty() {
            return Ok(Vec::new());
        }
        if scripts.len() != boxes.len() {
            return Err(TranslatorError::new(
                TranslatorErrorKind::InvalidInput,
                "ppocr recognition scripts length must match boxes length",
            ));
        }
        let crops: Vec<DynamicImage> = strips
            .iter()
            .map(|s| match s {
                Some(d) => DynamicImage::ImageRgb8(d.image.clone()),
                None => DynamicImage::ImageRgb8(RgbImage::new(1, 1)),
            })
            .collect();
        let dewarp_count = strips.iter().filter(|s| s.is_some()).count();
        self.recognize_from_crops(
            crops,
            dewarp_count,
            dewarp_ms,
            src_w,
            src_h,
            boxes,
            scripts,
            profile,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn recognize_from_crops(
        &self,
        crops: Vec<DynamicImage>,
        dewarp_count: usize,
        crops_ms: f32,
        width: u32,
        height: u32,
        boxes: &[translator_core::ocr::DetectedTextBox],
        scripts: &[PpocrScript],
        profile: PpocrProfile,
    ) -> Result<Vec<translator_core::ocr::RecognizedTextLine>, TranslatorError> {
        let thresholds = profile.thresholds();
        let mean_crop_w: f32 =
            crops.iter().map(|c| c.width() as f32).sum::<f32>() / crops.len() as f32;
        let mean_crop_h: f32 =
            crops.iter().map(|c| c.height() as f32).sum::<f32>() / crops.len() as f32;

        let mut rec_chunks = Vec::new();
        let mut owner_chunks = (0..boxes.len()).map(|_| Vec::new()).collect::<Vec<_>>();
        let mut wide_count = 0usize;
        let mut split_count = 0usize;
        for (owner, crop) in crops.iter().enumerate() {
            if resized_rec_width(crop) > REC_WHITESPACE_SPLIT_MAX_WIDTH {
                wide_count += 1;
            }
            let chunks =
                split_crop_on_whitespace_for_rec_width(crop, REC_WHITESPACE_SPLIT_MAX_WIDTH);
            split_count += chunks.len().saturating_sub(1);
            let cw = crop.width().max(1) as f32;
            for chunk in chunks {
                let idx = rec_chunks.len();
                owner_chunks[owner].push(idx);
                rec_chunks.push(RecChunk {
                    owner,
                    image: chunk.image,
                    join_before: chunk.join_before,
                    frac0: chunk.src_x0 as f32 / cw,
                    frac1: chunk.src_x1 as f32 / cw,
                });
            }
        }
        let mean_chunk_w: f32 = rec_chunks
            .iter()
            .map(|c| c.image.width() as f32)
            .sum::<f32>()
            / rec_chunks.len().max(1) as f32;
        let mean_chunk_h: f32 = rec_chunks
            .iter()
            .map(|c| c.image.height() as f32)
            .sum::<f32>()
            / rec_chunks.len().max(1) as f32;

        let mut results: Vec<Option<RecResult>> = (0..rec_chunks.len()).map(|_| None).collect();
        let mut rec_wall_ms = 0.0;
        let mut rec_pre_ms = 0.0;
        let mut rec_infer_ms = 0.0;
        let mut rec_post_ms = 0.0;
        let mut grouped = HashMap::<PpocrScript, Vec<usize>>::new();
        for (idx, chunk) in rec_chunks.iter().enumerate() {
            grouped.entry(scripts[chunk.owner]).or_default().push(idx);
        }
        for (script, mut indices) in grouped {
            let recognizer = self.recognizer(script)?;
            indices.sort_unstable_by_key(|&idx| {
                let (w, h) = rec_chunks[idx].image.dimensions();
                let sw = ((w as f32 * REC_TARGET_HEIGHT as f32 / h as f32)
                    .round()
                    .max(1.0)) as usize;
                (sw + REC_WIDTH_BUCKET - 1) / REC_WIDTH_BUCKET * REC_WIDTH_BUCKET
            });
            let timings_us = (AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0));
            let t_rec_wall = Instant::now();
            let group_results: Vec<Result<RecResult, TranslatorError>> =
                recognizer.pool.install(|| {
                    indices
                        .par_iter()
                        .map(|&idx| {
                            let worker_idx = rayon::current_thread_index().unwrap_or(0)
                                % recognizer.sessions.len();
                            recognizer.recognize_one(
                                &rec_chunks[idx].image,
                                worker_idx,
                                &timings_us,
                            )
                        })
                        .collect()
                });
            rec_wall_ms += t_rec_wall.elapsed().as_secs_f32() * 1000.0;
            rec_pre_ms += timings_us.0.load(Ordering::Relaxed) as f32 / 1000.0;
            rec_infer_ms += timings_us.1.load(Ordering::Relaxed) as f32 / 1000.0;
            rec_post_ms += timings_us.2.load(Ordering::Relaxed) as f32 / 1000.0;
            for (idx, result) in indices.into_iter().zip(group_results.into_iter()) {
                results[idx] = Some(result?);
            }
        }

        let mut lines = Vec::with_capacity(boxes.len());
        let mut empty_count = 0usize;
        let mut low_score_count = 0usize;
        for (idx, chunk_indices) in owner_chunks.iter().enumerate() {
            let mut text = String::new();
            let mut raw_text = String::new();
            let mut confidence_sum = 0.0f32;
            let mut confidence_count = 0usize;
            let mut contrib_chunks: Vec<usize> = Vec::new();
            for &chunk_idx in chunk_indices {
                let r = results[chunk_idx]
                    .as_ref()
                    .expect("all routed ppocr recognition results populated");
                let chunk_raw = r.text.trim();
                if chunk_raw.is_empty() {
                    continue;
                }
                if !raw_text.is_empty() {
                    raw_text.push_str(rec_chunks[chunk_idx].join_before);
                }
                raw_text.push_str(chunk_raw);

                let chunk_text = normalize_rec_text_for_script(scripts[idx], chunk_raw.to_owned());
                if chunk_text.is_empty() {
                    continue;
                }
                if !text.is_empty() {
                    text.push_str(rec_chunks[chunk_idx].join_before);
                }
                text.push_str(&chunk_text);
                confidence_sum += r.confidence;
                confidence_count += 1;
                contrib_chunks.push(chunk_idx);
            }
            // RTL recognizers emit the whole strip in visual (left-to-right) order, so
            // the assembled line — characters and word order both — is the reversal of
            // logical order. Recover logical order once the full line is joined.
            let text = if scripts[idx].is_rtl() {
                reverse_visual_to_logical(&text)
            } else {
                text
            };
            let raw_confidence = if confidence_count > 0 {
                confidence_sum / confidence_count as f32
            } else {
                0.0
            };
            let (text, confidence, status) = if text.trim().is_empty() {
                empty_count += 1;
                (String::new(), 0.0, "empty")
            } else if raw_confidence < thresholds.rec_drop_score {
                low_score_count += 1;
                (String::new(), 0.0, "low_score")
            } else {
                (text, raw_confidence, "accepted")
            };
            log::debug!(
                "ppocr rec strip={} script={} det_score={:.3} width={} height={} area={} chunks={} conf={:.3} status={} text=\"{}\"",
                idx,
                scripts[idx].as_slug(),
                boxes[idx].score,
                boxes[idx].rect.width(),
                boxes[idx].rect.height(),
                boxes[idx]
                    .rect
                    .width()
                    .saturating_mul(boxes[idx].rect.height()),
                chunk_indices.len(),
                raw_confidence,
                status,
                log_text_preview(&raw_text),
            );
            // Per-word bold needs CTC firing positions aligned to the final text. Only safe
            // for a single contributing chunk (multi-chunk fractions are chunk-local) on a
            // non-RTL line (RTL reverses visual→logical, breaking the firing order). Edge
            // whitespace trimmed from `text` is fine: it never forms a word unit.
            // Visual-order firings (1:1 with the visually-ordered glyphs) drive the selectable
            // source-word geometry and stay populated for RTL. `firings` (logical order, for style
            // pooling) drops RTL, where the visual→logical reversal breaks the 1:1 alignment.
            let visual_firings: Vec<translator_core::ocr::CharFiring> = if text.is_empty() {
                Vec::new()
            } else {
                stitch_chunk_firings(&contrib_chunks, &rec_chunks, &results)
            };
            let firings: Vec<translator_core::ocr::CharFiring> = if scripts[idx].is_rtl() {
                Vec::new()
            } else {
                visual_firings.clone()
            };
            lines.push(translator_core::ocr::RecognizedTextLine {
                rect: boxes[idx].rect,
                oriented_box: boxes[idx].oriented_box,
                text,
                confidence,
                source_code: None,
                firings,
                visual_firings,
            });
        }
        log::info!(
            "ppocr rec: src={}x{} boxes={} dewarped={}/{} mean_crop={:.0}x{:.0} \
             wide_boxes={} chunks={} splits={} mean_chunk={:.0}x{:.0} max_rec_w={} \
             accepted={} empty={} low_score={} (drop {:.2}) — \
             crops/dewarp={:.1}ms \
             rec_wall={:.1}ms (cpu pre={:.1}ms infer={:.1}ms post={:.1}ms)",
            width,
            height,
            boxes.len(),
            dewarp_count,
            boxes.len(),
            mean_crop_w,
            mean_crop_h,
            wide_count,
            rec_chunks.len(),
            split_count,
            mean_chunk_w,
            mean_chunk_h,
            REC_WHITESPACE_SPLIT_MAX_WIDTH,
            lines.iter().filter(|l| !l.text.is_empty()).count(),
            empty_count,
            low_score_count,
            thresholds.rec_drop_score,
            crops_ms,
            rec_wall_ms,
            rec_pre_ms,
            rec_infer_ms,
            rec_post_ms,
        );
        Ok(lines)
    }
}

fn log_text_preview(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().flat_map(char::escape_default).take(80) {
        out.push(ch);
    }
    out
}

fn normalize_rec_text_for_script(script: PpocrScript, text: String) -> String {
    match script {
        PpocrScript::Cyrillic | PpocrScript::Eslav => {
            translator_core::script_normalize::repair_cyrillic_word_mixing(&text)
        }
        _ => text,
    }
}

/// Convert an RTL recognizer's visual-order line back to logical order (PaddleOCR's
/// `pred_reverse`). Runs of LTR characters — Latin letters, digits, spaces and the
/// common shared punctuation — stay forward; every other character is its own unit.
/// Reversing the sequence of units flips the RTL script (and overall word order) while
/// keeping embedded Latin words and numbers readable.
pub(crate) fn reverse_visual_to_logical(text: &str) -> String {
    fn is_ltr_run_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, ' ' | ':' | '*' | '.' | '/' | '%' | '+' | '-')
    }
    let mut units: Vec<String> = Vec::new();
    let mut ltr_run = String::new();
    for c in text.chars() {
        if is_ltr_run_char(c) {
            ltr_run.push(c);
            continue;
        }
        if !ltr_run.is_empty() {
            units.push(std::mem::take(&mut ltr_run));
        }
        units.push(c.to_string());
    }
    if !ltr_run.is_empty() {
        units.push(ltr_run);
    }
    units.into_iter().rev().collect()
}

#[cfg(test)]
mod rtl_reverse_tests {
    use super::reverse_visual_to_logical;

    #[test]
    fn pure_hebrew_word_reverses() {
        // Camera-validated case: the model emits the visual order "למשח"; the logical
        // (correct) spelling is "חשמל".
        assert_eq!(reverse_visual_to_logical("למשח"), "חשמל");
    }

    #[test]
    fn multi_word_reverses_word_order_and_keeps_maqaf() {
        assert_eq!(reverse_visual_to_logical("הל־תיב"), "בית־לה");
        assert_eq!(reverse_visual_to_logical("דג בא"), "אב גד");
    }

    #[test]
    fn embedded_latin_and_digits_stay_forward() {
        // "USB2" sits inside a Hebrew line; the Latin/number run must not be flipped.
        assert_eq!(reverse_visual_to_logical("בא USB2 גד"), "דג USB2 אב");
    }
}

fn pulc_strip_eligible(
    box_: &translator_core::ocr::DetectedTextBox,
    crop: &DynamicImage,
    image_area: f32,
) -> bool {
    let box_area = box_.rect.width().saturating_mul(box_.rect.height());
    let crop_area = crop.width().saturating_mul(crop.height());
    let box_area_ratio = box_area as f32 / image_area.max(1.0);
    box_.rect.width() >= PULC_MIN_STRIP_WIDTH
        && box_.rect.height() >= PULC_MIN_STRIP_HEIGHT
        && box_area >= PULC_MIN_STRIP_AREA
        && box_area_ratio >= PULC_MIN_IMAGE_AREA_RATIO
        && crop.width() >= PULC_MIN_STRIP_WIDTH
        && crop.height() >= PULC_MIN_STRIP_HEIGHT
        && crop_area >= PULC_MIN_STRIP_AREA
}

fn crop_dynamic(image: &DynamicImage, rect: &PpocrRect) -> DynamicImage {
    let w = rect.width().max(1);
    let h = rect.height().max(1);
    image.crop_imm(rect.left, rect.top, w, h)
}

/// Crop one text strip per box, dewarping along the PCA principal axis when a
/// contour is available and otherwise falling back to an AABB cutout.
///
/// When `canonical_quadrant` is supplied, `dewarp_contour_to_strip` aligns
/// the strip's +x to the canonical reading direction (using a dot-product
/// sign check against the PCA principal axis) — deterministic per strip,
/// no per-frame classifier inference. AABB fallback strips are rotated by
/// `R0.sub(canonical)` so the camera-frame reading direction becomes the
/// strip's reading direction.
pub(crate) fn crop_text_strips(
    image: &DynamicImage,
    boxes: &[translator_core::ocr::DetectedTextBox],
    canonical_quadrant: Option<translator_core::coords::Quadrant>,
    thickness_pad: f32,
) -> (Vec<DynamicImage>, usize) {
    let mut dewarp_count = 0usize;
    let rgb = image.to_rgb8();
    let crops = boxes
        .iter()
        .map(|b| {
            let contour_pairs: Option<Vec<(f32, f32)>> =
                if !b.contour.is_empty() && b.contour.len() % 2 == 0 {
                    Some(b.contour.chunks_exact(2).map(|c| (c[0], c[1])).collect())
                } else {
                    None
                };
            let dewarped = contour_pairs
                .as_deref()
                .and_then(|c| {
                    dewarp_contour_to_strip_rgb(&rgb, c, canonical_quadrant, thickness_pad)
                })
                .map(DynamicImage::ImageRgb8);
            if let Some(dewarped) = dewarped {
                dewarp_count += 1;
                dewarped
            } else {
                let rect = PpocrRect {
                    left: b.rect.left,
                    top: b.rect.top,
                    right: b.rect.right,
                    bottom: b.rect.bottom,
                };
                let aabb = crop_dynamic(image, &rect);
                match canonical_quadrant {
                    // `rotate_strip_ccw(_, q)` subtracts `q` from
                    // the source's angle convention. The source's
                    // reading-direction angle is `canon`; we want
                    // the dst's reading-direction angle to be R0
                    // (= +x, what the recognizer expects). Rotation
                    // amount = canon, not `R0.sub(canon)`. Earlier
                    // sign was invisible while canon was almost
                    // always R0 in display frame; surfaced once
                    // canonical moved to sensor frame and R90/R270
                    // became the common cases.
                    Some(canon) => rotate_strip_ccw(aabb, canon),
                    None => aabb,
                }
            }
        })
        .collect();
    (crops, dewarp_count)
}

fn rotate_strip_ccw(image: DynamicImage, by: translator_core::coords::Quadrant) -> DynamicImage {
    use translator_core::coords::Quadrant;
    match by {
        Quadrant::R0 => image,
        // image crate's rotate90/180/270 are clockwise. CCW 90° == CW 270°.
        Quadrant::R90 => image.rotate270(),
        Quadrant::R180 => image.rotate180(),
        Quadrant::R270 => image.rotate90(),
    }
}

fn resized_rec_width(crop: &DynamicImage) -> u32 {
    let (w, h) = crop.dimensions();
    if w == 0 || h == 0 {
        return 0;
    }
    ((w as f32 * REC_TARGET_HEIGHT as f32) / h as f32).ceil() as u32
}

fn split_crop_on_whitespace_for_rec_width(
    crop: &DynamicImage,
    max_resized_width: u32,
) -> Vec<SplitCrop> {
    let (w, h) = crop.dimensions();
    if w == 0 || h == 0 || max_resized_width == 0 {
        return vec![SplitCrop {
            image: crop.clone(),
            join_before: "",
            src_x0: 0,
            src_x1: w,
        }];
    }
    let resized_w = resized_rec_width(crop);
    if resized_w <= max_resized_width {
        return vec![SplitCrop {
            image: crop.clone(),
            join_before: "",
            src_x0: 0,
            src_x1: w,
        }];
    }

    let max_crop_width = ((max_resized_width as f32 * h as f32) / REC_TARGET_HEIGHT as f32)
        .floor()
        .max(1.0) as usize;
    if w as usize <= max_crop_width {
        return vec![SplitCrop {
            image: crop.clone(),
            join_before: "",
            src_x0: 0,
            src_x1: w,
        }];
    }

    let gray = crop.to_luma8();
    let valleys = whitespace_valleys(&gray);
    if valleys.is_empty() {
        return vec![SplitCrop {
            image: crop.clone(),
            join_before: "",
            src_x0: 0,
            src_x1: w,
        }];
    }

    let mut cuts = Vec::new();
    collect_whitespace_cuts(0, w as usize, max_crop_width, &valleys, &mut cuts);
    cuts.sort_unstable();
    cuts.dedup();
    if cuts.is_empty() {
        return vec![SplitCrop {
            image: crop.clone(),
            join_before: "",
            src_x0: 0,
            src_x1: w,
        }];
    }

    let mut chunks = Vec::with_capacity(cuts.len() + 1);
    let mut prev_x = 0u32;
    let mut first = true;
    for cut in cuts {
        let x = (cut as u32).clamp(prev_x + 1, w.saturating_sub(1));
        chunks.push(SplitCrop {
            image: crop.crop_imm(prev_x, 0, x - prev_x, h),
            join_before: if first { "" } else { " " },
            src_x0: prev_x,
            src_x1: x,
        });
        first = false;
        prev_x = x;
    }
    if prev_x < w {
        chunks.push(SplitCrop {
            image: crop.crop_imm(prev_x, 0, w - prev_x, h),
            join_before: if first { "" } else { " " },
            src_x0: prev_x,
            src_x1: w,
        });
    }
    chunks
}

fn collect_whitespace_cuts(
    start: usize,
    end: usize,
    max_width: usize,
    valleys: &[(usize, usize)],
    cuts: &mut Vec<usize>,
) {
    let width = end.saturating_sub(start);
    if width <= max_width {
        return;
    }
    let min_piece = (max_width / 4).max(24).min(max_width.saturating_sub(1));
    if width < min_piece * 2 {
        return;
    }

    let target = if width <= max_width * 2 {
        start + width / 2
    } else {
        start + max_width
    };
    let search_limit = max_width / 2;
    let search_end = (target + search_limit).min(end.saturating_sub(min_piece));
    let search_start = (start + min_piece).max(target.saturating_sub(search_limit));

    let right = find_valley_cut(valleys, target, search_end, target, end);
    let left = find_valley_cut(valleys, search_start, target, target, end);
    let cut = match (right, left) {
        (Some(r), Some(l)) => {
            if r.abs_diff(target) <= l.abs_diff(target) {
                r
            } else {
                l
            }
        }
        (Some(r), None) => r,
        (None, Some(l)) => l,
        (None, None) => return,
    };
    if cut <= start + min_piece || cut + min_piece >= end {
        return;
    }
    cuts.push(cut);
    collect_whitespace_cuts(start, cut, max_width, valleys, cuts);
    collect_whitespace_cuts(cut, end, max_width, valleys, cuts);
}

fn find_valley_cut(
    valleys: &[(usize, usize)],
    lo: usize,
    hi: usize,
    target: usize,
    segment_end: usize,
) -> Option<usize> {
    let mut best = None;
    let mut best_dist = usize::MAX;
    for &(a, b) in valleys {
        if b <= lo || a >= hi {
            continue;
        }
        let cut = (a + b) / 2;
        if cut <= lo || cut >= hi || cut >= segment_end {
            continue;
        }
        let dist = cut.abs_diff(target);
        if dist < best_dist {
            best = Some(cut);
            best_dist = dist;
        }
    }
    best
}

fn whitespace_valleys(gray: &GrayImage) -> Vec<(usize, usize)> {
    let (w, h) = gray.dimensions();
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut hist = [0usize; 256];
    for pixel in gray.pixels() {
        hist[pixel[0] as usize] += 1;
    }
    let total = (w * h) as usize;
    let p10 = percentile_from_hist(&hist, total, 10);
    let bg = percentile_from_hist(&hist, total, 50);
    let p90 = percentile_from_hist(&hist, total, 90);
    let contrast = p90.saturating_sub(p10).max(p10.saturating_sub(p90));
    if contrast < 18 {
        return Vec::new();
    }
    let threshold = (contrast / 4).max(12);
    let mut ink = vec![0u16; w as usize];
    for x in 0..w {
        let mut count = 0u16;
        for y in 0..h {
            let v = gray.get_pixel(x, y)[0];
            if v.abs_diff(bg) >= threshold {
                count += 1;
            }
        }
        ink[x as usize] = count;
    }

    let max_ink = ((h as f32) * 0.04).ceil().max(1.0) as u16;
    let min_space_w = ((h as f32) * 0.22).ceil().max(2.0) as usize;
    let mut valleys = Vec::new();
    let mut start = None;
    for x in 0..ink.len() {
        let a = x.saturating_sub(1);
        let b = (x + 2).min(ink.len());
        let smoothed = ink[a..b].iter().copied().map(u32::from).sum::<u32>() / (b - a) as u32;
        if smoothed <= u32::from(max_ink) {
            start.get_or_insert(x);
        } else if let Some(s) = start.take() {
            if x.saturating_sub(s) >= min_space_w {
                valleys.push((s, x));
            }
        }
    }
    if let Some(s) = start {
        if ink.len().saturating_sub(s) >= min_space_w {
            valleys.push((s, ink.len()));
        }
    }
    valleys
}

fn percentile_from_hist(hist: &[usize; 256], total: usize, percentile: usize) -> u8 {
    let target = total.saturating_mul(percentile).div_ceil(100);
    let mut seen = 0usize;
    for (value, count) in hist.iter().enumerate() {
        seen += count;
        if seen >= target {
            return value as u8;
        }
    }
    255
}

fn expand_box(rect: &PpocrRect, border: u32, max_w: u32, max_h: u32) -> PpocrRect {
    PpocrRect {
        left: rect.left.saturating_sub(border),
        top: rect.top.saturating_sub(border),
        right: (rect.right + border).min(max_w),
        bottom: (rect.bottom + border).min(max_h),
    }
}

// ---------- Detector ----------

#[derive(Debug, Clone)]
struct DetBox {
    rect: PpocrRect,
    contour: Option<Vec<(f32, f32)>>,
    score: f32,
}

impl PpocrDetector {
    fn load(model_path: &Path, intra_threads: usize) -> Result<Self, TranslatorError> {
        let session = MnnSession::load(model_path, intra_threads)?;
        let probe = vec![0.0f32; 3 * 64 * 64];
        let (_, out_shape) = session.run(&probe, &[1, 3, 64, 64])?;
        if out_shape.len() < 4 {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!("ppocr det probe output shape unexpected: {:?}", out_shape),
            ));
        }
        let output_stride = 64.0 / out_shape[out_shape.len() - 1] as f32;
        Ok(Self {
            session,
            output_stride,
        })
    }

    /// How far the binarised text band shrinks per side (in model-input px)
    /// when the probability map is emitted below input resolution: the folded
    /// low-res heads average-pool the logit map, which pulls the threshold
    /// crossing inward by up to a mask pixel. Zero at native resolution.
    fn pool_comp(&self) -> f32 {
        (self.output_stride - 1.0).max(0.0)
    }

    fn detect_with_thresholds(
        &self,
        image: &DynamicImage,
        thresholds: PpocrThresholds,
    ) -> Result<Vec<DetBox>, TranslatorError> {
        let (orig_w, orig_h) = image.dimensions();
        let t_pre = Instant::now();
        let scaled = resize_to_det_aligned(image, DET_MAX_SIDE);
        let (content_w, content_h) = scaled.dimensions();
        // DBNet requires multiple-of-32 dims; pad up with zeros (preprocess fills
        // the content into a larger zeroed buffer) instead of resampling.
        let scaled_w = content_w.div_ceil(32) * 32;
        let scaled_h = content_h.div_ceil(32) * 32;
        let tensor_buf = preprocess_for_det(&scaled, scaled_w, scaled_h);
        let pre_ms = t_pre.elapsed().as_secs_f32() * 1000.0;

        let t_infer = Instant::now();
        let (mask, out_shape) = self
            .session
            .run(&tensor_buf, &[1, 3, scaled_h as usize, scaled_w as usize])?;
        let infer_ms = t_infer.elapsed().as_secs_f32() * 1000.0;

        if out_shape.len() < 4 {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!("ppocr det output shape unexpected: {:?}", out_shape),
            ));
        }
        let out_h = out_shape[out_shape.len() - 2] as u32;
        let out_w = out_shape[out_shape.len() - 1] as u32;
        let mut mask_min = f32::INFINITY;
        let mut mask_max = f32::NEG_INFINITY;
        let mut mask_sum = 0.0f32;
        let mut over_thresh = 0usize;
        for &v in &mask {
            if v < mask_min {
                mask_min = v;
            }
            if v > mask_max {
                mask_max = v;
            }
            mask_sum += v;
            if v > thresholds.det_score_threshold {
                over_thresh += 1;
            }
        }
        let mask_mean = mask_sum / mask.len() as f32;
        let t_post = Instant::now();
        let binary: Vec<u8> = mask
            .iter()
            .map(|&v| {
                if v > thresholds.det_score_threshold {
                    255
                } else {
                    0
                }
            })
            .collect();
        // The mask may come back at a lower resolution than the model input
        // (heads that emit the prob map at 1/2 or 1/4 res). extract_boxes
        // works in mask-grid units, so express the content region and the
        // area gate on that grid.
        let stride_x = scaled_w as f32 / out_w as f32;
        let stride_y = scaled_h as f32 / out_h as f32;
        let content_mask_w = (content_w as f32 / stride_x).round() as u32;
        let content_mask_h = (content_h as f32 / stride_y).round() as u32;
        let mask_thresholds = PpocrThresholds {
            det_min_area: ((thresholds.det_min_area as f32 / (stride_x * stride_y)).round() as u32)
                .max(1),
            ..thresholds
        };
        let boxes = extract_boxes(
            &binary,
            &mask,
            out_w,
            out_h,
            content_mask_w,
            content_mask_h,
            orig_w,
            orig_h,
            (stride_x * stride_y).sqrt(),
            mask_thresholds,
        );
        let post_ms = t_post.elapsed().as_secs_f32() * 1000.0;
        log::info!(
            "ppocr det: det={}x{} recv={}x{} out={}x{} mask[min/max/mean]={:.3}/{:.3}/{:.3} over_{}={}/{} — pre={:.1}ms infer={:.1}ms post={:.1}ms",
            scaled_w,
            scaled_h,
            orig_w,
            orig_h,
            out_w,
            out_h,
            mask_min,
            mask_max,
            mask_mean,
            thresholds.det_score_threshold,
            over_thresh,
            mask.len(),
            pre_ms,
            infer_ms,
            post_ms,
        );
        Ok(boxes)
    }

    fn probability_map(&self, image: &DynamicImage) -> Result<GrayImage, TranslatorError> {
        let (orig_w, orig_h) = image.dimensions();
        let scaled = resize_to_det_aligned(image, DET_MAX_SIDE);
        let (content_w, content_h) = scaled.dimensions();
        // DBNet requires multiple-of-32 dims; pad up with zeros (preprocess fills
        // the content into a larger zeroed buffer) instead of resampling.
        let scaled_w = content_w.div_ceil(32) * 32;
        let scaled_h = content_h.div_ceil(32) * 32;
        let tensor_buf = preprocess_for_det(&scaled, scaled_w, scaled_h);
        let (mask, out_shape) = self
            .session
            .run(&tensor_buf, &[1, 3, scaled_h as usize, scaled_w as usize])?;
        if out_shape.len() < 4 {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!("ppocr det output shape unexpected: {:?}", out_shape),
            ));
        }
        let out_h = out_shape[out_shape.len() - 2] as u32;
        let out_w = out_shape[out_shape.len() - 1] as u32;
        // The output map covers the zero-padded multiple-of-32 canvas; keep
        // only the content region before resampling, otherwise the padding
        // margin compresses the map and positions drift toward the origin.
        let stride_x = scaled_w as f32 / out_w as f32;
        let stride_y = scaled_h as f32 / out_h as f32;
        let crop_w = ((content_w as f32 / stride_x).round() as u32).min(out_w);
        let crop_h = ((content_h as f32 / stride_y).round() as u32).min(out_h);
        let mut buf = vec![0u8; (crop_w * crop_h) as usize];
        for y in 0..crop_h {
            for x in 0..crop_w {
                let v = mask[(y * out_w + x) as usize].clamp(0.0, 1.0);
                buf[(y * crop_w + x) as usize] = (v * 255.0).round() as u8;
            }
        }
        let content = GrayImage::from_raw(crop_w, crop_h, buf).ok_or_else(|| {
            TranslatorError::new(
                TranslatorErrorKind::Internal,
                "failed to build heatmap image",
            )
        })?;
        Ok(DynamicImage::ImageLuma8(content)
            .resize_exact(orig_w, orig_h, FilterType::Triangle)
            .to_luma8())
    }
}

impl PpocrScriptClassifier {
    fn load(model_path: &Path) -> Result<Self, TranslatorError> {
        let session = MnnSession::load(model_path, 4)?;
        Ok(Self { session })
    }

    fn classify_many(
        &self,
        crops: &[DynamicImage],
    ) -> Result<Vec<Option<PpocrScriptCandidate>>, TranslatorError> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let input = preprocess_for_pulc(crops);
        let (out, out_shape) = self.session.run(
            &input,
            &[crops.len(), 3, PULC_HEIGHT as usize, PULC_WIDTH as usize],
        )?;
        if out_shape.len() != 2 || out_shape[0] != crops.len() || out_shape[1] != PULC_CLASSES.len()
        {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!(
                    "ppocr script classifier output shape unexpected: {:?}",
                    out_shape
                ),
            ));
        }
        Ok(out
            .chunks_exact(PULC_CLASSES.len())
            .map(top_pulc_candidate)
            .collect())
    }
}

impl PpocrTextlineOrientationClassifier {
    fn load(model_path: &Path) -> Result<Self, TranslatorError> {
        let session = MnnSession::load(model_path, 4)?;
        Ok(Self { session })
    }

    fn classify_many(
        &self,
        crops: &[DynamicImage],
    ) -> Result<Vec<Option<TextlineOriCandidate>>, TranslatorError> {
        if crops.is_empty() {
            return Ok(Vec::new());
        }
        let input = preprocess_for_textline_ori(crops);
        let (out, out_shape) = self.session.run(
            &input,
            &[
                crops.len(),
                3,
                TEXTLINE_ORI_HEIGHT as usize,
                TEXTLINE_ORI_WIDTH as usize,
            ],
        )?;
        if out_shape.len() != 2
            || out_shape[0] != crops.len()
            || out_shape[1] != TEXTLINE_ORI_CLASSES
        {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!(
                    "ppocr textline orientation classifier output shape unexpected: {:?}",
                    out_shape
                ),
            ));
        }
        Ok(out
            .chunks_exact(TEXTLINE_ORI_CLASSES)
            .map(top_textline_ori_candidate)
            .collect())
    }
}

fn preprocess_for_textline_ori(crops: &[DynamicImage]) -> Vec<f32> {
    let plane = (TEXTLINE_ORI_HEIGHT as usize) * (TEXTLINE_ORI_WIDTH as usize);
    let mut buf = vec![0.0f32; crops.len() * 3 * plane];
    for (batch, crop) in crops.iter().enumerate() {
        let resized = crop.resize_exact(
            TEXTLINE_ORI_WIDTH,
            TEXTLINE_ORI_HEIGHT,
            FilterType::Triangle,
        );
        let rgb = resized.to_rgb8();
        let batch_base = batch * 3 * plane;
        for y in 0..TEXTLINE_ORI_HEIGHT as usize {
            for x in 0..TEXTLINE_ORI_WIDTH as usize {
                let pixel = rgb.get_pixel(x as u32, y as u32);
                let idx = y * TEXTLINE_ORI_WIDTH as usize + x;
                buf[batch_base + idx] =
                    (pixel[0] as f32 / 255.0 - TEXTLINE_ORI_MEAN[0]) / TEXTLINE_ORI_STD[0];
                buf[batch_base + plane + idx] =
                    (pixel[1] as f32 / 255.0 - TEXTLINE_ORI_MEAN[1]) / TEXTLINE_ORI_STD[1];
                buf[batch_base + 2 * plane + idx] =
                    (pixel[2] as f32 / 255.0 - TEXTLINE_ORI_MEAN[2]) / TEXTLINE_ORI_STD[2];
            }
        }
    }
    buf
}

fn top_textline_ori_candidate(row: &[f32]) -> Option<TextlineOriCandidate> {
    let probs = if row.iter().all(|v| v.is_finite()) {
        let sum = row.iter().sum::<f32>();
        if (0.95..=1.05).contains(&sum) {
            row.to_vec()
        } else {
            softmax(row)
        }
    } else {
        return None;
    };
    let (idx, score) = probs
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())?;
    let label = match idx {
        0 => TextlineOriLabel::Up,
        1 => TextlineOriLabel::Flipped180,
        _ => return None,
    };
    Some(TextlineOriCandidate { label, score })
}

fn resize_to_det_aligned(image: &DynamicImage, max_side: u32) -> DynamicImage {
    let (w, h) = image.dimensions();
    let max_dim = w.max(h);
    if max_dim <= max_side {
        // No downscale needed: skip the resample entirely. The caller pads the
        // (possibly unaligned) dimensions up to a multiple of 32 with zeros,
        // which is a strided copy rather than a full Triangle resize.
        return image.clone();
    }
    let scale = max_side as f32 / max_dim as f32;
    let nw = ((w as f32 * scale) as u32 / 32).max(1) * 32;
    let nh = ((h as f32 * scale) as u32 / 32).max(1) * 32;
    image.resize_exact(nw, nh, FilterType::Triangle)
}

fn preprocess_for_det(image: &DynamicImage, pad_w: u32, pad_h: u32) -> Vec<f32> {
    let plane = (pad_w as usize) * (pad_h as usize);
    let mut buf = vec![0.0f32; 3 * plane];
    let pad_w_u = pad_w as usize;
    // Per-channel u8 -> normalized-f32 lookup tables: the input is u8, so the
    // `(v/255 - mean)/std` normalization has only 256 possible outputs per
    // channel. Precompute them once and replace the per-pixel float math (and
    // bounds-checked `get_pixel`) with a table read over the raw buffer.
    let make_lut = |c: usize| {
        let mut t = [0.0f32; 256];
        for (v, e) in t.iter_mut().enumerate() {
            *e = (v as f32 / 255.0 - PPOCR_DET_MEAN[c]) / PPOCR_DET_STD[c];
        }
        t
    };
    let (lut_r, lut_g, lut_b) = (make_lut(0), make_lut(1), make_lut(2));

    // Fast path for a gray detector input (the GPU split path renders luma
    // straight at the aligned size): read the single channel and write the same
    // value to all three, skipping the `to_rgb8()` channel-replication alloc.
    if let DynamicImage::ImageLuma8(luma) = image {
        let (w, h) = luma.dimensions();
        let (w_u, raw) = (w as usize, luma.as_raw());
        for y in 0..h as usize {
            let row = &raw[y * w_u..y * w_u + w_u];
            let base = y * pad_w_u;
            for (x, &v) in row.iter().enumerate() {
                let idx = base + x;
                buf[idx] = lut_r[v as usize];
                buf[plane + idx] = lut_g[v as usize];
                buf[2 * plane + idx] = lut_b[v as usize];
            }
        }
        return buf;
    }
    let rgb = image.to_rgb8();
    let (w, h) = rgb.dimensions();
    let (w_u, raw) = (w as usize, rgb.as_raw());
    for y in 0..h as usize {
        let row = &raw[y * w_u * 3..(y * w_u + w_u) * 3];
        let base = y * pad_w_u;
        for (x, px) in row.chunks_exact(3).enumerate() {
            let idx = base + x;
            buf[idx] = lut_r[px[0] as usize];
            buf[plane + idx] = lut_g[px[1] as usize];
            buf[2 * plane + idx] = lut_b[px[2] as usize];
        }
    }
    buf
}

fn preprocess_for_pulc(crops: &[DynamicImage]) -> Vec<f32> {
    let plane = (PULC_HEIGHT as usize) * (PULC_WIDTH as usize);
    let mut buf = vec![0.0f32; crops.len() * 3 * plane];
    for (batch, crop) in crops.iter().enumerate() {
        let resized = crop.resize_exact(PULC_WIDTH, PULC_HEIGHT, FilterType::Triangle);
        let rgb = resized.to_rgb8();
        let batch_base = batch * 3 * plane;
        for y in 0..PULC_HEIGHT as usize {
            for x in 0..PULC_WIDTH as usize {
                let pixel = rgb.get_pixel(x as u32, y as u32);
                let idx = y * PULC_WIDTH as usize + x;
                buf[batch_base + idx] =
                    (pixel[0] as f32 / 255.0 - PPOCR_DET_MEAN[0]) / PPOCR_DET_STD[0];
                buf[batch_base + plane + idx] =
                    (pixel[1] as f32 / 255.0 - PPOCR_DET_MEAN[1]) / PPOCR_DET_STD[1];
                buf[batch_base + 2 * plane + idx] =
                    (pixel[2] as f32 / 255.0 - PPOCR_DET_MEAN[2]) / PPOCR_DET_STD[2];
            }
        }
    }
    buf
}

fn top_pulc_candidate(row: &[f32]) -> Option<PpocrScriptCandidate> {
    let probs = if row.iter().all(|v| v.is_finite()) {
        let sum = row.iter().sum::<f32>();
        if (0.95..=1.05).contains(&sum) {
            row.to_vec()
        } else {
            softmax(row)
        }
    } else {
        return None;
    };
    let (idx, score) = probs
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())?;
    Some(PpocrScriptCandidate {
        class: PULC_CLASSES[idx],
        score,
    })
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps = logits.iter().map(|v| (v - max).exp()).collect::<Vec<_>>();
    let sum = exps.iter().sum::<f32>();
    exps.into_iter().map(|v| v / sum).collect()
}

fn extract_boxes(
    mask: &[u8],
    heatmap: &[f32],
    mask_w: u32,
    mask_h: u32,
    content_w: u32,
    content_h: u32,
    orig_w: u32,
    orig_h: u32,
    mask_stride: f32,
    thresholds: PpocrThresholds,
) -> Vec<DetBox> {
    let Some(gray) = GrayImage::from_raw(mask_w, mask_h, mask.to_vec()) else {
        return Vec::new();
    };
    let contours = find_contours::<i32>(&gray);
    // Map mask coordinates back to the original image. The mask shares the model-input grid,
    // where the resized content sits top-left inside a buffer padded up to a multiple of 32. The
    // padding carries no text, so the content region — not the padded buffer — is what maps onto
    // the original: scaling by `orig / content`. Using the padded height here instead would shrink
    // every coordinate by `content / padded`, dragging detections upward by an amount that grows
    // with y (a fraction of a line at the top of the page, ~half a line at the bottom).
    let scale_x = orig_w as f32 / content_w as f32;
    let scale_y = orig_h as f32 / content_h as f32;
    let mut boxes = Vec::new();
    let mut weak_score_count = 0usize;

    for contour in contours {
        if contour.parent.is_some() || contour.points.len() < 4 {
            continue;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for p in &contour.points {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        if min_x >= content_w as i32 || min_y >= content_h as i32 {
            continue;
        }
        let min_x = min_x.max(0);
        let min_y = min_y.max(0);
        let max_x = max_x.min(content_w as i32);
        let max_y = max_y.min(content_h as i32);
        let box_w = (max_x - min_x) as u32;
        let box_h = (max_y - min_y) as u32;
        if box_w * box_h < thresholds.det_min_area {
            continue;
        }

        // Box-score gate (PaddleOCR's `det_db_box_thresh`): mean heatmap probability over the
        // contour's *binarised interior* on the raw float output. We restrict the average to
        // pixels above `DET_SCORE_THRESHOLD` (i.e. pixels that the binarised mask considers
        // text) rather than the full AABB, because the AABB of a tilted line contains a lot
        // of background corner pixels that would dilute the mean — penalising tilted text
        // unfairly. This is effectively the polygon-interior mode upstream calls "slow",
        // approximated cheaply with the binary mask we already have. Real text masks average
        // 0.7+ here; texture/compression noise that just barely cleared the per-pixel
        // threshold rarely makes it past 0.5.
        let mut score_sum = 0.0f32;
        let mut score_n = 0usize;
        for y in (min_y as u32)..(max_y as u32).min(mask_h) {
            let row = (y as usize) * (mask_w as usize);
            for x in (min_x as u32)..(max_x as u32).min(mask_w) {
                let idx = row + x as usize;
                if mask[idx] != 0 {
                    score_sum += heatmap[idx];
                    score_n += 1;
                }
            }
        }
        let box_score = if score_n > 0 {
            score_sum / score_n as f32
        } else {
            0.0
        };
        if box_score < thresholds.det_box_min_score {
            weak_score_count += 1;
            continue;
        }

        // DB unclip. Upstream uses `distance = area * ratio / perimeter`, which equals
        // `ratio·t/2 · long/(long+t)` for a long×t box — the inflation a box gets relative
        // to its stroke-band thickness `t` then depends on its aspect ratio (full lines get
        // ~0.8·t per side, square single-word boxes only ~0.4·t), so the same font reports
        // different heights depending on how much text sits on the line. Use the long-box
        // limit `ratio·t/2` for every shape instead: identical for the calibrated common
        // case, and short boxes now inflate by the same proportion as their neighbors.
        let thickness = box_w.min(box_h) as f32;
        // Low-res heads average-pool the logit map, which smooths the steep
        // logit edge at a line's boundary and pulls the threshold crossing
        // inward by up to a mask pixel per side. Compensate by roughly
        // (stride - 1) input px per side; zero at native resolution.
        let pool_comp = 1.0 - 1.0 / mask_stride;
        // A mask pixel covers a stride-wide input block; mapping its index to
        // the block's top-left corner biases every coordinate toward the
        // origin by ~(stride - 1) input px. Recenter; zero at stride 1, where
        // the corner convention is what the thresholds were calibrated with.
        let center_off = pool_comp;
        let expand_dist = (DET_UNCLIP_RATIO * thickness / 2.0).max(1.0) + pool_comp;
        let ex_min_x = (min_x as f32 + center_off - expand_dist).max(0.0) as i32;
        let ex_min_y = (min_y as f32 + center_off - expand_dist).max(0.0) as i32;
        let ex_max_x = (max_x as f32 + center_off + expand_dist).min(content_w as f32) as i32;
        let ex_max_y = (max_y as f32 + center_off + expand_dist).min(content_h as f32) as i32;
        let ex_w = (ex_max_x - ex_min_x) as u32;
        let ex_h = (ex_max_y - ex_min_y) as u32;

        let scaled_x = (ex_min_x as f32 * scale_x) as i32;
        let scaled_y = (ex_min_y as f32 * scale_y) as i32;
        let scaled_w = (ex_w as f32 * scale_x) as u32;
        let scaled_h = (ex_h as f32 * scale_y) as u32;
        let final_x = scaled_x.max(0) as u32;
        let final_y = scaled_y.max(0) as u32;
        let final_w = scaled_w.min(orig_w.saturating_sub(final_x));
        let final_h = scaled_h.min(orig_h.saturating_sub(final_y));
        if final_w == 0 || final_h == 0 {
            continue;
        }

        let scaled_contour: Vec<(f32, f32)> = contour
            .points
            .iter()
            .map(|p: &Point<i32>| {
                (
                    (p.x as f32 + center_off) * scale_x,
                    (p.y as f32 + center_off) * scale_y,
                )
            })
            .collect();

        boxes.push(DetBox {
            rect: PpocrRect {
                left: final_x,
                top: final_y,
                right: final_x + final_w,
                bottom: final_y + final_h,
            },
            contour: Some(scaled_contour),
            score: box_score,
        });
    }
    if weak_score_count > 0 {
        log::info!(
            "ppocr det: {} contour(s) below box-score gate {:.2}",
            weak_score_count,
            thresholds.det_box_min_score,
        );
    }
    boxes
}

// ---------- Recognizer ----------

struct RecResult {
    text: String,
    confidence: f32,
    chars: Vec<RecChar>,
}

/// One decoded character with the strip position where its CTC run fired, exposed for
/// debug tooling. `at` is a fraction `0.0..=1.0` along the strip's reading axis — the
/// leading edge of the glyph's CTC run.
pub struct StripChar {
    pub ch: char,
    pub score: f32,
    pub at: f32,
}

/// Position along the recognizer strip's reading axis, in `0.0..=1.0`. This is the only
/// stable scalar for a CTC firing: the strip's pixel width is a resize artefact, and on a
/// tilted line an image-space cut runs perpendicular to the reading axis (two points, not
/// one x). Callers convert a fraction to an image-space quad once, through the line's
/// dewarp transform.
#[derive(Debug, Clone, Copy, PartialEq)]
struct StripFraction(f32);

/// One decoded character with the strip position where its CTC run fired.
struct RecChar {
    ch: char,
    score: f32,
    at: StripFraction,
}

/// Assemble one line's CTC firings from its contributing chunks, staying 1:1 with the line text
/// the same chunks build. Each chunk's chunk-local firing fraction is remapped onto the line's
/// reading axis via its `[frac0, frac1)` span. Leading/trailing whitespace firings are dropped
/// per chunk to mirror the `r.text.trim()` the text assembly applies (the recognizer emits a
/// space class, so it pads line ends), and a whitespace firing is injected at each chunk
/// boundary that contributed a `join_before` space. The per-script text normalize is
/// count-preserving and never touches whitespace, so the result aligns exactly with the joined
/// text. Caller gates out RTL lines (visual→logical reversal breaks firing order).
fn stitch_chunk_firings(
    contrib_chunks: &[usize],
    rec_chunks: &[RecChunk],
    results: &[Option<RecResult>],
) -> Vec<translator_core::ocr::CharFiring> {
    let mut out: Vec<translator_core::ocr::CharFiring> = Vec::new();
    for &ci in contrib_chunks {
        let chunk = &rec_chunks[ci];
        let r = results[ci]
            .as_ref()
            .expect("contributing chunk result populated");
        let first = r.chars.iter().position(|c| !c.ch.is_whitespace());
        let last = r.chars.iter().rposition(|c| !c.ch.is_whitespace());
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        if !out.is_empty() && chunk.join_before == " " {
            out.push(translator_core::ocr::CharFiring {
                ch: ' ' as u32,
                at: chunk.frac0,
            });
        }
        let span = chunk.frac1 - chunk.frac0;
        for c in &r.chars[first..=last] {
            out.push(translator_core::ocr::CharFiring {
                ch: c.ch as u32,
                at: chunk.frac0 + c.at.0 * span,
            });
        }
    }
    out
}

struct RecChunk {
    owner: usize,
    image: DynamicImage,
    join_before: &'static str,
    /// This chunk's column span within the owner crop, as fractions of the crop width. Lets the
    /// per-chunk CTC firing fractions (local to the chunk strip) be remapped to line-global
    /// fractions when stitching a multi-chunk line's firings.
    frac0: f32,
    frac1: f32,
}

struct SplitCrop {
    image: DynamicImage,
    join_before: &'static str,
    /// Source column range `[src_x0, src_x1)` within the crop this was cut from.
    src_x0: u32,
    src_x1: u32,
}

impl PpocrRecognizer {
    fn load(model_path: &Path, keys_path: &Path) -> Result<Self, TranslatorError> {
        let mut sessions = Vec::with_capacity(REC_PARALLELISM);
        for _ in 0..REC_PARALLELISM {
            sessions.push(Mutex::new(MnnSession::load(model_path, 1)?));
        }
        let charset = load_charset(keys_path)?;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(REC_PARALLELISM)
            .thread_name(|i| format!("ppocr-rec-{i}"))
            .build()
            .map_err(|e| {
                TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    format!("failed to build ppocr rec thread pool: {e}"),
                )
            })?;
        Ok(Self {
            sessions,
            charset,
            pool,
        })
    }

    /// Run rec on a single crop using `sessions[session_idx]`. Each call is a batch-of-1 MNN
    /// inference at the crop's actual aspect ratio — no padding waste.
    fn recognize_one(
        &self,
        image: &DynamicImage,
        session_idx: usize,
        timings_us: &(AtomicU64, AtomicU64, AtomicU64),
    ) -> Result<RecResult, TranslatorError> {
        let t_pre = Instant::now();
        let target_h = REC_TARGET_HEIGHT as usize;
        let (orig_w, orig_h) = image.dimensions();
        let scale = REC_TARGET_HEIGHT as f32 / orig_h as f32;
        let sw = ((orig_w as f32 * scale).round().max(1.0)) as u32;

        let resized = image.resize_exact(sw, REC_TARGET_HEIGHT, FilterType::Triangle);
        let rgb = resized.to_rgb8();
        let w_exact = sw as usize;
        let w_us = (w_exact + REC_WIDTH_BUCKET - 1) / REC_WIDTH_BUCKET * REC_WIDTH_BUCKET;
        let mut buf = vec![0.0f32; 3 * target_h * w_us];
        let plane = target_h * w_us;
        let raw = rgb.as_raw();
        for y in 0..target_h {
            let row = &raw[y * w_exact * 3..(y * w_exact + w_exact) * 3];
            let base = y * w_us;
            for (x, px) in row.chunks_exact(3).enumerate() {
                let idx = base + x;
                buf[idx] = REC_NORM_LUT[0][px[0] as usize];
                buf[plane + idx] = REC_NORM_LUT[1][px[1] as usize];
                buf[2 * plane + idx] = REC_NORM_LUT[2][px[2] as usize];
            }
        }
        timings_us
            .0
            .fetch_add(t_pre.elapsed().as_micros() as u64, Ordering::Relaxed);

        let t_infer = Instant::now();
        let (out_data, out_shape) = {
            let session = self.sessions[session_idx]
                .lock()
                .expect("ppocr rec session mutex poisoned");
            session.run(&buf, &[1, 3, target_h, w_us])?
        };
        timings_us
            .1
            .fetch_add(t_infer.elapsed().as_micros() as u64, Ordering::Relaxed);

        if out_shape.len() != 3 || out_shape[0] != 1 {
            return Err(TranslatorError::new(
                TranslatorErrorKind::Internal,
                format!("ppocr rec output shape unexpected: {:?}", out_shape),
            ));
        }
        let t_post = Instant::now();
        let seq_len = out_shape[1];
        let num_classes = out_shape[2];
        // The model ran on the bucket-padded width `w_us`, so `seq_len` spans that, but the
        // strip content occupies only `w_exact`. `content_fraction` rescales firing positions
        // off the padded axis onto the content reading axis.
        let content_fraction = w_exact as f32 / w_us as f32;
        let result = decode_ctc(
            &out_data,
            seq_len,
            num_classes,
            &self.charset,
            content_fraction,
        );
        timings_us
            .2
            .fetch_add(t_post.elapsed().as_micros() as u64, Ordering::Relaxed);
        Ok(result)
    }
}

fn load_charset(path: &Path) -> Result<Vec<char>, TranslatorError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        TranslatorError::new(
            TranslatorErrorKind::Internal,
            format!("failed to read ppocr charset {}: {e}", path.display()),
        )
    })?;
    // PaddleOCR conventions: index 0 is blank, last index is padding/space, so we wrap with
    // a leading blank and a trailing pad character (' ').
    let mut charset = Vec::with_capacity(content.chars().count() + 2);
    charset.push(' ');
    for line in content.lines() {
        if let Some(ch) = line.chars().next() {
            charset.push(ch);
        }
    }
    charset.push(' ');
    if charset.len() < 3 {
        return Err(TranslatorError::new(
            TranslatorErrorKind::Internal,
            format!("ppocr charset {} is too small", path.display()),
        ));
    }
    Ok(charset)
}

fn decode_ctc(
    logits: &[f32],
    seq_len: usize,
    num_classes: usize,
    charset: &[char],
    content_fraction: f32,
) -> RecResult {
    let mut chars: Vec<RecChar> = Vec::new();
    let mut prev_idx = 0usize;
    for t in 0..seq_len {
        let start = t * num_classes;
        let end = start + num_classes;
        let probs = &logits[start..end];
        let mut max_idx = 0usize;
        let mut max_prob = f32::NEG_INFINITY;
        for (i, &p) in probs.iter().enumerate() {
            if p > max_prob {
                max_prob = p;
                max_idx = i;
            }
        }
        if max_idx != 0 && max_idx != prev_idx && max_idx < charset.len() {
            let ch = charset[max_idx];
            let threshold = if PUNCTUATIONS.contains(&ch) {
                REC_PUNCT_MIN_SCORE
            } else {
                REC_MIN_SCORE
            };
            if max_prob >= threshold {
                // (t + 0.5)/seq_len is the leading edge of this glyph's CTC run as a
                // fraction of the padded model input; dividing by content_fraction maps it
                // onto the strip's content reading axis (peaky CTC biases it ~one stride
                // forward). seq_len >= 1 inside this loop, so the division is safe.
                let at =
                    StripFraction(((t as f32 + 0.5) / seq_len as f32 / content_fraction).min(1.0));
                chars.push(RecChar {
                    ch,
                    score: max_prob,
                    at,
                });
            }
        }
        prev_idx = max_idx;
    }
    let confidence = if chars.is_empty() {
        0.0
    } else {
        chars.iter().map(|c| c.score).sum::<f32>() / chars.len() as f32
    };
    let text: String = chars.iter().map(|c| c.ch).collect();
    RecResult {
        text,
        confidence,
        chars,
    }
}

use translator_raster::text_metrics::WORD_GAP_FACTOR;

/// Median strip-advance between consecutive decoded characters. Robust to the few large
/// inter-word gaps, which are high outliers the median ignores. Returns 1.0 for fewer than
/// two characters, which disables gap splitting (no firing fraction exceeds 1.0).
#[allow(dead_code)]
fn median_advance(chars: &[RecChar]) -> f32 {
    if chars.len() < 2 {
        return 1.0;
    }
    let mut advances: Vec<f32> = chars.windows(2).map(|w| w[1].at.0 - w[0].at.0).collect();
    advances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    advances[advances.len() / 2]
}

/// Split a recognized line into word index-ranges over `chars`. A break falls on a
/// whitespace character (the recognizer's own space class) or on a firing gap wider than
/// [`WORD_GAP_FACTOR`] times the median advance. Whitespace belongs to no word, so the
/// returned ranges cover only non-whitespace runs.
#[allow(dead_code)]
fn word_ranges(chars: &[RecChar]) -> Vec<Range<usize>> {
    let gap_thresh = WORD_GAP_FACTOR * median_advance(chars);
    let mut ranges = Vec::new();
    let mut cur: Option<(usize, usize)> = None;
    for (i, c) in chars.iter().enumerate() {
        if c.ch.is_whitespace() {
            if let Some((start, last)) = cur.take() {
                ranges.push(start..last + 1);
            }
            continue;
        }
        cur = match cur {
            None => Some((i, i)),
            Some((start, last)) => {
                if chars[i].at.0 - chars[last].at.0 > gap_thresh {
                    ranges.push(start..last + 1);
                    Some((i, i))
                } else {
                    Some((start, i))
                }
            }
        };
    }
    if let Some((start, last)) = cur {
        ranges.push(start..last + 1);
    }
    ranges
}

#[cfg(test)]
mod word_segmentation_tests {
    use super::{RecChar, StripFraction, word_ranges};

    fn rc(ch: char, at: f32) -> RecChar {
        RecChar {
            ch,
            score: 1.0,
            at: StripFraction(at),
        }
    }

    #[test]
    fn empty_line_has_no_words() {
        assert!(word_ranges(&[]).is_empty());
    }

    #[test]
    fn single_char_is_one_word() {
        assert_eq!(word_ranges(&[rc('a', 0.5)]), vec![0..1]);
    }

    #[test]
    fn uniform_advance_stays_one_word() {
        let chars = [rc('w', 0.1), rc('o', 0.2), rc('r', 0.3), rc('d', 0.4)];
        assert_eq!(word_ranges(&chars), vec![0..4]);
    }

    #[test]
    fn splits_on_whitespace_excluding_the_space() {
        // "ab cd": the space at index 2 belongs to no word.
        let chars = [
            rc('a', 0.1),
            rc('b', 0.2),
            rc(' ', 0.3),
            rc('c', 0.4),
            rc('d', 0.5),
        ];
        assert_eq!(word_ranges(&chars), vec![0..2, 3..5]);
    }

    #[test]
    fn splits_on_firing_gap_without_a_space_token() {
        // No whitespace: a wide gap (0.15 -> 0.55) against a 0.05 median advance
        // exceeds 1.8x and breaks the run; the small gaps do not.
        let chars = [
            rc('h', 0.05),
            rc('i', 0.10),
            rc('!', 0.15),
            rc('y', 0.55),
            rc('o', 0.60),
        ];
        assert_eq!(word_ranges(&chars), vec![0..3, 3..5]);
    }
}

// ---------- Per-line contour-based dewarp (ported from OCR PoC) ----------

/// Compute the min-area-aligned (PCA-axis) rotated rectangle around a detection contour. The
/// principal axis is the line's reading direction; perpendicular extent gives the height. For
/// elongated text shapes this is within a fraction of a degree of the true min-area rect, but
/// is much cheaper than the rotating-calipers algorithm.
#[derive(Debug, Clone, Copy)]
struct ContourBoxes {
    /// Min-area rotated rectangle around the *raw* DB mask contour. Tight to the segmentation
    /// kernel — no ascender/descender padding, no unclip inflation. Used for layout heuristics
    /// (paragraph grouping, line-height clustering) where character whitespace would skew the
    /// metric.
    tight: translator_core::ocr::OrientedRect,
    /// `tight` inflated by `unclip + DET_BOX_BORDER`, matching the AABB pipeline. Used for
    /// erase/render so the box covers actual ink including ascenders/descenders.
    inflated: translator_core::ocr::OrientedRect,
}

/// Number of x-bins used to extract the top and bottom edge profile from a contour. 16 is
/// enough to fit a regression through a real word's edges while staying coarse enough that a
/// single noisy contour point doesn't dominate any one bin's value.
const TILT_X_BINS: usize = 16;

/// Maximum |top_slope − bottom_slope| (in dy/dx units) for the contour to be considered
/// genuinely tilted. ~0.05 ≈ 2.9° — well below typical glyph-asymmetry slope ("Menu"'s top
/// edge drops 30% of its height across the word, slope ≈ 0.10–0.20) but above any real-world
/// scanning skew jitter.
const TILT_AGREEMENT_SLOPE: f32 = 0.05;

/// Per-bin extreme-y tolerance for outlier filtering, expressed as a fraction of the contour's
/// vertical extent. Descenders / ascenders / random spikes stick out beyond the median edge by
/// more than this and get dropped from the regression.
const TILT_EDGE_OUTLIER_FRACTION: f32 = 0.20;

/// Extra vertical strip span reserved on the descender side, as a fraction of the
/// text-band thickness. Covers descender tails that the p05–p95 band drops on long
/// lines (rare descenders are excluded and the spine rides up to the ascenders).
/// Shared by recognition and the ink matte so their geometries stay identical; the
/// extra is whitespace recognition tolerates and background the matte ignores.
const STRIP_DESCENDER_VPAD_FRAC: f32 = 0.4;
/// Maximum PCA eigenvalue ratio (λ₂/λ₁) for the contour to have a trustworthy
/// principal axis. Above it the cloud is too square (a lone glyph, "3%") for PCA
/// to mean anything, so the dewarp aligns to the reading frame instead of a fluke
/// lean. ~0.12 ⇒ needs roughly a 2.5:1 aspect, which any real word clears.
const STRIP_ELONG_MAX: f32 = 0.12;

/// Per-contour tilt measurement, in oriented terms (no AABB). `angle` is the reading-
/// direction lean the contour measured for itself (0 when degenerate or content-
/// asymmetric); `aspect` is its oriented long:short extent ratio along that lean — the
/// length-confidence input that decides, in `resolve_box_angle`, how much the measurement
/// is trusted against the frame. `(cx, cy)` is the contour centroid, where the consensus
/// field is sampled. `vote` is the angle this contour offers to the frame consensus —
/// present only when its edges agree *and* it is elongated enough to carry an axis.
struct TiltMeasurement {
    angle: f32,
    aspect: f32,
    cx: f32,
    cy: f32,
    vote: Option<f32>,
}

impl TiltMeasurement {
    fn none(cx: f32, cy: f32) -> Self {
        TiltMeasurement {
            angle: 0.0,
            aspect: 0.0,
            cx,
            cy,
            vote: None,
        }
    }
}

/// Oriented extents of a contour about its centroid, along `angle` (reading axis) and
/// perpendicular to it. Tilt-invariant, unlike an axis-aligned bounding box.
fn oriented_extents(contour: &[(f32, f32)], angle: f32, cx: f32, cy: f32) -> (f32, f32) {
    let (ux, uy) = (angle.cos(), angle.sin());
    let (vx, vy) = (-uy, ux);
    let (mut u_min, mut u_max, mut v_min, mut v_max) = (
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
    );
    for &(x, y) in contour {
        let (dx, dy) = (x - cx, y - cy);
        let u = dx * ux + dy * uy;
        let v = dx * vx + dy * vy;
        u_min = u_min.min(u);
        u_max = u_max.max(u);
        v_min = v_min.min(v);
        v_max = v_max.max(v);
    }
    (u_max - u_min, v_max - v_min)
}

/// Estimate a contour's reading-direction lean by regressing the top and bottom edges
/// separately and only trusting an angle when both edges agree. The angle is in radians
/// (+x ⇒ 0, downward-to-the-right ⇒ positive, image y points down).
///
/// Algorithm:
///   1. Bin contour points by x.
///   2. Per bin, take the minimum-y point (top edge) and maximum-y point (bottom edge).
///   3. Winsorize each edge's regression residuals to `TILT_EDGE_OUTLIER_FRACTION ×
///      contour_height` (handles ascenders / descenders / asymmetric capital opener like
///      "Menu"'s `M` without discarding the bin's x lever arm).
///   4. Linear-regress slope on the winsorized top points and on the winsorized bottom points.
///   5. If the two slopes differ by more than `TILT_AGREEMENT_SLOPE`, the contour is
///      content-asymmetric: report no usable angle and abstain from the consensus vote.
///      Otherwise the average is the lean.
///
/// How far to *trust* that lean — against its own noise and against the frame — is not
/// decided here: it is left to `length_confidence`/`angle_confidence` in `resolve_box_angle`,
/// fed by the oriented `aspect` measured here. The contour votes into the frame consensus
/// only when it is elongated enough to carry a reading axis at all.
fn estimate_horizontal_tilt(contour: &[(f32, f32)]) -> TiltMeasurement {
    let n = contour.len() as f32;
    let (mut sum_x, mut sum_y) = (0.0f32, 0.0f32);
    let (mut min_x, mut max_x) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut min_y, mut max_y) = (f32::INFINITY, f32::NEG_INFINITY);
    for &(x, y) in contour {
        sum_x += x;
        sum_y += y;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let (cx, cy) = (sum_x / n, sum_y / n);
    let width = max_x - min_x;
    let height = max_y - min_y;
    if width < 8.0 || height < 2.0 {
        return TiltMeasurement::none(cx, cy);
    }

    let bin_w = width / TILT_X_BINS as f32;
    let mut top_per_bin: [Option<(f32, f32)>; TILT_X_BINS] = [None; TILT_X_BINS];
    let mut bot_per_bin: [Option<(f32, f32)>; TILT_X_BINS] = [None; TILT_X_BINS];
    for &(x, y) in contour {
        let bin = (((x - min_x) / bin_w) as usize).min(TILT_X_BINS - 1);
        match top_per_bin[bin] {
            None => top_per_bin[bin] = Some((x, y)),
            Some((_, ty)) if y < ty => top_per_bin[bin] = Some((x, y)),
            _ => {}
        }
        match bot_per_bin[bin] {
            None => bot_per_bin[bin] = Some((x, y)),
            Some((_, by)) if y > by => bot_per_bin[bin] = Some((x, y)),
            _ => {}
        }
    }

    let top_pts: Vec<(f32, f32)> = top_per_bin.iter().filter_map(|p| *p).collect();
    let bot_pts: Vec<(f32, f32)> = bot_per_bin.iter().filter_map(|p| *p).collect();
    if top_pts.len() < 4 || bot_pts.len() < 4 {
        return TiltMeasurement::none(cx, cy);
    }

    let tol = height * TILT_EDGE_OUTLIER_FRACTION;
    let top_slope = robust_regression_slope(&top_pts, tol);
    let bot_slope = robust_regression_slope(&bot_pts, tol);
    let diff = (top_slope - bot_slope).abs();
    if diff > TILT_AGREEMENT_SLOPE {
        // Content-asymmetric: the edges disagree about the lean, so this contour has no
        // trustworthy axis of its own — adopt the frame, abstain from the vote.
        return TiltMeasurement::none(cx, cy);
    }

    let angle = ((top_slope + bot_slope) * 0.5).atan();
    let (raw_u, raw_v) = oriented_extents(contour, angle, cx, cy);
    let aspect = raw_u / raw_v.max(1.0);
    let vote = (aspect >= translator_raster::text_metrics::MIN_LINE_ASPECT).then_some(angle);
    log::debug!(
        "ppocr tilt: w={width:.0} h={height:.0} aspect={aspect:.1} \
         top_slope={top_slope:.4} bot_slope={bot_slope:.4} diff={diff:.4} → {:.2}° vote={}",
        angle.to_degrees(),
        vote.is_some(),
    );
    TiltMeasurement {
        angle,
        aspect,
        cx,
        cy,
        vote,
    }
}

/// Two-pass OLS with a residual-based outlier filter. The first pass captures the dominant
/// linear trend through the points; the second pass refits after dropping points whose
/// residual from the first-pass line exceeds `tol`. This is what makes the estimator robust
/// to descenders/ascenders that protrude from an otherwise straight edge while *keeping* the
/// edge's tilted endpoints (a median-based filter would discard them as "far from the centre"
/// which is exactly the position they should occupy in a tilted line).
fn robust_regression_slope(pts: &[(f32, f32)], tol: f32) -> f32 {
    if pts.len() < 3 {
        return 0.0;
    }
    let first = linear_regression_slope(pts);
    let mean_x = pts.iter().map(|(x, _)| x).sum::<f32>() / pts.len() as f32;
    let mean_y = pts.iter().map(|(_, y)| y).sum::<f32>() / pts.len() as f32;
    let intercept = mean_y - first * mean_x;
    // Winsorize residuals against the first-pass fit rather than dropping the offending bins.
    // Ascenders / descenders that the DB contour partially catches sit beyond `tol` from the
    // line; clamping their residual to ±tol neutralises their leverage on the refit while
    // keeping every bin's x lever arm. Dropping them (the previous approach) threw away a
    // quarter of the support on a short word and pushed variance back up.
    let winsorized: Vec<(f32, f32)> = pts
        .iter()
        .copied()
        .map(|(x, y)| {
            let fit = first * x + intercept;
            (x, fit + (y - fit).clamp(-tol, tol))
        })
        .collect();
    linear_regression_slope(&winsorized)
}

fn linear_regression_slope(pts: &[(f32, f32)]) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    let n = pts.len() as f32;
    let mean_x = pts.iter().map(|(x, _)| x).sum::<f32>() / n;
    let mean_y = pts.iter().map(|(_, y)| y).sum::<f32>() / n;
    let mut num = 0.0f32;
    let mut den = 0.0f32;
    for &(x, y) in pts {
        let dx = x - mean_x;
        num += dx * (y - mean_y);
        den += dx * dx;
    }
    if den < 1e-6 { 0.0 } else { num / den }
}

/// Minimum number of agreeing per-box tilt votes before a frame-level consensus angle is
/// trusted. Below this the frame has too little evidence of a shared reading direction, so each
/// box keeps its own measured tilt.
const TILT_CONSENSUS_MIN_VOTERS: usize = 3;

/// A scene whose consensus reading-direction is within this of image-horizontal is snapped to
/// exactly horizontal. The median of the per-box votes carries sub-degree noise even on a page
/// that is visibly upright, and every committed box then snaps to that noisy value (see
/// `resolve_box_angle`), tilting all the text by a fraction of a degree and softening it under the
/// render rotation. Collapsing a near-upright scene to 0° keeps that text crisp. ~1°; a genuinely
/// tilted scene (a held phone, a skewed scan) sits well beyond this and keeps its measured angle.
const TILT_CONSENSUS_DEADZONE: f32 = 0.0175;

/// Shrinkage length scale for the tilt field's gradient terms. The ridge added to the
/// normal equations equals the total vote weight times this length squared, so gradients only
/// emerge once the votes' positional spread is comfortably past this scale — a frame whose
/// text clusters in one corner can't hallucinate a perspective gradient from two noisy votes,
/// it degrades to the constant (pure-rotation) fit instead.
const TILT_FIELD_RIDGE_PX: f32 = 200.0;

struct TiltVote {
    x: f32,
    y: f32,
    /// Lever arm of the measurement: wide boxes regress their baseline angle over many more
    /// x-bins, so their vote is proportionally more precise than a short word's.
    weight: f32,
    angle: f32,
}

/// Linear angle field over the frame: `angle(x, y) = a + gx·(x − x0) + gy·(y − y0)`.
/// A flat scene (pure in-plane rotation) is the special case `gx = gy = 0`; perspective
/// foreshortening and page curl show up as smooth spatial variation of the reading direction,
/// which a first-order field captures well at the angle scales involved (a few degrees across
/// the frame). Angles are all near-horizontal (the ±90° reading-axis swap happens later in
/// `build_oriented_boxes`), so plain linear math needs no angular wrap handling.
struct TiltField {
    x0: f32,
    y0: f32,
    a: f32,
    gx: f32,
    gy: f32,
}

impl TiltField {
    fn at(&self, x: f32, y: f32) -> f32 {
        self.a + self.gx * (x - self.x0) + self.gy * (y - self.y0)
    }
}

/// Robust weighted fit of the frame's reading-direction field from per-box votes. `None` when
/// too few boxes voted to trust a shared direction. Outliers (a rotated label, a price sticker,
/// a second surface at a different orientation) are shed by re-weighting: votes whose residual
/// exceeds 3× the weighted-median residual get dropped and the field refit, so the majority
/// surface defines the field and breakaway boxes keep their own angles via `resolve_box_angle`.
/// A fitted field that stays within `TILT_CONSENSUS_DEADZONE` everywhere the votes live is
/// collapsed to exactly 0° for render crispness, same as the old scalar consensus.
fn fit_tilt_field(votes: &[TiltVote]) -> Option<TiltField> {
    if votes.len() < TILT_CONSENSUS_MIN_VOTERS {
        return None;
    }
    let mut weights: Vec<f32> = votes.iter().map(|v| v.weight).collect();
    let mut field = TiltField {
        x0: 0.0,
        y0: 0.0,
        a: 0.0,
        gx: 0.0,
        gy: 0.0,
    };
    for _ in 0..3 {
        let sw: f32 = weights.iter().sum();
        if sw <= 0.0 {
            return None;
        }
        let x0 = votes
            .iter()
            .zip(&weights)
            .map(|(v, w)| w * v.x)
            .sum::<f32>()
            / sw;
        let y0 = votes
            .iter()
            .zip(&weights)
            .map(|(v, w)| w * v.y)
            .sum::<f32>()
            / sw;
        let a = votes
            .iter()
            .zip(&weights)
            .map(|(v, w)| w * v.angle)
            .sum::<f32>()
            / sw;
        let ridge = sw * TILT_FIELD_RIDGE_PX * TILT_FIELD_RIDGE_PX;
        let (mut sxx, mut syy, mut sxy, mut sxa, mut sya) = (ridge, ridge, 0.0f32, 0.0f32, 0.0f32);
        for (v, w) in votes.iter().zip(&weights) {
            let dx = v.x - x0;
            let dy = v.y - y0;
            let da = v.angle - a;
            sxx += w * dx * dx;
            syy += w * dy * dy;
            sxy += w * dx * dy;
            sxa += w * dx * da;
            sya += w * dy * da;
        }
        let det = sxx * syy - sxy * sxy;
        field = TiltField {
            x0,
            y0,
            a,
            gx: (sxa * syy - sya * sxy) / det,
            gy: (sya * sxx - sxa * sxy) / det,
        };

        let mut residuals: Vec<f32> = votes
            .iter()
            .map(|v| (v.angle - field.at(v.x, v.y)).abs())
            .collect();
        let mut sorted = residuals.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("tilt residuals are finite"));
        let mad = sorted[sorted.len() / 2];
        let cut = (3.0 * mad).max(TILT_CONSENSUS_DEADZONE);
        for ((w, v), r) in weights.iter_mut().zip(votes).zip(residuals.drain(..)) {
            *w = if r <= cut { v.weight } else { 0.0 };
        }
    }
    let max_abs = votes
        .iter()
        .map(|v| field.at(v.x, v.y).abs())
        .fold(0.0f32, f32::max);
    if max_abs < TILT_CONSENSUS_DEADZONE {
        return Some(TiltField {
            x0: field.x0,
            y0: field.y0,
            a: 0.0,
            gx: 0.0,
            gy: 0.0,
        });
    }
    Some(field)
}

/// Reconcile a box's own measured lean with the frame consensus by an adaptive clamp.
///
/// The measurement is kept whole — never blended — when there is enough evidence that it is
/// real, and otherwise snapped to the frame. Evidence is the product of two confidences:
/// `length_confidence(aspect)` (a square box carries no axis; a long line does) and
/// `angle_confidence(|Δ|)` (a small deviation from the frame is likely noise; a large one is
/// likely a genuine lean). So a short label clamps to the frame at any plausible angle, a long
/// line honors even a fraction of a degree, and the middle keeps a lean only once it is large
/// enough to clear its length's noise floor. With no consensus the frame is image-horizontal.
///
/// Only the in-axis lean is decided here; the ±90° reading-axis choice is `build_oriented_boxes`.
/// With no consensus the frame is image-horizontal.
fn resolve_box_angle(m: &TiltMeasurement, consensus: Option<f32>) -> f32 {
    translator_raster::text_metrics::clamp_lean_to_frame(
        m.angle,
        m.aspect,
        consensus.unwrap_or(0.0),
    )
}

#[cfg(test)]
fn oriented_boxes_from_contour(contour: &[(f32, f32)]) -> Option<ContourBoxes> {
    // Estimate the line's tilt from the top *and* bottom edges of the contour separately, and
    // accept a non-zero angle only when both edges agree. PCA on all points (the previous
    // approach) is fooled by content-asymmetric masks — e.g. "Menu" has a tall M and short
    // enu, so the top edge slopes down while the baseline stays flat, and PCA's covariance
    // splits the difference into a phantom tilt. The bottom edge is the baseline (stable for
    // most words), and demanding agreement with the top filters out asymmetric shapes.
    // Standalone (no frame): the lean is clamped against image-horizontal by its own evidence.
    let m = estimate_horizontal_tilt(contour);
    build_oriented_boxes(contour, resolve_box_angle(&m, None), 0.0)
}

fn build_oriented_boxes(
    contour: &[(f32, f32)],
    angle_radians: f32,
    pool_comp_px: f32,
) -> Option<ContourBoxes> {
    if contour.len() < 4 {
        return None;
    }
    let n = contour.len() as f32;

    let mut mean_x = 0.0f32;
    let mut mean_y = 0.0f32;
    for &(x, y) in contour {
        mean_x += x;
        mean_y += y;
    }
    mean_x /= n;
    mean_y /= n;

    let ux = angle_radians.cos();
    let uy = angle_radians.sin();
    let vx = -uy;
    let vy = ux;

    let mut u_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    let mut v_min = f32::INFINITY;
    let mut v_max = f32::NEG_INFINITY;
    for &(x, y) in contour {
        let dx = x - mean_x;
        let dy = y - mean_y;
        let u = dx * ux + dy * uy;
        let v = dx * vx + dy * vy;
        u_min = u_min.min(u);
        u_max = u_max.max(u);
        v_min = v_min.min(v);
        v_max = v_max.max(v);
    }
    let raw_u = u_max - u_min;
    let raw_v = v_max - v_min;
    let u_center = (u_min + u_max) * 0.5;
    let v_center = (v_min + v_max) * 0.5;
    let cx = mean_x + ux * u_center + vx * v_center;
    let cy = mean_y + uy * u_center + vy * v_center;

    // OrientedRect convention: `width` = reading axis (long), `height` =
    // perpendicular. `estimate_horizontal_tilt` only resolves near-
    // horizontal tilts; for text that's actually rotated ~90° (e.g.
    // phone held in portrait while shooting a landscape doc) it
    // returns 0 and the contour's y-extent is then the long side.
    // Swap so the rect's long side is the reading axis, with angle bumped
    // by π/2.
    let (final_width, final_height, final_angle) =
        if raw_v > raw_u * translator_raster::text_metrics::MIN_LINE_ASPECT {
            (raw_v, raw_u, angle_radians + std::f32::consts::FRAC_PI_2)
        } else {
            (raw_u, raw_v, angle_radians)
        };
    if final_width < 4.0 || final_height < 2.0 {
        return None;
    }

    // The tight box is the measured mask band; low-res heads underestimate
    // it by ~(stride - 1) px total (pooling pulls the threshold crossing
    // inward), which downstream consumers — live block merging, render
    // geometry — read as genuinely thinner lines. Restore the deficit.
    let tight = translator_core::ocr::OrientedRect {
        cx,
        cy,
        width: final_width + pool_comp_px,
        height: final_height + pool_comp_px,
        angle_radians: final_angle,
    };

    // DB segmentation produces a *shrunken* contour relative to the actual ink. The AABB path
    // recovers the full text region by expanding by `expand_dist = UNCLIP * thickness / 2`
    // plus a small border (see `extract_boxes` and `expand_box`). Apply the same inflation
    // here so the oriented rect covers ascenders/descenders for erase, and so its height
    // matches the AABB pipeline's height — what the renderer uses to size the font.
    let thickness = final_width.min(final_height);
    let expand_dist = (DET_UNCLIP_RATIO * thickness / 2.0).max(1.0);
    let pad = expand_dist + DET_BOX_BORDER as f32 + pool_comp_px;
    let inflated = translator_core::ocr::OrientedRect {
        cx,
        cy,
        width: final_width + 2.0 * pad,
        height: final_height + 2.0 * pad,
        angle_radians: final_angle,
    };

    Some(ContourBoxes { tight, inflated })
}

/// Contour PCA principal-axis angle in image coords, with `ux >= 0`
/// canonicalisation (so the angle lives in `[0, π)` — sign-ambiguous
/// reading direction). Used by the orientation estimator to deskew
/// each detected strip before feeding it to the textline-ori model;
/// the classifier then resolves the ±x ambiguity. Same PCA math as
/// `dewarp_contour_to_strip` below, factored out so both callers
/// agree on the axis.
pub fn contour_principal_axis_angle(contour: &[(f32, f32)]) -> Option<f32> {
    if contour.len() < 8 {
        return None;
    }
    let n = contour.len() as f32;
    let mut mean_x = 0.0f32;
    let mut mean_y = 0.0f32;
    for &(x, y) in contour {
        mean_x += x;
        mean_y += y;
    }
    mean_x /= n;
    mean_y /= n;
    let mut cxx = 0.0f32;
    let mut cyy = 0.0f32;
    let mut cxy = 0.0f32;
    for &(x, y) in contour {
        let dx = x - mean_x;
        let dy = y - mean_y;
        cxx += dx * dx;
        cyy += dy * dy;
        cxy += dx * dy;
    }
    cxx /= n;
    cyy /= n;
    cxy /= n;
    let trace = cxx + cyy;
    let det = cxx * cyy - cxy * cxy;
    let disc = (trace * trace - 4.0 * det).max(0.0).sqrt();
    let lambda1 = (trace + disc) * 0.5;
    let (ex, ey) = if cxy.abs() > 1e-6 {
        (lambda1 - cyy, cxy)
    } else if cxx >= cyy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let norm = (ex * ex + ey * ey).sqrt().max(1e-6);
    let mut ux = ex / norm;
    let mut uy = ey / norm;
    if ux < 0.0 {
        ux = -ux;
        uy = -uy;
    }
    Some(uy.atan2(ux))
}

/// Mean, principal-axis unit vector (`ux >= 0` convention) and the two PCA
/// eigenvalues (λ₁ ≥ λ₂) of a point set. The shared core of every contour-PCA in
/// this module.
fn pca_axis(pts: &[(f32, f32)]) -> Option<(f32, f32, f32, f32, f32, f32)> {
    let n = pts.len() as f32;
    if n < 3.0 {
        return None;
    }
    let (mut mean_x, mut mean_y) = (0.0f32, 0.0f32);
    for &(x, y) in pts {
        mean_x += x;
        mean_y += y;
    }
    mean_x /= n;
    mean_y /= n;
    let (mut cxx, mut cyy, mut cxy) = (0.0f32, 0.0f32, 0.0f32);
    for &(x, y) in pts {
        let (dx, dy) = (x - mean_x, y - mean_y);
        cxx += dx * dx;
        cyy += dy * dy;
        cxy += dx * dy;
    }
    cxx /= n;
    cyy /= n;
    cxy /= n;
    let trace = cxx + cyy;
    let det = cxx * cyy - cxy * cxy;
    let disc = (trace * trace - 4.0 * det).max(0.0).sqrt();
    let lambda1 = (trace + disc) * 0.5;
    let lambda2 = (trace - disc) * 0.5;
    let (ex, ey) = if cxy.abs() > 1e-6 {
        (lambda1 - cyy, cxy)
    } else if cxx >= cyy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let norm = (ex * ex + ey * ey).sqrt().max(1e-6);
    let (mut ux, mut uy) = (ex / norm, ey / norm);
    if ux < 0.0 {
        ux = -ux;
        uy = -uy;
    }
    Some((mean_x, mean_y, ux, uy, lambda1, lambda2))
}

struct ContourStripWarp {
    width: u32,
    height: u32,
    mean_x: f32,
    mean_y: f32,
    ux: f32,
    uy: f32,
    vx: f32,
    vy: f32,
    u_center: f32,
    u_half_span: f32,
    padded_u_min: f32,
    padded_u_span: f32,
    spine_fit: (f32, f32, f32),
    global_thickness: f32,
}

/// PCA principal axis + per-edge quadratic fit for `contour`, plus the output
/// strip dimensions. Factored out so the gray (recognizer) and rgb (viz) dewarps
/// share identical geometry and differ only in how they sample source pixels.
fn contour_strip_warp(
    contour: &[(f32, f32)],
    canonical_quadrant: Option<translator_core::coords::Quadrant>,
    thickness_pad: f32,
) -> Option<ContourStripWarp> {
    if contour.len() < 8 {
        return None;
    }
    // 1. PCA on contour points -> principal axis (u) and perpendicular (v).
    let (mut mean_x, mut mean_y, mut ux, mut uy, lambda1, lambda2) = pca_axis(contour)?;

    // A short, near-square contour ("3%", a lone glyph) has no trustworthy axis —
    // the eigenvalues are close and PCA latches onto noise, leaning the strip
    // wildly. Fall back to the reading frame's own direction (canonical quadrant,
    // else screen-horizontal).
    if lambda2 / lambda1.max(1e-6) > STRIP_ELONG_MAX {
        let theta = canonical_quadrant.map(|q| q.radians()).unwrap_or(0.0);
        ux = theta.cos();
        uy = theta.sin();
        mean_x = contour.iter().map(|p| p.0).sum::<f32>() / contour.len() as f32;
        mean_y = contour.iter().map(|p| p.1).sum::<f32>() / contour.len() as f32;
    }
    // PCA gives a sign-ambiguous principal axis; we have to pick which
    // way along it the strip's +x should point. Without a reference,
    // `ux >= 0` is the only deterministic choice — and it must stay
    // exactly that, because `estimate_canonical_quadrant` deskews by
    // the `ux >= 0`-canonicalised `contour_principal_axis_angle` and
    // relies on this dewarp agreeing with it sign-for-sign. When the
    // scene's reading direction is known (canonical_quadrant), align
    // the strip's +x with it by dot-product sign instead. That works
    // while the axis has a meaningful component along the reference,
    // but a *vertical* text column (CJK top-to-bottom) is nearly
    // perpendicular to it, so the dot product is ≈0 and its sign is
    // per-column noise — adjacent columns of one page dewarp in
    // opposite directions and half of them recognize as empty. For
    // those cross-axis strips we align against the reference rotated
    // 90° CW in screen coords instead (reading-frame "down"), which
    // pins vertical columns to top-char-first — the orientation
    // PaddleOCR's recognizer was trained on (`np.rot90` of the crop).
    let need_flip = match canonical_quadrant {
        Some(q) => {
            let theta = q.radians();
            let along = ux * theta.cos() + uy * theta.sin();
            if along.abs() >= std::f32::consts::FRAC_1_SQRT_2 {
                along < 0.0
            } else {
                ux * -theta.sin() + uy * theta.cos() < 0.0
            }
        }
        None => ux < 0.0,
    };
    if need_flip {
        ux = -ux;
        uy = -uy;
    }
    let vx = -uy;
    let vy = ux;

    // 2. Project contour into the final (u, v) frame.
    let projected: Vec<(f32, f32)> = contour
        .iter()
        .map(|&(x, y)| {
            let dx = x - mean_x;
            let dy = y - mean_y;
            (dx * ux + dy * uy, dx * vx + dy * vy)
        })
        .collect();
    let (u_min, u_max) = projected
        .iter()
        .map(|&(u, _)| u)
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), u| {
            (lo.min(u), hi.max(u))
        });
    let u_span = u_max - u_min;
    if u_span < 8.0 {
        return None;
    }

    // 3. Fit one quadratic spine through all contour points. u normalized to
    //    [-1, 1] for numerical stability before fitting. Splitting the contour
    //    into a top edge (v <= 0) and bottom edge (v >= 0) by each point's offset
    //    from the centroid only works while the line is flatter than it is thick:
    //    once it bows by more than ~half the text thickness, the bowed ends of
    //    both edges cross the centroid, so each edge-set is polluted by the
    //    other's points and both fits flatten toward the chord — averaging them
    //    recovers only a fraction of the real curvature. A single least-squares
    //    quadratic over every point is the centerline directly: the two edges sit
    //    symmetrically at ±thickness/2, so those offsets cancel and the spine
    //    tracks the full bow no matter how sharply the line curves.
    let u_half_span = u_span * 0.5;
    let u_center = (u_min + u_max) * 0.5;
    let normalized: Vec<(f32, f32, f32)> = projected
        .iter()
        .map(|&(u, v)| ((u - u_center) / u_half_span, u, v))
        .collect();
    let all_pts: Vec<(f32, f32)> = normalized.iter().map(|&(un, _, v)| (un, v)).collect();
    let mut spine_fit = fit_quadratic(&all_pts)?;

    // 4. Text-band thickness from the spread of points about the spine. A robust
    //    5th–95th-percentile band ignores stray contour points and the odd
    //    ascender/descender; 2.4x inflation gives the whitespace margin PP-OCR was
    //    trained with around the glyphs.
    let mut residuals: Vec<f32> = normalized
        .iter()
        .map(|&(un, _, v)| v - eval_quadratic(spine_fit, un))
        .collect();
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let percentile = |p: f32| residuals[(((residuals.len() - 1) as f32) * p).round() as usize];
    let band_thickness = (percentile(0.95) - percentile(0.05)).max(1.0) + thickness_pad;
    // The p05–p95 band drops the rare descenders on a long line (they're <5% of the
    // contour points) and the spine rides up toward the more common ascenders, so a
    // symmetric 2.4x band clips descender tails — they never reach the strip, so the
    // matte misses them and the erase leaves the un-matted tip as dots. Reserve a
    // descender slice below: grow the span and push the centre down by half of it, so
    // the ascender side (already covered) holds and the new room lands under the
    // baseline. The text-band thickness used for `u_pad` stays the same.
    let desc_extra = band_thickness * STRIP_DESCENDER_VPAD_FRAC;
    let global_thickness = band_thickness * 2.4 + desc_extra;
    spine_fit.2 += desc_extra * 0.5;
    let u_pad = band_thickness;
    let padded_u_min = u_min - u_pad;
    let padded_u_span = u_span + 2.0 * u_pad;
    let strip_h = (global_thickness.round() as u32).clamp(8, 256);
    let strip_w = (padded_u_span.round() as u32).clamp(16, 4096);

    Some(ContourStripWarp {
        width: strip_w,
        height: strip_h,
        mean_x,
        mean_y,
        ux,
        uy,
        vx,
        vy,
        u_center,
        u_half_span,
        padded_u_min,
        padded_u_span,
        spine_fit,
        global_thickness,
    })
}

/// Source `(x, y)` in the original image for output strip pixel `(sx, sy)`.
fn strip_source_coord(w: &ContourStripWarp, sx: u32, sy: u32) -> (f32, f32) {
    let u_local = w.padded_u_min + (sx as f32 + 0.5) * (w.padded_u_span / w.width as f32);
    let un = (u_local - w.u_center) / w.u_half_span;
    let spine_v = eval_quadratic(w.spine_fit, un);
    let v_norm = (sy as f32 + 0.5) / w.height as f32 - 0.5;
    let v_local = spine_v + v_norm * w.global_thickness;
    (
        w.mean_x + u_local * w.ux + v_local * w.vx,
        w.mean_y + u_local * w.uy + v_local * w.vy,
    )
}

/// Inverse-warp a tilted/curving text contour into a flat grayscale strip by
/// sampling luma along the fitted spine. This is the recognizer's input.
pub fn dewarp_contour_to_strip(
    gray: &GrayImage,
    contour: &[(f32, f32)],
    canonical_quadrant: Option<translator_core::coords::Quadrant>,
    thickness_pad: f32,
) -> Option<GrayImage> {
    let warp = contour_strip_warp(contour, canonical_quadrant, thickness_pad)?;
    let gray_w = gray.width() as f32;
    let gray_h = gray.height() as f32;
    let gray_raw = gray.as_raw();
    let stride = gray.width() as usize;
    let mut out_buf = vec![0u8; (warp.width * warp.height) as usize];
    for x in 0..warp.width {
        for y in 0..warp.height {
            let (src_x, src_y) = strip_source_coord(&warp, x, y);
            out_buf[(y * warp.width + x) as usize] =
                bilinear_luma(gray_raw, stride, gray_w, gray_h, src_x, src_y);
        }
    }
    GrayImage::from_raw(warp.width, warp.height, out_buf)
}

/// Color counterpart of [`dewarp_contour_to_strip`]: identical geometry, but
/// samples the RGB image so the strip keeps its original colors. Not used by the
/// recognizer (which works on luma) — exposed for visualization/debugging.
/// Rectify an oriented box's tight text band into an `out_w × out_h` strip via inverse
/// warp. The strip's local axes map to the box's (cx, cy, angle); `out_w/out_h` set the
/// sampling resolution. Uses the same `(u, v) -> (px, py)` convention as the matting strip
/// in `color_matting`, so a mask produced here registers 1:1 against that strip.
pub fn dewarp_oriented_to_strip_rgb(
    rgb: &RgbImage,
    oriented: &translator_core::ocr::OrientedRect,
    out_w: u32,
    out_h: u32,
) -> RgbImage {
    let cos_a = oriented.angle_radians.cos();
    let sin_a = oriented.angle_radians.sin();
    let sx_scale = oriented.width / out_w as f32;
    let sy_scale = oriented.height / out_h as f32;
    let half_w = oriented.width * 0.5;
    let half_h = oriented.height * 0.5;
    let mut out = RgbImage::new(out_w, out_h);
    for sy in 0..out_h {
        for sx in 0..out_w {
            let u = (sx as f32 + 0.5) * sx_scale - half_w;
            let v = (sy as f32 + 0.5) * sy_scale - half_h;
            let px = u * cos_a - v * sin_a + oriented.cx;
            let py = u * sin_a + v * cos_a + oriented.cy;
            out.put_pixel(sx, sy, bilinear_rgb(rgb, px, py));
        }
    }
    out
}

pub fn dewarp_contour_to_strip_rgb(
    rgb: &RgbImage,
    contour: &[(f32, f32)],
    canonical_quadrant: Option<translator_core::coords::Quadrant>,
    thickness_pad: f32,
) -> Option<RgbImage> {
    let warp = contour_strip_warp(contour, canonical_quadrant, thickness_pad)?;
    let mut out = RgbImage::new(warp.width, warp.height);
    for x in 0..warp.width {
        for y in 0..warp.height {
            let (src_x, src_y) = strip_source_coord(&warp, x, y);
            out.put_pixel(x, y, bilinear_rgb(rgb, src_x, src_y));
        }
    }
    Some(out)
}

/// Like [`dewarp_contour_to_strip_rgb`] but also returns, per strip pixel, the
/// source-image coordinate it sampled (row-major `y * width + x`). Lets a caller
/// splat a strip-space result — e.g. an ink erase — back onto the original image.
pub fn dewarp_contour_to_strip_rgb_with_map(
    rgb: &RgbImage,
    contour: &[(f32, f32)],
    canonical_quadrant: Option<translator_core::coords::Quadrant>,
    thickness_pad: f32,
) -> Option<(RgbImage, Vec<(f32, f32)>)> {
    let warp = contour_strip_warp(contour, canonical_quadrant, thickness_pad)?;
    Some(render_contour_strip_rgb_with_map(rgb, &warp))
}

/// Sample `warp`'s strip out of `rgb` and return, per strip pixel (row-major
/// `y * width + x`), the source-image coordinate it sampled. `strip_source_coord`
/// normalizes by `warp.width`/`warp.height`, so a caller may override those on the
/// warp to render at a different resolution (e.g. the ink model's 48px height)
/// and the map comes back at that resolution.
fn render_contour_strip_rgb_with_map(
    rgb: &RgbImage,
    warp: &ContourStripWarp,
) -> (RgbImage, Vec<(f32, f32)>) {
    let mut out = RgbImage::new(warp.width, warp.height);
    let mut map = Vec::with_capacity((warp.width * warp.height) as usize);
    for y in 0..warp.height {
        for x in 0..warp.width {
            let (src_x, src_y) = strip_source_coord(warp, x, y);
            out.put_pixel(x, y, bilinear_rgb(rgb, src_x, src_y));
            map.push((src_x, src_y));
        }
    }
    (out, map)
}

fn fit_quadratic(points: &[(f32, f32)]) -> Option<(f32, f32, f32)> {
    if points.len() < 3 {
        return None;
    }
    let mut s = [0.0f64; 5];
    let mut t = [0.0f64; 3];
    s[0] = points.len() as f64;
    for &(u, v) in points {
        let u = u as f64;
        let v = v as f64;
        let u2 = u * u;
        s[1] += u;
        s[2] += u2;
        s[3] += u2 * u;
        s[4] += u2 * u2;
        t[0] += v;
        t[1] += u * v;
        t[2] += u2 * v;
    }
    let m = [[s[4], s[3], s[2]], [s[3], s[2], s[1]], [s[2], s[1], s[0]]];
    let inv = invert_3x3(&m)?;
    let a = inv[0][0] * t[2] + inv[0][1] * t[1] + inv[0][2] * t[0];
    let b = inv[1][0] * t[2] + inv[1][1] * t[1] + inv[1][2] * t[0];
    let c = inv[2][0] * t[2] + inv[2][1] * t[1] + inv[2][2] * t[0];
    Some((a as f32, b as f32, c as f32))
}

fn eval_quadratic(coeffs: (f32, f32, f32), u: f32) -> f32 {
    let (a, b, c) = coeffs;
    a * u * u + b * u + c
}

fn invert_3x3(m: &[[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det,
        ],
    ])
}

fn bilinear_luma(raw: &[u8], stride: usize, w: f32, h: f32, x: f32, y: f32) -> u8 {
    if x < 0.0 || y < 0.0 || x > w - 1.0 || y > h - 1.0 {
        return 0;
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let x1 = (x0 + 1.0).min(w - 1.0);
    let y1 = (y0 + 1.0).min(h - 1.0);
    let dx = x - x0;
    let dy = y - y0;
    let i00 = (y0 as usize) * stride + x0 as usize;
    let i10 = (y0 as usize) * stride + x1 as usize;
    let i01 = (y1 as usize) * stride + x0 as usize;
    let i11 = (y1 as usize) * stride + x1 as usize;
    let v00 = raw[i00] as f32;
    let v10 = raw[i10] as f32;
    let v01 = raw[i01] as f32;
    let v11 = raw[i11] as f32;
    let top = v00 + (v10 - v00) * dx;
    let bot = v01 + (v11 - v01) * dx;
    (top + (bot - top) * dy).clamp(0.0, 255.0) as u8
}

fn bilinear_rgb(img: &RgbImage, x: f32, y: f32) -> Rgb<u8> {
    let w = img.width() as f32;
    let h = img.height() as f32;
    if x < 0.0 || y < 0.0 || x > w - 1.0 || y > h - 1.0 {
        return Rgb([0, 0, 0]);
    }
    let x0 = x.floor();
    let y0 = y.floor();
    let x1 = (x0 + 1.0).min(w - 1.0);
    let y1 = (y0 + 1.0).min(h - 1.0);
    let dx = x - x0;
    let dy = y - y0;
    let p00 = img.get_pixel(x0 as u32, y0 as u32).0;
    let p10 = img.get_pixel(x1 as u32, y0 as u32).0;
    let p01 = img.get_pixel(x0 as u32, y1 as u32).0;
    let p11 = img.get_pixel(x1 as u32, y1 as u32).0;
    let mut out = [0u8; 3];
    for c in 0..3 {
        let top = p00[c] as f32 + (p10[c] as f32 - p00[c] as f32) * dx;
        let bot = p01[c] as f32 + (p11[c] as f32 - p01[c] as f32) * dx;
        out[c] = (top + (bot - top) * dy).clamp(0.0, 255.0) as u8;
    }
    Rgb(out)
}

#[cfg(test)]
mod tests {
    use super::{contour_strip_warp, eval_quadratic, oriented_boxes_from_contour};

    /// Closed-polygon contour of a curved text band: a centerline that bows by
    /// `bow` pixels (parabolic, minimum at the centre) across width `w`, with a
    /// constant `thickness` between the top and bottom edges. Mimics what
    /// `find_contours` returns for a strongly arced line on a bottle/page.
    fn curved_band(w: f32, thickness: f32, bow: f32) -> Vec<(f32, f32)> {
        let center = |x: f32| {
            let n = (x - w * 0.5) / (w * 0.5);
            bow * n * n
        };
        let half = thickness * 0.5;
        let mut out = Vec::new();
        let n = w.round() as i32;
        for i in 0..=n {
            let x = i as f32;
            out.push((x, center(x) - half));
        }
        for i in (0..=n).rev() {
            let x = i as f32;
            out.push((x, center(x) + half));
        }
        out
    }

    /// Build a closed polygon contour by sampling each edge at 1-pixel steps. Mimics what
    /// `imageproc::contours::find_contours` returns for a mask of the given outline shape.
    fn sample_edges(corners: &[(f32, f32)]) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for i in 0..corners.len() {
            let (x0, y0) = corners[i];
            let (x1, y1) = corners[(i + 1) % corners.len()];
            let dx = x1 - x0;
            let dy = y1 - y0;
            let steps = dx.abs().max(dy.abs()).round() as usize;
            for s in 0..steps {
                let t = s as f32 / steps.max(1) as f32;
                out.push((x0 + dx * t, y0 + dy * t));
            }
        }
        out
    }

    #[test]
    fn tilt_estimate_axis_aligns_word_with_uneven_ascenders() {
        // "Menu"-shaped contour: tall M on the left (cap-height), short enu on the right
        // (x-height), flat baseline. PCA on all points reports a downward-right tilt because
        // the top edge slopes down across the word; the new estimator must reject this and
        // return ~0° because the bottom edge (baseline) is flat.
        let contour = sample_edges(&[
            (0.0, 0.0),   // top-left of M cap
            (20.0, 0.0),  // top-right of M cap
            (20.0, 10.0), // step down to enu x-height
            (80.0, 10.0), // top-right of u
            (80.0, 30.0), // baseline right
            (0.0, 30.0),  // baseline left
        ]);
        let rect = oriented_boxes_from_contour(&contour).expect("rect").tight;
        let angle_deg = rect.angle_radians.to_degrees().abs();
        assert!(
            angle_deg < 1.0,
            "expected near-horizontal angle for asymmetric word mask, got {:.2}°",
            angle_deg,
        );
    }

    #[test]
    fn tilt_estimate_recovers_real_skew_when_top_and_bottom_agree() {
        // A flat horizontal rectangle (0,0)–(80,12) sheared so both edges slope down by 8 px
        // across the 80 px width — about 5.7°. Both top and bottom slope the same, so the
        // estimator should accept the tilt.
        let contour = sample_edges(&[(0.0, 0.0), (80.0, 8.0), (80.0, 20.0), (0.0, 12.0)]);
        let rect = oriented_boxes_from_contour(&contour).expect("rect").tight;
        let angle_deg = rect.angle_radians.to_degrees();
        assert!(
            (angle_deg - 5.7).abs() < 1.0,
            "expected ~5.7° tilt for genuinely sheared rectangle, got {:.2}°",
            angle_deg,
        );
    }

    #[test]
    fn tilt_estimate_axis_aligns_short_word_with_coincident_top_and_bottom_bias() {
        // Real failure from the cyberseceurope.com "Menu" label: short 4-letter word, the
        // cap-height M biases the top regression down, and AA / DBNet mask asymmetry biases
        // the bottom regression by a similar amount in the same direction. Top/bot
        // disagreement is tiny so the agreement gate accepted a ~2° phantom tilt. Sub-visible
        // deviation rejection catches it because slope × width is only ~14% of height — well
        // below the 20% visibility floor.
        let contour = sample_edges(&[
            (0.0, 0.0),  // top-left of M cap
            (16.0, 0.0), // top-right of M cap
            (16.0, 4.0), // step down to enu x-height
            (66.0, 5.0), // very slight bottom drift on top edge of enu (DBNet artefact)
            (66.0, 18.0),
            (0.0, 16.0), // very slight bottom drift on baseline (DBNet/AA artefact)
        ]);
        let rect = oriented_boxes_from_contour(&contour).expect("rect").tight;
        let angle_deg = rect.angle_radians.to_degrees().abs();
        assert!(
            angle_deg < 0.1,
            "short word with coincident top/bot bias should axis-align, got {:.2}°",
            angle_deg,
        );
    }

    #[test]
    fn tilt_estimate_recovers_small_tilt_on_long_line_with_descender_noise() {
        // Mimics one row of the multilingual packaging label: a long contour (~600 px wide,
        // ~20 px tall) tilted by ~1° (slope ≈ 0.018) with a single descender protruding 4 px
        // below the baseline. A median-y outlier filter would have rejected the tilted
        // endpoints because they sit *farthest* from the centre y, collapsing the estimate
        // to 0°. Residual-based filtering keeps the endpoints because they sit *on* the
        // fitted line, and drops only the descender.
        let tilt = 0.018f32; // ≈ 1.03°
        let mut contour = sample_edges(&[
            (0.0, 0.0),
            (600.0, 600.0 * tilt),
            (600.0, 20.0 + 600.0 * tilt),
            (350.0, 20.0 + 350.0 * tilt),
            (350.0, 24.0 + 350.0 * tilt), // descender drops 4 px below baseline
            (320.0, 24.0 + 320.0 * tilt),
            (320.0, 20.0 + 320.0 * tilt),
            (0.0, 20.0),
        ]);
        // Padding to keep enough points in the dense regions for binning.
        contour.extend(sample_edges(&[
            (10.0, 10.0 + 10.0 * tilt),
            (590.0, 10.0 + 590.0 * tilt),
        ]));
        let rect = oriented_boxes_from_contour(&contour).expect("rect").tight;
        let angle_deg = rect.angle_radians.to_degrees();
        assert!(
            (angle_deg - 1.03).abs() < 0.5,
            "expected ~1° tilt for long sloped line with one descender, got {:.2}°",
            angle_deg,
        );
    }

    #[test]
    fn tilt_estimate_ignores_descender_on_otherwise_flat_baseline() {
        // Like "page": flat top + baseline, with a single descender protrusion in one bin.
        // Outlier filtering on the bottom edge should drop the descender so the regression
        // sees a flat baseline and the angle stays ~0°.
        let contour = sample_edges(&[
            (0.0, 0.0),
            (80.0, 0.0),
            (80.0, 12.0),
            (50.0, 12.0),
            (50.0, 16.0), // descender drops 4 px below baseline
            (40.0, 16.0),
            (40.0, 12.0),
            (0.0, 12.0),
        ]);
        let rect = oriented_boxes_from_contour(&contour).expect("rect").tight;
        let angle_deg = rect.angle_radians.to_degrees().abs();
        assert!(
            angle_deg < 1.0,
            "expected near-horizontal angle for word with one descender, got {:.2}°",
            angle_deg,
        );
    }

    #[test]
    fn dewarp_spine_recovers_full_bow_of_strongly_curved_line() {
        // A 200 px-wide band bowing by 40 px — four times its 10 px thickness. The
        // old top/bottom-edge split breaks down well before this: the bowed ends of
        // both edges cross the centroid, each fit is polluted by the other edge, and
        // the averaged spine recovers only ~half the bow. The single all-points fit
        // must land on the true curvature.
        let (w, thickness, bow) = (200.0f32, 10.0f32, 40.0f32);
        let contour = curved_band(w, thickness, bow);
        let warp = contour_strip_warp(&contour, None, 0.0).expect("warp");

        // The spine is v = a·un² + b·un + c over un ∈ [-1, 1]; `a` is the bow
        // amplitude in pixels. un maps linearly onto the band width, so it should
        // recover `bow`, not the old half-corrected ~20.
        let recovered = eval_quadratic(warp.spine_fit, 1.0) - eval_quadratic(warp.spine_fit, 0.0);
        assert!(
            (recovered - bow).abs() < 4.0,
            "expected spine to recover full {bow} px bow, got {recovered:.1} px",
        );
    }

    #[test]
    fn dewarp_spine_stays_flat_for_straight_line() {
        // A straight band must not invent curvature.
        let contour = curved_band(200.0, 10.0, 0.0);
        let warp = contour_strip_warp(&contour, None, 0.0).expect("warp");
        let bow = eval_quadratic(warp.spine_fit, 1.0) - eval_quadratic(warp.spine_fit, 0.0);
        assert!(bow.abs() < 1.0, "expected flat spine, got {bow:.2} px bow");
    }

    use super::{TiltMeasurement, resolve_box_angle};

    fn measured(deg: f32, aspect: f32) -> TiltMeasurement {
        let a = deg.to_radians();
        TiltMeasurement {
            angle: a,
            aspect,
            cx: 0.0,
            cy: 0.0,
            vote: Some(a),
        }
    }

    #[test]
    fn short_box_adopts_the_scene() {
        // A square / sub-confidence box (no axis of its own) follows the frame rather than
        // snapping to image-horizontal: in a 15° scene it becomes 15°.
        let scene = 15.0f32.to_radians();
        let short = measured(0.0, 1.5);
        assert!((resolve_box_angle(&short, Some(scene)) - scene).abs() < 1e-6);
        // With no consensus the frame is image-horizontal, so it clamps to 0.
        assert_eq!(resolve_box_angle(&short, None), 0.0);
    }

    #[test]
    fn long_line_keeps_its_lean_against_a_flat_scene() {
        // A long line carries enough length confidence that its measured lean is kept whole
        // against a flat frame — a small genuine lean and a large rotation alike.
        let scene = Some(0.0f32);
        for deg in [2.6f32, 5.0, 35.0] {
            let est = measured(deg, 20.0);
            assert!(
                (resolve_box_angle(&est, scene) - deg.to_radians()).abs() < 1e-6,
                "long line at {deg}° must keep its lean against a flat scene",
            );
        }
    }

    #[test]
    fn few_word_line_clamps_a_tiny_lean_but_keeps_a_real_one() {
        // The crux: a ~3-word line (aspect ~8) treats a 1° read as noise (clamp to frame)
        // but a 15° read as real (keep).
        let scene = Some(0.0f32);
        assert_eq!(resolve_box_angle(&measured(1.0, 8.0), scene), 0.0);
        assert!(
            (resolve_box_angle(&measured(15.0, 8.0), scene) - 15.0f32.to_radians()).abs() < 1e-6
        );
    }
}
