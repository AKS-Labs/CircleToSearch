//! Tier-3 real-footage harness for the per-box screen monitor
//! (`SCREEN_CHANGE_DETECTION.md`, `SCREEN_MONITOR_REFINEMENT_PLAN.md`).
//!
//! Replays a screen-recording mp4 frame by frame, drawing our opaque pills (with
//! the 50%-alpha pinhole lattice) over each frame the way a MediaProjection capture
//! mirrors them, then runs the **real** GPU pinhole recovery (`LatticeProbe`), the
//! **real** classifier (`ScreenMonitor`), and **real** PP-OCR detect/recognise on
//! re-acquire. It mirrors the decision logic of `LiveScreenPipeline::monitor_screen_v2`
//! using the same prod constants, so the baseline behaviour matches the device, and
//! writes an annotated `composite.mp4` + a per-frame `summary.jsonl` so the trip /
//! hold / scroll decisions can be observed against the footage.
//!
//! Run (env-driven; a no-op skip when the clip env var is unset):
//!   SCREEN_VIDEO_MP4_FILE=text-screen-scroll.mp4 \
//!   SCREEN_VIDEO_DUMP_DIR=smoke-out/screen-monitor/scroll \
//!   cargo test --release --features ppocr,gpu --test screen_monitor_video_smoke -- --nocapture
//!
//! Fidelity boundary (see the plan): the full `monitor_screen_v2` is not host-drivable
//! (it needs the async translate + GL-render worker), so this harness drives the real
//! change-detection core and reproduces the orchestration shell. Two deliberate
//! simplifications, neither of which affects the warmup-ghost or background-false-trip
//! phenomena under study: (1) re-acquire is synchronous on the current frame rather
//! than dispatched async a few frames later; (2) a full-clear re-acquire rebuilds the
//! box set rather than preserving unchanged boxes' baselines by content-hash the way
//! `reconcile_boxes` does (targeted re-acquire still preserves survivors by position).

#![cfg(all(feature = "ppocr", feature = "gpu"))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use glow::HasContext;
use image::{GrayImage, Rgba, RgbaImage};
use imageproc::drawing::{draw_filled_rect_mut, draw_hollow_rect_mut};
use imageproc::rect::Rect as IpRect;
use khronos_egl as egl;
use std::ffi::c_void;
use std::io::Write;
use translator::DetectedTextBox;
use translator::PpocrScript;
use translator::gl_renderer::GlesRenderer;
use translator::live_frame::OrientedImage;
use translator::ocr::{OrientedRect, Rect};
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};
use translator::screen_monitor::{FrameClassification, Lattice, MonitorConfig, ScreenMonitor};

// ── prod constants mirrored from src/live_screen.rs + src/screen_monitor_gpu.rs ──
// (kept in sync by hand so the baseline matches the device; see the plan)
const LATTICE_SPACING: f32 = 2.0; // SCREEN_LATTICE_SPACING
const PILL_LUMA: u8 = 0; // SCREEN_PILL_LUMA = 0.0
const MIN_BOX_HOLES: usize = 4; // SCREEN_MIN_BOX_HOLES
const MAX_BOXES: usize = 64; // MAX_PILLS in screen_monitor_gpu (= device REC_MAX_PILLS)
const V2_MOTION_THR: i32 = 40;
const V2_MOTION_MIN_POINTS: usize = 20;
const V2_SCROLL_MOTION_FRAC: f32 = 0.30;
const V2_SETTLE_MOTION_FRAC: f32 = 0.10;
const SCROLL_CHANGED_FRAC: f32 = 0.75;
const V2_SETTLE_NS: i64 = 100_000_000;
const V2_PERIODIC_NS: i64 = 1_000_000_000;
const V2_TRIP_COOLDOWN_NS: i64 = 100_000_000;
const DET_MAX_PIXELS: u32 = 1_000_000;
const DEFAULT_FPS: u32 = 30;
const DEFAULT_MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

/// Mirror of `live_screen::screen_monitor_config()` — the shipped tuning.
fn prod_monitor_config() -> MonitorConfig {
    MonitorConfig {
        warmup_frames: 6,
        hard_threshold: 110,
        // Backstop for wholesale removal; NCC carries scroll now.
        hard_frac: 0.25,
        scroll_frac: 0.7,
        scroll_min_boxes: 2,
    }
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok().and_then(|v| v.parse().ok())
}

fn model_dir() -> PathBuf {
    env_path("SCREEN_MODEL_DIR").unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_DIR))
}

fn load_engine() -> PpocrEngine {
    let dir = model_dir();
    let det = dir.join("PP-OCRv5_mobile_det.mnn");
    let rec = dir.join("latin_PP-OCRv5_mobile_rec_infer.mnn");
    let keys = dir.join("latin_PP-OCRv5_keys.txt");
    for p in [&det, &rec, &keys] {
        assert!(p.exists(), "required PP-OCR model missing: {}", p.display());
    }
    let spec = PpocrRecognizerSpec {
        script: PpocrScript::Latin,
        model_path: rec,
        keys_path: keys,
    };
    PpocrEngine::load(&det, None, None, vec![spec], 1, None).expect("load ppocr")
}

