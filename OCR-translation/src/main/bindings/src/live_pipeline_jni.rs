//! Single-call JNI fast-path for the live-camera planar OCR pipeline.
//!
//! Per-frame Kotlin calls `LivePipelineJni.processFrameGl(pipelinePtr,
//! rendererPtr, cameraTexId, canonicalW/H, surfaceW/H, uvXform,
//! displayXform, tsNs)` on the GL render thread. The renderer borrows the
//! camera's `GL_TEXTURE_EXTERNAL_OES` (from CameraX `Preview` →
//! `SurfaceTexture`), GPU-renders the canonical luma into a small `Vec<u8>`
//! to feed the tracker, then composites the external camera + overlay
//! straight into the EGL surface. No CPU camera bytes cross the JNI seam;
//! per-frame transfer is one ~650 KB R8 readback instead of a full-res
//! RGBA copy + CPU luma walk.
//!
//! Acquire/refresh path: when the tracker needs full-res RGBA, the shared
//! [`translator::live_gpu_tick::run_tracker_with_acquire`] helper does a
//! one-shot GPU readback inside this same JNI call and feeds the worker.
//!
//! Returns a packed `jlong` carrying tracker state + anchor id + inliers +
//! ok flag so Kotlin doesn't need a follow-up uniffi call for the debug
//! pill update. Detailed async-job telemetry is *not* packed in — Kotlin
//! polls `LivePlanarTracker.lastAcquireTelemetry()` for that.

#![cfg(feature = "planar-tracker")]

use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jint, jlong};

use translator::live_tracker_pipeline::{LiveTrackerPipeline, PlanarTrackerState};

/// Pack the per-frame result into a single `jlong`. Layout (bit 63 →
/// bit 0):
///
///   - 63..62 : state (Idle=0, Acquiring=1, Locked=2, Lost=3)
///   - 61     : composite_ok (1 if a frame was presented)
///   - 60..45 : inliers (16 bits, saturating)
///   - 44..29 : anchor_id low 16 bits — sequential, so the bottom bits
///              are stable across acquires until the engine wraps
///              (would take ~65 k acquires; debug pill loses uniqueness
///              past that but never blocks behavior)
///   - 28     : started_acquire
///   - 27     : started_refresh
///   - 20..0  : scale × 1024, fixed-point (21 bits, saturating) — the
///              tracked plane's magnification vs acquire, used by the
///              camera layer to drive focus distance without AF
///   - 26..21 : reserved (0)
fn pack_result(
    state: PlanarTrackerState,
    anchor_id: u64,
    inliers: u32,
    scale: f32,
    composite_ok: bool,
    started_acquire: bool,
    started_refresh: bool,
) -> jlong {
    let state_bits: u64 = match state {
        PlanarTrackerState::Idle => 0,
        PlanarTrackerState::Acquiring => 1,
        PlanarTrackerState::Locked => 2,
        PlanarTrackerState::Lost => 3,
    };
    let inliers_capped = inliers.min(0xFFFF) as u64;
    let anchor_lo = (anchor_id & 0xFFFF) as u64;
    let scale_fixed = ((scale.max(0.0) * 1024.0).round() as u64).min(0x1F_FFFF);
    let packed: u64 = (state_bits << 62)
        | (if composite_ok { 1u64 << 61 } else { 0 })
        | (inliers_capped << 45)
        | (anchor_lo << 29)
        | (if started_acquire { 1u64 << 28 } else { 0 })
        | (if started_refresh { 1u64 << 27 } else { 0 })
        | scale_fixed;
    packed as jlong
}

// ---------------------------------------------------------------------
// GPU per-frame path. Camera arrives as a GL_TEXTURE_EXTERNAL_OES; all
// three calls MUST run on the GL render thread Kotlin owns, with its EGL
// context current.
// ---------------------------------------------------------------------

