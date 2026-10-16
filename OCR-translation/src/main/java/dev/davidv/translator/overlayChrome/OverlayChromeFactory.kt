package dev.davidv.translator.overlayChrome

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.compose.ui.graphics.toArgb
import dev.davidv.translator.Language
import dev.davidv.translator.R
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.localizedName

data class LanguageToolbarViews(
  val root: View,
  val sourceLabel: TextView,
  val targetLabel: TextView,
  val readingOrderButton: View? = null,
  val readingOrderIcon: ImageView? = null,
  val pauseIcon: ImageView? = null,
  val flipIcon: ImageView? = null,
  val flipButton: View? = null,
  val refreshButton: View? = null,
  val selectIcon: ImageView? = null,
)

object OverlayChromeFactory {
  val ACTIVE_ICON_TINT: Int = Color.parseColor("#8AB4F8")

  /**
   * Exact CTS bottom-bar colors published by CircleToSearchScreen via SideEffect:
   * the outer card (`surfaceContainerHigh`) and the search pill (`surfaceContainer`).
   * When set, the toolbar uses them pixel-for-pixel instead of the locally derived scheme.
   */
  @Volatile
  var appBarContainerColor: Int? = null

  /** Button-pill fill (`surfaceContainerLowest`) — used for the toolbar's button pills. */
  @Volatile
  var appPillColor: Int? = null

  /**
   * Toolbar/chrome colors derived from the same Material3 color scheme the app's
   * Compose UI uses — including Material You dynamic colors on Android 12+ — so the
   * bar background matches the translate button's `surfaceContainer` exactly.
   */
  data class ChromePalette(
    val onSurface: Int,
    val barColor: Int,
    val barStroke: Int,
    val chipColor: Int,
    val gripColor: Int,
    val activeTint: Int,
    val menuBg: Int,
    val menuText: Int,
    val menuTextDisabled: Int,
  )

  fun isDark(context: Context): Boolean =
    (context.resources.configuration.uiMode and android.content.res.Configuration.UI_MODE_NIGHT_MASK) ==
      android.content.res.Configuration.UI_MODE_NIGHT_YES

  private fun opaque(color: Int): Int = color or 0xFF000000.toInt()

  private fun withAlpha(color: Int, alpha: Float): Int =
    (color and 0x00FFFFFF) or (((alpha * 255).toInt() and 0xFF) shl 24)

  fun palette(context: Context): ChromePalette {
    val dark = isDark(context)
    val scheme =
      if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {
        if (dark) androidx.compose.material3.dynamicDarkColorScheme(context)
        else androidx.compose.material3.dynamicLightColorScheme(context)
      } else {
        if (dark) androidx.compose.material3.darkColorScheme()
        else androidx.compose.material3.lightColorScheme()
      }
    val onSurface = scheme.onSurface.toArgb()
    return ChromePalette(
      onSurface = onSurface,
      // CTS bottom-bar outer card (surfaceContainerHigh) — prefer the exact value
      // published by the app UI so the bar matches pixel-for-pixel.
      barColor = opaque(appBarContainerColor ?: scheme.surfaceContainerHigh.toArgb()),
      barStroke = withAlpha(onSurface, if (dark) 0.18f else 0.15f),
      // Button-pill fill (surfaceContainerLowest) — same fill for every button pill.
      chipColor = opaque(appPillColor ?: scheme.surfaceContainerLowest.toArgb()),
      gripColor = withAlpha(onSurface, 0.5f),
      activeTint = scheme.primary.toArgb(),
      menuBg = opaque(scheme.surfaceContainerHighest.toArgb()),
      menuText = onSurface,
      menuTextDisabled = withAlpha(onSurface, 0.38f),
    )
  }

  fun activeIconTint(context: Context): Int = palette(context).activeTint

  fun formatSourceLabel(
    forcedSourceLanguage: Language?,
    isAutoSource: Boolean,
  ): String =
    when {
      isAutoSource && forcedSourceLanguage != null -> "✨ ${forcedSourceLanguage.shortDisplayName}"
      isAutoSource -> "✨ Auto"
      else -> forcedSourceLanguage?.shortDisplayName ?: "?"
    }