/// Axis-aligned pill footprint in canonical coords. The recovery only needs "is
/// this lattice point under a pill", so an oriented pill reduces to its enclosing
/// box (a slight corner over-cover, harmless — corner holes outside the real box
/// aren't in any monitored set).
#[derive(Debug, Clone, Copy)]
struct PillRegion {
    cx: f32,
    cy: f32,
    half_w: f32,
    half_h: f32,
}

impl PillRegion {
    fn from_oriented(r: &OrientedRect) -> Self {
        let (c, s) = (r.angle_radians.cos().abs(), r.angle_radians.sin().abs());
        let (hw, hh) = (r.width * 0.5, r.height * 0.5);
        PillRegion {
            cx: r.cx,
            cy: r.cy,
            half_w: hw * c + hh * s,
            half_h: hw * s + hh * c,
        }
    }
}

const TEXTURE_EXTERNAL_OES: u32 = 0x8D65;
const IDENTITY_H: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

struct TexSet {
    src: glow::Texture,
    ext: glow::Texture,
    image: egl::Image,
    w: u32,
    h: u32,
}

fn set_nearest_clamp(gl: &glow::Context, target: u32) {
    unsafe {
        gl.tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    }
}

/// Surfaceless GLES3 context driving the real `gl_renderer` recovery. The captured
/// luma is fed through `samplerExternalOES` by wrapping a `GL_TEXTURE_2D` in an
/// EGLImage bound to `GL_TEXTURE_EXTERNAL_OES`, so the shipping device shader runs
/// unchanged off-device (replacing the former host re-implementation). The EGL
/// handles are leaked deliberately: they must outlive every GL call and the
/// process exits at test end.
struct Gpu {
    lib: &'static egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    ctx: egl::Context,
    gl: glow::Context,
    renderer: GlesRenderer,
    image_target: extern "system" fn(u32, *const c_void),
    tex: Option<TexSet>,
}

impl Gpu {
    fn new() -> Self {
        let lib = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .expect("load libEGL (install Mesa/llvmpipe for headless GL)");
        let lib: &'static egl::DynamicInstance<egl::EGL1_5> = Box::leak(Box::new(lib));
        let display = unsafe {
            lib.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }
        .expect("surfaceless display");
        lib.initialize(display).expect("eglInitialize");
        lib.bind_api(egl::OPENGL_ES_API).expect("bind GLES");
        let config = lib
            .choose_first_config(
                display,
                &[
                    egl::SURFACE_TYPE,
                    egl::PBUFFER_BIT,
                    egl::RENDERABLE_TYPE,
                    egl::OPENGL_ES2_BIT,
                    egl::RED_SIZE,
                    8,
                    egl::NONE,
                ],
            )
            .expect("choose_config")
            .expect("a matching config");
        let ctx = lib
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
            )
            .expect("create GLES3 context");
        lib.make_current(display, None, None, Some(ctx))
            .expect("make current");
        let loader = |name: &str| {
            lib.get_proc_address(name)
                .map(|p| p as *const c_void)
                .unwrap_or(std::ptr::null())
        };
        let gl = unsafe { glow::Context::from_loader_function(loader) };
        let renderer = GlesRenderer::new(loader).expect("build GlesRenderer");
        let image_target: extern "system" fn(u32, *const c_void) = unsafe {
            std::mem::transmute(
                lib.get_proc_address("glEGLImageTargetTexture2DOES")
                    .expect("glEGLImageTargetTexture2DOES (needs GL_OES_EGL_image_external)"),
            )
        };
        Gpu {
            lib,
            display,
            ctx,
            gl,
            renderer,
            image_target,
            tex: None,
        }
    }

    /// (Re)build the EGLImage-backed external camera texture when the frame size
    /// changes (the clip's dims are constant, so this fires once).
    fn ensure(&mut self, w: u32, h: u32) {
        if matches!(&self.tex, Some(t) if t.w == w && t.h == h) {
            return;
        }
        if let Some(t) = self.tex.take() {
            let _ = self.lib.destroy_image(self.display, t.image);
            unsafe {
                self.gl.delete_texture(t.src);
                self.gl.delete_texture(t.ext);
            }
        }
        let src = unsafe { self.gl.create_texture() }.expect("create src tex");
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(src));
            set_nearest_clamp(&self.gl, glow::TEXTURE_2D);
            self.gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                w as i32,
                h as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&vec![0u8; (w * h * 4) as usize])),
            );
        }
        let image = self
            .lib
            .create_image(
                self.display,
                self.ctx,
                egl::GL_TEXTURE_2D as egl::Enum,
                unsafe { egl::ClientBuffer::from_ptr(src.0.get() as usize as *mut c_void) },
                &[egl::ATTRIB_NONE],
            )
            .expect("create EGLImage from GL texture (needs EGL_KHR_gl_texture_2D_image)");
        let ext = unsafe { self.gl.create_texture() }.expect("create ext tex");
        unsafe {
            self.gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(ext));
            (self.image_target)(TEXTURE_EXTERNAL_OES, image.as_ptr() as *const c_void);
            set_nearest_clamp(&self.gl, TEXTURE_EXTERNAL_OES);
            self.gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
        }
        self.renderer.set_camera_external(ext.0.get(), IDENTITY_H);
        self.tex = Some(TexSet {
            src,
            ext,
            image,
            w,
            h,
        });
    }

    /// Recover `screen_est` at every lattice point through the real renderer, given
    /// a `w×h` luma capture that already carries our overlay.
    fn recover(
        &mut self,
        captured_luma: &[u8],
        w: u32,
        h: u32,
        lat: &Lattice,
        pill_luma: u8,
        pills: &[PillRegion],
    ) -> Vec<[u8; 3]> {
        assert_eq!(captured_luma.len(), (w * h) as usize);
        self.ensure(w, h);
        // Upload bottom-up: the recovery shader applies the acquire's 1−v Y-flip, so
        // a top-down capture must land flipped in the texture to read back upright.
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for ty in 0..h {
            let row = h - 1 - ty;
            for x in 0..w {
                let v = captured_luma[(row * w + x) as usize];
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let src = self.tex.as_ref().expect("ensured").src;
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(src));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                w as i32,
                h as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
        let tuples: Vec<(f32, f32, f32, f32)> = pills
            .iter()
            .map(|p| (p.cx, p.cy, p.half_w, p.half_h))
            .collect();
        self.renderer
            .read_lattice_screen_est(
                lat.cols(),
                lat.rows(),
                w,
                h,
                lat.spacing(),
                &tuples,
                pill_luma as f32 / 255.0,
                0.5,
            )
            .expect("read_lattice_screen_est (rec shader / external camera)")
    }
}