#[cfg(feature = "gpu")]
use std::ffi::{CString, c_char, c_void};
#[cfg(feature = "gpu")]
use translator::gl_renderer::{GlesRenderer, PresentContent};
#[cfg(feature = "gpu")]
use translator::live_gpu_tick::{
    frame_from_camera_gray, run_tracker_with_acquire, screen_acquire_frame,
};
#[cfg(feature = "gpu")]
use translator::live_screen::{LiveScreenPipeline, MonitorAction};

#[cfg(feature = "gpu")]
#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(procname: *const c_char) -> *const c_void;
}

// dlopen/dlsym live in libc on Android; used to resolve core GLES2
// entry points that `eglGetProcAddress` may not (it only guarantees
// EGL/extension functions pre-API-18).
#[cfg(feature = "gpu")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

#[cfg(feature = "gpu")]
const RTLD_NOW: i32 = 2;

#[cfg(feature = "gpu")]
fn gl_proc(libgles: *mut c_void, name: &str) -> *const c_void {
    let Ok(c) = CString::new(name) else {
        return std::ptr::null();
    };
    if !libgles.is_null() {
        let p = unsafe { dlsym(libgles, c.as_ptr()) };
        if !p.is_null() {
            return p as *const c_void;
        }
    }
    unsafe { eglGetProcAddress(c.as_ptr()) }
}

/// Build a `GlesRenderer` on the calling thread. The caller (Kotlin's GL
/// render thread) must have created and made-current a GLES3 context on
/// the SurfaceView's surface first. Returns a raw `*mut GlesRenderer` as
/// a `jlong`, or 0 on failure; release it with [`destroyGlRenderer`] on
/// the same thread.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_createGlRenderer(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    let libgles = {
        // GLES3 first; fall back to GLES2 if libGLESv3.so isn't present
        // (some older devices still ship it as libGLESv2.so only).
        let v3 = CString::new("libGLESv3.so").unwrap();
        let p = unsafe { dlopen(v3.as_ptr(), RTLD_NOW) };
        if !p.is_null() {
            p
        } else {
            let v2 = CString::new("libGLESv2.so").unwrap();
            unsafe { dlopen(v2.as_ptr(), RTLD_NOW) }
        }
    };
    if libgles.is_null() {
        log::warn!("dlopen(libGLESv3/2.so) failed; relying on eglGetProcAddress only");
    }
    match GlesRenderer::new(|name| gl_proc(libgles, name)) {
        Ok(r) => Box::into_raw(Box::new(r)) as jlong,
        Err(e) => {
            log::error!("GlesRenderer::new failed: {e:?}");
            0
        }
    }
}

#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_destroyGlRenderer(
    _env: JNIEnv,
    _class: JClass,
    renderer_ptr: jlong,
) {
    if renderer_ptr != 0 {
        // SAFETY: ptr came from `createGlRenderer`'s `Box::into_raw`,
        // dropped exactly once, on the GL thread that owns the context.
        unsafe { drop(Box::from_raw(renderer_ptr as *mut GlesRenderer)) };
    }
}

/// Switch a renderer to overlay-only present: subsequent `processFrameGl`
/// calls draw only the overlays over a transparent clear, skipping the camera
/// passthrough, for a translucent window floating over live content (the
/// MediaProjection screen-translate overlay). Call once on the GL thread after
/// [`createGlRenderer`]. The tracker gray/RGBA readbacks are unaffected.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_setRendererOverlayOnly(
    _env: JNIEnv,
    _class: JClass,
    renderer_ptr: jlong,
) {
    if renderer_ptr == 0 {
        return;
    }
    // SAFETY: ptr came from `createGlRenderer`; used only on its owning thread.
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };
    renderer.set_present_content(PresentContent::OverlayOnly);
}

/// Set the renderer's parametric overlay opacity (0..1). Camera leaves it at
/// the 1.0 default; the screen path sets it to control overlay opacity
/// independently of the touch-capped window alpha. Call on the GL thread.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_setRendererOverlayAlpha(
    _env: JNIEnv,
    _class: JClass,
    renderer_ptr: jlong,
    alpha: f32,
) {
    if renderer_ptr == 0 {
        return;
    }
    // SAFETY: ptr came from `createGlRenderer`; used only on its owning thread.
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };
    renderer.set_overlay_alpha(alpha);
}

