/*
 * Copyright (C) 2024 David V
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

package dev.davidv.translator

/** Single-call JNI fast-path for the live-camera planar OCR pipeline.
 *
 *  Per-frame entry point: [processFrameGl] borrows the camera's
 *  `GL_TEXTURE_EXTERNAL_OES`, GPU-renders canonical luma into a small
 *  `Vec<u8>` to feed the tracker, then presents the camera + overlay
 *  composite straight to the bound EGL surface via the native
 *  `GlesRenderer` — all in one JNI call. Acquire/refresh frames read
 *  full-res RGBA back from the same texture inside the call.
 *
 *  Returns a packed `Long` so Kotlin doesn't need a follow-up uniffi
 *  call for the debug-pill update. Detailed async-job telemetry (rec
 *  counts, ms, cancel) is *not* packed in — poll
 *  `LivePlanarTracker.lastAcquireTelemetry()` to refresh the pill.
 */
internal object LivePipelineJni {
  init {
    System.loadLibrary("bindings")
  }

  /** Build a native `GlesRenderer` for the GPU present path. MUST be
   *  called on the GL render thread with its EGL GLES3 context already
   *  current. Returns a native pointer (`Long`), or 0 on failure.
   *  Release with [destroyGlRenderer] on the same thread. */
  @JvmStatic
  external fun createGlRenderer(): Long

  /** Free a renderer from [createGlRenderer]. Same-thread / context as
   *  creation. */
  @JvmStatic
  external fun destroyGlRenderer(rendererPtr: Long)

  /** Switch a renderer to overlay-only present: [processFrameGl] then draws
   *  only the overlays over a transparent clear (premultiplied), skipping the
   *  camera passthrough — for a translucent FLAG_SECURE window floating over
   *  the live screen (the MediaProjection screen-translate path). Call once on
   *  the GL thread after [createGlRenderer]. Tracker readbacks are unaffected. */
  @JvmStatic
  external fun setRendererOverlayOnly(rendererPtr: Long)

  /** Set the renderer's parametric overlay opacity (0..1). The screen path
   *  uses this to control overlay opacity independently of the touch-capped
   *  window alpha; camera leaves it at the 1.0 default. */
  @JvmStatic
  external fun setRendererOverlayAlpha(
    rendererPtr: Long,
    alpha: Float,
  )

  /** Dispatch a screen acquire: GPU-render the detector gray + recognition RGBA
   *  off the captured texture and hand the frame to the background OCR worker.
   *  Non-blocking — detect/rec/translate runs off the GL thread; poll
   *  [screenAcquireState] and call [screenPresentOverlay] when the version bumps.
   *  Returns 1 if dispatched. */
  @JvmStatic
  external fun screenDispatchAcquire(
    pipelinePtr: Long,
    rendererPtr: Long,
    cameraTexId: Int,
    canonicalWidth: Int,
    canonicalHeight: Int,
    surfaceWidth: Int,
    surfaceHeight: Int,
    uvXform: FloatArray,
  ): Int

  /** Present the screen pipeline's resident overlay canvas straight into the
   *  currently-bound EGL window surface (the overlay TextureView): renders it if a
   *  deferred upsert left it stale, then uploads it to a texture and draws one
   *  premultiplied quad — no Bitmap, no CPU readback. Make the window surface
   *  current before calling and `eglSwapBuffers` after. Returns 1 if a frame was
   *  drawn, 0 when there's nothing to show. */
  @JvmStatic
  external fun screenPresentOverlayGl(
    pipelinePtr: Long,
    rendererPtr: Long,
    canonicalWidth: Int,
    canonicalHeight: Int,
    surfaceWidth: Int,
    surfaceHeight: Int,
  ): Int

  /** Acquire state for the poll loop: `(busy shl 32) or overlayVersion`. `busy`
   *  = an acquire is in flight; `overlayVersion` bumps each time the worker
   *  upserts overlays (provisional, then full). */
  @JvmStatic
  external fun screenAcquireState(pipelinePtr: Long): Long

  /** Abort an in-flight screen acquire (the screen moved). */
  @JvmStatic
  external fun screenAbortAcquire(pipelinePtr: Long)