/// Real detect + recognise on a CPU-built frame. Returns the full-res canonical
/// `gray` (the screen we composite pills over) plus per-box recognised text.
fn detect_and_rec(
    engine: &PpocrEngine,
    rgba: &RgbaImage,
) -> (GrayImage, Vec<DetectedTextBox>, Vec<String>) {
    let (w, h) = (rgba.width(), rgba.height());
    let crop = Rect {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    let frame = OrientedImage::build_with_rgb(rgba.as_raw(), w, h, 0, crop, DET_MAX_PIXELS)
        .expect("build frame");
    let rgb_det = frame.rgb_det.as_ref().expect("rgb_det populated");
    let det = engine
        .detect_only_image(rgb_det, PpocrProfile::Live)
        .expect("detection");
    let (sx, sy) = frame.det_to_full;
    let boxes: Vec<DetectedTextBox> = det.into_iter().map(|b| b.scaled_xy(sx, sy)).collect();
    let rgb = frame.rgb.as_ref().expect("rgb populated");
    let scripts = vec![PpocrScript::Latin; boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(rgb, &boxes, &scripts, PpocrProfile::Live, None)
        .expect("recognition");
    let texts = lines.into_iter().map(|l| l.text).collect();
    (frame.gray, boxes, texts)
}

/// Composite the opaque pills + 50%-alpha hole grid over a luma source, producing
/// the frame a MediaProjection capture hands us: pill colour over each pill, except
/// at the lattice hole pixels (a half blend of pill and screen).
fn composite_capture(
    source: &[u8],
    w: u32,
    h: u32,
    lat: &Lattice,
    pills: &[PillRegion],
) -> Vec<u8> {
    let mut cap = source.to_vec();
    for p in pills {
        let l = (p.cx - p.half_w).floor().max(0.0) as u32;
        let t = (p.cy - p.half_h).floor().max(0.0) as u32;
        let r = ((p.cx + p.half_w).ceil() as u32).min(w);
        let b = ((p.cy + p.half_h).ceil() as u32).min(h);
        for y in t..b {
            for x in l..r {
                cap[(y * w + x) as usize] = PILL_LUMA;
            }
        }
    }
    let inside = |x: f32, y: f32| {
        pills
            .iter()
            .any(|p| (x - p.cx).abs() <= p.half_w && (y - p.cy).abs() <= p.half_h)
    };
    for pt in lat.points() {
        if !inside(pt.x, pt.y) {
            continue;
        }
        let (px, py) = ((pt.x as u32).min(w - 1), (pt.y as u32).min(h - 1));
        let s = source[(py * w + px) as usize] as u16;
        cap[(py * w + px) as usize] = ((PILL_LUMA as u16 + s + 1) / 2) as u8;
    }
    cap
}

/// The captured COLOR mirror with our opaque pills laid over each footprint — what the
/// device's detection pass actually sees (text under a pill is occluded). Holes are
/// omitted: they're sub-pixel and don't help the detector, only the recovery.
fn composite_overlay_rgba(src: &RgbaImage, pills: &[PillRegion]) -> RgbaImage {
    let mut img = src.clone();
    let (w, h) = (img.width(), img.height());
    for p in pills {
        let l = (p.cx - p.half_w).floor().max(0.0) as u32;
        let t = (p.cy - p.half_h).floor().max(0.0) as u32;
        let r = ((p.cx + p.half_w).ceil() as u32).min(w);
        let b = ((p.cy + p.half_h).ceil() as u32).min(h);
        for y in t..b {
            for x in l..r {
                img.put_pixel(x, y, Rgba([PILL_LUMA, PILL_LUMA, PILL_LUMA, 255]));
            }
        }
    }
    img
}

/// Axis-aligned bbox of an oriented rect, clamped to the frame, as (x, y, w, h).
fn aabb(r: &OrientedRect, w: u32, h: u32) -> (u32, u32, u32, u32) {
    let (c, s) = (r.angle_radians.cos().abs(), r.angle_radians.sin().abs());
    let hw = r.width * 0.5 * c + r.height * 0.5 * s;
    let hh = r.width * 0.5 * s + r.height * 0.5 * c;
    let l = (r.cx - hw).round().max(0.0) as u32;
    let t = (r.cy - hh).round().max(0.0) as u32;
    let right = ((r.cx + hw).round() as u32).min(w);
    let bottom = ((r.cy + hh).round() as u32).min(h);
    (
        l,
        t,
        right.saturating_sub(l).max(1),
        bottom.saturating_sub(t).max(1),
    )
}

struct Mp4Writer {
    child: Child,
    stdin: Option<ChildStdin>,
    output: PathBuf,
}

impl Mp4Writer {
    /// Pipe raw rgb24 straight to ffmpeg/x264 — no in-process JPEG encode (that was
    /// ~74% of the harness wall-clock); the encoder runs concurrently in the subprocess.
    fn new(path: &Path, w: u32, h: u32, fps: u32) -> Self {
        let mut child = Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
            .args(["-video_size", &format!("{w}x{h}")])
            .args(["-framerate", &fps.to_string()])
            .args(["-i", "-"])
            .args([
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
            ])
            .arg(path)
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|e| panic!("spawn ffmpeg to encode {}: {e}", path.display()));
        let stdin = child.stdin.take().expect("ffmpeg stdin");
        Self {
            child,
            stdin: Some(stdin),
            output: path.to_path_buf(),
        }
    }

    fn write_frame(&mut self, rgba: &[u8]) {
        let stdin = self.stdin.as_mut().expect("Mp4Writer stdin closed");
        let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
        for px in rgba.chunks_exact(4) {
            rgb.extend_from_slice(&px[..3]);
        }
        stdin.write_all(&rgb).expect("write rgb frame to ffmpeg");
    }

    fn finish(mut self) {
        self.stdin.take();
        let status = self
            .child
            .wait()
            .unwrap_or_else(|e| panic!("wait ffmpeg encoder for {}: {e}", self.output.display()));
        assert!(
            status.success(),
            "ffmpeg encoder for {} failed: {status}",
            self.output.display()
        );
    }
}

fn decode_mp4_to_mjpeg(path: &Path) -> Vec<u8> {
    let out = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-i"])
        .arg(path)
        .args(["-f", "image2pipe", "-vcodec", "mjpeg", "-q:v", "3", "-"])
        .stderr(Stdio::inherit())
        .output()
        .unwrap_or_else(|e| panic!("spawn ffmpeg to decode {}: {e}", path.display()));
    if !out.status.success() {
        panic!("ffmpeg decode {} failed: {}", path.display(), out.status);
    }
    out.stdout
}

fn split_mjpeg_frames(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        if bytes[i] != 0xff || bytes[i + 1] != 0xd8 {
            i += 1;
            continue;
        }
        let start = i;
        i += 2;
        while i + 1 < bytes.len() {
            if bytes[i] == 0xff && bytes[i + 1] == 0xd9 {
                let end = i + 2;
                frames.push(bytes[start..end].to_vec());
                i = end;
                break;
            }
            i += 1;
        }
    }
    frames
}