/// Screen-translate acquire dispatch: GPU-render the two OCR inputs (detector
/// gray at the 32-aligned size, recognition RGBA at half canonical) off the
/// captured external texture, then hand the frame to the background worker —
/// non-blocking. The heavy detect/rec/translate runs off the GL thread; results
/// land in the session and are presented by [`screenPresentOverlayGl`] when
/// [`screenAcquireState`] reports a new overlay version. Returns 1 if dispatched.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenDispatchAcquire(
    env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
    renderer_ptr: jlong,
    camera_tex_id: jint,
    canonical_w: jint,
    canonical_h: jint,
    surface_w: jint,
    surface_h: jint,
    uv_xform: jni::objects::JFloatArray,
) -> jint {
    if pipeline_ptr == 0
        || renderer_ptr == 0
        || camera_tex_id <= 0
        || canonical_w <= 0
        || canonical_h <= 0
        || surface_w <= 0
        || surface_h <= 0
    {
        return 0;
    }
    // SAFETY: ptr from `LiveScreenTracker::raw_address_for_jni`; Kotlin holds the
    // Arc across the call. Renderer used only on its owning (GL) thread.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };

    // The worker is still draining a prior (likely just-aborted) job — skip the
    // readback entirely and let the monitor retry next tick (it stays Settling).
    if pipeline.acquire_busy() {
        return 0;
    }
    let mut uv = [0f32; 9];
    if env.get_float_array_region(&uv_xform, 0, &mut uv).is_err() {
        return 0;
    }
    let cw = canonical_w as u32;
    let ch = canonical_h as u32;
    let canonical_long = cw.max(ch).max(1) as f32;
    let surface_long = surface_w.max(surface_h).max(1) as f32;
    // Set before dispatch: the worker bakes this oversample into the rendered
    // canvas (overlays authored in canonical coords, presented at full res).
    pipeline.set_overlay_oversample(surface_long / canonical_long);

    let Some(frame) = screen_acquire_frame(
        renderer,
        camera_tex_id as u32,
        cw,
        ch,
        uv,
        pipeline.det_max_pixels(),
        pipeline.region(),
    ) else {
        return 0;
    };
    if pipeline.dispatch_acquire(frame) {
        1
    } else {
        0
    }
}

