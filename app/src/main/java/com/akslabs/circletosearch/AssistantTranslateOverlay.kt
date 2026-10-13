package com.akslabs.circletosearch

import android.graphics.Bitmap

/**
 * Activity-scoped holder for the assistant-only translate overlay. Lets
 * CircleToSearchScreen start translation (no accessibility permission) through the
 * same OverlayUI chrome the accessibility path uses, hosted in our own activity.
 */
object AssistantTranslateOverlay {
  @Volatile
  var bridge: AssistantTranslateOverlayBridge? = null
    private set

  fun attach(b: AssistantTranslateOverlayBridge) {
    bridge = b
  }

  fun detach(b: AssistantTranslateOverlayBridge) {
    if (bridge === b) bridge = null
  }

  /**
   * Starts OCR + translation of an already-captured screenshot using the
   * activity-hosted overlay. Returns false if no bridge is attached.
   */
  fun start(screenshot: Bitmap, uiDelayMs: Long = 0L, onClosed: (() -> Unit)? = null): Boolean {
    val b = bridge ?: return false
    b.startCapturedTranslation(screenshot, uiDelayMs, onClosed)
    return true
  }

  fun close() {
    bridge?.deactivate()
  }
}
