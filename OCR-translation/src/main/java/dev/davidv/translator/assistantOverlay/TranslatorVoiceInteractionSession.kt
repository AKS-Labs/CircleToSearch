package dev.davidv.translator.assistantOverlay

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.provider.Settings
import android.service.voice.VoiceInteractionSession
import android.util.Log
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.TextView
import androidx.core.view.WindowInsetsCompat
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageStateManager
import dev.davidv.translator.MainActivity
import dev.davidv.translator.OverlayTextTranslationHelper
import dev.davidv.translator.R
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.SettingsManager
import dev.davidv.translator.TranslationCoordinator
import dev.davidv.translator.overlayChrome.OverlayChromeFactory
import dev.davidv.translator.overlayChrome.OverlayInsets
import dev.davidv.translator.overlayChrome.OverlayMenuHost
import dev.davidv.translator.overlayChrome.OverlayMenuManager
import dev.davidv.translator.overlayChrome.spanFullDisplay
import dev.davidv.translator.screenTranslate.ScreenCaptureRequestActivity
import dev.davidv.translator.ui.components.DetectedRegions
import dev.davidv.translator.ui.components.ImageWordSelection
import dev.davidv.translator.ui.components.SelectionSurface
import dev.davidv.translator.ui.components.SelectionSurfaceState
import dev.davidv.translator.ui.components.WindowComposeHost
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import androidx.core.graphics.Insets as ContentInsets