/// Present the screen pipeline's resident overlay canvas straight into the bound
/// EGL window surface (the overlay `TextureView`): render it first if a deferred
/// upsert left it stale, then upload it to a texture and draw one premultiplied
/// quad — no Bitmap, no CPU readback. The caller (the GL worker) must have made
/// the window surface current; it swaps after. `canonical_*` is the OCR frame
/// size the overlay origin is in, `surface_*` the display/window size. Returns 1
/// if a frame was drawn, 0 when there's nothing to show.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenPresentOverlayGl(
    _env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
    renderer_ptr: jlong,
    canonical_w: jint,
    canonical_h: jint,
    surface_w: jint,
    surface_h: jint,
) -> jint {
    if pipeline_ptr == 0
        || renderer_ptr == 0
        || canonical_w <= 0
        || canonical_h <= 0
        || surface_w <= 0
        || surface_h <= 0
    {
        return 0;
    }
    // SAFETY: see `screenDispatchAcquire` — same ownership contract; the renderer
    // is used only on its owning (GL) thread.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };
    // Build the GPU draw list (pills + per-glyph instance data), bake it into the
    // overlay texture on the GPU (rounded pills opaque, then glyph quads), and present
    // that texture into the window surface — no CPU canvas raster. Screen pills
    // are opaque (the window alpha dims them) and text is opaque.
    //
    // Phase timing: `dl` = collect_overlay_glyphs (CPU text shaping + glyph mask
    // collection); `bake` = atlas upload + two-pass GPU composite; `present` = window blit.
    let t_dl = std::time::Instant::now();
    let Some(dl) = pipeline.overlay_draw_list() else {
        return 0;
    };
    let dl_ms = t_dl.elapsed().as_secs_f64() * 1000.0;
    let (n_pills, n_glyphs) = (dl.pills.len(), dl.glyphs.instances.len());
    let t_bake = std::time::Instant::now();
    // Punch the pinhole hole grid so the captured mirror exposes screen_est under
    // the pills for the v2 under-pill change detector. The hole's baked alpha is
    // sized against the effective opaque alpha (pill_alpha × window alpha).
    let (hole_radius, hole_alpha) = pipeline.hole_params(SCREEN_PILL_ALPHA * SCREEN_WINDOW_ALPHA);
    let holes = translator::gl_renderer::HoleGrid {
        spacing: pipeline.lattice_spacing(),
        radius: hole_radius,
        alpha: hole_alpha,
    };
    if !renderer.render_overlay_to_texture(
        &dl,
        SCREEN_PILL_ALPHA,
        SCREEN_TEXT_ALPHA,
        false,
        Some(holes),
    ) {
        return 0;
    }
    let bake_ms = t_bake.elapsed().as_secs_f64() * 1000.0;
    let t_present = std::time::Instant::now();
    let drawn = renderer.present_screen_overlay_fbo(
        canonical_w as u32,
        canonical_h as u32,
        surface_w as u32,
        surface_h as u32,
    );
    let present_ms = t_present.elapsed().as_secs_f64() * 1000.0;
    log::info!(
        "[screen-present] dl={dl_ms:.1}ms({n_pills}p/{n_glyphs}g) bake={bake_ms:.1} present={present_ms:.1}"
    );
    if drawn { 1 } else { 0 }
}

/// Screen overlay opacities baked into the overlay texture. Pills opaque (the
/// touch-capped window alpha does the dimming); text opaque for crispness.
#[cfg(feature = "gpu")]
const SCREEN_PILL_ALPHA: f32 = 1.0;
#[cfg(feature = "gpu")]
const SCREEN_TEXT_ALPHA: f32 = 1.0;
/// The overlay window's View alpha (touch-passthrough dimming). Must match
/// `ScreenTranslateService.WINDOW_ALPHA`; used to size the pinhole hole alpha so
/// the captured blend at a hole lands at the recovery's fixed fraction.
#[cfg(feature = "gpu")]
const SCREEN_WINDOW_ALPHA: f32 = 0.79;

/// Acquire state for the GL worker's poll loop: `(busy << 32) | overlay_version`.
/// `busy` = an acquire is in flight; `overlay_version` bumps each time the worker
/// upserts overlays (provisional, then full) so the worker knows to re-present.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenAcquireState(
    _env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
) -> jlong {
    if pipeline_ptr == 0 {
        return 0;
    }
    // SAFETY: see `screenDispatchAcquire`.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    let busy = if pipeline.acquire_busy() { 1u64 } else { 0 };
    let version = pipeline.overlay_version() & 0xFFFF_FFFF;
    ((busy << 32) | version) as jlong
}

/// Abort an in-flight screen acquire (the screen moved): bumps the generation so
/// the worker bails at the next rec batch.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenAbortAcquire(
    _env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
) {
    if pipeline_ptr == 0 {
        return;
    }
    // SAFETY: see `screenDispatchAcquire`.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    pipeline.abort_acquire();
}

/// Pack a [`MonitorAction`] + `wants_tick` into a jint for the GL worker:
/// bits 0-1 = action (None=0, Hide=1, Acquire=2), bit 8 = wants_tick.
#[cfg(feature = "gpu")]
fn pack_monitor(action: MonitorAction, wants_tick: bool) -> jint {
    let a = match action {
        MonitorAction::None => 0,
        MonitorAction::Hide => 1,
        MonitorAction::Acquire => 2,
    };
    let tick = if wants_tick { 1 << 8 } else { 0 };
    a | tick
}