  fun createLanguageToolbar(
    context: Context,
    dpToPx: (Int) -> Int,
    forcedSourceLanguage: Language?,
    forcedTargetLanguage: Language?,
    defaultTargetLanguage: Language,
    onClose: () -> Unit,
    onSelectModeClick: (() -> Unit)? = null,
    onSourceClick: () -> Unit,
    onSwap: () -> Unit,
    onTargetClick: () -> Unit,
    showReadingOrderButton: Boolean = false,
    readingOrder: ReadingOrder? = null,
    onReadingOrderClick: (() -> Unit)? = null,
    onRefreshClick: (() -> Unit)? = null,
    onRegionClick: (() -> Unit)? = null,
    onFlipOriginal: (() -> Unit)? = null,
    onPauseClick: (() -> Unit)? = null,
    isPaused: Boolean = false,
    onMenuClick: (() -> Unit)? = null,
    isAutoSource: Boolean = false,
  ): LanguageToolbarViews {
    val btnSize = dpToPx(40)
    val iconPad = dpToPx(8)
    val p = palette(context)

    // M3 Expressive drag grip: a small tonal pill above the bar (bottom-sheet style).
    val grip =
      FrameLayout(context).apply {
        val gripBar =
          View(context).apply {
            background =
              GradientDrawable().apply {
                setColor(p.gripColor)
                cornerRadius = dpToPx(2).toFloat()
              }
          }
        addView(
          gripBar,
          FrameLayout.LayoutParams(dpToPx(28), dpToPx(4)).apply { gravity = Gravity.CENTER },
        )
      }

    // Single M3-style bar: large corner radius, tonal surface, thin stroke.
    val bar =
      LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        background =
          GradientDrawable().apply {
            setColor(p.barColor)
            cornerRadius = dpToPx(28).toFloat()
            setStroke(dpToPx(1), p.barStroke)
          }
        setPadding(dpToPx(8), dpToPx(6), dpToPx(8), dpToPx(6))
      }