class TranslatorVoiceInteractionSession(
  context: Context,
  private val settingsManager: SettingsManager,
  private val translationCoordinator: TranslationCoordinator,
  private val overlayTextTranslationHelper: OverlayTextTranslationHelper,
  private val langStateManager: LanguageStateManager,
) : VoiceInteractionSession(context) {
  companion object {
    private const val ASSIST_SCREENSHOT_ENABLED_SETTING = "assist_screenshot_enabled"
    private const val CAPTURE_TIMEOUT_MS = 1500L
  }

  private val tag = "TranslatorAssistant"
  private val sessionScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

  private lateinit var rootView: FrameLayout
  private lateinit var screenshotView: ImageView
  private lateinit var overlayContainer: FrameLayout
  private lateinit var statusView: TextView
  private lateinit var topBarView: View
  private var sourceLabelView: TextView? = null
  private var targetLabelView: TextView? = null
  private var readingOrderButtonView: View? = null
  private var readingOrderIconView: ImageView? = null
  private var flipIconView: ImageView? = null
  private var flipButtonView: View? = null
  private var menuManager: OverlayMenuManager? = null
  private var contentInsets = ContentInsets.NONE

  private var pendingScreenshot: Bitmap? = null
  private var screenshotBitmap: Bitmap? = null
  private var croppedBitmap: Bitmap? = null
  private var processing = false
  private var translationJob: Job? = null
  private var captureTimeoutJob: Job? = null
  private var statusHideJob: Job? = null
  private var forcedSourceLanguage: Language? = null
  private var isAutoSource: Boolean = true
  private var forcedTargetLanguage: Language? = null
  private var ocrReadingOrder: ReadingOrder? = null

  // The interactive image surface, hosted in a ComposeView over the screenshot backdrop: it shows
  // the screenshot with the scan animation while OCR runs, then the translated image with the
  // word-selection overlay. An empty state keeps the layer hidden/pass-through.
  private val surfaceState = SelectionSurfaceState()
  private var resultHost: WindowComposeHost? = null

  override fun onCreate() {
    super.onCreate()
    configureSessionWindow()
    langStateManager.refreshLanguageAvailability()
  }

  override fun onCreateContentView(): View {
    rootView = FrameLayout(context)
    rootView.setBackgroundColor(Color.TRANSPARENT)
    // The screenshot backdrop replaces the whole screen, so the toolbar lives in
    // the status-bar strip — but it still has to clear a display cutout.
    rootView.setOnApplyWindowInsetsListener { _, insets ->
      val current =
        WindowInsetsCompat
          .toWindowInsetsCompat(insets, rootView)
          .getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      if (current != contentInsets) {
        contentInsets = current
        applyContentGeometry()
        consumePendingScreenshot()
      }
      insets
    }
    rootView.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
      applyContentGeometry()
      consumePendingScreenshot()
    }
    menuManager =
      OverlayMenuManager(
        context,
        ::dpToPx,
        object : OverlayMenuHost {
          override fun addDismissLayer(view: View) {
            rootView.addView(
              view,
              FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
              ),
            )
          }

          override fun addMenuView(view: View) {
            rootView.addView(
              view,
              FrameLayout
                .LayoutParams(
                  dpToPx(180),
                  FrameLayout.LayoutParams.WRAP_CONTENT,
                ).apply {
                  gravity = Gravity.TOP or Gravity.END
                  topMargin = contentInsets.top + dpToPx(48)
                  marginEnd = contentInsets.right + dpToPx(8)
                },
            )
          }

          override fun addPickerView(view: View) {
            rootView.addView(
              view,
              FrameLayout
                .LayoutParams(
                  dpToPx(250),
                  dpToPx(400),
                ).apply { gravity = Gravity.CENTER },
            )
          }

          override fun removeMenuChild(view: View) {
            rootView.removeView(view)
          }
        },
      )

    screenshotView =
      ImageView(context).apply {
        scaleType = ImageView.ScaleType.FIT_XY
        setBackgroundColor(Color.TRANSPARENT)
      }
    rootView.addView(
      screenshotView,
      FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.MATCH_PARENT,
        FrameLayout.LayoutParams.MATCH_PARENT,
      ),
    )

    overlayContainer = FrameLayout(context)
    rootView.addView(
      overlayContainer,
      FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.MATCH_PARENT,
        FrameLayout.LayoutParams.MATCH_PARENT,
      ),
    )

    // Interactive image layer: the scan animation while OCR runs, then the translated image with
    // word selection. Stays transparent (pass-through) until there's something to show.
    resultHost =
      WindowComposeHost(context).apply {
        view.visibility = View.GONE
        setContent {
          SelectionSurface(surfaceState)
        }
      }
    rootView.addView(
      resultHost!!.view,
      FrameLayout.LayoutParams(
        FrameLayout.LayoutParams.MATCH_PARENT,
        FrameLayout.LayoutParams.MATCH_PARENT,
      ),
    )

    topBarView = buildTopBar()
    rootView.addView(
      topBarView,
      FrameLayout
        .LayoutParams(
          FrameLayout.LayoutParams.MATCH_PARENT,
          FrameLayout.LayoutParams.WRAP_CONTENT,
        ).apply {
          gravity = Gravity.TOP or Gravity.START
          topMargin = contentInsets.top
          leftMargin = contentInsets.left
          rightMargin = contentInsets.right
        },
    )

    statusView =
      TextView(context).apply {
        setTextColor(Color.WHITE)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        gravity = Gravity.CENTER
        setPadding(dpToPx(20), dpToPx(12), dpToPx(20), dpToPx(12))
        background =
          GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = dpToPx(12).toFloat()
            setColor(Color.parseColor("#CC202020"))
          }
      }
    val statusParams =
      FrameLayout
        .LayoutParams(
          FrameLayout.LayoutParams.WRAP_CONTENT,
          FrameLayout.LayoutParams.WRAP_CONTENT,
        ).apply {
          gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
          bottomMargin = contentInsets.bottom + dpToPx(24)
        }
    rootView.addView(statusView, statusParams)

    resultHost!!.installOn(rootView)

    showStatus(context.getString(R.string.assistant_invoke_prompt))
    updateBackdrop()
    return rootView
  }

  override fun onShow(
    args: Bundle?,
    showFlags: Int,
  ) {
    super.onShow(args, showFlags)
    configureSessionWindow()
    clearCapture()
    overlayContainer.removeAllViews()
    dismissMenu()
    updateBackdrop()

    if (settingsManager.settings.value.assistantAction == dev.davidv.translator.AssistantAction.LIVE_SCREEN) {
      launchLiveScreen()
      return
    }

    showStatus(context.getString(R.string.assistant_collecting_context))

    if (!isAssistScreenshotEnabled()) {
      Log.w(tag, "Assistant screenshot capture is disabled in system settings")
      showStatus(context.getString(R.string.assistant_screenshot_disabled))
      return
    }

    captureTimeoutJob =
      sessionScope.launch {
        delay(CAPTURE_TIMEOUT_MS)
        if (screenshotBitmap == null && !processing) {
          showStatus(context.getString(R.string.assistant_no_screen_data))
        }
      }
  }

  override fun onComputeInsets(outInsets: Insets) {
    outInsets.contentInsets.set(0, 0, 0, 0)
    outInsets.touchableInsets = Insets.TOUCHABLE_INSETS_FRAME
  }

  override fun onHide() {
    super.onHide()
    clearCapture()
    clearResultOverlay()
    overlayContainer.removeAllViews()
    dismissMenu()
    screenshotView.setImageDrawable(null)
    updateBackdrop()
  }

  override fun onDestroy() {
    sessionScope.cancel()
    clearCapture()
    clearResultOverlay()
    resultHost?.dispose()
    resultHost = null
    super.onDestroy()
  }

  /** Drop the interactive image layer and recycle the flip's original-image copy. */
  private fun clearResultOverlay() {
    surfaceState.original.value?.recycle()
    surfaceState.clear()
    setFlipButtonVisible(false)
    flipIconView?.setColorFilter(Color.WHITE)
    resultHost?.view?.visibility = View.GONE
  }

  override fun onHandleScreenshot(screenshot: Bitmap?) {
    super.onHandleScreenshot(screenshot)
    // In live mode onShow already handed off to the projection flow; the system
    // still delivers the screenshot (we asked for it), but we must not run the
    // still OCR or it would override the live action.
    if (settingsManager.settings.value.assistantAction == dev.davidv.translator.AssistantAction.LIVE_SCREEN) return
    captureTimeoutJob?.cancel()
    captureTimeoutJob = null
    Log.d(tag, "Screenshot callback received bitmap=${screenshot?.width}x${screenshot?.height}")
    if (screenshot == null) {
      showStatus(context.getString(R.string.assistant_no_screenshot))
      return
    }
    // The capture can beat the window's first traversal, and until that has happened the insets
    // this crop needs have not been delivered. Park the frame and let layout drive it instead.
    pendingScreenshot?.recycle()
    pendingScreenshot = screenshot.copy(Bitmap.Config.ARGB_8888, false)
    consumePendingScreenshot()
  }

  private fun consumePendingScreenshot() {
    val full = pendingScreenshot ?: return
    if (rootView.width == 0 || rootView.height == 0) return
    pendingScreenshot = null

    val crop = contentCrop(full.width, full.height)
    val cropped =
      if (crop.left == 0 && crop.top == 0 && crop.width() == full.width && crop.height() == full.height) {
        full
      } else {
        Bitmap.createBitmap(full, crop.left, crop.top, crop.width(), crop.height()).also { full.recycle() }
      }

    val oldSs = screenshotBitmap
    val oldCr = croppedBitmap
    oldSs?.recycle()
    if (oldCr != null && oldCr !== oldSs) oldCr.recycle()
    // `screenshotBitmap` is the cropped, lockable source of truth (reused by retranslate);
    // `croppedBitmap` is the currently-displayed bitmap (this source during the scan, then the
    // translated result), so they start as the same object.
    screenshotBitmap = cropped
    croppedBitmap = cropped
    screenshotView.setImageBitmap(croppedBitmap)
    updateBackdrop()
    runFullScreenOcr()
  }

  private fun runFullScreenOcr() {
    val screenshot = screenshotBitmap ?: return
    overlayContainer.removeAllViews()
    hideStatus()
    val sourceLanguage = ocrSourceLanguage()
    if (!isAutoSource && sourceLanguage == null) {
      processing = false
      showStatus(context.getString(R.string.assistant_set_source_ocr))
      return
    }

    val targetLanguage = forcedTargetLanguage ?: langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode)
    if (targetLanguage == null) {
      processing = false
      showStatus(context.getString(R.string.assistant_langs_not_ready))
      return
    }
    val ocrSourceLanguage = sourceLanguage ?: targetLanguage
    // `screenshot` is already the cropped software source; copy it so the A/B original survives
    // independently of `croppedBitmap` (which gets reassigned to the translated result).
    val workingBitmap = screenshot.copy(Bitmap.Config.ARGB_8888, false)

    // Show the screenshot with the scan animation while OCR/translation run; the detection callback
    // fills in the regions the scan sweeps over, then the result swaps in the translated image with
    // the word-selection overlay. `workingBitmap` doubles as the flip's original and is not recycled
    // by the pipeline, so it stays alive until the next clear.
    clearResultOverlay()
    processing = true
    surfaceState.showProcessing(workingBitmap)
    resultHost?.view?.visibility = View.VISIBLE

    translationJob =
      sessionScope.launch {
        var ocrUnavailable = false
        val result =
          withContext(Dispatchers.IO) {
            translationCoordinator.translateImageWithOverlay(
              ocrSourceLanguage,
              targetLanguage,
              workingBitmap,
              onMessage = {},
              readingOrder = currentReadingOrderFor(sourceLanguage),
              isAutoSource = isAutoSource,
              onOcrUnavailable = { ocrUnavailable = true },
              onDetectedRegions = { boxes, w, h ->
                surfaceState.regions.value = DetectedRegions(w, h, boxes)
              },
            )
          }
        ensureActive()
        processing = false
        if (result == null) {
          val statusRes = if (ocrUnavailable) R.string.ocr_models_missing else R.string.assistant_ocr_failed
          showStatus(context.getString(statusRes))
          clearResultOverlay()
          return@launch
        }
        val display = result.correctedBitmap
        val oldCropped = croppedBitmap
        if (oldCropped != null && oldCropped !== screenshotBitmap) oldCropped.recycle()
        croppedBitmap = display
        screenshotView.setImageBitmap(croppedBitmap)
        val selection =
          ImageWordSelection(
            imageWidth = result.metadata.width.toInt(),
            imageHeight = result.metadata.height.toInt(),
            sourceWords = result.metadata.sourceWords,
            translatedWords = result.translatedWords,
          )
        surfaceState.showResult(display, workingBitmap, selection)
        setFlipButtonVisible(true)
        resultHost?.view?.visibility = View.VISIBLE
        updateBackdrop()
        hideStatus()
      }
  }

  private fun setFlipButtonVisible(visible: Boolean) {
    flipButtonView?.visibility = if (visible) View.VISIBLE else View.GONE
  }

  private fun toggleAssistantOriginal() {
    if (surfaceState.display.value == null) return
    val show = surfaceState.toggleShowOriginal()
    flipIconView?.setColorFilter(if (show) OverlayChromeFactory.ACTIVE_ICON_TINT else Color.WHITE)
  }

  @Suppress("DEPRECATION")
  private fun configureSessionWindow() {
    val dialog = window ?: return
    val win = dialog.window ?: return
    win.setLayout(
      WindowManager.LayoutParams.MATCH_PARENT,
      WindowManager.LayoutParams.MATCH_PARENT,
    )
    win.setGravity(Gravity.TOP or Gravity.START)
    win.attributes = win.attributes.apply { spanFullDisplay() }
    win.setBackgroundDrawable(null)
    win.decorView.setBackgroundColor(Color.TRANSPARENT)
    win.decorView.setPadding(0, 0, 0, 0)
    win.setWindowAnimations(0)
    val contentFrame = win.decorView.findViewById<View>(android.R.id.content)
    contentFrame?.setBackgroundColor(Color.TRANSPARENT)
    (contentFrame?.parent as? View)?.setBackgroundColor(Color.TRANSPARENT)
  }

  private fun showStatus(
    message: String,
    autoHideAfterMs: Long? = null,
  ) {
    if (!::statusView.isInitialized) return
    statusHideJob?.cancel()
    statusHideJob = null
    statusView.text = message
    statusView.visibility = View.VISIBLE
    if (autoHideAfterMs != null) {
      statusHideJob =
        sessionScope.launch {
          delay(autoHideAfterMs)
          hideStatus()
        }
    }
  }

  private fun hideStatus() {
    if (!::statusView.isInitialized) return
    statusHideJob?.cancel()
    statusHideJob = null
    statusView.visibility = View.GONE
  }

  // Screenshot pixels and window coordinates are the same space once the session window spans the
  // physical display, so one rect drives both the crop and where the layers that show it sit.
  private fun contentCrop(
    width: Int,
    height: Int,
  ): Rect = OverlayInsets.contentBounds(Rect(0, 0, width, height), contentInsets)

  private fun applyContentGeometry() {
    if (rootView.width == 0 || rootView.height == 0) return
    val crop = contentCrop(rootView.width, rootView.height)
    var changed = false
    for (layer in listOfNotNull(screenshotView, overlayContainer, resultHost?.view)) {
      val lp = layer.layoutParams as FrameLayout.LayoutParams
      if (lp.width == crop.width() && lp.height == crop.height() &&
        lp.leftMargin == crop.left && lp.topMargin == crop.top
      ) {
        continue
      }
      lp.gravity = Gravity.TOP or Gravity.START
      lp.width = crop.width()
      lp.height = crop.height()
      lp.leftMargin = crop.left
      lp.topMargin = crop.top
      changed = true
    }
    val bar = topBarView.layoutParams as FrameLayout.LayoutParams
    if (bar.topMargin != crop.top || bar.leftMargin != crop.left || bar.rightMargin != contentInsets.right) {
      bar.topMargin = crop.top
      bar.leftMargin = crop.left
      bar.rightMargin = contentInsets.right
      changed = true
    }
    val status = statusView.layoutParams as FrameLayout.LayoutParams
    val statusBottom = contentInsets.bottom + dpToPx(24)
    if (status.bottomMargin != statusBottom) {
      status.bottomMargin = statusBottom
      changed = true
    }
    if (changed) rootView.requestLayout()
  }

  private fun clearCapture() {
    translationJob?.cancel()
    translationJob = null
    captureTimeoutJob?.cancel()
    captureTimeoutJob = null
    statusHideJob?.cancel()
    statusHideJob = null
    pendingScreenshot?.recycle()
    pendingScreenshot = null
    val ss = screenshotBitmap
    val cr = croppedBitmap
    ss?.recycle()
    if (cr != null && cr !== ss) cr.recycle()
    screenshotBitmap = null
    croppedBitmap = null
    processing = false
  }

  private fun dpToPx(dp: Int): Int = (dp * context.resources.displayMetrics.density).toInt()

  private fun updateBackdrop() {
    rootView.setBackgroundColor(Color.TRANSPARENT)
    screenshotView.setBackgroundColor(Color.TRANSPARENT)
  }

  private fun isAssistScreenshotEnabled(): Boolean =
    Settings.Secure.getInt(context.contentResolver, ASSIST_SCREENSHOT_ENABLED_SETTING, 1) != 0

  private fun ocrSourceLanguage(): Language? =
    forcedSourceLanguage ?: settingsManager.settings.value.defaultSourceLanguageCode?.let {
      langStateManager.languageByCode(it)
    }

  private fun buildTopBar(): View {
    val toolbarViews =
      OverlayChromeFactory.createLanguageToolbar(
        context = context,
        dpToPx = ::dpToPx,
        forcedSourceLanguage = forcedSourceLanguage,
        forcedTargetLanguage = forcedTargetLanguage,
        defaultTargetLanguage = langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode) ?: langStateManager.languageByCode("en")!!,
        onClose = { hide() },
        onSourceClick = { showLanguagePicker(true) },
        onSwap = { swapLanguages() },
        onTargetClick = { showLanguagePicker(false) },
        onFlipOriginal = { toggleAssistantOriginal() },
        showReadingOrderButton = forcedSourceLanguage?.code == "ja",
        readingOrder = currentReadingOrderFor(forcedSourceLanguage),
        onReadingOrderClick = { toggleJapaneseOcrMode() },
        onMenuClick = { showDotsMenu() },
        isAutoSource = isAutoSource,
      )
    sourceLabelView = toolbarViews.sourceLabel
    targetLabelView = toolbarViews.targetLabel
    readingOrderButtonView = toolbarViews.readingOrderButton
    readingOrderIconView = toolbarViews.readingOrderIcon
    flipIconView = toolbarViews.flipIcon
    flipButtonView = toolbarViews.flipButton
    flipButtonView?.visibility = View.GONE
    return toolbarViews.root
  }

  /** Live screen translate set as the assistant-invocation action: skip the still
   *  overlay entirely, launch the consent flow with the default languages from
   *  settings, and dismiss this session. Needs a fixed default source language
   *  (live can't run the per-frame script classifier). The language *codes* are
   *  passed straight through — [ScreenTranslateService] resolves them against the
   *  app catalog, which is always loaded, unlike this session's lazy catalog. */
  private fun launchLiveScreen() {
    val settings = settingsManager.settings.value
    val sourceCode = settings.defaultSourceLanguageCode
    if (sourceCode == null) {
      showStatus(context.getString(R.string.assistant_set_source_live))
      return
    }
    runCatching {
      context.startActivity(
        ScreenCaptureRequestActivity.intent(context, sourceCode, settings.defaultTargetLanguageCode, isAutoSource = false),
      )
    }.onFailure { Log.w(tag, "failed to launch live screen translate", it) }
    hide()
  }

  private fun swapLanguages() {
    val oldSource = forcedSourceLanguage
    val oldTarget = forcedTargetLanguage ?: langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode) ?: return
    isAutoSource = false
    forcedSourceLanguage = oldTarget
    forcedTargetLanguage = oldSource
    syncReadingOrderForSource()
    updateToolbarLabels()
    retranslate()
  }

  private fun retranslate() {
    translationJob?.cancel()
    translationJob = null
    processing = false
    if (screenshotBitmap != null) {
      runFullScreenOcr()
    } else {
      overlayContainer.removeAllViews()
      hideStatus()
    }
  }

  private fun updateToolbarLabels() {
    sourceLabelView?.text = OverlayChromeFactory.formatSourceLabel(forcedSourceLanguage, isAutoSource)
    val currentTarget = forcedTargetLanguage ?: langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode)
    targetLabelView?.text = currentTarget?.shortDisplayName ?: "?"
    OverlayChromeFactory.updateReadingOrderButtonState(
      readingButton = readingOrderButtonView,
      readingIcon = readingOrderIconView,
      visible = forcedSourceLanguage?.code == "ja",
      readingOrder = currentReadingOrderFor(forcedSourceLanguage),
    )
  }

  private fun showLanguagePicker(isSource: Boolean) {
    sessionScope.launch {
      langStateManager.refreshLanguageAvailability()
      val availableLangs = overlayTextTranslationHelper.awaitAvailableLanguages(isSource)
      menuManager?.showLanguagePicker(
        isSource = isSource,
        availableLangs = availableLangs,
      ) { language ->
        if (isSource) {
          if (language == null) {
            isAutoSource = true
          } else {
            isAutoSource = false
            forcedSourceLanguage = language
          }
          syncReadingOrderForSource()
        } else {
          forcedTargetLanguage = language
        }
        updateToolbarLabels()
        retranslate()
      }
    }
  }

  private fun toggleJapaneseOcrMode() {
    if (forcedSourceLanguage?.code != "ja") return
    ocrReadingOrder =
      when (ocrReadingOrder) {
        null -> ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT
        ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT -> ReadingOrder.LEFT_TO_RIGHT
        ReadingOrder.LEFT_TO_RIGHT -> null
      }
    updateToolbarLabels()
    retranslate()
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

  private fun showDotsMenu() {
    menuManager?.showDotsMenu(
      // Copy is handled by the selection action bar; the flip button swaps original/translated.
      listOf(
        "Open App" to { openMainApp() },
      ),
    )
  }

  private fun openMainApp() {
    startAssistantActivity(Intent(context, MainActivity::class.java))
    hide()
  }

  private fun dismissMenu() {
    menuManager?.dismiss()
  }
}