/// A resident pill being monitored, with the frame it was (re)acquired at so the
/// annotation can show its warmup window (where the box can't yet trip).
struct ResidentBox {
    id: u64,
    rect: OrientedRect,
    pill: PillRegion,
    text: String,
    acquired_frame: usize,
    /// Lattice holes inside this box, and the *raw* gray at those holes at acquire.
    /// Lets us measure actual content change (gray-now vs gray-then) independent of
    /// the recovery, to tell "content moved" from "monitor didn't see it".
    holes: Vec<usize>,
    base_gray: Vec<u8>,
}

/// Pearson correlation of two equal-length byte vectors, in [-1, 1] (1.0 if either is
/// constant). Mirrors the monitor's, for the harness's raw-content check.
fn pearson_u8(a: &[u8], b: &[u8]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 1.0;
    }
    let nf = n as f64;
    let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (x, y) = (a[i] as f64, b[i] as f64);
        sa += x;
        sb += y;
        saa += x * x;
        sbb += y * y;
        sab += x * y;
    }
    let cov = sab - sa * sb / nf;
    let va = saa - sa * sa / nf;
    let vb = sbb - sb * sb / nf;
    let d = (va * vb).sqrt();
    if d < 1e-3 {
        1.0
    } else {
        (cov / d).clamp(-1.0, 1.0) as f32
    }
}