    fun iconButton(icon: ImageView): FrameLayout {
      val fl = FrameLayout(context)
      // Tonal pill behind the icon (M3 filled-tonal feel) with a bounded ripple.
      val pill =
        GradientDrawable().apply {
          setColor(p.chipColor)
          cornerRadius = dpToPx(20).toFloat()
        }
      val out = TypedValue()
      if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, out, true) && out.resourceId != 0) {
        val layer =
          android.graphics.drawable.LayerDrawable(
            arrayOf(
              pill,
              context.getDrawable(out.resourceId) ?: android.graphics.drawable.ColorDrawable(Color.TRANSPARENT),
            ),
          )
        fl.background = layer
      } else {
        fl.background = pill
      }
      fl.addView(
        icon,
        FrameLayout.LayoutParams(btnSize, btnSize).apply { gravity = Gravity.CENTER },
      )
      return fl
    }

    fun iconLayoutParams(endMarginDp: Int = 2): LinearLayout.LayoutParams =
      LinearLayout.LayoutParams(btnSize, btnSize).apply { marginEnd = dpToPx(endMarginDp) }

    val closeBtn =
      ImageView(context).apply {
        setImageResource(R.drawable.cancel)
        setColorFilter(p.onSurface)
        setPadding(iconPad, iconPad, iconPad, iconPad)
        setOnClickListener { onClose() }
        contentDescription = context.getString(R.string.a11y_close)
      }
    bar.addView(iconButton(closeBtn), iconLayoutParams())

    val hasSelectMode = onSelectModeClick != null
    var selectIcon: ImageView? = null
    onSelectModeClick?.let {
      val selectBtn =
        ImageView(context).apply {
          setImageResource(R.drawable.text_select_start)
          setColorFilter(p.onSurface)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onSelectModeClick() }
          contentDescription = context.getString(R.string.a11y_select_text)
        }
      selectIcon = selectBtn
      bar.addView(iconButton(selectBtn), iconLayoutParams())
    }

    var readingOrderIcon: ImageView? = null
    val readingOrderPill =
      onReadingOrderClick?.let {
        val readingBtn =
          ImageView(context).apply {
            setPadding(iconPad, iconPad, iconPad, iconPad)
            setOnClickListener { onReadingOrderClick() }
          }
        readingOrderIcon = readingBtn
        updateReadingOrderButtonState(
          readingButton = null,
          readingIcon = readingBtn,
          visible = showReadingOrderButton,
          readingOrder = readingOrder,
        )
        iconButton(readingBtn).also { pill ->
          pill.visibility = if (showReadingOrderButton) View.VISIBLE else View.GONE
          bar.addView(pill, iconLayoutParams())
        }
      }

    val langRow = LinearLayout(context)
    langRow.orientation = LinearLayout.HORIZONTAL
    langRow.gravity = Gravity.CENTER_VERTICAL
    langRow.setPadding(dpToPx(10), dpToPx(4), dpToPx(10), dpToPx(4))

    val sourceLabel = TextView(context)
    sourceLabel.text = formatSourceLabel(forcedSourceLanguage, isAutoSource)
    sourceLabel.setTextColor(p.onSurface)
    sourceLabel.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
    sourceLabel.gravity = Gravity.CENTER
    sourceLabel.maxLines = 1
    sourceLabel.ellipsize = TextUtils.TruncateAt.END
    sourceLabel.setOnClickListener { onSourceClick() }
    langRow.addView(sourceLabel, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.MATCH_PARENT, 1f))

    val swapBtn = ImageView(context)
    swapBtn.setImageResource(R.drawable.compare)
    swapBtn.setColorFilter(p.onSurface)
    val swapSize = dpToPx(20)
    val swapPad = dpToPx(6)
    swapBtn.setPadding(swapPad, 0, swapPad, 0)
    swapBtn.setOnClickListener { onSwap() }
    langRow.addView(swapBtn, LinearLayout.LayoutParams(swapSize + swapPad * 2, swapSize))

    val currentTarget = forcedTargetLanguage ?: defaultTargetLanguage
    val targetLabel = TextView(context)
    targetLabel.text = currentTarget.shortDisplayName
    targetLabel.setTextColor(p.onSurface)
    targetLabel.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
    targetLabel.gravity = Gravity.CENTER
    targetLabel.maxLines = 1
    targetLabel.ellipsize = TextUtils.TruncateAt.END
    targetLabel.setOnClickListener { onTargetClick() }
    langRow.addView(targetLabel, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.MATCH_PARENT, 1f))

    // Tonal chip behind the language row (M3 secondary-container feel).
    val langChip =
      FrameLayout(context).apply {
        background =
          GradientDrawable().apply {
            setColor(p.chipColor)
            cornerRadius = dpToPx(20).toFloat()
          }
        addView(
          langRow,
          FrameLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.MATCH_PARENT)
            .apply { gravity = Gravity.CENTER_VERTICAL },
        )
      }

    // Thin divider that visually separates the language picker/swap group from the
    // action buttons on either side.
    fun divider(): View =
      View(context).apply {
        background =
          GradientDrawable().apply {
            setColor(p.barStroke)
            cornerRadius = dpToPx(1).toFloat()
          }
      }
    bar.addView(
      divider(),
      LinearLayout.LayoutParams(dpToPx(1), dpToPx(24)).apply {
        marginStart = dpToPx(6)
        marginEnd = dpToPx(4)
      },
    )
    // Same pill size as the other toolbar buttons (40dp tall), so the language
    // picker + swap sit in their own pill, clearly off the bar's container color.
    bar.addView(
      langChip,
      LinearLayout.LayoutParams(0, btnSize, 1f),
    )
    bar.addView(
      divider(),
      LinearLayout.LayoutParams(dpToPx(1), dpToPx(24)).apply {
        marginStart = dpToPx(4)
        marginEnd = dpToPx(6)
      },
    )

    val rightActions =
      LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
      }

    var refreshButton: View? = null
    onRefreshClick?.let {
      val refreshBtn =
        ImageView(context).apply {
          setImageResource(R.drawable.refresh)
          setColorFilter(p.onSurface)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onRefreshClick() }
          contentDescription = context.getString(R.string.a11y_refresh)
        }
      val refreshPill = iconButton(refreshBtn)
      refreshButton = refreshPill
      rightActions.addView(refreshPill, iconLayoutParams(endMarginDp = 0))
    }

    onRegionClick?.let {
      val regionBtn =
        ImageView(context).apply {
          setImageResource(R.drawable.activity_zone)
          setColorFilter(p.onSurface)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onRegionClick() }
          contentDescription = context.getString(R.string.a11y_set_translation_area)
        }
      rightActions.addView(iconButton(regionBtn), iconLayoutParams(endMarginDp = 0))
    }

    var flipIcon: ImageView? = null
    var flipButton: View? = null
    onFlipOriginal?.let {
      val flipBtn =
        ImageView(context).apply {
          setImageResource(R.drawable.flip)
          setColorFilter(p.onSurface)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onFlipOriginal() }
          contentDescription = context.getString(R.string.a11y_show_original)
        }
      flipIcon = flipBtn
      val flipPill = iconButton(flipBtn)
      flipButton = flipPill
      rightActions.addView(flipPill, iconLayoutParams(endMarginDp = 0))
    }

    var pauseIcon: ImageView? = null
    onPauseClick?.let {
      val pauseBtn =
        ImageView(context).apply {
          setImageResource(if (isPaused) R.drawable.play_arrow else R.drawable.pause)
          setColorFilter(p.onSurface)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onPauseClick() }
          contentDescription = context.getString(R.string.a11y_pause_resume)
        }
      pauseIcon = pauseBtn
      rightActions.addView(iconButton(pauseBtn), iconLayoutParams(endMarginDp = 0))
    }

    onMenuClick?.let {
      val menuBtn =
        ImageView(context).apply {
          setImageResource(R.drawable.more_vert)
          setPadding(iconPad, iconPad, iconPad, iconPad)
          setOnClickListener { onMenuClick() }
        }
      rightActions.addView(iconButton(menuBtn), LinearLayout.LayoutParams(btnSize, btnSize))
    }
    bar.addView(rightActions)

    // Root: grip stacked on the bar; the whole thing is wrap-content so it can be
    // dragged around the screen (OverlayUI moves it via translation / window params).
    val root =
      LinearLayout(context).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER_HORIZONTAL
        addView(
          grip,
          LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { bottomMargin = dpToPx(2) },
        )
        addView(bar, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT))
      }

    return LanguageToolbarViews(
      root = root,
      sourceLabel = sourceLabel,
      targetLabel = targetLabel,
      readingOrderButton = readingOrderPill,
      readingOrderIcon = readingOrderIcon,
      pauseIcon = pauseIcon,
      flipIcon = flipIcon,
      flipButton = flipButton,
      refreshButton = refreshButton,
      selectIcon = selectIcon,
    )
  }

  fun setPauseIconState(
    pauseIcon: ImageView?,
    isPaused: Boolean,
  ) {
    pauseIcon?.setImageResource(if (isPaused) R.drawable.play_arrow else R.drawable.pause)
    pauseIcon?.setColorFilter(palette(pauseIcon.context).onSurface)
  }

  fun updateReadingOrderButtonState(
    readingButton: View?,
    readingIcon: ImageView?,
    visible: Boolean,
    readingOrder: ReadingOrder?,
  ) {
    readingButton?.visibility = if (visible) View.VISIBLE else View.GONE
    readingIcon?.apply {
      val (iconRes, description) =
        when (readingOrder) {
          null -> R.drawable.text_rotation_auto to "Japanese OCR auto mode"
          ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT -> R.drawable.text_rotate_vertical to "Japanese OCR vertical mode"
          ReadingOrder.LEFT_TO_RIGHT -> R.drawable.text_rotation_none to "Japanese OCR horizontal mode"
        }
      setImageResource(iconRes)
      setColorFilter(palette(context).onSurface)
      contentDescription = description
    }
  }

  fun createLanguagePicker(
    context: Context,
    dpToPx: (Int) -> Int,
    isSource: Boolean,
    availableLangs: List<Language>,
    allowAuto: Boolean = true,
    onPick: (Language?) -> Unit,
  ): ScrollView {
    val scroll = ScrollView(context)
    val list = LinearLayout(context)
    list.orientation = LinearLayout.VERTICAL
    val bg = GradientDrawable()
    bg.setColor(palette(context).menuBg)
    bg.cornerRadius = dpToPx(20).toFloat()
    list.background = bg
    val listPad = dpToPx(8)
    list.setPadding(listPad, listPad, listPad, listPad)

    if (isSource && allowAuto) {
      addMenuItem(context, list, dpToPx, "\u2728  Auto-detect") {
        onPick(null)
      }
    }

    for (lang in availableLangs) {
      addMenuItem(context, list, dpToPx, lang.localizedName()) {
        onPick(lang)
      }
    }

    scroll.addView(list)
    return scroll
  }

  fun createMenuContainer(
    context: Context,
    dpToPx: (Int) -> Int,
  ): LinearLayout {
    val container = LinearLayout(context)
    container.orientation = LinearLayout.VERTICAL
    val bg = GradientDrawable()
    bg.setColor(palette(context).menuBg)
    bg.cornerRadius = dpToPx(20).toFloat()
    container.background = bg
    val pad = dpToPx(8)
    container.setPadding(pad, pad, pad, pad)
    return container
  }

  fun makePill(
    context: Context,
    dpToPx: (Int) -> Int,
    view: View,
  ): FrameLayout {
    val pill = FrameLayout(context)
    val bg = GradientDrawable()
    bg.setColor(Color.parseColor("#CC303030"))
    bg.cornerRadius = dpToPx(20).toFloat()
    pill.background = bg
    pill.addView(view)
    return pill
  }

  fun addMenuItem(
    context: Context,
    parent: LinearLayout,
    dpToPx: (Int) -> Int,
    label: String,
    enabled: Boolean = true,
    onClick: () -> Unit,
  ) {
    val item = TextView(context)
    item.text = label
    val p = palette(context)
    item.setTextColor(if (enabled) p.menuText else p.menuTextDisabled)
    item.setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
    val pad = dpToPx(12)
    item.setPadding(pad, pad, pad, pad)
    item.setOnClickListener { onClick() }
    parent.addView(item)
  }
}
