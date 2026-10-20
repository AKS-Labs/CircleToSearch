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

package dev.davidv.translator.ui.components

import android.content.Context
import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLSurface
import android.opengl.GLES20
import android.util.Log
import android.view.Surface
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.camera.core.SurfaceRequest
import dev.davidv.translator.GlEgl
import dev.davidv.translator.LivePipelineJni
import java.util.concurrent.Executor
import kotlin.math.max

/**
 *  GPU camera surface. Owns an EGL GLES3 context on the SurfaceView's
 *  surface plus a `GL_TEXTURE_EXTERNAL_OES` texture wrapped in a
 *  `SurfaceTexture` that CameraX's `Preview` writes into. Per camera
 *  frame, the render thread (woken by `onFrameAvailable`) hands the
 *  external texture to the native `GlesRenderer`, which:
 *
 *    1. GPU-renders the canonical (downscaled, upright) luma into a small
 *       `Vec<u8>` (~650 KB) for the tracker.
 *    2. Composites the same external camera + overlay into the EGL
 *       surface (no second camera upload).
 *
 *  No CPU camera-pixel walk per frame; the analyzer pipeline that used
 *  to deliver RGBA bytes through `ImageAnalysis` is gone.
 */
class LiveGlSurfaceView(context: Context) :
  SurfaceView(context), SurfaceHolder.Callback {
  @Volatile
  var onSizeChanged: ((Int, Int) -> Unit)? = null

  /** Invoked on the GL thread after each frame is presented, carrying
   *  the packed [LivePipelineJni.processFrameGl] result. Hosted in
   *  [LivePlanarOcrEngine] which forwards it to the debug pill. */
  @Volatile
  var onFrameResult: ((Long) -> Unit)? = null

  /** Pointer to the `LiveTrackerPipeline` to drive each frame, written
   *  by the engine once it has created its tracker. The GL loop skips
   *  frames while this is 0. */
  @Volatile
  var pipelinePtr: Long = 0L

  /** Sensor → display rotation in CW 90° quadrants (0/1/2/3 =
   *  0°/90°/180°/270°), derived from `CameraInfo.sensorRotationDegrees`
   *  once the camera is bound. Defaults to 1 (90°), matching the
   *  common back-camera-on-portrait-phone case so the very first few
   *  frames before the camera binds still render upright. */
  @Volatile
  var rotQuadrant: Int = 1

  /** Set the sensor mount rotation from a degrees value (0/90/180/270).
   *  Bound to CameraInfo.sensorRotationDegrees in LiveCameraScreen. */
  fun setCameraOrientationDegrees(degrees: Int) {
    val norm = ((degrees % 360) + 360) % 360
    rotQuadrant = norm / 90
  }

  private var glThread: GlThread? = null

  init {
    holder.addCallback(this)
  }

  // Reconciliation between "CameraX wants a surface" (an open SurfaceRequest)
  // and "the GL thread has a live Surface to give it". Both sides change
  // independently: CameraX (re)issues requests as the camera opens/closes,
  // the GL surface comes and goes with surfaceCreated/surfaceDestroyed. All
  // three fields are guarded by surfaceLock and reconciled whenever either
  // side moves, so a request that arrives before the surface exists is
  // fulfilled the moment it's minted, and vice versa.
  private val surfaceLock = Object()

  /** Live Surface the GL thread minted for CameraX to write into; null
   *  while the EGL context / OES texture aren't up. */
  private var cameraSurface: Surface? = null

  /** A request from CameraX we haven't handed a surface to yet (surface
   *  wasn't live when it arrived). Fulfilled on the next mint. */
  private var pendingRequest: SurfaceRequest? = null

  /** The request we've already provided [cameraSurface] to. Kept so that
   *  when the surface dies (surfaceDestroyed) we can `invalidate()` it —
   *  otherwise CameraX reuses the now-dead surface across a stop/resume
   *  and the preview goes black (issue #248). */
  private var providedRequest: SurfaceRequest? = null

  /** Executor CameraX gave us for surface callbacks; needed to fulfill a
   *  pending request from the GL thread once the surface is minted. */
  @Volatile
  private var requestExecutor: Executor? = null

  /** Last camera buffer size seen from a SurfaceRequest, kept at the view
   *  level so it survives GL-thread recreation. A fresh GlThread starts
   *  with buffer size 0 and gates its whole render loop on it; without
   *  this it would stay black until (if ever) CameraX re-issued a request
   *  after the thread existed. */
  @Volatile
  private var lastCamBufW = 0

  @Volatile
  private var lastCamBufH = 0

  /** Entry point for CameraX's `Preview.SurfaceProvider`. Owns the whole
   *  request lifecycle: records the buffer size, provides the live surface
   *  if we have one, else parks the request until the GL thread mints one. */
  fun provideSurfaceRequest(
    request: SurfaceRequest,
    executor: Executor,
  ) {
    val resolution = request.resolution
    Log.i(TAG, "Preview SurfaceRequest resolution=${resolution.width}x${resolution.height}")
    synchronized(surfaceLock) {
      requestExecutor = executor
      lastCamBufW = resolution.width
      lastCamBufH = resolution.height
      glThread?.setCameraBufferSize(resolution.width, resolution.height)
      request.addRequestCancellationListener(executor) {
        synchronized(surfaceLock) {
          if (pendingRequest === request) pendingRequest = null
          if (providedRequest === request) providedRequest = null
        }
      }
      val surface = cameraSurface
      if (surface != null) {
        fulfill(request, surface, executor)
      } else {
        pendingRequest = request
      }
    }
  }

  /** Provide [surface] to [request]. Caller holds [surfaceLock]. */
  private fun fulfill(
    request: SurfaceRequest,
    surface: Surface,
    executor: Executor,
  ) {
    runCatching {
      request.provideSurface(surface, executor) {
        // The Surface is released by the GL thread on teardown
        // (surfaceDestroyed); CameraX's release callback is a no-op.
      }
    }.onFailure { Log.w(TAG, "provideSurface failed", it) }
    providedRequest = request
    pendingRequest = null
  }

  override fun surfaceCreated(holder: SurfaceHolder) {
    glThread =
      GlThread(holder.surface).also { thread ->
        synchronized(surfaceLock) {
          if (lastCamBufW > 0 && lastCamBufH > 0) {
            thread.setCameraBufferSize(lastCamBufW, lastCamBufH)
          }
        }
        thread.start()
      }
  }

  override fun surfaceChanged(
    holder: SurfaceHolder,
    format: Int,
    width: Int,
    height: Int,
  ) {
    glThread?.resize(width, height)
    onSizeChanged?.invoke(width, height)
  }

  override fun surfaceDestroyed(holder: SurfaceHolder) {
    synchronized(surfaceLock) {
      cameraSurface?.release()
      cameraSurface = null
      // Tell CameraX the surface it holds is dead so it re-requests one
      // when the camera reopens, instead of rebuilding the capture session
      // around the released surface (issue #248).
      runCatching { providedRequest?.invalidate() }
        .onFailure { Log.w(TAG, "surfaceRequest invalidate failed", it) }
      providedRequest = null
      pendingRequest = null
    }
    glThread?.shutdown()
    glThread = null
  }

  private fun publishCameraSurface(s: Surface) {
    synchronized(surfaceLock) {
      cameraSurface = s
      val req = pendingRequest
      val exec = requestExecutor
      if (req != null && exec != null) {
        fulfill(req, s, exec)
      }
    }
  }

  /** Owns the EGL context + OES texture + SurfaceTexture + native
   *  renderer; all GL touches happen here. Woken per camera frame by
   *  `onFrameAvailable`. */
  private inner class GlThread(private val surface: Surface) : Thread("live-gl") {
    private var eglDisplay: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE
    private var rendererPtr: Long = 0L
    private var cameraTexId: Int = 0

    // Guards the (surfaceTexture, camBufW, camBufH) trio so the camera's
    // default buffer size is applied exactly once whichever thread wins:
    // CameraX's SurfaceProvider thread (`setCameraBufferSize`) or the GL
    // thread that mints the SurfaceTexture. If `setDefaultBufferSize` is
    // missed, the camera HAL falls back to its minimum stream size (176x144)
    // and the whole preview + OCR readback come through as garbage.
    private val stLock = Object()
    private var surfaceTexture: SurfaceTexture? = null

    @Volatile
    private var running = true

    @Volatile
    private var surfW = 0

    @Volatile
    private var surfH = 0

    @Volatile
    private var camBufW = 0

    @Volatile
    private var camBufH = 0

    private val lock = Object()
    private var frameAvailable = false

    fun resize(
      w: Int,
      h: Int,
    ) {
      surfW = w
      surfH = h
    }

    fun setCameraBufferSize(
      w: Int,
      h: Int,
    ) {
      // SurfaceTexture must be touched after creation, on any thread (it's
      // documented thread-safe for setDefaultBufferSize). If the GL thread
      // hasn't minted it yet, `run()` applies these dims when it does.
      synchronized(stLock) {
        camBufW = w
        camBufH = h
        surfaceTexture?.setDefaultBufferSize(w, h)
      }
    }

    fun shutdown() {
      synchronized(lock) {
        running = false
        lock.notifyAll()
      }
      try {
        join(500)
      } catch (_: InterruptedException) {
      }
    }

    override fun run() {
      // Render thread must win scheduling against the OCR worker (MNN
      // saturates cores during detect/recognize); without this the
      // camera producer eventually stalls waiting for our consumer.
      android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_URGENT_DISPLAY)
      if (!setupEgl()) {
        Log.e(TAG, "EGL setup failed; GL present disabled")
        return
      }
      rendererPtr = LivePipelineJni.createGlRenderer()
      if (rendererPtr == 0L) {
        Log.e(TAG, "createGlRenderer failed")
        teardownEgl()
        return
      }
      cameraTexId = GlEgl.createExternalOesTexture()
      if (cameraTexId == 0) {
        Log.e(TAG, "createExternalOesTexture failed")
        LivePipelineJni.destroyGlRenderer(rendererPtr)
        rendererPtr = 0L
        teardownEgl()
        return
      }
      val st = SurfaceTexture(cameraTexId)
      synchronized(stLock) {
        surfaceTexture = st
        if (camBufW > 0 && camBufH > 0) st.setDefaultBufferSize(camBufW, camBufH)
      }
      // Frame-available wakes the GL loop. We pass `null` for the handler
      // so it fires on a binder thread; the callback just flips a flag.
      st.setOnFrameAvailableListener {
        synchronized(lock) {
          frameAvailable = true
          lock.notifyAll()
        }
      }
      publishCameraSurface(Surface(st))

      while (true) {
        synchronized(lock) {
          while (running && !frameAvailable) {
            try {
              lock.wait()
            } catch (_: InterruptedException) {
            }
          }
          if (!running) return@synchronized
          frameAvailable = false
        }
        if (!running) break

        st.updateTexImage()
        val sw = surfW
        val sh = surfH
        val pipeline = pipelinePtr
        val cbw = camBufW
        val cbh = camBufH
        if (sw <= 0 || sh <= 0 || pipeline == 0L || cbw <= 0 || cbh <= 0) continue
        val (cw, ch) = canonicalDims(sw, sh)
        val uv = computeUvMat(cbw, cbh, cw, ch, rotQuadrant)
        val dx = displayXform(cw, ch)
        val packed =
          LivePipelineJni.processFrameGl(
            pipeline,
            rendererPtr,
            cameraTexId,
            cw,
            ch,
            sw,
            sh,
            uv,
            dx,
            st.timestamp,
          )
        EGL14.eglSwapBuffers(eglDisplay, eglSurface)
        onFrameResult?.invoke(packed)
      }

      surfaceTexture?.release()
      surfaceTexture = null
      if (cameraTexId != 0) {
        GLES20.glDeleteTextures(1, intArrayOf(cameraTexId), 0)
        cameraTexId = 0
      }
      if (rendererPtr != 0L) {
        LivePipelineJni.destroyGlRenderer(rendererPtr)
        rendererPtr = 0L
      }
      teardownEgl()
    }

    private fun setupEgl(): Boolean {
      // Window surface, no alpha — the camera composite is opaque.
      val core =
        GlEgl.initContext(
          intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, GlEgl.EGL_OPENGL_ES3_BIT,
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 0,
            EGL14.EGL_NONE,
          ),
        ) ?: return false
      eglDisplay = core.display
      eglContext = core.context
      eglSurface =
        EGL14.eglCreateWindowSurface(
          eglDisplay,
          core.config,
          surface,
          intArrayOf(EGL14.EGL_NONE),
          0,
        )
      if (eglSurface == EGL14.EGL_NO_SURFACE) return false
      return EGL14.eglMakeCurrent(eglDisplay, eglSurface, eglSurface, eglContext)
    }

    private fun teardownEgl() {
      if (eglDisplay != EGL14.EGL_NO_DISPLAY) {
        if (eglSurface != EGL14.EGL_NO_SURFACE) {
          EGL14.eglMakeCurrent(
            eglDisplay,
            EGL14.EGL_NO_SURFACE,
            EGL14.EGL_NO_SURFACE,
            EGL14.EGL_NO_CONTEXT,
          )
          EGL14.eglDestroySurface(eglDisplay, eglSurface)
        }
        GlEgl.destroyContext(eglDisplay, eglContext)
      }
      eglDisplay = EGL14.EGL_NO_DISPLAY
      eglContext = EGL14.EGL_NO_CONTEXT
      eglSurface = EGL14.EGL_NO_SURFACE
    }
  }

  companion object {
    private const val TAG = "LiveGlSurfaceView"

    /** Cap canonical (tracker/composite) frame at 1000 px on the long
     *  side, preserving display aspect — same rule the Linux GPU path
     *  uses. The R8 readback is `cw * ch` bytes per frame, so capping
     *  keeps the per-frame readback ~600–700 KB regardless of the
     *  surface (or camera) resolution. */
    private const val CANONICAL_MAX_SIDE: Int = 1000

    fun canonicalDims(
      surfaceW: Int,
      surfaceH: Int,
    ): Pair<Int, Int> {
      if (surfaceW <= 0 || surfaceH <= 0) return Pair(0, 0)
      val long = max(surfaceW, surfaceH).toFloat()
      val s = (CANONICAL_MAX_SIDE.toFloat() / long).coerceAtMost(1f)
      return Pair(
        (surfaceW * s).toInt().coerceAtLeast(1),
        (surfaceH * s).toInt().coerceAtLeast(1),
      )
    }

    /** Row-major 3×3 dst-pixel (canonical, bottom-left origin) → clip-space
     *  transform. Matches the Linux GPU path's `display_xform`
     *  (`DISPLAY_FLIP_Y=false` branch): unit-quad vertex (0,0) lands at
     *  clip (-1,-1) and (1,1) at clip (1,1). [computeUvMat] is built
     *  against this same convention. The canonical frame's aspect is
     *  preserved across the composite because the UV xform crops the
     *  camera to the canonical aspect (aspect-fill); `bind_present_framebuffer`
     *  then stretches the canonical frame uniformly to the EGL surface. */
    fun displayXform(
      canonicalW: Int,
      canonicalH: Int,
    ): FloatArray {
      val cw = canonicalW.toFloat().coerceAtLeast(1f)
      val ch = canonicalH.toFloat().coerceAtLeast(1f)
      return floatArrayOf(
        2f / cw, 0f, -1f,
        0f, 2f / ch, -1f,
        0f, 0f, 1f,
      )
    }

    /** Port of `offline-translator-linux/src/live_gpu.rs::compute_uv_mat`.
     *  Builds the row-major 3×3 sampler transform mapping the unit-quad
     *  vertex space (0..1, 0..1) to the external-OES texture's sensor uv,
     *  encoding:
     *
     *    - sensor → display rotation (`rotQuadrant`: 0/90/180/270 CW),
     *      from `CameraInfo.sensorRotationDegrees` once the camera binds
     *    - aspect-fill cropping (canonical aspect vs camera aspect): the
     *      narrower axis is shown 100%, the wider axis is cropped to
     *      match canonical aspect
     *    - per-axis mirror correction ([FLIP_U] / [FLIP_V]; tunable on
     *      device — Android's SurfaceTexture for a back camera in
     *      portrait typically wants `FLIP_U=true, FLIP_V=false`, matching
     *      the Linux defaults)
     *
     *  `camW/H` are the SurfaceTexture buffer dims (sensor orientation,
     *  typically landscape e.g. 1920×1080). `canonicalW/H` is the output
     *  aspect (matches the display aspect, downscaled). */
    fun computeUvMat(
      camW: Int,
      camH: Int,
      canonicalW: Int,
      canonicalH: Int,
      rotQuadrant: Int,
    ): FloatArray {
      val q = ((rotQuadrant % 4) + 4) % 4
      val odd = q == 1 || q == 3
      val cw = camW.toFloat().coerceAtLeast(1f)
      val ch = camH.toFloat().coerceAtLeast(1f)
      val fw = canonicalW.toFloat().coerceAtLeast(1f)
      val fh = canonicalH.toFloat().coerceAtLeast(1f)
      val displayedAspect = if (odd) ch / cw else cw / ch
      val outAspect = fw / fh
      val visW: Float
      val visH: Float
      if (outAspect <= displayedAspect) {
        visW = outAspect / displayedAspect
        visH = 1f
      } else {
        visW = 1f
        visH = displayedAspect / outAspect
      }
      val fracU = if (odd) visH else visW
      val fracV = if (odd) visW else visH
      val r00: Float
      val r01: Float
      val r10: Float
      val r11: Float
      when (q) {
        0 -> {
          r00 = 1f
          r01 = 0f
          r10 = 0f
          r11 = 1f
        }
        1 -> {
          r00 = 0f
          r01 = 1f
          r10 = -1f
          r11 = 0f
        }
        2 -> {
          r00 = -1f
          r01 = 0f
          r10 = 0f
          r11 = -1f
        }
        else -> {
          r00 = 0f
          r01 = -1f
          r10 = 1f
          r11 = 0f
        }
      }
      var l00 = fracU * r00
      var l01 = fracU * r01
      var l10 = fracV * r10
      var l11 = fracV * r11
      var t0 = 0.5f - 0.5f * (l00 + l01)
      var t1 = 0.5f - 0.5f * (l10 + l11)
      if (FLIP_U) {
        l00 = -l00
        l01 = -l01
        t0 = 1f - t0
      }
      if (FLIP_V) {
        l10 = -l10
        l11 = -l11
        t1 = 1f - t1
      }
      return floatArrayOf(
        l00, l01, t0,
        l10, l11, t1,
        0f, 0f, 1f,
      )
    }

    private const val FLIP_U: Boolean = true
    private const val FLIP_V: Boolean = false
  }
}