/// Run a (full or masked-additive) acquire on `gray`: detect + recognise, recover a
/// clean baseline under the current overlay, and register boxes with the monitor.
/// `full_rebuild` clears all resident state first; otherwise survivors (a detection
/// whose centre falls inside an existing box) are kept and only fresh regions added.
#[allow(clippy::too_many_arguments)]
fn run_acquire(
    engine: &PpocrEngine,
    gpu: &mut Gpu,
    monitor: &mut ScreenMonitor,
    color: &RgbaImage,
    gray: &GrayImage,
    resident: &mut Vec<ResidentBox>,
    next_id: &mut u64,
    frame_idx: usize,
    full_rebuild: bool,
) -> usize {
    let (w, h) = (gray.width(), gray.height());
    if full_rebuild {
        monitor.clear_boxes();
        resident.clear();
    }
    // Detect on the captured mirror, not the clean frame: opaque pills over the
    // surviving boxes occlude their text (so it isn't re-detected — the device's masked
    // periodic detect), while freshly-exposed/dropped regions read through. "The
    // capture includes our overlay."
    let cur_pills: Vec<PillRegion> = resident.iter().map(|b| b.pill).collect();
    let det_input = composite_overlay_rgba(color, &cur_pills);
    let (_g, boxes, texts) = detect_and_rec(engine, &det_input);
    // Clean read: recover screen_est with the CURRENT overlay up. Freshly-exposed
    // regions (full clear, or a dropped pill) read as raw screen; surviving pills are
    // inverted through their holes. This is the baseline the new boxes bind to.
    let clean = gpu.recover(
        &composite_capture(gray.as_raw(), w, h, monitor.lattice(), &cur_pills),
        w,
        h,
        monitor.lattice(),
        PILL_LUMA,
        &cur_pills,
    );
    let mut added = 0usize;
    for (b, text) in boxes.iter().zip(&texts) {
        if text.trim().is_empty() {
            continue;
        }
        if resident.len() >= MAX_BOXES {
            eprintln!(
                "[acquire f{frame_idx}] hit MAX_BOXES={MAX_BOXES}; dropping further detections"
            );
            break;
        }
        let center = (b.tight_box.cx, b.tight_box.cy);
        if !full_rebuild && covered_by_resident(center, resident, w, h) {
            continue; // survivor owns this region (masked-additive)
        }
        // Monitor the detection CONTOUR (tight to the text run), not the box — so the
        // hole set isn't padded with background margin that dilutes the hard fraction.
        // Fall back to the box if the contour is missing/degenerate.
        let holes = {
            let c = monitor.lattice().holes_in_polygon(&b.contour);
            if c.len() >= MIN_BOX_HOLES {
                c
            } else {
                monitor.lattice().holes_in_rect(&b.tight_box)
            }
        };
        if holes.len() < MIN_BOX_HOLES {
            continue;
        }
        let base_gray: Vec<u8> = holes
            .iter()
            .map(|&hi| {
                let p = monitor.lattice().points()[hi];
                gray.get_pixel((p.x as u32).min(w - 1), (p.y as u32).min(h - 1))[0]
            })
            .collect();
        let id = *next_id;
        *next_id += 1;
        let box_holes = holes.clone();
        monitor.set_box(id, holes, &clean);
        resident.push(ResidentBox {
            id,
            rect: b.tight_box.clone(),
            pill: PillRegion::from_oriented(&b.tight_box),
            text: text.trim().to_string(),
            acquired_frame: frame_idx,
            holes: box_holes,
            base_gray,
        });
        added += 1;
    }
    added
}

fn covered_by_resident(center: (f32, f32), resident: &[ResidentBox], w: u32, h: u32) -> bool {
    resident.iter().any(|b| {
        let (x, y, bw, bh) = aabb(&b.rect, w, h);
        center.0 >= x as f32
            && center.0 <= (x + bw) as f32
            && center.1 >= y as f32
            && center.1 <= (y + bh) as f32
    })
}

/// Cheap Rec.601 luma — the per-frame canonical screen we composite pills over.
/// Built directly from the captured frame so monitoring frames skip the full
/// `OrientedImage` build (det-downscale + rgb), which is only needed on acquires.
fn luma_of(rgba: &RgbaImage) -> GrayImage {
    let (w, h) = (rgba.width(), rgba.height());
    let mut g = GrayImage::new(w, h);
    for (gp, rp) in g.pixels_mut().zip(rgba.pixels()) {
        let [r, gg, b, _] = rp.0;
        gp.0[0] = ((r as u32 * 299 + gg as u32 * 587 + b as u32 * 114) / 1000) as u8;
    }
    g
}