  /** Screen-translate change detection: GPU-reads a coarse gray off the captured
   *  external texture and feeds the `LiveScreenPipeline` monitor. Returns a
   *  packed `Int`: bits 0-1 = action (0=none, 1=hide, 2=acquire), bit 8 =
   *  wants-tick (a settle deadline is armed / an acquire is in flight → poll on a
   *  timer). Cheap (no overlay readback); the heavy detect/rec runs on the worker
   *  via [screenDispatchAcquire] once an acquire is decided. */
  @JvmStatic
  external fun screenMonitorFrameGl(
    pipelinePtr: Long,
    rendererPtr: Long,
    cameraTexId: Int,
    canonicalWidth: Int,
    canonicalHeight: Int,
    uvXform: FloatArray,
    nowNs: Long,
  ): Int

  /** Timed tick for the screen monitor (no new frame): fires a pending settle so
   *  the screen settles even when the mirror stops emitting frames. Same packed
   *  `Int` as [screenMonitorFrameGl]. */
  @JvmStatic
  external fun screenMonitorTick(
    pipelinePtr: Long,
    nowNs: Long,
  ): Int

  /** DEBUG: read back the canonical RGBA frame (top-down, `cw*ch*4` bytes) the
   *  tracker/recognizer sees, for on-device inspection. Call on the GL thread
   *  after a [processFrameGl] (so the external source + uv are set). Empty on
   *  failure. */
  @JvmStatic
  external fun debugReadCanonicalRgba(
    rendererPtr: Long,
    canonicalWidth: Int,
    canonicalHeight: Int,
    displayXform: FloatArray,
  ): ByteArray

  /** Per-frame GPU path. Call on the GL render thread; follow with
   *  `eglSwapBuffers`. Returns a packed `Long` (see [FrameResult.unpack]);
   *  `compositeOk` means a frame was drawn.
   *
   *  - `cameraTexId`: id of the `GL_TEXTURE_EXTERNAL_OES` the camera's
   *    `SurfaceTexture` writes into. Must be valid for the duration of
   *    the call.
   *  - `canonicalWidth/Height`: small downscaled tracker frame size (the
   *    R8 readback the GPU produces feeds the BRIEF tracker at these
   *    dims). Capped to ~1000 on the long side; preserves surface aspect.
   *  - `surfaceWidth/Height`: EGL window surface dims; viewport for the
   *    final composite into FBO 0.
   *  - `uvXform`: row-major 3×3 mapping display UV [0,1]² → external-OES
   *    texture UV (sensor orientation). Derived from
   *    `SurfaceTexture.getTransformMatrix`.
   *  - `displayXform`: row-major 3×3 mapping canonical-pixel coords
   *    (top-left origin, y-down) → clip space. */
  @JvmStatic
  external fun processFrameGl(
    pipelinePtr: Long,
    rendererPtr: Long,
    cameraTexId: Int,
    canonicalWidth: Int,
    canonicalHeight: Int,
    surfaceWidth: Int,
    surfaceHeight: Int,
    uvXform: FloatArray,
    displayXform: FloatArray,
    timestampNs: Long,
  ): Long

  /** Decoded per-frame result (see Rust `pack_result` for the bit
   *  layout). `compositeOk == false` means no frame was drawn this
   *  tick. */
  data class FrameResult(
    val state: uniffi.bindings.PlanarTrackerState,
    val anchorIdLow16: Long,
    val inliers: Int,
    val scale: Float,
    val compositeOk: Boolean,
    val startedAcquire: Boolean,
    val startedRefresh: Boolean,
  ) {
    companion object {
      fun unpack(packed: Long): FrameResult? {
        if (packed == 0L) return null
        val state =
          when ((packed ushr 62) and 0x3L) {
            0L -> uniffi.bindings.PlanarTrackerState.IDLE
            1L -> uniffi.bindings.PlanarTrackerState.ACQUIRING
            2L -> uniffi.bindings.PlanarTrackerState.LOCKED
            else -> uniffi.bindings.PlanarTrackerState.LOST
          }
        val compositeOk = ((packed ushr 61) and 0x1L) != 0L
        val inliers = ((packed ushr 45) and 0xFFFFL).toInt()
        val anchorLo = (packed ushr 29) and 0xFFFFL
        val startedAcquire = ((packed ushr 28) and 0x1L) != 0L
        val startedRefresh = ((packed ushr 27) and 0x1L) != 0L
        val scale = (packed and 0x1FFFFFL).toFloat() / 1024f
        return FrameResult(
          state,
          anchorLo,
          inliers,
          scale,
          compositeOk,
          startedAcquire,
          startedRefresh,
        )
      }
    }
  }
}
