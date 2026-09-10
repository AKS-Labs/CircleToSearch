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

package dev.davidv.translator.screenTranslate

import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLSurface
import android.opengl.GLES20
import android.util.Log
import android.view.Surface
import dev.davidv.translator.GlEgl
import dev.davidv.translator.LivePipelineJni

/**
 *  Off-screen GPU worker for screen-translate. A `VirtualDisplay` writes the
 *  captured screen into a `GL_TEXTURE_EXTERNAL_OES`; per frame this drives the
 *  same native overlay-only `GlesRenderer` the camera path uses to read back the
 *  monitor gray / OCR inputs. When the overlay updates it is composited **on the
 *  GPU straight into the overlay TextureView's EGL window surface** — no CPU
 *  canvas readback, no Canvas view. See [GpuOverlayTextureView] for why a
 *  TextureView (in-window, dimmed by the window alpha, taps pass through) and not
 *  a SurfaceView (separate full-opacity layer that blocks taps).
 *
 *  The GL context is created on a PBuffer surface so it is valid before the
 *  TextureView's SurfaceTexture exists; the window surface is created lazily from
 *  [setOutputSurfaceTexture] and kept current once available (the monitor
 *  readbacks render to their own FBOs, so the bound surface is irrelevant to
 *  them — only present/clear touch the window's back buffer and swap).
 */