#[test]
fn screen_monitor_video_smoke() {
    let _ = env_logger::builder().is_test(true).try_init();
    let Some(clip) = env_path("SCREEN_VIDEO_MP4_FILE") else {
        eprintln!("SCREEN_VIDEO_MP4_FILE unset — skipping screen_monitor_video_smoke");
        return;
    };
    let fps = env_u32("SCREEN_VIDEO_FPS").unwrap_or(DEFAULT_FPS);
    let interval_ns = 1_000_000_000i64 / fps.max(1) as i64;
    let max_frames = env_u32("SCREEN_VIDEO_MAX_FRAMES").map(|v| v as usize);
    let stem = clip.file_stem().and_then(|s| s.to_str()).unwrap_or("clip");
    let dump_dir = env_path("SCREEN_VIDEO_DUMP_DIR")
        .unwrap_or_else(|| PathBuf::from("smoke-out/screen-monitor").join(stem));
    std::fs::create_dir_all(&dump_dir).expect("create dump dir");

    let warmup_frames = prod_monitor_config().warmup_frames;
    let engine = load_engine();
    let mut gpu = Gpu::new();

    let bytes = decode_mp4_to_mjpeg(&clip);
    let mut frames = split_mjpeg_frames(&bytes);
    assert!(
        !frames.is_empty(),
        "ffmpeg produced no frames from {}",
        clip.display()
    );
    if let Some(cap) = max_frames {
        frames.truncate(cap);
    }
    eprintln!(
        "{}: {} frames @ {fps}fps → {}",
        clip.display(),
        frames.len(),
        dump_dir.display()
    );

    // Established lazily on the first frame (canonical dims).
    let mut monitor: Option<ScreenMonitor> = None;
    let mut dims = (0u32, 0u32);
    let mut resident: Vec<ResidentBox> = Vec::new();
    let mut next_id: u64 = 1;

    // Mirror of MonitorV2State's scheduling fields.
    let mut prev_samples: Option<Vec<[u8; 3]>> = None;
    let mut prev_covered: Option<Vec<bool>> = None;
    let mut suppress_trips_until_ns: i64 = 0;
    let mut reacquire_not_before_ns: i64 = 0;
    let mut next_acquire_ns: i64 = 0;
    let mut pending_reacquire = true; // bootstrap
    let mut scrolling = false;

    let mut writer: Option<Mp4Writer> = None;
    let mut jsonl: Vec<String> = Vec::with_capacity(frames.len());

    // Coarse timing: where the wall-clock goes.
    let (
        mut td_decode,
        mut td_build,
        mut td_composite,
        mut td_recover,
        mut td_observe,
        mut td_acquire,
        mut td_encode,
    ) = (
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
        Duration::ZERO,
    );
    let mut acquire_count = 0usize;
    let t_all = Instant::now();

    for (idx, raw) in frames.iter().enumerate() {
        let now_ns = idx as i64 * interval_ns;
        let s = Instant::now();
        let rgba = image::load_from_memory(raw)
            .unwrap_or_else(|e| panic!("decode frame {idx}: {e}"))
            .to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        td_decode += s.elapsed();
        // Cheap per-frame luma; the full OrientedImage (det-downscale + rgb) is built
        // only inside run_acquire, on the few acquire frames.
        let s = Instant::now();
        let gray = luma_of(&rgba);
        td_build += s.elapsed();
        let (gw, gh) = (w, h);

        if monitor.is_none() || dims != (gw, gh) {
            monitor = Some(ScreenMonitor::new(
                Lattice::build(gw, gh, LATTICE_SPACING),
                prod_monitor_config(),
            ));
            dims = (gw, gh);
            resident.clear();
            prev_samples = None;
            prev_covered = None;
            pending_reacquire = true;
        }
        let mon = monitor.as_mut().unwrap();

        // 1. Composite our pills + holes over the screen, recover screen_est (GPU).
        let pills: Vec<PillRegion> = resident.iter().map(|b| b.pill).collect();
        let s = Instant::now();
        let captured = composite_capture(gray.as_raw(), gw, gh, mon.lattice(), &pills);
        td_composite += s.elapsed();
        let s = Instant::now();
        let samples = gpu.recover(&captured, gw, gh, mon.lattice(), PILL_LUMA, &pills);
        td_recover += s.elapsed();

        // 2. Gap-point inter-frame motion (mirror monitor_screen_v2).
        let mut covered = vec![false; samples.len()];
        mon.fill_covered(&mut covered);
        let motion_frac = match (&prev_samples, &prev_covered) {
            (Some(prev), Some(prev_cov)) if prev.len() == samples.len() => {
                let mut moved = 0usize;
                let mut eligible = 0usize;
                for i in 0..samples.len() {
                    if covered[i] || prev_cov[i] {
                        continue;
                    }
                    eligible += 1;
                    if (samples[i][0] as i32 - prev[i][0] as i32).abs() > V2_MOTION_THR {
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
        prev_samples = Some(samples.clone());
        prev_covered = Some(covered);

        // 3. Classify.
        let s = Instant::now();
        let classification = mon.observe(&samples);
        td_observe += s.elapsed();

        // 4. Decision (mirror monitor_screen_v2; busy is always false — synchronous).
        let act = now_ns >= suppress_trips_until_ns;
        let total_boxes = resident.len();
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
            if total_boxes >= 3 && changed.len() as f32 >= SCROLL_CHANGED_FRAC * total_boxes as f32
            {
                full_clear = true;
            }
        }

        let mut action = "none";
        if full_clear {
            if !scrolling {
                scrolling = true;
            }
            mon.clear_boxes();
            resident.clear();
            suppress_trips_until_ns = now_ns + V2_TRIP_COOLDOWN_NS;
            reacquire_not_before_ns = now_ns + V2_SETTLE_NS;
            pending_reacquire = true;
            action = "hide";
        } else {
            scrolling = false;
            if !changed.is_empty() {
                changed.sort_unstable();
                changed.dedup();
                for id in &changed {
                    mon.remove_box(*id);
                }
                resident.retain(|b| !changed.contains(&b.id));
                suppress_trips_until_ns = now_ns + V2_TRIP_COOLDOWN_NS;
                reacquire_not_before_ns = now_ns + V2_SETTLE_NS;
                pending_reacquire = true;
                action = "drop";
            }
        }

        // 5. Re-acquire gate (synchronous on this frame once settled).
        let settled = motion_frac < V2_SETTLE_MOTION_FRAC;
        let mut acquired = 0usize;
        if now_ns >= reacquire_not_before_ns
            && settled
            && (pending_reacquire || now_ns >= next_acquire_ns)
        {
            let full_rebuild = resident.is_empty();
            let s = Instant::now();
            acquired = run_acquire(
                &engine,
                &mut gpu,
                mon,
                &rgba,
                &gray,
                &mut resident,
                &mut next_id,
                idx,
                full_rebuild,
            );
            td_acquire += s.elapsed();
            acquire_count += 1;
            next_acquire_ns = now_ns + V2_PERIODIC_NS;
            reacquire_not_before_ns = 0;
            pending_reacquire = false;
            if acquired > 0 && action == "none" {
                action = "acquire";
            }
        }

        // 6. Annotate + record.
        // Per-box debug: (holes, hard_frac_pct, max_delta, changed).
        let devs: HashMap<u64, (usize, usize, u32, bool)> = mon
            .debug_boxes()
            .iter()
            .map(|&(id, holes, frac, maxd, _mean, changed)| (id, (holes, frac, maxd, changed)))
            .collect();
        let changed_set: Vec<u64> = changed.clone();
        let frame_img = annotate(
            &rgba,
            &resident,
            &devs,
            &changed_set,
            action,
            idx,
            frames.len(),
            warmup_frames,
        );
        let writer = writer
            .get_or_insert_with(|| Mp4Writer::new(&dump_dir.join("composite.mp4"), w, h, fps));
        let s = Instant::now();
        writer.write_frame(frame_img.as_raw());
        td_encode += s.elapsed();

        let boxes_json: Vec<String> = resident
            .iter()
            .map(|b| {
                let (holes, frac, _maxd, changed_box) =
                    devs.get(&b.id).copied().unwrap_or((0, 0, 0, false));
                // Raw content change: current gray vs gray-at-acquire at the box's
                // holes — independent of the monitor. Low raw_corr = the content
                // genuinely moved here; if it drops but `frac` (hard-swing %) stays low,
                // the monitor isn't seeing a real change.
                let cur_gray: Vec<u8> = b
                    .holes
                    .iter()
                    .map(|&hi| {
                        let p = mon.lattice().points()[hi];
                        gray.get_pixel((p.x as u32).min(gw - 1), (p.y as u32).min(gh - 1))[0]
                    })
                    .collect();
                let raw_corr = (pearson_u8(&cur_gray, &b.base_gray) * 100.0) as i32;
                format!(
                    "{{\"id\":{},\"holes\":{holes},\"raw_corr\":{raw_corr},\"frac\":{frac},\"changed\":{changed_box},\"text\":{:?}}}",
                    b.id, b.text
                )
            })
            .collect();
        let class_str = match &classification {
            FrameClassification::Quiet => "quiet".to_string(),
            FrameClassification::Scroll => "scroll".to_string(),
            FrameClassification::BoxesChanged(ids) => format!("changed{ids:?}"),
        };
        jsonl.push(format!(
            "{{\"frame\":{idx},\"ms\":{},\"action\":\"{action}\",\"class\":\"{class_str}\",\"motion\":{motion_frac:.3},\"total_boxes\":{total_boxes},\"acquired\":{acquired},\"boxes\":[{}]}}",
            now_ns / 1_000_000,
            boxes_json.join(",")
        ));
    }

    if let Some(w) = writer {
        w.finish();
    }
    std::fs::write(dump_dir.join("summary.jsonl"), jsonl.join("\n")).expect("write summary");

    let n = frames.len().max(1) as f64;
    let total = t_all.elapsed();
    let secs = |d: Duration| d.as_secs_f64();
    let per_acq = if acquire_count > 0 {
        td_acquire.as_secs_f64() * 1000.0 / acquire_count as f64
    } else {
        0.0
    };
    eprintln!(
        "timing: {} frames in {:.1}s ({:.0}ms/frame), {acquire_count} acquires",
        frames.len(),
        secs(total),
        secs(total) * 1000.0 / n
    );
    eprintln!(
        "  decode {:.1}s  luma {:.1}s  composite {:.1}s  recover(GPU) {:.1}s  observe {:.1}s  acquire(det+rec) {:.1}s  encode(pipe) {:.1}s",
        secs(td_decode),
        secs(td_build),
        secs(td_composite),
        secs(td_recover),
        secs(td_observe),
        secs(td_acquire),
        secs(td_encode)
    );
    eprintln!(
        "  per-frame avg: composite {:.1}ms  recover {:.1}ms  observe {:.1}ms | per-acquire avg: {:.0}ms",
        secs(td_composite) * 1000.0 / n,
        secs(td_recover) * 1000.0 / n,
        secs(td_observe) * 1000.0 / n,
        per_acq
    );
    eprintln!("wrote {}/composite.mp4 + summary.jsonl", dump_dir.display());
}

/// Draw resident boxes coloured by status, a per-box deviation bar, and a top action
/// band + frame-progress tick. No font dependency — frame index lives in the jsonl,
/// synchronised one-to-one with the mp4.
#[allow(clippy::too_many_arguments)]
fn annotate(
    rgba: &RgbaImage,
    resident: &[ResidentBox],
    devs: &HashMap<u64, (usize, usize, u32, bool)>,
    changed: &[u64],
    action: &str,
    idx: usize,
    total: usize,
    warmup: u32,
) -> RgbaImage {
    const GREEN: Rgba<u8> = Rgba([0, 200, 0, 255]);
    const RED: Rgba<u8> = Rgba([220, 0, 0, 255]);
    const MAGENTA: Rgba<u8> = Rgba([220, 0, 220, 255]);
    const BLUE: Rgba<u8> = Rgba([0, 120, 255, 255]);
    const GRAY: Rgba<u8> = Rgba([140, 140, 140, 255]);
    let (w, h) = (rgba.width(), rgba.height());
    let mut img = rgba.clone();

    // Render the pill footprints (what the recovery sees): darken the page under each
    // resident box so the pills are visible over the content, the way they are on screen.
    for b in resident {
        let (x, y, bw, bh) = aabb(&b.rect, w, h);
        for yy in y..(y + bh).min(h) {
            for xx in x..(x + bw).min(w) {
                let px = img.get_pixel_mut(xx, yy);
                px.0[0] = (px.0[0] as u32 * 3 / 10) as u8;
                px.0[1] = (px.0[1] as u32 * 3 / 10) as u8;
                px.0[2] = (px.0[2] as u32 * 3 / 10) as u8;
            }
        }
    }

    for b in resident {
        let (x, y, bw, bh) = aabb(&b.rect, w, h);
        let warming = (idx.saturating_sub(b.acquired_frame) as u32) < warmup;
        let color = if changed.contains(&b.id) {
            RED
        } else if warming {
            GRAY
        } else {
            GREEN
        };
        draw_hollow_rect_mut(
            &mut img,
            IpRect::at(x as i32, y as i32).of_size(bw, bh),
            color,
        );
        // Bar along the box bottom: width ∝ change-fraction vs its trip threshold.
        if let Some(&(_holes, frac_pct, _maxd, changed_box)) = devs.get(&b.id) {
            let thr = prod_monitor_config().hard_frac * 100.0;
            let fill = (frac_pct as f32 / (thr * 2.0)).clamp(0.0, 1.0);
            let barw = ((bw as f32 * fill) as u32).max(1).min(bw);
            let bar = if changed_box { RED } else { GREEN };
            let by = (y + bh).min(h.saturating_sub(2));
            draw_filled_rect_mut(
                &mut img,
                IpRect::at(x as i32, by as i32).of_size(barw, 2),
                bar,
            );
        }
    }

    let band = match action {
        "hide" => MAGENTA,
        "drop" => RED,
        "acquire" => BLUE,
        _ => GREEN,
    };
    draw_filled_rect_mut(&mut img, IpRect::at(0, 0).of_size(w, 6), band);
    // Frame-progress tick along the very top row.
    if total > 1 {
        let tx = ((w.saturating_sub(1)) as usize * idx / (total - 1)) as i32;
        draw_filled_rect_mut(
            &mut img,
            IpRect::at(tx.max(0), 0).of_size(2, 6),
            Rgba([255, 255, 255, 255]),
        );
    }
    img
}
