//! Per-frame plumbing shared by the platform GPU bridges (Linux QtQuick,
//! Android EGL). Each bridge owns its own [`GlesRenderer`] and drives a
//! per-frame loop; the work between "external camera texture available" and
//! "composite drawn into the bound framebuffer" is identical.
//!
//! Split into two calls because the caller has to bind its own present
//! framebuffer + viewport in the middle — after the gray readback (which
//! leaves its own R8 FBO bound) and before [`LiveTrackerPipeline::process_frame`]
//! draws the composite.
//!
//! ```ignore
//! let Some(frame) = frame_from_camera_gray(gles, id, fw, fh, uv, dx) else { return };
//! // platform-specific: bind the present target
//! unsafe { gl.bind_framebuffer(FRAMEBUFFER, present_fbo); gl.viewport(0,0,sw,sh); }
//! run_tracker_with_acquire(pipeline, gles, &frame, fw, fh, dx, ts)?;
//! ```

use std::sync::Arc;

use crate::gl_renderer::GlesRenderer;
use translator_core::api::{TranslatorError, TranslatorErrorKind};
use translator_core::ocr::Rect;
use translator_live::live_screen::NormRect;
use translator_live::live_tracker_pipeline::{LiveTrackerPipeline, ProcessFrameResult};
use translator_raster::live_frame::{LiveFrame, OrientedImage, aligned_det_dims};

/// Row-major 3×3 mapping `w×h` dst-pixel coords → clip `[-1,1]` — the
/// resolution-independent normalize `read_camera_*` wants as `dst_to_clip`
/// (orientation lives in the `uv` transform set by [`frame_from_camera_gray`]).
pub fn clip_xform(w: u32, h: u32) -> [f32; 9] {
    let w = w.max(1) as f32;
    let h = h.max(1) as f32;
    [2.0 / w, 0.0, -1.0, 0.0, 2.0 / h, -1.0, 0.0, 0.0, 1.0]
}

/// Crop a tightly-packed `src_w`-wide image of `channels` bytes/pixel to the
/// pixel sub-rect `sub`, returning the packed sub-image and its dims.
fn crop_packed(src: &[u8], src_w: u32, channels: usize, sub: Rect) -> (Vec<u8>, u32, u32) {
    let (l, t) = (sub.left as usize, sub.top as usize);
    let (w, h) = (
        (sub.right - sub.left) as usize,
        (sub.bottom - sub.top) as usize,
    );
    let stride = src_w as usize * channels;
    let row_bytes = w * channels;
    let mut out = Vec::with_capacity(row_bytes * h);
    for row in 0..h {
        let start = (t + row) * stride + l * channels;
        out.extend_from_slice(&src[start..start + row_bytes]);
    }
    (out, w as u32, h as u32)
}

/// Borrow the external camera texture, GPU-render the canonical luma into a
/// `width*height` byte buffer, and wrap it in a fresh [`LiveFrame`] ready for
/// the tracker. Returns `None` if no external camera source is set or the
/// GLES extension is missing (matches [`GlesRenderer::read_camera_gray`]).
///
/// A fresh `Arc<LiveFrame>` per tick is deliberate: an in-flight acquire holds
/// its own frame's state lock for the whole det+rec (~1s); reusing one frame
/// would make this call's `reset_gray` block on that lock and freeze the
/// render thread for the entire acquire.
pub fn frame_from_camera_gray(
    gles: &mut GlesRenderer,
    camera_tex: u32,
    canonical_w: u32,
    canonical_h: u32,
    uv_xform: [f32; 9],
    display_xform: [f32; 9],
) -> Option<Arc<LiveFrame>> {
    gles.set_camera_external(camera_tex, uv_xform);
    let gray = gles.read_camera_gray(canonical_w, canonical_h, &display_xform)?;
    let frame = Arc::new(LiveFrame::new(0));
    frame.reset_gray(gray, canonical_w, canonical_h);
    Some(frame)
}

