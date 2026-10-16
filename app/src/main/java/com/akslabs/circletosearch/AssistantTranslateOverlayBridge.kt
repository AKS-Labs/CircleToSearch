package com.akslabs.circletosearch

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import android.os.Handler
import android.os.Looper
import android.view.View
import android.view.WindowManager
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageMetadataManager
import dev.davidv.translator.LanguageStateManager
import dev.davidv.translator.OverlayTextTranslationHelper
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.TranslatorApplication
import dev.davidv.translator.accessibilityOverlay.OverlayUI
import dev.davidv.translator.accessibilityOverlay.TranslatorOverlayHost
import dev.davidv.translator.ui.components.DetectedRegions
import dev.davidv.translator.ui.components.ImageWordSelection as WordSelection
import dev.davidv.translator.ui.components.SelectionSurfaceState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Activity-hosted translate overlay for the assistant-only path (no accessibility
 * permission). Reuses the exact same OverlayUI chrome (toolbar, scan, selection surface)
 * the accessibility path uses, hosted in our own activity's window with a normal
 * application-overlay window type. The OCR + translation itself is just the
 * translationCoordinator running on the bitmap — it needs no accessibility permission;
 * only the TYPE_ACCESSIBILITY_OVERLAY window type did, which is what this replaces.
 */
