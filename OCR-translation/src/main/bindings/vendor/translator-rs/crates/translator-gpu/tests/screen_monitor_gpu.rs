//! Validates the screen monitor's GPU recovery by driving the **real device
//! path**: the captured frame (our opaque pill + 50%-alpha pinhole grid over a
//! synthetic screen) is handed to `gl_renderer` as an external-OES camera
//! texture, `RecProgram` recovers `screen_est` at every lattice hole via the
//! shipping `(raw − (1−f)·pill)/f` inversion, and the recovered grid drives the
//! real [`ScreenMonitor`] classifier.
//!
//! Runs headless on a surfaceless GLES3 context (Mesa llvmpipe / radeonsi). The
//! `samplerExternalOES` input is fed by wrapping a `GL_TEXTURE_2D` in an EGLImage
//! (`EGL_KHR_gl_texture_2D_image` + `GL_OES_EGL_image_external`) and binding it
//! to `GL_TEXTURE_EXTERNAL_OES`, so the on-device shader runs unchanged off-device
//! — there is no separate host re-implementation to keep in sync.

use std::ffi::c_void;

use glow::HasContext;
use khronos_egl as egl;
use translator_core::ocr::OrientedRect;
use translator_gpu::gl_renderer::GlesRenderer;
use translator_tracker::screen_monitor::{
    FrameClassification, Lattice, MonitorConfig, ScreenMonitor,
};

const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;
const TEXTURE_EXTERNAL_OES: u32 = 0x8D65;

const W: u32 = 120;
const H: u32 = 88;
// Even integer pitch: the lattice lands on integer canonical coords, so a 1×
// camera texture under an identity uv-transform samples each hole pixel-exact
// (the device's 2×-mirror snap collapses to the integer texel).
const SPACING: f32 = 4.0;
const PILL_LUMA: u8 = 0; // opaque black pill, matching the screen overlay
const SCREEN_FRAC: f32 = 0.5; // the 50% hole blend the renderer punches
const BG: u8 = 230; // band background between strokes (page-like, high contrast)
const INK: u8 = 20; // dark text strokes
const IDENTITY_H: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// The monitored band: full width, rows ~[56,72).
fn band_rect() -> OrientedRect {
    OrientedRect {
        cx: W as f32 / 2.0,
        cy: 64.0,
        width: W as f32,
        height: 16.0,
        angle_radians: 0.0,
    }
}

fn in_band(y: u32) -> bool {
    (56..72).contains(&y)
}

/// Axis-aligned pill footprint `(cx, cy, half_w, half_h)` in canonical coords,
/// the form `read_lattice_screen_est` occludes against.
fn pill_tuple(r: &OrientedRect) -> (f32, f32, f32, f32) {
    let (c, s) = (r.angle_radians.cos().abs(), r.angle_radians.sin().abs());
    let (hw, hh) = (r.width * 0.5, r.height * 0.5);
    (r.cx, r.cy, hw * c + hh * s, hw * s + hh * c)
}

/// The screen behind the overlay: striped dark text in the band, page elsewhere.
fn screen_frame(strokes: bool) -> Vec<u8> {
    let mut v = vec![BG; (W * H) as usize];
    for y in 0..H {
        if !in_band(y) {
            continue;
        }
        for x in 0..W {
            // 4px ink / 12px gap: background-dominated "letters".
            let ink = strokes && (x % 16) < 4;
            v[(y * W + x) as usize] = if ink { INK } else { BG };
        }
    }
    v
}

/// The frame a capture hands us: the screen, plus our overlay composited on top.
/// Inside the pill the holes carry `0.5·pill + 0.5·screen` and everything else is
/// the opaque pill; outside the pill the screen shows through. Only lattice holes
/// are ever sampled, so non-hole pill pixels are cosmetic.
fn composite_capture(screen: &[u8], lat: &Lattice, pill: &(f32, f32, f32, f32)) -> Vec<u8> {
    let mut cap = screen.to_vec();
    let occluded = |x: f32, y: f32| (x - pill.0).abs() <= pill.2 && (y - pill.1).abs() <= pill.3;
    for y in 0..H {
        for x in 0..W {
            if occluded(x as f32 + 0.5, y as f32 + 0.5) {
                cap[(y * W + x) as usize] = PILL_LUMA;
            }
        }
    }
    for p in lat.points() {
        if !occluded(p.x, p.y) {
            continue;
        }
        let (px, py) = (p.x as u32, p.y as u32);
        if px >= W || py >= H {
            continue;
        }
        let s = screen[(py * W + px) as usize] as u16;
        cap[(py * W + px) as usize] = ((PILL_LUMA as u16 + s + 1) / 2) as u8;
    }
    cap
}

