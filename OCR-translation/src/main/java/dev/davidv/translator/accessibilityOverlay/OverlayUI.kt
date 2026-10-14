package dev.davidv.translator.accessibilityOverlay

import android.app.Dialog
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.ImageView
import android.widget.TextView
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import dev.davidv.translator.Language
import dev.davidv.translator.MainActivity
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.SettingsManager
import dev.davidv.translator.assistantOverlay.BorderWaveView
import dev.davidv.translator.overlayChrome.FloatingBubble
import dev.davidv.translator.overlayChrome.OverlayChromeFactory
import dev.davidv.translator.overlayChrome.OverlayInsets
import dev.davidv.translator.overlayChrome.OverlayMenuHost
import dev.davidv.translator.overlayChrome.OverlayMenuManager
import dev.davidv.translator.overlayChrome.spanFullDisplay
import dev.davidv.translator.ui.components.ScanAnimationOverlay
import dev.davidv.translator.ui.components.SelectionSurface
import dev.davidv.translator.ui.components.SelectionSurfaceState
import dev.davidv.translator.ui.components.WindowComposeHost

class OverlayUI(
  private val service: TranslatorOverlayHost,
  private val serviceContext: Context,
  private val windowManager: WindowManager,
  private val settingsManager: SettingsManager,
  private val windowTypeOverride: Int? = null,
  private val container: android.view.ViewGroup? = null,
) {
  private fun windowType(): Int =
    windowTypeOverride ?: WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY

  /** Adds a chrome view: to the in-activity container when hosted by our overlay activity
   *  (which is already a full-screen window, so no separate overlay window is needed), or
   *  to the WindowManager when running in the accessibility service. */
  private fun addChromeView(view: View, lp: android.view.ViewGroup.LayoutParams) {
    if (container != null) {
      container.addView(view, lp)
    } else {
      windowManager.addView(view, lp as WindowManager.LayoutParams)
    }
  }

  private fun removeChromeView(view: View) {
    if (container != null) {
      container.removeView(view)
    } else {
      windowManager.removeView(view)
    }
  }
  private val handler = Handler(Looper.getMainLooper())
  private var launcherBubble: FloatingBubble? = null
  private var toolbarView: View? = null
  private var toolbarParams: WindowManager.LayoutParams? = null
  private var sourceLabelView: TextView? = null
  private var targetLabelView: TextView? = null
  private var readingOrderButtonView: View? = null
  private var readingOrderIconView: ImageView? = null
  private var selectIconView: ImageView? = null
  private var refreshButtonView: View? = null
  private var flipButtonView: View? = null
  private var flipIconView: ImageView? = null
  private val translationOverlays = mutableListOf<View>()
  private var touchWatcher: View? = null
  private var borderView: BorderWaveView? = null
  private var selectDialog: Dialog? = null
  private var selectHost: WindowComposeHost? = null
  private var selectSurfaceView: View? = null
  private var scanHost: WindowComposeHost? = null
  private var messageView: TextView? = null
  private val menuChildren = mutableListOf<View>()

  private val menuManager =
    OverlayMenuManager(
      serviceContext,
      ::dpToPx,
      object : OverlayMenuHost {
        override fun addDismissLayer(view: View) {
          val lp =
            if (container != null) {
              android.widget.FrameLayout.LayoutParams(
                android.view.ViewGroup.LayoutParams.MATCH_PARENT,
                android.view.ViewGroup.LayoutParams.MATCH_PARENT,
              )
            } else {
              WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.MATCH_PARENT,
                windowType(),
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
                PixelFormat.TRANSLUCENT,
              ).apply { windowAnimations = 0 }
            }
          addChromeView(view, lp)
          menuChildren.add(view)
        }

        override fun addMenuView(view: View) {
          val lp =
            if (container != null) {
              android.widget.FrameLayout.LayoutParams(dpToPx(180), android.view.ViewGroup.LayoutParams.WRAP_CONTENT).apply {
                gravity = Gravity.TOP or Gravity.END
                topMargin = dpToPx(48)
                marginEnd = dpToPx(8)
              }
            } else {
              WindowManager.LayoutParams(
                dpToPx(180),
                WindowManager.LayoutParams.WRAP_CONTENT,
                windowType(),
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
                PixelFormat.TRANSLUCENT,
              ).apply {
                gravity = Gravity.TOP or Gravity.END
                x = dpToPx(8)
                y = dpToPx(48)
                windowAnimations = 0
              }
            }
          addChromeView(view, lp)
          menuChildren.add(view)
        }

        override fun addPickerView(view: View) {
          val lp =
            if (container != null) {
              android.widget.FrameLayout.LayoutParams(dpToPx(250), dpToPx(400)).apply {
                gravity = Gravity.CENTER
              }
            } else {
              WindowManager.LayoutParams(
                dpToPx(250),
                dpToPx(400),
                windowType(),
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE,
                PixelFormat.TRANSLUCENT,
              ).apply {
                gravity = Gravity.CENTER
                windowAnimations = 0
              }
            }
          addChromeView(view, lp)
          menuChildren.add(view)
        }

        override fun removeMenuChild(view: View) {
          menuChildren.remove(view)
          removeChromeView(view)
        }
      },
    )

  fun showFloatingButton() {
    val bubble = launcherBubble
    if (bubble == null) {
      launcherBubble =
        FloatingBubble(serviceContext, windowManager, windowType(), ::dpToPx) {
          service.activate()
        }.also { it.show() }
    } else {
      bubble.setShown(true)
    }
  }

  fun removeFloatingButton() {
    launcherBubble?.setShown(false)
  }

  fun restoreFloatingButton() {
    showFloatingButton()
  }

  fun showToolbar(
    forcedSourceLanguage: Language?,
    forcedTargetLanguage: Language?,
    readingOrder: ReadingOrder?,
    isAutoSource: Boolean,
  ) {
    if (toolbarView != null) return

    val toolbarViews =
      OverlayChromeFactory.createLanguageToolbar(
          context = serviceContext,
        dpToPx = ::dpToPx,
        forcedSourceLanguage = forcedSourceLanguage,
        forcedTargetLanguage = forcedTargetLanguage,
        defaultTargetLanguage = service.langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode) ?: return,
        onClose = { service.deactivate() },
        onSelectModeClick = { service.toggleSelectMode() },
        onSourceClick = { service.showLanguagePicker(true) },
        onSwap = { service.swapLanguages() },
        onTargetClick = { service.showLanguagePicker(false) },
        showReadingOrderButton = forcedSourceLanguage?.code == "ja",
        readingOrder = readingOrder,
        onReadingOrderClick = { service.toggleJapaneseOcrMode() },
        onRefreshClick = { service.handleFullScreenOcr() },
        onFlipOriginal = { service.toggleFlipOriginal() },
        onMenuClick = { service.showDotsMenu() },
        isAutoSource = isAutoSource,
      )
    val toolbar = toolbarViews.root
    sourceLabelView = toolbarViews.sourceLabel
    targetLabelView = toolbarViews.targetLabel
    readingOrderButtonView = toolbarViews.readingOrderButton
    readingOrderIconView = toolbarViews.readingOrderIcon
    selectIconView = toolbarViews.selectIcon
    refreshButtonView = toolbarViews.refreshButton
    flipButtonView = toolbarViews.flipButton
    flipIconView = toolbarViews.flipIcon
    flipButtonView?.visibility = View.GONE

    val lp: android.view.ViewGroup.LayoutParams =
      if (container != null) {
        android.widget.FrameLayout.LayoutParams(
          android.view.ViewGroup.LayoutParams.MATCH_PARENT,
          android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply { topMargin = getStatusBarHeight() }
      } else {
        WindowManager.LayoutParams(
          WindowManager.LayoutParams.MATCH_PARENT,
          WindowManager.LayoutParams.WRAP_CONTENT,
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
          PixelFormat.TRANSLUCENT,
        ).apply {
          gravity = Gravity.TOP or Gravity.START
          x = 0
          y = getStatusBarHeight()
          windowAnimations = 0
        }
      }

    addChromeView(toolbar, lp)
    toolbarView = toolbar
    toolbarParams = lp as? WindowManager.LayoutParams
  }

  fun removeToolbar() {
    toolbarView?.let {
      removeChromeView(it)
      toolbarView = null
      toolbarParams = null
      sourceLabelView = null
      targetLabelView = null
      readingOrderButtonView = null
      readingOrderIconView = null
      selectIconView = null
      refreshButtonView = null
      flipButtonView = null
      flipIconView = null
    }
  }

  /** Swap the mode-dependent toolbar slot: refresh drives the live screen, flip the frozen one. */
  fun setSelectModeUi(active: Boolean) {
    selectIconView?.setColorFilter(if (active) OverlayChromeFactory.ACTIVE_ICON_TINT else Color.WHITE)
    refreshButtonView?.visibility = if (active) View.GONE else View.VISIBLE
    flipButtonView?.visibility = if (active) View.VISIBLE else View.GONE
    if (!active) flipIconView?.setColorFilter(Color.WHITE)
  }

  fun setFlipActive(active: Boolean) {
    flipIconView?.setColorFilter(if (active) OverlayChromeFactory.ACTIVE_ICON_TINT else Color.WHITE)
  }

  fun updateToolbarState(
    forcedSourceLanguage: Language?,
    forcedTargetLanguage: Language?,
    readingOrder: ReadingOrder?,
    isAutoSource: Boolean,
  ) {
    sourceLabelView?.text = OverlayChromeFactory.formatSourceLabel(forcedSourceLanguage, isAutoSource)
    val currentTarget = forcedTargetLanguage ?: service.langStateManager.languageByCode(settingsManager.settings.value.defaultTargetLanguageCode)
    targetLabelView?.text = currentTarget?.shortDisplayName ?: "?"
    OverlayChromeFactory.updateReadingOrderButtonState(
      readingButton = readingOrderButtonView,
      readingIcon = readingOrderIconView,
      visible = forcedSourceLanguage?.code == "ja",
      readingOrder = readingOrder,
    )
  }

  fun showDotsMenu() {
    menuManager.showDotsMenu(
      listOf(
        "Open App" to {
          service.deactivate()
          val intent = Intent(serviceContext, MainActivity::class.java)
          intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
          serviceContext.startActivity(intent)
        },
        "Disable Service" to {
          service.deactivate()
          service.disableService()
        },
      ),
    )
  }

  fun showLanguagePicker(
    isSource: Boolean,
    availableLangs: List<Language>,
    onPick: (Language?) -> Unit,
  ) {
    menuManager.showLanguagePicker(isSource, availableLangs) { lang ->
      onPick(lang)
    }
  }

  fun dismissMenu() {
    menuManager.dismiss()
  }

  fun showBitmapOverlay(
    bitmap: Bitmap,
    bounds: Rect,
  ) {
    val imageView = ImageView(serviceContext)
    imageView.setImageBitmap(bitmap)
    imageView.scaleType = ImageView.ScaleType.FIT_XY

    // Non-touchable so taps and swipes pass straight through to the app underneath;
    // the overlay clears itself off the next scroll/click via onAccessibilityEvent.
    val lp: android.view.ViewGroup.LayoutParams =
      if (container != null) {
        android.widget.FrameLayout.LayoutParams(bounds.width(), bounds.height()).apply {
          leftMargin = bounds.left
          topMargin = bounds.top
        }
      } else {
        WindowManager.LayoutParams(
          bounds.width(),
          bounds.height(),
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
          PixelFormat.TRANSLUCENT,
        ).apply {
          gravity = Gravity.TOP or Gravity.START
          x = bounds.left
          y = bounds.top
          spanFullDisplay()
        }
      }

    if (container != null) {
      // Activity-hosted (assistant mode): the translated result is interactive — tap or
      // long-press switches to the word-selection surface (tap a word → floating
      // Copy/Share/Search bar), matching the Circle to Search text-selection UX. The
      // window path keeps its non-touchable pass-through (taps land on the app below
      // and clear the overlay).
      imageView.setOnClickListener { service.toggleSelectMode() }
      imageView.setOnLongClickListener {
        service.toggleSelectMode()
        true
      }
    }

    addChromeView(imageView, lp)
    translationOverlays.add(imageView)
    raiseToolbar()
    ensureTouchWatcher()
  }

  /**
   * The frozen select-text surface: one touchable window over the translated region hosting the
   * same Compose selection stack as the assistant. A Dialog rather than a raw WindowManager view
   * because the word-selection action bar needs `startActionMode(TYPE_FLOATING)`, which only a
   * DecorView provides; NOT_TOUCH_MODAL keeps the toolbar's own window clickable, and the back key
   * cancels the dialog (the service then exits select mode via `onDismiss`).
   */
  fun showSelectSurface(
    state: SelectionSurfaceState,
    region: Rect,
    onDismiss: () -> Unit,
  ) {
    if (container != null) {
      // In-activity hosting: the selection surface is a child view, not a dialog window
      // (no SYSTEM_ALERT_WINDOW / a11y overlay window type is available to us).
      val host = WindowComposeHost(serviceContext)
      host.setContent { SelectionSurface(state) }
      val lp = android.widget.FrameLayout.LayoutParams(region.width(), region.height()).apply {
        leftMargin = region.left
        topMargin = region.top
      }
      container.addView(host.view, lp)
      selectHost = host
      selectSurfaceView = host.view
      host.view.setOnClickListener { onDismiss() }
    } else {
      val dialog = Dialog(serviceContext, android.R.style.Theme_Material_NoActionBar)
      val host = WindowComposeHost(serviceContext)
      host.setContent { SelectionSurface(state) }
      dialog.setContentView(host.view)
      dialog.setCanceledOnTouchOutside(false)
      dialog.window?.apply {
        setType(windowType())
        addFlags(
          WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
        )
        clearFlags(WindowManager.LayoutParams.FLAG_DIM_BEHIND)
        setWindowAnimations(0)
        setGravity(Gravity.TOP or Gravity.START)
        attributes =
          attributes.apply {
            x = region.left
            y = region.top
            width = region.width()
            height = region.height()
            spanFullDisplay()
          }
        decorView.setPadding(0, 0, 0, 0)
        host.installOn(decorView)
      }
      dialog.setOnCancelListener { onDismiss() }
      dialog.show()
      selectDialog = dialog
      selectHost = host
    }
    raiseToolbar()
  }

  fun removeSelectSurface() {
    selectSurfaceView?.let {
      container?.removeView(it)
      selectSurfaceView = null
    }
    selectHost?.let {
      if (selectSurfaceView == null) it.dispose()
    }
    selectHost = null
    val dialog = selectDialog ?: return
    selectDialog = null
    dialog.setOnCancelListener(null)
    dialog.dismiss()
  }

  fun hasSelectSurface(): Boolean = selectDialog != null || selectSurfaceView != null

  /** A 1×1 watcher window: WATCH_OUTSIDE_TOUCH fires ACTION_OUTSIDE on the *down* of
   *  any touch anywhere on screen, and NOT_TOUCH_MODAL lets that same touch reach the
   *  app — so a tap or swipe both clear the result instantly and pass straight through. */
  @android.annotation.SuppressLint("ClickableViewAccessibility")
  private fun ensureTouchWatcher() {
    if (touchWatcher != null) return
    val watcher = View(serviceContext)
    watcher.setOnTouchListener { _, event ->
      // The system zeroes ACTION_OUTSIDE coordinates unless the touched window belongs to our
      // own UID: real coordinates mean the touch landed on our chrome (toolbar, menus, their
      // dismiss layer), which can't change the screen underneath — keep the overlays up.
      if (event.action == MotionEvent.ACTION_OUTSIDE && event.rawX == 0f && event.rawY == 0f) {
        removeTranslationOverlays()
      }
      false
    }
    val lp: android.view.ViewGroup.LayoutParams =
      if (container != null) {
        android.widget.FrameLayout.LayoutParams(1, 1)
      } else {
        WindowManager.LayoutParams(
          1,
          1,
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
            WindowManager.LayoutParams.FLAG_WATCH_OUTSIDE_TOUCH or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
          PixelFormat.TRANSLUCENT,
        ).apply {
          gravity = Gravity.TOP or Gravity.START
          windowAnimations = 0
        }
      }
    addChromeView(watcher, lp)
    touchWatcher = watcher
  }

  private fun removeTouchWatcher() {
    touchWatcher?.let {
      try {
        if (container != null) container.removeView(it) else windowManager.removeView(it)
      } catch (_: Exception) {
      }
    }
    touchWatcher = null
  }

  /** The detect-pass progress animation, drawn over the live screen itself: a transparent
   *  non-touchable window whose scan pills appear once the detector reports regions. */
  fun showScanOverlay(
    state: SelectionSurfaceState,
    region: Rect,
  ) {
    if (scanHost != null) return
    val host = WindowComposeHost(serviceContext)
    host.setContent {
      state.regions.value?.let {
        ScanAnimationOverlay(regions = it, modifier = Modifier.fillMaxSize())
      }
    }
    if (container != null) {
      val lp = android.widget.FrameLayout.LayoutParams(region.width(), region.height()).apply {
        leftMargin = region.left
        topMargin = region.top
      }
      container.addView(host.view, lp)
    } else {
      val params =
        WindowManager.LayoutParams(
          region.width(),
          region.height(),
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
          PixelFormat.TRANSLUCENT,
        )
      params.gravity = Gravity.TOP or Gravity.START
      params.x = region.left
      params.y = region.top
      params.spanFullDisplay()
      params.windowAnimations = 0
      windowManager.addView(host.view, params)
    }
    scanHost = host
  }

  private fun removeScanOverlay() {
    scanHost?.let {
      try {
        if (container != null) container.removeView(it.view) else windowManager.removeView(it.view)
      } catch (_: Exception) {
      }
      it.dispose()
    }
    scanHost = null
  }

  fun showOverlayMessage(message: String) {
    val textView = TextView(serviceContext)
    textView.text = message
    textView.setTextColor(Color.WHITE)
    textView.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
    val pad = dpToPx(16)
    textView.setPadding(pad, pad, pad, pad)
    val bg = GradientDrawable()
    bg.setColor(Color.parseColor("#DD333333"))
    bg.cornerRadius = dpToPx(8).toFloat()
    textView.background = bg

    if (container != null) {
      val lp = android.widget.FrameLayout.LayoutParams(
        android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
        android.view.ViewGroup.LayoutParams.WRAP_CONTENT,
      ).apply {
        gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
        bottomMargin = dpToPx(80)
      }
      container.addView(textView, lp)
    } else {
      val params =
        WindowManager.LayoutParams(
          WindowManager.LayoutParams.WRAP_CONTENT,
          WindowManager.LayoutParams.WRAP_CONTENT,
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE,
          PixelFormat.TRANSLUCENT,
        )
      params.gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
      params.y = dpToPx(80)
      windowManager.addView(textView, params)
    }
    messageView = textView
    handler.postDelayed({
      if (messageView === textView) messageView = null
      try {
        if (container != null) container.removeView(textView) else windowManager.removeView(textView)
      } catch (_: Exception) {
      }
    }, 3000)
  }

  /**
   * Sets the alpha of every window this UI owns ([1f] = normal). The system's
   * back-gesture arrow is drawn *below* accessibility overlays, so during a predictive
   * back gesture they must become transparent to keep the arrow visible.
   */
  fun setOverlayAlpha(alpha: Float) {
    toolbarView?.alpha = alpha
    borderView?.alpha = alpha
    touchWatcher?.alpha = alpha
    scanHost?.view?.alpha = alpha
    selectDialog?.window?.decorView?.alpha = alpha
    messageView?.alpha = alpha
    translationOverlays.forEach { it.alpha = alpha }
    menuChildren.forEach { it.alpha = alpha }
  }

  fun hasToolbar(): Boolean = toolbarView != null

  /** Re-inserts the toolbar so it stacks above windows added after it (result bitmap,
   *  select surface) — windows stack in insertion order. */
  private fun raiseToolbar() {
    val tv = toolbarView ?: return
    runCatching {
      if (container != null) {
        container.removeView(tv)
        container.addView(tv, tv.layoutParams)
      } else {
        // Container mode stores FrameLayout.LayoutParams, so toolbarParams is null there;
        // the window path needs the original WindowManager.LayoutParams to re-add.
        val tp = toolbarParams ?: return
        windowManager.removeView(tv)
        windowManager.addView(tv, tp)
      }
    }
  }

  fun removeTranslationOverlays() {
    for (view in translationOverlays) {
      try {
        if (container != null) container.removeView(view) else windowManager.removeView(view)
      } catch (_: Exception) {
      }
    }
    translationOverlays.clear()
    removeScanOverlay()
    removeTouchWatcher()
  }

  fun getStatusBarHeight(): Int = OverlayInsets.topInset(windowManager, serviceContext.resources)

  fun showBorderWave() {
    if (borderView != null) return
    val view = BorderWaveView.create(serviceContext)
    if (container != null) {
      container.addView(
        view,
        android.widget.FrameLayout.LayoutParams(
          android.view.ViewGroup.LayoutParams.MATCH_PARENT,
          android.view.ViewGroup.LayoutParams.MATCH_PARENT,
        ),
      )
    } else {
      val params =
        WindowManager.LayoutParams(
          WindowManager.LayoutParams.MATCH_PARENT,
          WindowManager.LayoutParams.MATCH_PARENT,
          windowType(),
          WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
            WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
          PixelFormat.TRANSLUCENT,
        )
      params.spanFullDisplay()
      windowManager.addView(view, params)
    }
    borderView = view
    view.startAnimation()
  }

  fun removeBorderWave() {
    borderView?.stopAnimation()
    borderView?.let {
      try {
        if (container != null) container.removeView(it) else windowManager.removeView(it)
      } catch (_: Exception) {
      }
    }
    borderView = null
  }

  fun cleanup() {
    handler.removeCallbacksAndMessages(null)
    launcherBubble?.remove()
    launcherBubble = null
  }

  fun dpToPx(dp: Int): Int = (dp * serviceContext.resources.displayMetrics.density).toInt()
}