/// Run the tracker for one frame, present the result on the GPU (camera
/// passthrough + the baked overlay warped by the tracker homography), and, if the
/// tracker asked for an acquire/refresh on this gray-only frame, satisfy the
/// [`AcquireRequest`](translator_live::live_tracker_pipeline::AcquireRequest) by reading
/// back the OCR inputs from the same external camera texture and handing them to
/// [`LiveTrackerPipeline::provide_acquire_rgb`].
///
/// The overlay is rebaked into the renderer's overlay FBO only when the session's
/// content version moves; every frame just warps that baked texture by the current
/// `compose_h`. `display_xform` maps the canonical frame to clip; `surface_*` are
/// the window dims the present renders into.
pub fn run_tracker_with_acquire(
    pipeline: &LiveTrackerPipeline,
    gles: &mut GlesRenderer,
    frame: &Arc<LiveFrame>,
    canonical_w: u32,
    canonical_h: u32,
    surface_w: u32,
    surface_h: u32,
    display_xform: [f32; 9],
    timestamp_ns: u64,
) -> Result<ProcessFrameResult, TranslatorError> {
    let crop = Rect {
        left: 0,
        top: 0,
        right: canonical_w,
        bottom: canonical_h,
    };
    let mut result = pipeline.process_frame(frame, crop, canonical_w, canonical_h, timestamp_ns)?;

    // GPU present. Rebake the overlay only when the content version moved (new
    // acquire/refresh); otherwise reuse the texture baked on a prior frame and just
    // re-warp it by this frame's homography.
    let version = pipeline.session().content_version();
    if gles.overlay_baked_version() != Some(version) {
        match pipeline.session().overlay_draw_list(result.anchor_id) {
            Some(dl) => {
                // Camera pills are translucent (the bg's own alpha); text is opaque.
                let pill_alpha = pipeline.session().overlay_bg()[3] as f32 / 255.0;
                if gles.render_overlay_to_texture(&dl, pill_alpha, 1.0, true, None) {
                    gles.set_overlay_baked_version(version);
                }
            }
            None => {
                // Anchor has no content (cleared / not yet acquired): drop any stale
                // bake so the present shows the camera alone.
                gles.clear_baked_overlay();
                gles.set_overlay_baked_version(version);
            }
        }
    }
    let drew = gles.present_camera(
        &display_xform,
        result.compose_h,
        canonical_w,
        canonical_h,
        surface_w,
        surface_h,
    );
    result.composite_bytes = if drew {
        surface_w.saturating_mul(surface_h).saturating_mul(4)
    } else {
        0
    };

    if let Some(req) = result.rgb_request.clone() {
        // Render the two OCR inputs directly on the GPU instead of reading back
        // full-res RGBA and CPU-resizing: detector gray at the 32-aligned size,
        // recognition RGBA at half canonical. Orientation is already in `uv`
        // (set by `frame_from_camera_gray`); `clip_xform` just sizes each FBO.
        let (det_w, det_h) = aligned_det_dims(canonical_w, canonical_h, req.det_max_pixels());
        // Recognition reads back at full canonical (the camera's canonical is
        // already modest, ≤1000 long edge); half-res lost too much on small
        // glyphs. `rec_scale` then becomes 1.0 → crops straight from full rgb.
        let (rec_w, rec_h) = (canonical_w, canonical_h);
        let det_gray = gles
            .read_camera_gray(det_w, det_h, &clip_xform(det_w, det_h))
            .ok_or_else(|| {
                TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    "read_camera_gray returned None on acquire",
                )
            })?;
        let rec_rgba = gles
            .read_camera_rgba(rec_w, rec_h, &clip_xform(rec_w, rec_h))
            .ok_or_else(|| {
                TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    "read_camera_rgba returned None on acquire",
                )
            })?;
        let oriented = OrientedImage::from_gpu_split(
            det_gray,
            det_w,
            det_h,
            &rec_rgba,
            rec_w,
            rec_h,
            canonical_w,
            canonical_h,
            req.display_crop(),
        )?;
        // The planar tracker's anchor registration needs a canonical-res gray
        // (matching the per-frame tracker gray's size + transform, so anchor
        // features align). Read it with the same `display_xform`.
        let tracker_gray = gles
            .read_camera_gray(canonical_w, canonical_h, &display_xform)
            .ok_or_else(|| {
                TranslatorError::new(
                    TranslatorErrorKind::Internal,
                    "read_camera_gray (tracker) returned None on acquire",
                )
            })?;
        let rgb_frame = Arc::new(LiveFrame::new(0));
        rgb_frame.reset_oriented_split(oriented, Some((tracker_gray, canonical_w, canonical_h)));
        pipeline.provide_acquire_rgb(req, &rgb_frame);
    }
    Ok(result)
}