class ScreenCaptureGlWorker(
  displayWidth: Int,
  displayHeight: Int,
) {
  @Volatile
  var pipelinePtr: Long = 0L

  // OCR canonical is half the display: read_camera_rgba renders into a half-res
  // FBO (cheap ~4.6MB readback) which rec crops from (det downsamples further).
  // The overlay PRESENT runs at full display res (pw/ph): the canvas rasterizes
  // glyphs at oversample=2 and is drawn 1:1, so present canonical=cw/ch and
  // surface=pw/ph (the canvas origin is in canonical coords, scaled to display).
  // `@Volatile var` (not val): a rotation resizes these in place via [resize] —
  // the GL loop reads them fresh each iteration. (The VirtualDisplay can't be
  // recreated on Android 14+, so resize is the only path; the loop also passes
  // these to the native present/readback, so updating them is sufficient.)
  @Volatile
  private var cw = (displayWidth / 2).coerceAtLeast(1)

  @Volatile
  private var ch = (displayHeight / 2).coerceAtLeast(1)

  @Volatile
  private var pw = displayWidth.coerceAtLeast(1)

  @Volatile
  private var ph = displayHeight.coerceAtLeast(1)

  private var glThread: GlThread? = null

  @Volatile
  private var sourceSurface: Surface? = null
  private val sourceSurfaceLock = Object()

  // The overlay TextureView's SurfaceTexture, handed off to the GL thread which
  // turns it into an EGL window surface. `dirty` is consumed once per change.
  private val outputLock = Object()
  private var pendingOutputSt: SurfaceTexture? = null
  private var outputDirty = false

  fun start() {
    glThread = GlThread().also { it.start() }
  }

  fun stop() {
    glThread?.shutdown()
    glThread = null
  }

  fun awaitSourceSurface(timeoutMs: Long = 5_000): Surface? {
    val deadline = System.currentTimeMillis() + timeoutMs
    synchronized(sourceSurfaceLock) {
      while (sourceSurface == null) {
        val remaining = deadline - System.currentTimeMillis()
        if (remaining <= 0) return null
        try {
          sourceSurfaceLock.wait(remaining)
        } catch (_: InterruptedException) {
          return null
        }
      }
      return sourceSurface
    }
  }

  fun setSourceBufferSize(
    width: Int,
    height: Int,
  ) {
    glThread?.setSourceBufferSize(width, height)
  }

  /** Pause/resume OCR + translation without tearing down the capture (used while
   *  the region editor is open, or for a manual pause). */
  fun setPaused(paused: Boolean) {
    glThread?.setPaused(paused)
  }

  /** Clear the on-screen overlay window once (used when manually pausing). */
  fun clearOverlayOutput() {
    glThread?.requestClearOutput()
  }

  /** Rotation / display resize: update the canonical (OCR) and present (surface)
   *  dimensions in place and resize the capture buffer. The GL loop reads the new
   *  dims on its next iteration and passes them to the native present/readback;
   *  the EGL window surface is recreated separately when the overlay TextureView
   *  re-hands its (now-resized) SurfaceTexture. The caller resizes the
   *  VirtualDisplay to match. */
  fun resize(
    displayWidth: Int,
    displayHeight: Int,
  ) {
    cw = (displayWidth / 2).coerceAtLeast(1)
    ch = (displayHeight / 2).coerceAtLeast(1)
    pw = displayWidth.coerceAtLeast(1)
    ph = displayHeight.coerceAtLeast(1)
    setSourceBufferSize(displayWidth, displayHeight)
  }

  /** Hand the overlay TextureView's SurfaceTexture (or `null` on destroy) to the
   *  GL thread, which (re)creates the EGL window surface it presents into. */
  fun setOutputSurfaceTexture(st: SurfaceTexture?) {
    synchronized(outputLock) {
      pendingOutputSt = st
      outputDirty = true
    }
    glThread?.wake()
  }

  private fun publishSourceSurface(s: Surface) {
    synchronized(sourceSurfaceLock) {
      sourceSurface = s
      sourceSurfaceLock.notifyAll()
    }
  }

  private inner class GlThread : Thread("screen-translate-gl") {
    private var eglDisplay: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglConfig: EGLConfig? = null
    private var eglPbuffer: EGLSurface = EGL14.EGL_NO_SURFACE
    private var eglWindowSurface: EGLSurface = EGL14.EGL_NO_SURFACE
    private var rendererPtr: Long = 0L
    private var sourceTexId: Int = 0

    private val stLock = Object()
    private var surfaceTexture: SurfaceTexture? = null

    @Volatile
    private var running = true

    // Pause OCR/translation (e.g. while the region editor is up) without tearing
    // the capture down: the loop keeps draining frames but skips monitor/dispatch/
    // present, so the overlay freezes and we don't OCR our own editor UI.
    @Volatile
    private var paused = false

    // One-shot: clear the overlay window on the GL thread even while paused (a
    // manual pause clears the on-screen overlays without resuming OCR).
    @Volatile
    private var clearRequested = false

    @Volatile
    private var srcBufW = 0

    @Volatile
    private var srcBufH = 0

    private val lock = Object()
    private var frameAvailable = false
    private val stMatrix = FloatArray(16)

    fun setSourceBufferSize(
      w: Int,
      h: Int,
    ) {
      synchronized(stLock) {
        srcBufW = w
        srcBufH = h
        surfaceTexture?.setDefaultBufferSize(w, h)
      }
    }

    /** Wake the loop from an idle `wait` (a new output surface, or shutdown). */
    fun wake() {
      synchronized(lock) {
        lock.notifyAll()
      }
    }

    fun setPaused(p: Boolean) {
      paused = p
      synchronized(lock) {
        lock.notifyAll()
      }
    }

    fun requestClearOutput() {
      clearRequested = true
      synchronized(lock) {
        lock.notifyAll()
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
      android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_URGENT_DISPLAY)
      if (!setupEgl()) {
        Log.e(TAG, "EGL setup failed")
        return
      }
      rendererPtr = LivePipelineJni.createGlRenderer()
      if (rendererPtr == 0L) {
        Log.e(TAG, "createGlRenderer failed")
        teardownEgl()
        return
      }
      LivePipelineJni.setRendererOverlayOnly(rendererPtr)
      // Modal overlay opacity for the screen path. On-screen opacity is
      // OVERLAY_ALPHA × window-alpha; 1.0 keeps pills as opaque as the window
      // allows. Lower it for a more see-through overlay.
      LivePipelineJni.setRendererOverlayAlpha(rendererPtr, OVERLAY_ALPHA)
      sourceTexId = GlEgl.createExternalOesTexture()
      if (sourceTexId == 0) {
        Log.e(TAG, "createExternalOesTexture failed")
        LivePipelineJni.destroyGlRenderer(rendererPtr)
        rendererPtr = 0L
        teardownEgl()
        return
      }
      val st = SurfaceTexture(sourceTexId)
      synchronized(stLock) {
        surfaceTexture = st
        if (srcBufW > 0 && srcBufH > 0) st.setDefaultBufferSize(srcBufW, srcBufH)
      }
      st.setOnFrameAvailableListener {
        synchronized(lock) {
          frameAvailable = true
          lock.notifyAll()
        }
      }
      publishSourceSurface(Surface(st))
      Log.i(TAG, "screen-translate GL ready, canonical ${cw}x$ch, pipelinePtr=$pipelinePtr")

      // Auto change detection (SCREEN_CHANGE_DETECTION.md): per captured frame the
      // native monitor computes a coarse-gray diff (pill regions masked out) and
      // decides hide (movement) / acquire (settled) / nothing. On settle we
      // *dispatch* OCR to a background worker (non-blocking) and poll for results,
      // presenting provisional pills the instant detection lands and the full
      // translation when ready. The monitor keeps running during the acquire, so
      // movement aborts the in-flight OCR. `wantsTick`/`acquiring` drive timed
      // polling (the mirror stops emitting frames once the screen is static).
      var wantsTick = false
      var acquiring = false
      var lastVersion = 0L
      while (true) {
        var hadFrame = false
        synchronized(lock) {
          if (wantsTick || acquiring) {
            if (running && !frameAvailable) {
              try {
                lock.wait(POLL_MS)
              } catch (_: InterruptedException) {
              }
            }
          } else {
            while (running && !frameAvailable) {
              try {
                lock.wait()
              } catch (_: InterruptedException) {
              }
            }
          }
          if (!running) return@synchronized
          hadFrame = frameAvailable
          frameAvailable = false
        }
        if (!running) break

        // Pick up a new/destroyed output surface before touching it below.
        applyPendingOutputSurface()

        // Always drain the producer so its BufferQueue keeps flowing.
        if (hadFrame) {
          st.updateTexImage()
          st.getTransformMatrix(stMatrix)
        }
        val pipeline = pipelinePtr
        if (pipeline == 0L) continue

        // Honour a one-shot output clear even while paused (a manual pause wipes the
        // on-screen overlays without resuming OCR).
        if (clearRequested) {
          clearRequested = false
          clearOutput()
          lastVersion = LivePipelineJni.screenAcquireState(pipeline) and 0xFFFFFFFFL
        }

        // Paused (region editor up, or a manual pause): drained the frame above to
        // keep the queue flowing, now skip all OCR/present and abort anything in
        // flight so the overlay freezes. Frames from the editor add/remove still wake
        // the loop; it resumes on the next frame after unpausing.
        if (paused) {
          if (acquiring) {
            LivePipelineJni.screenAbortAcquire(pipeline)
            acquiring = false
          }
          continue
        }

        // Monitor movement/settle — runs even while acquiring, so movement during
        // an in-flight OCR aborts it.
        val now = System.nanoTime()
        val ret =
          if (hadFrame) {
            val uv = uvFromSurfaceTexture(stMatrix)
            LivePipelineJni.screenMonitorFrameGl(
              pipeline,
              rendererPtr,
              sourceTexId,
              cw,
              ch,
              uv,
              now,
            )
          } else {
            LivePipelineJni.screenMonitorTick(pipeline, now)
          }
        wantsTick = (ret and MONITOR_WANTS_TICK) != 0
        when (ret and MONITOR_ACTION_MASK) {
          MONITOR_ACTION_HIDE -> {
            clearOutput()
            if (acquiring) {
              LivePipelineJni.screenAbortAcquire(pipeline)
              acquiring = false
              Log.i(TAG, "movement during acquire → abort")
            }
          }
          MONITOR_ACTION_ACQUIRE -> {
            // Settled: dispatch OCR to the worker (non-blocking). The monitor only
            // returns Acquire once any post-drop settle has elapsed, so dropped
            // pills have left the captured frame; resident pills that stayed are
            // masked out of detection natively by their strip rects.
            val uv = uvFromSurfaceTexture(stMatrix)
            val dispatched =
              LivePipelineJni.screenDispatchAcquire(
                pipeline,
                rendererPtr,
                sourceTexId,
                cw,
                ch,
                pw,
                ph,
                uv,
              )
            if (dispatched != 0) {
              acquiring = true
              lastVersion = LivePipelineJni.screenAcquireState(pipeline) and 0xFFFFFFFFL
            } else {
              Log.w(TAG, "screenDispatchAcquire dropped (worker busy)")
            }
          }
        }

        // Present any overlay content change — the streamed provisional/full
        // overlays during an acquire, *and* a drop/clear the monitor made with no
        // acquire in flight. Gating this behind `acquiring` stranded a dropped pill
        // (never re-presented), so it must run on every version bump.
        val state = LivePipelineJni.screenAcquireState(pipeline)
        val version = state and 0xFFFFFFFFL
        val busy = (state ushr 32) != 0L
        if (version != lastVersion) {
          val tPresent = System.nanoTime()
          // An emptied overlay (last pill dropped) draws nothing — clear the window
          // so the stale pill leaves the screen.
          val drawn = presentOverlay(pipeline)
          if (!drawn) clearOutput()
          Log.i(TAG, "present v=$version drawn=$drawn (${(System.nanoTime() - tPresent) / 1_000_000}ms)")
          lastVersion = version
        }
        if (acquiring && !busy) acquiring = false
      }

      releaseWindowSurface()
      surfaceTexture?.release()
      surfaceTexture = null
      if (sourceTexId != 0) {
        GLES20.glDeleteTextures(1, intArrayOf(sourceTexId), 0)
        sourceTexId = 0
      }
      if (rendererPtr != 0L) {
        LivePipelineJni.destroyGlRenderer(rendererPtr)
        rendererPtr = 0L
      }
      teardownEgl()
    }

    /** Render the resident overlay into the window surface and swap it on. */
    private fun presentOverlay(pipeline: Long): Boolean {
      if (eglWindowSurface == EGL14.EGL_NO_SURFACE || pipeline == 0L) return false
      val drawn = LivePipelineJni.screenPresentOverlayGl(pipeline, rendererPtr, cw, ch, pw, ph)
      if (drawn != 0) {
        EGL14.eglSwapBuffers(eglDisplay, eglWindowSurface)
        return true
      }
      return false
    }

    /** Clear the window surface to transparent (the screen behind shows through)
     *  and swap — the GPU equivalent of dropping the Canvas overlay on movement. */
    private fun clearOutput() {
      if (eglWindowSurface == EGL14.EGL_NO_SURFACE) return
      GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0)
      GLES20.glViewport(0, 0, pw, ph)
      GLES20.glClearColor(0f, 0f, 0f, 0f)
      GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT)
      EGL14.eglSwapBuffers(eglDisplay, eglWindowSurface)
    }

    private fun applyPendingOutputSurface() {
      val st: SurfaceTexture?
      synchronized(outputLock) {
        if (!outputDirty) return
        outputDirty = false
        st = pendingOutputSt
      }
      updateOutputSurface(st)
    }

    private fun updateOutputSurface(st: SurfaceTexture?) {
      releaseWindowSurface()
      val cfg = eglConfig ?: return
      if (st == null) return
      val ws =
        EGL14.eglCreateWindowSurface(eglDisplay, cfg, st, intArrayOf(EGL14.EGL_NONE), 0)
      if (ws == EGL14.EGL_NO_SURFACE) {
        Log.e(TAG, "eglCreateWindowSurface failed: ${EGL14.eglGetError()}")
        return
      }
      eglWindowSurface = ws
      EGL14.eglMakeCurrent(eglDisplay, ws, ws, eglContext)
      // Initial transparent frame so the TextureView never composites an
      // undefined back buffer, then show whatever overlay is already resident.
      clearOutput()
      presentOverlay(pipelinePtr)
      Log.i(TAG, "output window surface ready ${pw}x$ph")
    }

    /** Destroy the window surface (if any), making the PBuffer current again so
     *  the context stays valid for monitor readbacks. */
    private fun releaseWindowSurface() {
      if (eglWindowSurface == EGL14.EGL_NO_SURFACE) return
      EGL14.eglMakeCurrent(eglDisplay, eglPbuffer, eglPbuffer, eglContext)
      EGL14.eglDestroySurface(eglDisplay, eglWindowSurface)
      eglWindowSurface = EGL14.EGL_NO_SURFACE
    }

    private fun setupEgl(): Boolean {
      // One config serving both the startup PBuffer (context-current before the
      // TextureView exists) and the window surface we present into; alpha for the
      // translucent overlay layer.
      val core =
        GlEgl.initContext(
          intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, GlEgl.EGL_OPENGL_ES3_BIT,
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT or EGL14.EGL_PBUFFER_BIT,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_NONE,
          ),
        ) ?: return false
      eglDisplay = core.display
      eglContext = core.context
      eglConfig = core.config
      eglPbuffer =
        EGL14.eglCreatePbufferSurface(
          eglDisplay,
          core.config,
          intArrayOf(EGL14.EGL_WIDTH, pw, EGL14.EGL_HEIGHT, ph, EGL14.EGL_NONE),
          0,
        )
      if (eglPbuffer == EGL14.EGL_NO_SURFACE) return false
      return EGL14.eglMakeCurrent(eglDisplay, eglPbuffer, eglPbuffer, eglContext)
    }

    private fun teardownEgl() {
      if (eglDisplay != EGL14.EGL_NO_DISPLAY) {
        EGL14.eglMakeCurrent(
          eglDisplay,
          EGL14.EGL_NO_SURFACE,
          EGL14.EGL_NO_SURFACE,
          EGL14.EGL_NO_CONTEXT,
        )
        if (eglWindowSurface != EGL14.EGL_NO_SURFACE) {
          EGL14.eglDestroySurface(eglDisplay, eglWindowSurface)
        }
        if (eglPbuffer != EGL14.EGL_NO_SURFACE) EGL14.eglDestroySurface(eglDisplay, eglPbuffer)
        GlEgl.destroyContext(eglDisplay, eglContext)
      }
      eglWindowSurface = EGL14.EGL_NO_SURFACE
      eglPbuffer = EGL14.EGL_NO_SURFACE
      eglDisplay = EGL14.EGL_NO_DISPLAY
      eglContext = EGL14.EGL_NO_CONTEXT
      eglConfig = null
    }
  }

  companion object {
    private const val TAG = "ScreenCaptureGlWorker"

    /** Overlay opacity multiplier for the screen path (× the window alpha).
     *  1.0 = as opaque as the window allows; lower for see-through. */
    private const val OVERLAY_ALPHA = 1.0f

    /** Poll interval while a settle deadline is armed (the monitor wants ticks):
     *  the mirror stops emitting frames once motion stops, so we wake on a timer
     *  to hit the deadline. */
    private const val POLL_MS = 30L

    // Packed result of the native screen monitor (see LivePipelineJni).
    private const val MONITOR_ACTION_MASK = 0x3
    private const val MONITOR_ACTION_HIDE = 1
    private const val MONITOR_ACTION_ACQUIRE = 2
    private const val MONITOR_WANTS_TICK = 0x100

    /** Row-major 3×3 unit-quad → sensor-uv from a `SurfaceTexture`
     *  4×4 transform (column-major): the capture has no rotation, just the
     *  producer's crop + the conventional vertical flip. */
    fun uvFromSurfaceTexture(m: FloatArray): FloatArray =
      floatArrayOf(
        m[0], m[4], m[12],
        m[1], m[5], m[13],
        0f, 0f, 1f,
      )
  }
}
