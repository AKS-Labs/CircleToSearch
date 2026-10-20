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

import android.content.Context
import android.graphics.SurfaceTexture
import android.view.TextureView

/**
 *  Output surface for the GPU screen overlay. A TextureView composites its GL
 *  content *through the window* (unlike a SurfaceView, which is a separate
 *  full-opacity SurfaceFlinger layer), so the window's sub-cap alpha dims it and
 *  Android 12+'s obscured-touch rule lets taps fall through to the app below —
 *  validated on device. The [ScreenCaptureGlWorker] owns all GL: it creates an
 *  EGL window surface from this view's SurfaceTexture and presents the overlay
 *  straight into it. This view only forwards the SurfaceTexture's lifecycle.
 */
class GpuOverlayTextureView(context: Context) :
  TextureView(context),
  TextureView.SurfaceTextureListener {
  /** Invoked on the UI thread with the live SurfaceTexture when it becomes
   *  available, and with `null` when it is destroyed. */
  var onSurfaceTexture: ((SurfaceTexture?) -> Unit)? = null

  init {
    isOpaque = false
    surfaceTextureListener = this
  }

  override fun onSurfaceTextureAvailable(
    surface: SurfaceTexture,
    width: Int,
    height: Int,
  ) {
    onSurfaceTexture?.invoke(surface)
  }

  override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
    onSurfaceTexture?.invoke(null)
    return true
  }

  override fun onSurfaceTextureSizeChanged(
    surface: SurfaceTexture,
    width: Int,
    height: Int,
  ) {
    // On rotation the window (MATCH_PARENT) resizes; re-hand the surface so the GL
    // worker recreates its EGL window surface at the new size and presents correctly.
    onSurfaceTexture?.invoke(surface)
  }

  override fun onSurfaceTextureUpdated(surface: SurfaceTexture) {}
}
