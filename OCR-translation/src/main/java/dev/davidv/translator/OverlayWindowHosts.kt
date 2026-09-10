package dev.davidv.translator

import android.view.View
import android.view.ViewGroup
import android.view.WindowManager

interface OverlayWindowHost {
  fun addOverlayView(
    view: View,
    params: WindowManager.LayoutParams,
  )

  fun removeOverlayView(view: View)

  fun updateOverlayView(
    view: View,
    params: WindowManager.LayoutParams,
  )
}

object OverlayWindowHosts {
  @Volatile
  var primary: OverlayWindowHost? = null
}

class RoutedWindowManager(
  private val direct: WindowManager,
) : WindowManager by direct {
  override fun addView(
    view: View?,
    params: ViewGroup.LayoutParams?,
  ) {
    val host = OverlayWindowHosts.primary
    if (view != null && params is WindowManager.LayoutParams && host != null) {
      params.type = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
      host.addOverlayView(view, params)
    } else {
      direct.addView(view, params)
    }
  }

  override fun removeView(view: View?) {
    val host = OverlayWindowHosts.primary
    if (view != null && host != null) {
      runCatching { host.removeOverlayView(view) }.onFailure { runCatching { direct.removeView(view) } }
    } else {
      direct.removeView(view)
    }
  }

  override fun updateViewLayout(
    view: View?,
    params: ViewGroup.LayoutParams?,
  ) {
    val host = OverlayWindowHosts.primary
    if (view != null && params is WindowManager.LayoutParams && host != null) {
      params.type = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
      host.updateOverlayView(view, params)
    } else {
      direct.updateViewLayout(view, params)
    }
  }
}