/// Screen-translate change detection: GPU-read a coarse gray off the captured
/// external texture and feed it to the [`LiveScreenPipeline`] monitor, which
/// decides whether the screen moved (hide), settled (acquire), or is unchanged.
/// Returns the packed action + `wants_tick` (see [`pack_monitor`]). No overlay
/// readback — the heavy detect/rec only runs on the worker's follow-up
/// the worker (`screenDispatchAcquire`) once an `Acquire` is decided.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenMonitorFrameGl(
    env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
    renderer_ptr: jlong,
    camera_tex_id: jint,
    canonical_w: jint,
    canonical_h: jint,
    uv_xform: jni::objects::JFloatArray,
    now_ns: jlong,
) -> jint {
    if pipeline_ptr == 0
        || renderer_ptr == 0
        || camera_tex_id <= 0
        || canonical_w <= 0
        || canonical_h <= 0
    {
        return 0;
    }
    // SAFETY: see `screenDispatchAcquire` — same ownership contract.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };

    let mut uv = [0f32; 9];
    if env.get_float_array_region(&uv_xform, 0, &mut uv).is_err() {
        return 0;
    }
    let cw = canonical_w as u32;
    let ch = canonical_h as u32;
    // v2 per-box detector is the sole authority for the screen path: recover
    // screen_est through the pinhole holes and let the monitor decide. It ignores
    // background motion (text-stable → None), drops + re-acquires a box whose text
    // changed (→ Acquire), and clears + re-acquires on a whole-screen change /
    // navigation (→ Hide, then a settle-delayed Acquire). No coarse global diff, so
    // a video in one region no longer tears the whole overlay down.
    let (cols, rows) = pipeline.lattice_dims(cw, ch);
    let pills = pipeline.monitor_pill_aabbs();
    let (pill_luma, screen_frac) = pipeline.recovery_params();
    let spacing = pipeline.lattice_spacing();
    renderer.set_camera_external(camera_tex_id as u32, uv);
    let action = renderer
        .read_lattice_screen_est(cols, rows, cw, ch, spacing, &pills, pill_luma, screen_frac)
        .map(|samples| pipeline.monitor_screen_v2(&samples, cw, ch, now_ns))
        .unwrap_or(MonitorAction::None);
    pack_monitor(action, pipeline.wants_tick())
}

/// Timed tick for the screen monitor (no new frame): fires a pending settle so
/// the screen settles even when the mirror stops emitting frames. Returns the
/// packed action + `wants_tick`.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_screenMonitorTick(
    _env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
    now_ns: jlong,
) -> jint {
    if pipeline_ptr == 0 {
        return 0;
    }
    // SAFETY: see `screenDispatchAcquire`.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveScreenPipeline) };
    let action = pipeline.monitor_screen_v2_tick(now_ns);
    pack_monitor(action, pipeline.wants_tick())
}

/// DEBUG: read back the canonical RGBA frame (top-down) for on-device
/// inspection of orientation / mirror / text size. Returns an empty array on
/// failure. Must run on the GL thread after a `processFrameGl` so the external
/// source + uv transform are set on the renderer.
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_debugReadCanonicalRgba<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    renderer_ptr: jlong,
    canonical_w: jint,
    canonical_h: jint,
    display_xform: jni::objects::JFloatArray<'local>,
) -> jni::objects::JByteArray<'local> {
    if renderer_ptr == 0 || canonical_w <= 0 || canonical_h <= 0 {
        return env.new_byte_array(0).unwrap_or_default();
    }
    // SAFETY: ptr came from `createGlRenderer`; used only on its owning thread.
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };
    let mut dx = [0f32; 9];
    if env
        .get_float_array_region(&display_xform, 0, &mut dx)
        .is_err()
    {
        return env.new_byte_array(0).unwrap_or_default();
    }
    let Some(rgba) = renderer.read_camera_rgba(canonical_w as u32, canonical_h as u32, &dx) else {
        return env.new_byte_array(0).unwrap_or_default();
    };
    let Ok(arr) = env.new_byte_array(rgba.len() as i32) else {
        return env.new_byte_array(0).unwrap_or_default();
    };
    let signed: &[i8] =
        unsafe { std::slice::from_raw_parts(rgba.as_ptr() as *const i8, rgba.len()) };
    let _ = env.set_byte_array_region(&arr, 0, signed);
    arr
}

