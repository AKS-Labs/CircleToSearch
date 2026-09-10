package dev.davidv.translator

import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.GLES20

/**
 *  GLES/EGL boilerplate shared by the live render threads — the camera
 *  ([dev.davidv.translator.ui.components.LiveGlSurfaceView]) and the screen-capture
 *  worker ([dev.davidv.translator.screenTranslate.ScreenCaptureGlWorker]). Both mint
 *  a `GL_TEXTURE_EXTERNAL_OES` source texture and bring up an ES3 display+context the
 *  same way; only the EGL config attribs (alpha size, WINDOW vs WINDOW|PBUFFER) and
 *  the surface lifecycle differ, so those stay in each thread.
 */
object GlEgl {
  // android.opengl.GLES11Ext.GL_TEXTURE_EXTERNAL_OES — using the literal avoids the
  // GLES11Ext import (which only exists for this one constant).
  const val GL_TEXTURE_EXTERNAL_OES: Int = 0x8D65

  // EGL10.EGL_OPENGL_ES3_BIT_KHR; EGL14 doesn't expose it as a named constant
  // despite being defined since API 18.
  const val EGL_OPENGL_ES3_BIT: Int = 0x40

  /** A brought-up EGL display + chosen config + ES3 context. The context is created
   *  but NOT made current — the caller creates its surface(s) and binds them. */
  data class EglCore(val display: EGLDisplay, val config: EGLConfig, val context: EGLContext)

  /** Mint a `GL_TEXTURE_EXTERNAL_OES` texture (linear filter, clamp-to-edge) for a
   *  SurfaceTexture-backed camera/screen source. Returns 0 on failure. Must run on
   *  the thread that owns the GL context. */
  fun createExternalOesTexture(): Int {
    val ids = IntArray(1)
    GLES20.glGenTextures(1, ids, 0)
    val id = ids[0]
    if (id == 0) return 0
    val target = GL_TEXTURE_EXTERNAL_OES
    GLES20.glBindTexture(target, id)
    GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR)
    GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
    GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
    GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
    GLES20.glBindTexture(target, 0)
    return id
  }

  /** Open the default display, initialize EGL, choose a config matching
   *  `configAttribs`, and create an ES3 context. Returns null on any failure. No
   *  surface is created and the context is not current yet. */
  fun initContext(configAttribs: IntArray): EglCore? {
    val display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
    if (display == EGL14.EGL_NO_DISPLAY) return null
    val ver = IntArray(2)
    if (!EGL14.eglInitialize(display, ver, 0, ver, 1)) return null
    val cfgs = arrayOfNulls<EGLConfig>(1)
    val num = IntArray(1)
    if (!EGL14.eglChooseConfig(display, configAttribs, 0, cfgs, 0, 1, num, 0) || num[0] <= 0) {
      return null
    }
    val cfg = cfgs[0] ?: return null
    val context =
      EGL14.eglCreateContext(
        display,
        cfg,
        EGL14.EGL_NO_CONTEXT,
        intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 3, EGL14.EGL_NONE),
        0,
      )
    if (context == EGL14.EGL_NO_CONTEXT) return null
    return EglCore(display, cfg, context)
  }

  /** Unbind the current context, destroy `context`, and terminate `display`. The
   *  caller must destroy its own surfaces first. */
  fun destroyContext(
    display: EGLDisplay,
    context: EGLContext,
  ) {
    if (display == EGL14.EGL_NO_DISPLAY) return
    EGL14.eglMakeCurrent(
      display,
      EGL14.EGL_NO_SURFACE,
      EGL14.EGL_NO_SURFACE,
      EGL14.EGL_NO_CONTEXT,
    )
    if (context != EGL14.EGL_NO_CONTEXT) EGL14.eglDestroyContext(display, context)
    EGL14.eglTerminate(display)
  }
}
