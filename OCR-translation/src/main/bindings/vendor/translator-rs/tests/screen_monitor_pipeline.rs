//! Tier-1 end-to-end test for the per-box screen monitor
//! (`SCREEN_CHANGE_DETECTION.md`): the real PP-OCR detect + recognize models on a
//! real frame, with the monitor fed through the real GPU pinhole recovery — our
//! opaque pill is composited over the source and the [`LatticeProbe`] shader
//! reads the holes back, exactly as on a MediaProjection capture.
//!
//! It validates the two cases the design exists for:
//!   * background motion *between* a subtitle's strokes is ignored (the stroke
//!     mask, derived by binarizing a real text crop, keeps the box Quiet);
//!   * replacing the text under the box trips a re-OCR, and the real recognizer
//!     confirms the text genuinely changed.
//!
//! Requires the PP-OCR model files and the fixture images — it hard-fails rather
//! than skipping, so the test can't silently rot. Models are pinned; fixtures are
//! repo-relative (set them up locally / in CI).

#![cfg(all(feature = "ppocr", feature = "gpu"))]

use std::path::{Path, PathBuf};

use glow::HasContext;
use image::imageops::FilterType;
use image::{GrayImage, ImageReader, RgbaImage};
use khronos_egl as egl;
use std::ffi::c_void;
use translator::DetectedTextBox;
use translator::PpocrScript;
use translator::gl_renderer::GlesRenderer;
use translator::live_frame::OrientedImage;
use translator::ocr::{OrientedRect, Rect};
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};
use translator::screen_monitor::{FrameClassification, Lattice, MonitorConfig, ScreenMonitor};

const DET_MAX_PIXELS: u32 = 1_000_000;
const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const FIXTURE_DIR: &str = "files/live-overlay";
const LATTICE_SPACING: f32 = 5.0;
const PILL_LUMA: u8 = 0; // opaque black pill, matching the screen overlay
/// Luma distance from the box mean that marks a lattice hole as on-ink.
const INK_CONTRAST: f32 = 35.0;
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