/// Per-frame GPU path: borrow the camera's external-OES texture, GPU-render
/// canonical luma into a small `Vec<u8>` for the tracker, then composite
/// the camera + overlay into the EGL surface (FBO 0, sized
/// `surface_w*surface_h`). On acquire/refresh, reads back full-res RGBA
/// from the same texture and feeds the worker. Caller (Kotlin GL thread)
/// follows with `eglSwapBuffers`.
///
/// Returns the packed result `jlong` (`composite_ok` reflects that a frame
/// was drawn; 0 means the texture wasn't ready or process_frame failed).
#[cfg(feature = "gpu")]
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_dev_davidv_translator_LivePipelineJni_processFrameGl(
    env: JNIEnv,
    _class: JClass,
    pipeline_ptr: jlong,
    renderer_ptr: jlong,
    camera_tex_id: jint,
    canonical_w: jint,
    canonical_h: jint,
    surface_w: jint,
    surface_h: jint,
    uv_xform: jni::objects::JFloatArray,
    display_xform: jni::objects::JFloatArray,
    timestamp_ns: jlong,
) -> jlong {
    if pipeline_ptr == 0
        || renderer_ptr == 0
        || camera_tex_id <= 0
        || canonical_w <= 0
        || canonical_h <= 0
        || surface_w <= 0
        || surface_h <= 0
    {
        return 0;
    }
    // SAFETY: Kotlin holds the wrapper Arc alive across the call; the
    // renderer ptr came from `createGlRenderer` and is used only on this
    // (its owning) thread.
    let pipeline = unsafe { &*(pipeline_ptr as *const LiveTrackerPipeline) };
    let renderer = unsafe { &mut *(renderer_ptr as *mut GlesRenderer) };

    let mut uv = [0f32; 9];
    let mut dx = [0f32; 9];
    if env.get_float_array_region(&uv_xform, 0, &mut uv).is_err()
        || env
            .get_float_array_region(&display_xform, 0, &mut dx)
            .is_err()
    {
        log::warn!("processFrameGl: bad xform array");
        return 0;
    }

    let cw = canonical_w as u32;
    let ch = canonical_h as u32;

    // Overlay glyphs rasterize at half the display footprint: the canonical
    // frame (tracker + OCR) stays cheap while the overlay renders near
    // display res, with the composite warp absorbing the residual ~2x.
    let canonical_long = cw.max(ch).max(1) as f32;
    let surface_long = surface_w.max(surface_h).max(1) as f32;
    pipeline.set_overlay_oversample(0.5 * surface_long / canonical_long);

    let Some(frame) = frame_from_camera_gray(renderer, camera_tex_id as u32, cw, ch, uv, dx) else {
        return 0;
    };

    // run_tracker_with_acquire presents on the GPU (it binds the window surface
    // itself), so no pre-bind here — read_camera_gray's R8 FBO is left bound and the
    // present re-targets FBO 0.
    match run_tracker_with_acquire(
        pipeline,
        renderer,
        &frame,
        cw,
        ch,
        surface_w as u32,
        surface_h as u32,
        dx,
        timestamp_ns as u64,
    ) {
        Ok(r) => pack_result(
            r.state,
            r.anchor_id,
            r.inliers,
            r.scale,
            r.composite_bytes > 0,
            r.started_acquire,
            r.started_refresh,
        ),
        Err(e) => {
            log::warn!("process_frame_gl failed: {e:?}");
            0
        }
    }
}