class AssistantTranslateOverlayBridge(
  private val activity: android.app.Activity,
) : TranslatorOverlayHost {
  override val hostContext: Context
    get() = activity

  private val app = activity.application as TranslatorApplication
  private val windowManager =
    activity.getSystemService(Context.WINDOW_SERVICE) as WindowManager
  private val settingsManager = app.settingsManager
  private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val mainHandler = Handler(Looper.getMainLooper())
  private lateinit var container: android.widget.FrameLayout

  private var active = false
  private var selectMode = false
  private var returnToCaller: (() -> Unit)? = null
  private var providedScreenshot: Bitmap? = null
  private var forcedSourceLanguage: Language? = null
  private var forcedTargetLanguage: Language? = null
  private var isAutoSource: Boolean = true
  private var capturedChrome: Runnable? = null
  private var capturedScanning = false
  private var capturedRegion: Rect? = null
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
  /** Flip-button state outside select mode: showing the original capture instead
   *  of the translated overlay. Reset whenever a new result is drawn. */
  private var showingOriginal = false

  private val translationCoordinator
    get() = runCatching { app.translationCoordinator }.getOrNull()

  override lateinit var langStateManager: LanguageStateManager
    private set
  private lateinit var overlayTextTranslationHelper: OverlayTextTranslationHelper
  private lateinit var ui: OverlayUI

  fun start() {
    langStateManager = LanguageStateManager(serviceScope, app.filePathManager, null)
    overlayTextTranslationHelper =
      OverlayTextTranslationHelper(
        langStateManager,
        LanguageMetadataManager(activity, app.languagesFlow),
      )
    // The chrome lives in a full-screen FrameLayout inside our overlay activity (already a
    // window on top of everything), not in separate overlay windows — our activity has no
    // SYSTEM_ALERT_WINDOW / a11y permission to add TYPE_APPLICATION_OVERLAY windows.
    container = android.widget.FrameLayout(activity)
    activity.addContentView(
      container,
      android.view.ViewGroup.LayoutParams(
        android.view.ViewGroup.LayoutParams.MATCH_PARENT,
        android.view.ViewGroup.LayoutParams.MATCH_PARENT,
      ),
    )
    ui = OverlayUI(this, activity, windowManager, settingsManager, container = container)
    langStateManager.refreshLanguageAvailability()
  }

  /**
   * Starts OCR + translation of the already-captured screenshot immediately. The
   * chrome (toolbar + scan) draws after [uiDelayMs], once the caller's UI has been
   * hidden. [onClosed] is invoked when the overlay is dismissed.
   */
  fun startCapturedTranslation(
    screenshot: Bitmap,
    uiDelayMs: Long = 0L,
    onClosed: (() -> Unit)? = null,
  ) {
    this.returnToCaller = onClosed
    providedScreenshot = screenshot

    if (active) {
      if (selectMode) exitSelectMode()
      beginProvidedTranslation(screenshot)
      showScanIfScanning()
      return
    }

    active = true
    // The catalog loads asynchronously inside LanguageStateManager.init, and both
    // OverlayUI.showToolbar and translateRegionBitmap early-return when languageByCode
    // is still null. Wait for it before starting OCR/translation and showing chrome.
    serviceScope.launch {
      langStateManager.catalog.first { it != null }
      if (!active) return@launch
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

  private fun showTranslatorChrome() {
    ui.showToolbar(
      forcedSourceLanguage,
      forcedTargetLanguage,
      currentReadingOrderFor(forcedSourceLanguage),
      isAutoSource,
      showSelectButton = false,
    )
  }

  override fun activate() {
    if (active) return
    active = true
    serviceScope.launch {
      langStateManager.catalog.first { it != null }
      if (!active) return@launch
      langStateManager.refreshLanguageAvailability()
      ui.removeTranslationOverlays()
      showTranslatorChrome()
      if (active) handleFullScreenOcr()
    }
  }

  override fun deactivate() {
    cancelCapturedChrome()
    overlaySession++
    active = false
    selectMode = false
    showingOriginal = false
    ui.removeSelectSurface()
    surfaceState.clear()
    lastResult = null
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
    lastResult?.let { ui.showBitmapOverlay(it.translated, it.region) }
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
    // No select button in this mode: flip swaps the static result overlay
    // between the original and translated bitmaps.
    val res = lastResult ?: return
    showingOriginal = !showingOriginal
    ui.setFlipActive(showingOriginal)
    ui.removeTranslationOverlays()
    ui.showBitmapOverlay(if (showingOriginal) res.original else res.translated, res.region)
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
    if (active) handleFullScreenOcr()
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
          if (active) handleFullScreenOcr()
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
    if (active) handleFullScreenOcr()
  }

  override fun showDotsMenu() {
    ui.showDotsMenu()
  }

  override fun disableService() {
    deactivate()
  }

  override fun handleFullScreenOcr() {
    val res = lastResult
    if (selectMode && ui.hasSelectSurface() && res != null) {
      surfaceState.showProcessing(res.original)
      serviceScope.launch { translateRegionBitmap(res.original, res.region) }
      return
    }
    ui.removeTranslationOverlays()

    val sourceLang = ocrSourceLanguage()
    if (!isAutoSource && sourceLang == null) {
      ui.showOverlayMessage("Set source language first")
      return
    }

    val region = fullCaptureRegion()
    ui.dismissMenu()

    val provided = providedScreenshot
    if (provided != null) {
      val cropped = cropToRegion(provided, region) ?: return
      surfaceState.regions.value = null
      ui.showScanOverlay(surfaceState, region)
      serviceScope.launch { translateRegionBitmap(cropped, region) }
      return
    }
  }

  private fun fullCaptureRegion(): Rect =
    dev.davidv.translator.overlayChrome.OverlayInsets.contentBounds(windowManager)

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

  private suspend fun translateRegionBitmap(bitmap: Bitmap, region: Rect) {
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
        ui.showOverlayMessage(activity.getString(dev.davidv.translator.R.string.ocr_models_missing))
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
      ui.showBitmapOverlay(result.correctedBitmap, region)
    }
  }

  private fun ocrSourceLanguage(): Language? =
    forcedSourceLanguage ?: settingsManager.settings.value.defaultSourceLanguageCode?.let {
      langStateManager.languageByCode(it)
    }

  private fun currentReadingOrderFor(language: Language?): ReadingOrder? =
    if (language?.code == "ja") ocrReadingOrder else null

  private fun syncReadingOrderForSource() {
    if (forcedSourceLanguage?.code != "ja") ocrReadingOrder = null
  }

  fun destroy() {
    deactivate()
    ui.dismissMenu()
    ui.cleanup()
    serviceScope.cancel()
    (container.parent as? android.view.ViewGroup)?.removeView(container)
  }
}