fn load_engine() -> PpocrEngine {
    let det = Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn");
    let rec = Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn");
    let keys = Path::new(MODEL_DIR).join("latin_PP-OCRv5_keys.txt");
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

fn fixture(name: &str) -> RgbaImage {
    let path = PathBuf::from(FIXTURE_DIR).join(name);
    assert!(
        path.exists(),
        "required fixture missing: {}",
        path.display()
    );
    ImageReader::open(&path)
        .expect("open fixture")
        .decode()
        .expect("decode fixture")
        .to_rgba8()
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
    /// changes (the fixtures are constant-size, so this fires once).
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

/// Real detect + recognize on a CPU-built frame. Returns the full-res canonical
/// `gray` (the screen we composite the pill over) plus per-box recognized text.
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
    assert_eq!(lines.len(), boxes.len(), "rec output aligns with boxes");
    let texts = lines.into_iter().map(|l| l.text).collect();
    (frame.gray, boxes, texts)
}

/// Composite our opaque pill + 50%-alpha hole grid over a luma source, producing
/// the frame a MediaProjection capture hands us: pill colour everywhere in the
/// box, except the lattice hole pixels (a half blend of pill and screen).
fn composite_capture(source: &[u8], w: u32, h: u32, lat: &Lattice, pill: &PillRegion) -> Vec<u8> {
    let inside =
        |x: f32, y: f32| (x - pill.cx).abs() <= pill.half_w && (y - pill.cy).abs() <= pill.half_h;
    let mut cap = source.to_vec();
    for y in 0..h {
        for x in 0..w {
            if inside(x as f32 + 0.5, y as f32 + 0.5) {
                cap[(y * w + x) as usize] = PILL_LUMA;
            }
        }
    }
    for p in lat.points() {
        if !inside(p.x, p.y) {
            continue;
        }
        let (px, py) = ((p.x as u32).min(w - 1), (p.y as u32).min(h - 1));
        let s = source[(py * w + px) as usize] as u16;
        cap[(py * w + px) as usize] = ((PILL_LUMA as u16 + s + 1) / 2) as u8;
    }
    cap
}

fn paste_into_box(dst: &mut RgbaImage, src: &RgbaImage, b: &OrientedRect) {
    let bw = (b.width.round() as u32).max(1).min(
        dst.width()
            .saturating_sub((b.cx - b.width / 2.0).max(0.0) as u32),
    );
    let bh = (b.height.round() as u32).max(1).min(
        dst.height()
            .saturating_sub((b.cy - b.height / 2.0).max(0.0) as u32),
    );
    let left = (b.cx - b.width / 2.0).round().max(0.0) as u32;
    let top = (b.cy - b.height / 2.0).round().max(0.0) as u32;
    if bw == 0 || bh == 0 {
        return;
    }
    let resized = image::imageops::resize(src, bw, bh, FilterType::Triangle);
    for (dx, dy, px) in resized.enumerate_pixels() {
        dst.put_pixel(left + dx, top + dy, *px);
    }
}

const MIN_HOLES: usize = 4;

fn cfg() -> MonitorConfig {
    MonitorConfig {
        warmup_frames: 4,
        // Real-photo fixtures (book/sign) are mid-contrast, not black-on-white, so the
        // hard threshold is lower here than the screen default (110); the jitter below
        // is kept well under it.
        hard_threshold: 55,
        hard_frac: 0.15,
        scroll_frac: 0.7,
        scroll_min_boxes: 2,
    }
}

#[test]
fn subtitle_change_detected_background_motion_ignored() {
    let _ = env_logger::builder().is_test(true).try_init();
    let engine = load_engine();
    let base_rgba = fixture("book.jpg");
    let other_rgba = fixture("sign.jpg");
    let mut gpu = Gpu::new();

    // Real detect + rec on the base frame.
    let (gray_a, boxes_a, texts_a) = detect_and_rec(&engine, &base_rgba);
    assert!(!boxes_a.is_empty(), "expected detections on the base frame");
    let (w, h) = (gray_a.width(), gray_a.height());
    let lat = Lattice::build(w, h, LATTICE_SPACING);

    // Monitor the densest recognized box, sampling its CONTOUR (tight to the text run)
    // so the hard-swing fraction isn't diluted by background margin.
    let (idx, holes) = boxes_a
        .iter()
        .enumerate()
        .filter(|(i, _)| !texts_a[*i].trim().is_empty())
        .map(|(i, b)| {
            let c = lat.holes_in_polygon(&b.contour);
            let h = if c.len() >= MIN_HOLES {
                c
            } else {
                lat.holes_in_rect(&b.tight_box)
            };
            (i, h)
        })
        .max_by_key(|(_, h)| h.len())
        .expect("a non-empty recognized box");
    let text_a = texts_a[idx].trim().to_string();
    let monitored_box = boxes_a[idx].tight_box.clone();
    let pill = PillRegion::from_oriented(&monitored_box);
    assert!(!text_a.is_empty(), "the monitored box recognized some text");
    assert!(
        holes.len() >= MIN_HOLES,
        "box lattice coverage: {}",
        holes.len()
    );
    eprintln!("monitored box text: {text_a:?} ({} holes)", holes.len());

    // Baseline: recover screen_est through the pill holes from the composited base.
    let src_a = gray_a.as_raw().clone();
    let (grid_cols, grid_rows) = (lat.cols(), lat.rows());
    let cap_base = composite_capture(&src_a, w, h, &lat, &pill);
    let baseline = gpu.recover(&cap_base, w, h, &lat, PILL_LUMA, &[pill]);

    // Binarize bootstrap on the recovered baseline.
    let box_lumas: Vec<f32> = holes.iter().map(|&i| baseline[i][0] as f32).collect();
    let mean = box_lumas.iter().sum::<f32>() / box_lumas.len() as f32;
    let bootstrap: Vec<bool> = box_lumas
        .iter()
        .map(|&l| (l - mean).abs() > INK_CONTRAST)
        .collect();
    let ink = bootstrap.iter().filter(|b| **b).count();
    assert!(ink >= MIN_HOLES, "ink holes via recovery: {ink}");

    let mut mon = ScreenMonitor::new(lat, cfg());
    mon.set_box(1, holes.clone(), &baseline);

    // Warmup + "video between the strokes": jitter the off-ink hole pixels in the
    // source, composite the pill over it, recover through the holes. Stays Quiet.
    let hole_points: Vec<(usize, u32, u32)> = holes
        .iter()
        .enumerate()
        .map(|(k, &i)| {
            let p = mon.lattice().points()[i];
            (k, (p.x as u32).min(w - 1), (p.y as u32).min(h - 1))
        })
        .collect();
    for f in 0..6u32 {
        let mut src = src_a.clone();
        for &(k, px, py) in &hole_points {
            if !bootstrap[k] {
                src[(py * w + px) as usize] = if (f + px) % 2 == 0 { 90 } else { 120 };
            }
        }
        let rec = gpu.recover(
            &composite_capture(&src, w, h, mon.lattice(), &pill),
            w,
            h,
            mon.lattice(),
            PILL_LUMA,
            &[pill],
        );
        assert_eq!(
            mon.observe(&rec),
            FrameClassification::Quiet,
            "frame {f}: background jitter under the pill must not trip the box"
        );
    }

    // Replace the text under the box and re-run the real pipeline.
    let mut changed_rgba = base_rgba.clone();
    paste_into_box(&mut changed_rgba, &other_rgba, &monitored_box);
    let (gray_b, boxes_b, texts_b) = detect_and_rec(&engine, &changed_rgba);
    assert_eq!(
        (gray_b.width(), gray_b.height()),
        (w, h),
        "frame dims stable"
    );

    let cap_changed = composite_capture(gray_b.as_raw(), w, h, mon.lattice(), &pill);
    let rec_b = gpu.recover(&cap_changed, w, h, mon.lattice(), PILL_LUMA, &[pill]);
    match mon.observe(&rec_b) {
        FrameClassification::BoxesChanged(ids) => assert!(ids.contains(&1), "{ids:?}"),
        other => panic!("expected the monitored box to change, got {other:?}"),
    }

    // One-shot artifact dump for inspection (set SCREEN_MONITOR_DUMP_DIR).
    if let Ok(dir) = std::env::var("SCREEN_MONITOR_DUMP_DIR") {
        std::fs::create_dir_all(&dir).expect("create dump dir");
        let save = |name: &str, buf: &[u8], iw: u32, ih: u32| {
            GrayImage::from_raw(iw, ih, buf.to_vec())
                .expect("gray buffer")
                .save(PathBuf::from(&dir).join(name))
                .expect("save png");
        };
        save("01_input.png", &src_a, w, h);
        save("02_pills_holes.png", &cap_base, w, h);
        save("03_pills_holes_new_text.png", &cap_changed, w, h);
        // What the shader reads back (recovered screen_est at the lattice). RGB → take
        // a channel for the gray dump (the synthetic fixtures are grayscale).
        let chan = |v: &[[u8; 3]]| v.iter().map(|p| p[0]).collect::<Vec<u8>>();
        save(
            "04_recovered_baseline.png",
            &chan(&baseline),
            grid_cols,
            grid_rows,
        );
        save(
            "05_recovered_changed.png",
            &chan(&rec_b),
            grid_cols,
            grid_rows,
        );
        eprintln!("dumped screen-monitor artifacts to {dir}");
    }

    // The recognizer confirms the text under the box is no longer what it was.
    let center = (monitored_box.cx, monitored_box.cy);
    let text_b = boxes_b
        .iter()
        .zip(&texts_b)
        .find(|(b, _)| {
            center.0 >= b.tight_box.cx - b.tight_box.width / 2.0
                && center.0 <= b.tight_box.cx + b.tight_box.width / 2.0
                && center.1 >= b.tight_box.cy - b.tight_box.height / 2.0
                && center.1 <= b.tight_box.cy + b.tight_box.height / 2.0
        })
        .map(|(_, t)| t.trim().to_string())
        .unwrap_or_default();
    eprintln!("text under box after change: {text_b:?}");
    assert_ne!(
        text_b, text_a,
        "recognizer should read different text under the box"
    );
}