/// A surfaceless GLES3 context plus a `GlesRenderer` whose external camera is an
/// EGLImage-backed texture we re-fill per frame. The EGL handles are leaked
/// deliberately: they must outlive every GL call and the process exits at test end.
struct Gpu {
    gl: glow::Context,
    renderer: GlesRenderer,
    src_tex: glow::Texture,
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

        // Source GL_TEXTURE_2D: allocate level 0 (required before wrapping it in an
        // EGLImage), then per frame we glTexSubImage2D into the same storage so the
        // image — and the external texture aliasing it — see the update.
        let src_tex = unsafe { gl.create_texture() }.expect("create src tex");
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(src_tex));
            set_nearest_clamp(&gl, glow::TEXTURE_2D);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                glow::RGBA as i32,
                W as i32,
                H as i32,
                0,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&vec![0u8; (W * H * 4) as usize])),
            );
        }

        // Wrap the source texture as an EGLImage and bind it to an external-OES
        // texture — the exact input shape gl_renderer expects from the camera.
        let image = lib
            .create_image(
                display,
                ctx,
                egl::GL_TEXTURE_2D as egl::Enum,
                unsafe { egl::ClientBuffer::from_ptr(src_tex.0.get() as usize as *mut c_void) },
                &[egl::ATTRIB_NONE],
            )
            .expect("create EGLImage from GL texture (needs EGL_KHR_gl_texture_2D_image)");
        let image_target: extern "system" fn(u32, *const c_void) = unsafe {
            std::mem::transmute(
                lib.get_proc_address("glEGLImageTargetTexture2DOES")
                    .expect("glEGLImageTargetTexture2DOES (needs GL_OES_EGL_image_external)"),
            )
        };
        let ext_tex = unsafe { gl.create_texture() }.expect("create ext tex");
        unsafe {
            gl.bind_texture(TEXTURE_EXTERNAL_OES, Some(ext_tex));
            image_target(TEXTURE_EXTERNAL_OES, image.as_ptr() as *const c_void);
            set_nearest_clamp(&gl, TEXTURE_EXTERNAL_OES);
            gl.bind_texture(TEXTURE_EXTERNAL_OES, None);
        }

        let mut renderer = renderer;
        renderer.set_camera_external(ext_tex.0.get(), IDENTITY_H);
        Gpu {
            gl,
            renderer,
            src_tex,
        }
    }

    /// Upload a luma capture into the external camera texture (luma replicated to
    /// RGB so the per-channel recovery sees a gray frame). Uploaded bottom-up: the
    /// recovery shader applies the acquire's `1 − v` Y-flip, so a top-down
    /// canonical capture must land flipped in the texture to read back upright.
    fn upload(&self, capture_luma: &[u8]) {
        assert_eq!(capture_luma.len(), (W * H) as usize);
        let mut rgba = Vec::with_capacity(capture_luma.len() * 4);
        for ty in 0..H {
            let row = H - 1 - ty;
            for x in 0..W {
                let v = capture_luma[(row * W + x) as usize];
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        unsafe {
            self.gl.bind_texture(glow::TEXTURE_2D, Some(self.src_tex));
            self.gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            self.gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                W as i32,
                H as i32,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(Some(&rgba)),
            );
            self.gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Recover `screen_est` at every lattice point through the real renderer.
    fn recover(&mut self, lat: &Lattice, pills: &[(f32, f32, f32, f32)]) -> Vec<[u8; 3]> {
        self.renderer
            .read_lattice_screen_est(
                lat.cols(),
                lat.rows(),
                W,
                H,
                SPACING,
                pills,
                PILL_LUMA as f32 / 255.0,
                SCREEN_FRAC,
            )
            .expect("read_lattice_screen_est (rec shader / external camera)")
    }
}

fn set_nearest_clamp(gl: &glow::Context, target: u32) {
    unsafe {
        gl.tex_parameter_i32(target, glow::TEXTURE_MIN_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_MAG_FILTER, glow::NEAREST as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
        gl.tex_parameter_i32(target, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
    }
}

#[test]
fn shader_recovers_screen_under_the_pill() {
    let mut gpu = Gpu::new();
    let lat = Lattice::build(W, H, SPACING);
    let pill = pill_tuple(&band_rect());
    let screen = screen_frame(true);
    gpu.upload(&composite_capture(&screen, &lat, &pill));

    let recovered = gpu.recover(&lat, &[pill]);
    assert_eq!(recovered.len(), lat.len());

    // Compare interior points only: the per-row stagger pushes a few edge holes
    // past the frame, where CLAMP_TO_EDGE reads a neighbour. Those holes fall
    // outside any monitored box, so they don't affect the classifier.
    let mut max_err = 0u32;
    let mut compared = 0u32;
    for (p, rec) in lat.points().iter().zip(&recovered) {
        let (px, py) = (p.x as u32, p.y as u32);
        if px >= W || py >= H {
            continue;
        }
        let want = screen[(py * W + px) as usize];
        let err = (rec[0] as i32 - want as i32).unsigned_abs();
        max_err = max_err.max(err);
        compared += 1;
    }
    eprintln!("interior holes compared: {compared}, recovery max error: {max_err}");
    assert!(compared > 100, "too few interior holes ({compared})");
    assert!(
        max_err <= 3,
        "recovered screen_est diverged: max_err={max_err}"
    );
}

#[test]
fn recovered_samples_drive_the_classifier() {
    let mut gpu = Gpu::new();
    let lat = Lattice::build(W, H, SPACING);
    let pill = pill_tuple(&band_rect());
    let cfg = MonitorConfig {
        warmup_frames: 4,
        hard_threshold: 110,
        // Sparse ink (~25% of band holes), so erasing it flips ~25%; trip below that.
        hard_frac: 0.15,
        scroll_frac: 0.7,
        scroll_min_boxes: 2,
    };

    let base_screen = screen_frame(true);
    gpu.upload(&composite_capture(&base_screen, &lat, &pill));
    let baseline = gpu.recover(&lat, &[pill]);

    let holes = lat.holes_in_rect(&band_rect());
    let mut mon = ScreenMonitor::new(lat, cfg);
    mon.set_box(1, holes, &baseline);

    // Warmup + background jitter between the strokes (the ink holds), all through
    // the GPU recovery. Must stay Quiet.
    for f in 0..6u32 {
        let mut screen = base_screen.clone();
        for y in 56..72 {
            for x in 0..W {
                if (x % 16) >= 4 {
                    // Background column: moderate, temporally-coherent motion
                    // (Δ < hard_threshold), a playing video rather than a strobe.
                    screen[(y * W + x) as usize] = if (f + x) % 2 == 0 { 180 } else { 254 };
                }
            }
        }
        gpu.upload(&composite_capture(&screen, mon.lattice(), &pill));
        let rec = gpu.recover(mon.lattice(), &[pill]);
        assert_eq!(
            mon.observe(&rec),
            FrameClassification::Quiet,
            "frame {f}: background jitter under the pill must not trip the box"
        );
    }

    // Text erased (subtitle advanced to blank): the stroke holes now read
    // background, so the box must trip through the same GPU recovery path.
    let changed = screen_frame(false);
    gpu.upload(&composite_capture(&changed, mon.lattice(), &pill));
    let rec = gpu.recover(mon.lattice(), &[pill]);
    match mon.observe(&rec) {
        FrameClassification::BoxesChanged(ids) => assert!(ids.contains(&1), "{ids:?}"),
        other => panic!("expected the box to trip, got {other:?}"),
    }
}
