package com.akslabs.circletosearch

import android.accessibilityservice.AccessibilityService
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Build
import android.util.Log
import android.view.Display
import android.view.View
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import androidx.core.content.ContextCompat
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageMetadataManager
import dev.davidv.translator.LanguageStateManager
import dev.davidv.translator.OverlayTextTranslationHelper
import dev.davidv.translator.OverlayWindowHost
import dev.davidv.translator.OverlayWindowHosts
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.TranslatorApplication
import dev.davidv.translator.accessibilityOverlay.OverlayUI
import dev.davidv.translator.accessibilityOverlay.TranslatorAccessibilityService
import dev.davidv.translator.ui.components.DetectedRegions
import dev.davidv.translator.ui.components.ImageWordSelection as WordSelection
import dev.davidv.translator.ui.components.SelectionSurfaceState
import dev.davidv.translator.R as TranslatorR
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class TranslatorOverlayBridge(
  private val host: CircleToSearchAccessibilityService,
  private val windowManager: WindowManager,
) : dev.davidv.translator.accessibilityOverlay.TranslatorOverlayHost,
  OverlayWindowHost {
  override val hostContext: Context
    get() = host

  private val tag = "TranslatorBridge"
  private var active = false
  private var selectMode = false
  private var returnToCaller: (() -> Unit)? = null
  private var providedScreenshot: Bitmap? = null
  private var forcedSourceLanguage: Language? = null
  private var forcedTargetLanguage: Language? = null
  private var isAutoSource: Boolean = true
  private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val mainHandler = android.os.Handler(android.os.Looper.getMainLooper())
  private var capturedChrome: Runnable? = null
  private var capturedScanning = false
  private var capturedRegion: Rect? = null
  /** Whether the translated result window consumes taps (word selection / copy).
   *  False for live-screen modes (long-press translate, bubble) where taps must
   *  pass through to the app underneath. */
  private var resultTapToCopy = true
  /** Flip-button state outside select mode: showing the original capture instead
   *  of the translated overlay. Reset whenever a new result is drawn. */
  private var showingOriginal = false

  /** Bumped on [deactivate] so a result finishing after close is dropped, not drawn. */
  private var overlaySession = 0

  private data class TranslatedScreen(
    val original: Bitmap,
    val translated: Bitmap,
    val words: WordSelection,
    val region: Rect,
  )

  private var lastResult: TranslatedScreen? = null
  private val surfaceState = SelectionSurfaceState()
  private var ocrReadingOrder: ReadingOrder? = null

  private val app = host.application as TranslatorApplication
  private val settingsManager = app.settingsManager
  private val translationCoordinator
    get() = runCatching { app.translationCoordinator }.getOrNull()

  override lateinit var langStateManager: LanguageStateManager
    private set

  private lateinit var overlayTextTranslationHelper: OverlayTextTranslationHelper
  private lateinit var ui: OverlayUI

  private val disableReceiver =
    object : BroadcastReceiver() {
      override fun onReceive(
        context: Context?,
        intent: Intent?,
      ) {
        deactivate()
      }
    }

  fun start() {
    langStateManager = LanguageStateManager(serviceScope, app.filePathManager, null)
    overlayTextTranslationHelper =
      OverlayTextTranslationHelper(
        langStateManager,
        LanguageMetadataManager(host, app.languagesFlow),
      )
    ui = OverlayUI(this, host, windowManager, settingsManager)
    OverlayWindowHosts.primary = this

    ContextCompat.registerReceiver(
      host,
      disableReceiver,
      IntentFilter(TranslatorAccessibilityService.ACTION_DISABLE),
      ContextCompat.RECEIVER_NOT_EXPORTED,
    )
  }

  override fun addOverlayView(
    view: View,
    params: WindowManager.LayoutParams,
  ) {
    params.type = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
    windowManager.addView(view, params)
  }

  override fun removeOverlayView(view: View) {
    windowManager.removeView(view)
  }

  override fun updateOverlayView(
    view: View,
    params: WindowManager.LayoutParams,
  ) {
    params.type = WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY
    windowManager.updateViewLayout(view, params)
  }

  fun destroy() {
    if (OverlayWindowHosts.primary === this) {
      OverlayWindowHosts.primary = null
    }
    try {
      host.unregisterReceiver(disableReceiver)
    } catch (_: Exception) {
    }
    if (::ui.isInitialized) {
      deactivate()
      ui.dismissMenu()
      ui.cleanup()
    }
    serviceScope.cancel()
  }

  fun onHostAccessibilityEvent(event: AccessibilityEvent?) {
    if (event == null || event.packageName == host.packageName) return
    when (event.eventType) {
      AccessibilityEvent.TYPE_VIEW_SCROLLED,
      AccessibilityEvent.TYPE_VIEW_CLICKED,
      -> {
        lastResult = null
        if (::ui.isInitialized) ui.removeTranslationOverlays()
      }
    }
  }

  override fun activate() {
    if (active) return
    // Bubble / live-screen mode: the app underneath stays interactive, so the
    // result window must pass taps through (auto-clears on the next click/scroll).
    resultTapToCopy = false
    enterActiveState()
    android.os.Handler(android.os.Looper.getMainLooper()).post {
      if (active) {
        handleFullScreenOcr()
      }
    }
  }

  /**
   * Starts OCR + translation of the screenshot captured when Circle to Search launched
   * immediately, so compute overlaps the caller's UI-hide delay. The translator chrome
   * (border wave + toolbar, then the scan overlay if still computing) is drawn only
   * after [uiDelayMs], once the caller's UI has been hidden. [returnToCaller] is invoked
   * when the overlay is deactivated (toolbar close, back press) so the caller can
   * restore its UI. [tapToCopy] — false when the live screen stays interactive
   * underneath (long-press translate closes Circle to Search); the result window
   * then passes taps through instead of consuming them for word selection.
   */
  fun startCapturedTranslation(
    screenshot: Bitmap,
    uiDelayMs: Long = 0L,
    returnToCaller: (() -> Unit)? = null,
    tapToCopy: Boolean = true,
  ) {
    this.returnToCaller = returnToCaller
    this.resultTapToCopy = tapToCopy
    providedScreenshot = screenshot

    if (active) {
      if (selectMode) exitSelectMode()
      beginProvidedTranslation(screenshot)
      // Re-sync the chrome for this trigger instead of leaving whatever the
      // previous session drew: showToolbar rebuilds when the mode (select
      // button vs flip-only) changed, otherwise just updates the labels.
      showCapturedChrome()
      return
    }

    // Mark active now (gates bubble restore / receivers) but draw the chrome only after
    // the caller's hide delay, so Circle to Search disappears first.
    active = true
    langStateManager.refreshLanguageAvailability()
    beginProvidedTranslation(screenshot)

    if (uiDelayMs <= 0L) {
      showCapturedChrome()
    } else {
      val show = Runnable {
        capturedChrome = null
        if (providedScreenshot != null) showCapturedChrome()
      }
      capturedChrome = show
      mainHandler.postDelayed(show, uiDelayMs)
    }
  }

  private fun showCapturedChrome() {
    showTranslatorChrome()
    showScanIfScanning()
  }

  private fun showScanIfScanning() {
    if (capturedScanning) capturedRegion?.let { ui.showScanOverlay(surfaceState, it) }
  }

  private fun cancelCapturedChrome() {
    capturedChrome?.let { mainHandler.removeCallbacks(it) }
    capturedChrome = null
  }

  /**
   * Fades the overlay windows during the system back gesture ([alpha] 1 = normal,
   * 0 = hidden) so the predictive back arrow, drawn below accessibility overlays,
   * stays visible.
   */
  fun setGestureAlpha(alpha: Float) {
    if (!active) return
    ui.setOverlayAlpha(alpha)
  }

  private fun showTranslatorChrome() {
    ui.showBorderWave()
    // Live-screen modes (resultTapToCopy == false: long-press translate, bubble)
    // need the select-text button — taps pass through the result there, so copy
    // is only reachable through select mode. Frozen modes get flip instead.
    ui.showToolbar(
      forcedSourceLanguage,
      forcedTargetLanguage,
      currentReadingOrderFor(forcedSourceLanguage),
      isAutoSource,
      showSelectButton = !resultTapToCopy,
    )
  }

  private fun enterActiveState() {
    active = true
    langStateManager.refreshLanguageAvailability()
    ui.removeTranslationOverlays()
    showTranslatorChrome()
  }

  override fun deactivate() {
    cancelCapturedChrome()
    overlaySession++
    active = false
    selectMode = false
    resultTapToCopy = true
    showingOriginal = false
    ui.removeSelectSurface()
    surfaceState.clear()
    lastResult = null
    ui.removeBorderWave()
    ui.removeToolbar()
    ui.removeTranslationOverlays()
    ui.dismissMenu()
    providedScreenshot = null
    returnToCaller?.let { cb ->
      returnToCaller = null
      cb()
    }
  }

  override fun toggleSelectMode() {
    if (selectMode) {
      exitSelectMode()
      return
    }
    selectMode = true
    ui.setSelectModeUi(true)
    val res = lastResult
    if (res != null) {
      ui.removeTranslationOverlays()
      surfaceState.showResult(res.translated, res.original, res.words)
      ui.showSelectSurface(surfaceState, res.region) { onSelectSurfaceDismissed() }
    } else {
      handleFullScreenOcr()
    }
  }

  private fun exitSelectMode() {
    selectMode = false
    showingOriginal = false
    ui.setFlipActive(false)
    lastResult?.let { ui.showBitmapOverlay(it.translated, it.region, tapToCopy = resultTapToCopy) }
    ui.removeSelectSurface()
    surfaceState.clear()
    ui.setSelectModeUi(false)
  }

  private fun onSelectSurfaceDismissed() {
    if (selectMode) exitSelectMode()
  }

  override fun toggleFlipOriginal() {
    if (selectMode) {
      ui.setFlipActive(surfaceState.toggleShowOriginal())
      return
    }
    // Outside select mode (no select button in this mode): swap the static result
    // overlay between the original and translated bitmaps.
    val res = lastResult ?: return
    showingOriginal = !showingOriginal
    ui.setFlipActive(showingOriginal)
    ui.removeTranslationOverlays()
    ui.showBitmapOverlay(
      if (showingOriginal) res.original else res.translated,
      res.region,
      tapToCopy = resultTapToCopy,
    )
  }

  override fun swapLanguages() {
    val oldSource = forcedSourceLanguage ?: return
    val oldTarget = forcedTargetLanguage ?: langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode) ?: return
    if (!langStateManager.canSwapLanguages(oldSource, oldTarget)) return
    isAutoSource = false
    forcedSourceLanguage = oldTarget
    forcedTargetLanguage = oldSource
    syncReadingOrderForSource()
    ui.updateToolbarState(forcedSourceLanguage, forcedTargetLanguage, currentReadingOrderFor(forcedSourceLanguage), isAutoSource)
    if (active) {
      handleFullScreenOcr()
    }
  }

  override fun showLanguagePicker(isSource: Boolean) {
    serviceScope.launch {
      langStateManager.refreshLanguageAvailability()
      overlayTextTranslationHelper.awaitPickerRows()
      ui.showLanguagePicker(
        isSource,
        rowsProvider = { overlayTextTranslationHelper.pickerRows() },
        onPick = { lang ->
          if (isSource) {
            if (lang == null) {
              isAutoSource = true
            } else {
              isAutoSource = false
              forcedSourceLanguage = lang
            }
            syncReadingOrderForSource()
          } else {
            forcedTargetLanguage = lang
          }
          ui.updateToolbarState(forcedSourceLanguage, forcedTargetLanguage, currentReadingOrderFor(forcedSourceLanguage), isAutoSource)
          if (active) {
            handleFullScreenOcr()
          }
        },
        onToggleFavorite = { language, favorite ->
          overlayTextTranslationHelper.setFavorite(language, favorite)
        },
        onDelete = { language ->
          langStateManager.deleteLanguage(language)
        },
      )
    }
  }

  override fun toggleJapaneseOcrMode() {
    if (forcedSourceLanguage?.code != "ja") return
    ocrReadingOrder =
      when (ocrReadingOrder) {
        null -> ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT
        ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT -> ReadingOrder.LEFT_TO_RIGHT
        ReadingOrder.LEFT_TO_RIGHT -> null
      }
    ui.updateToolbarState(forcedSourceLanguage, forcedTargetLanguage, ocrReadingOrder, isAutoSource)
    if (active) {
      handleFullScreenOcr()
    }
  }

  override fun showDotsMenu() {
    ui.showDotsMenu()
  }

  override fun disableService() {
    deactivate()
  }

  override fun handleFullScreenOcr() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return

    val res = lastResult
    if (selectMode && ui.hasSelectSurface() && res != null) {
      surfaceState.showProcessing(res.original)
      serviceScope.launch {
        translateRegionBitmap(res.original, res.region)
      }
      return
    }
    ui.removeTranslationOverlays()

    val sourceLang = ocrSourceLanguage()
    if (!isAutoSource && sourceLang == null) {
      ui.showOverlayMessage("Set source language first")
      return
    }

    val region = currentCaptureRegion()

    ui.dismissMenu()
    android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
      captureFullScreen(region)
    }, SCREENSHOT_RENDER_DELAY_MS)
  }

  /** The screen area above the bottom toolbar — capture and overlay region. */
  private fun currentCaptureRegion(): Rect {
    val content = dev.davidv.translator.overlayChrome.OverlayInsets.contentBounds(windowManager)
    return Rect(
      content.left,
      content.top,
      content.right,
      content.bottom - ui.dpToPx(80),
    )
  }

  /** Full content area — for a provided capture the result also fills the zone behind
   *  the translucent toolbar, so translated UI shows through it. */
  private fun fullCaptureRegion(): Rect =
    dev.davidv.translator.overlayChrome.OverlayInsets.contentBounds(windowManager)

  /** Crops the provided capture and starts OCR/translate immediately — no UI here;
   *  the caller draws the scan overlay and chrome separately (they may be delayed). */
  private fun beginProvidedTranslation(screenshot: Bitmap) {
    ui.removeTranslationOverlays()

    val sourceLang = ocrSourceLanguage()
    if (!isAutoSource && sourceLang == null) {
      ui.showOverlayMessage("Set source language first")
      return
    }

    val region = fullCaptureRegion()
    ui.dismissMenu()

    val cropped = cropToRegion(screenshot, region) ?: return
    surfaceState.regions.value = null
    capturedRegion = region
    capturedScanning = true
    serviceScope.launch {
      try {
        translateRegionBitmap(cropped, region)
      } finally {
        capturedScanning = false
      }
    }
  }

  private fun cropToRegion(full: Bitmap, region: Rect): Bitmap? {
    val cropLeft = region.left.coerceIn(0, full.width - 1)
    val cropTop = region.top.coerceIn(0, full.height - 1)
    val cropWidth = region.width().coerceAtMost(full.width - cropLeft)
    val cropHeight = region.height().coerceAtMost(full.height - cropTop)
    if (cropWidth <= 0 || cropHeight <= 0) return null
    return Bitmap.createBitmap(full, cropLeft, cropTop, cropWidth, cropHeight)
  }

  private fun captureFullScreen(region: Rect) {
    // Refresh / language change always re-captures the LIVE screen via a11y
    // takeScreenshot — the caller already removed the result overlays and waited
    // SCREENSHOT_RENDER_DELAY_MS so they aren't in the capture (the toolbar sits
    // above `region`). Re-translating the original provided screenshot here made
    // refresh a no-op after the underlying screen changed.
    host.takeScreenshot(
      Display.DEFAULT_DISPLAY,
      host.mainExecutor,
      object : AccessibilityService.TakeScreenshotCallback {
        override fun onSuccess(screenshot: AccessibilityService.ScreenshotResult) {
          val hwBitmap = Bitmap.wrapHardwareBuffer(screenshot.hardwareBuffer, screenshot.colorSpace)
          screenshot.hardwareBuffer.close()
          if (hwBitmap == null) return
          val fullBitmap = hwBitmap.copy(Bitmap.Config.ARGB_8888, false)
          hwBitmap.recycle()

          val croppedBitmap = cropToRegion(fullBitmap, region)
          if (croppedBitmap !== fullBitmap) fullBitmap.recycle()
          if (croppedBitmap == null) return

          surfaceState.regions.value = null
          ui.showScanOverlay(surfaceState, region)
          serviceScope.launch {
            translateRegionBitmap(croppedBitmap, region)
          }
        }

        override fun onFailure(errorCode: Int) {
          Log.w(tag, "Screenshot failed: $errorCode")
        }
      },
    )
  }

  private suspend fun translateRegionBitmap(
    bitmap: Bitmap,
    region: Rect,
  ) {
    val session = overlaySession
    val coordinator = translationCoordinator ?: return
    val sourceLang = ocrSourceLanguage()
    val targetLang = forcedTargetLanguage ?: langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode) ?: return
    val ocrSourceLang = sourceLang ?: targetLang

    var ocrUnavailable = false
    val result =
      withContext(Dispatchers.IO) {
        coordinator.translateImageWithOverlay(
          ocrSourceLang,
          targetLang,
          bitmap,
          onMessage = {},
          readingOrder = currentReadingOrderFor(sourceLang),
          isAutoSource = isAutoSource,
          onOcrUnavailable = { ocrUnavailable = true },
          onDetectedRegions = { boxes, w, h ->
            surfaceState.regions.value = DetectedRegions(w, h, boxes)
          },
        )
      }

    if (session != overlaySession || !active) return

    ui.removeTranslationOverlays()
    if (result == null) {
      if (selectMode) surfaceState.processing.value = false
      if (ocrUnavailable) {
        ui.showOverlayMessage(host.getString(TranslatorR.string.ocr_models_missing))
      }
      return
    }

    val words =
      WordSelection(
        imageWidth = result.metadata.width.toInt(),
        imageHeight = result.metadata.height.toInt(),
        sourceWords = result.metadata.sourceWords,
        translatedWords = result.translatedWords,
      )
    lastResult = TranslatedScreen(original = bitmap, translated = result.correctedBitmap, words = words, region = region)
    showingOriginal = false
    ui.setFlipActive(false)
    if (selectMode) {
      surfaceState.showResult(result.correctedBitmap, bitmap, words)
      if (!ui.hasSelectSurface()) {
        ui.showSelectSurface(surfaceState, region) { onSelectSurfaceDismissed() }
      }
    } else {
      ui.showBitmapOverlay(result.correctedBitmap, region, tapToCopy = resultTapToCopy)
    }
  }

  private fun ocrSourceLanguage(): Language? =
    forcedSourceLanguage ?: settingsManager.settings.value.defaultSourceLanguageCode?.let {
      langStateManager.languageByCode(it)
    }

  /**
   * The default target language. OCR-only flows (instant text selection over a screenshot)
   * still need a target because the module's plan runs through the translation pipeline.
   */
  fun defaultTargetLanguage(): Language? =
    if (::langStateManager.isInitialized) {
      langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode)
    } else {
      null
    }

  private fun currentReadingOrderFor(language: Language?): ReadingOrder? =
    if (language?.code == "ja") {
      ocrReadingOrder
    } else {
      null
    }

  private fun syncReadingOrderForSource() {
    if (forcedSourceLanguage?.code != "ja") {
      ocrReadingOrder = null
    }
  }

  companion object {
    private const val SCREENSHOT_RENDER_DELAY_MS = 120L
  }
}