/// Screen counterpart to the acquire readback in [`run_tracker_with_acquire`]:
/// borrow the captured external texture and GPU-render the two inference inputs
/// (detector gray at the 32-aligned size, recognition RGBA at half canonical),
/// wrapped in a fresh [`LiveFrame`] ready for the screen worker. `None` if a
/// readback or the split fails. Unlike the camera path there's no tracker gray
/// (the screen anchor is fixed) and the crop is the whole frame.
pub fn screen_acquire_frame(
    gles: &mut GlesRenderer,
    camera_tex: u32,
    canonical_w: u32,
    canonical_h: u32,
    uv_xform: [f32; 9],
    det_max_pixels: u32,
    region: Option<NormRect>,
) -> Option<Arc<LiveFrame>> {
    gles.set_camera_external(camera_tex, uv_xform);
    let (det_w, det_h) = aligned_det_dims(canonical_w, canonical_h, det_max_pixels);
    let (rec_w, rec_h) = ((canonical_w / 2).max(1), (canonical_h / 2).max(1));
    let t_readback = std::time::Instant::now();
    let det_gray = gles.read_camera_gray(det_w, det_h, &clip_xform(det_w, det_h))?;
    let rec_rgba = gles.read_camera_rgba(rec_w, rec_h, &clip_xform(rec_w, rec_h))?;
    // Restrict OCR to a region by cropping the detector + recognition buffers to
    // its sub-rect before inference, so the detector NN only sees those pixels (and
    // never produces boxes outside it). `crop` carries the region in full-frame
    // canonical coords; the worker's `view_to_sensor_h(crop)` re-offsets surviving
    // boxes back to their true screen position.
    let (det_gray, det_w, det_h, rec_rgba, rec_w, rec_h, canon_w, canon_h, crop) = match region {
        Some(r) => {
            let crop = r.to_px(canonical_w, canonical_h);
            let (det_gray, det_w, det_h) = crop_packed(&det_gray, det_w, 1, r.to_px(det_w, det_h));
            let (rec_rgba, rec_w, rec_h) = crop_packed(&rec_rgba, rec_w, 4, r.to_px(rec_w, rec_h));
            (
                det_gray,
                det_w,
                det_h,
                rec_rgba,
                rec_w,
                rec_h,
                crop.right - crop.left,
                crop.bottom - crop.top,
                crop,
            )
        }
        None => (
            det_gray,
            det_w,
            det_h,
            rec_rgba,
            rec_w,
            rec_h,
            canonical_w,
            canonical_h,
            Rect {
                left: 0,
                top: 0,
                right: canonical_w,
                bottom: canonical_h,
            },
        ),
    };
    let oriented = match OrientedImage::from_gpu_split(
        det_gray, det_w, det_h, &rec_rgba, rec_w, rec_h, canon_w, canon_h, crop,
    ) {
        Ok(o) => o,
        Err(e) => {
            log::warn!("[screen] from_gpu_split failed: {e:?}");
            return None;
        }
    };
    log::info!(
        "[screen] dispatch det {det_w}x{det_h} + rec {rec_w}x{rec_h} readback {:.0}ms",
        t_readback.elapsed().as_secs_f64() * 1000.0,
    );
    let frame = Arc::new(LiveFrame::new(0));
    frame.reset_oriented_split(oriented, None);
    Some(frame)
}
