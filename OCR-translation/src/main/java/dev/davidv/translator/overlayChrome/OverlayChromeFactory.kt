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
import kotlin.math.roundToInt

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

/** Live handle to a shown language picker: rows can be replaced (install-state
 *  flips after a download) and download rows refreshed in place on progress. */
class LanguagePickerHandle(
  val root: View,
  internal val setRows: (List<dev.davidv.translator.OverlayLanguagePickerRow>) -> Unit,
  internal val updateDownloadStates: () -> Unit,
)

object OverlayChromeFactory {
  val ACTIVE_ICON_TINT: Int = Color.parseColor("#8AB4F8")

  /**
   * Exact CTS bottom-bar color published by CircleToSearchScreen via SideEffect
   * (`surfaceContainerLow`). When set, the toolbar uses it pixel-for-pixel
   * instead of the locally derived scheme.
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
      // CTS bottom-bar container (surfaceContainerLow) — prefer the exact value
      // published by the app UI so the bar matches pixel-for-pixel.
      barColor = opaque(appBarContainerColor ?: scheme.surfaceContainerLow.toArgb()),
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

  /**
   * Size for the language-picker dialog in pixels: most of the screen so the
   * full language list is browsable, capped on wide (tablet) screens.
   */
  fun pickerWindowSize(context: Context): Pair<Int, Int> {
    val dm = context.resources.displayMetrics
    val width = (dm.widthPixels * 0.88f).toInt().coerceAtMost((480 * dm.density).toInt())
    val height = (dm.heightPixels * 0.78f).toInt().coerceAtMost((760 * dm.density).toInt())
    return width to height
  }

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

    // Same pill size as the other toolbar buttons (40dp tall), so the language
    // picker + swap sit in their own pill, clearly off the bar's container color.
    bar.addView(
      langChip,
      LinearLayout.LayoutParams(0, btnSize, 1f).apply {
        marginStart = dpToPx(2)
        marginEnd = dpToPx(2)
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
    rowsProvider: () -> List<dev.davidv.translator.OverlayLanguagePickerRow>,
    allowAuto: Boolean = true,
    downloadStateFor: (Language) -> dev.davidv.translator.DownloadState?,
    onPick: (Language?) -> Unit,
    onDownload: (dev.davidv.translator.OverlayLanguagePickerRow) -> Unit,
    onCancelDownload: (Language) -> Unit,
    onDelete: (dev.davidv.translator.OverlayLanguagePickerRow) -> Unit,
    onToggleFavorite: (Language, Boolean) -> Unit,
  ): LanguagePickerHandle {
    val p = palette(context)
    val scroll = ScrollView(context)
    val list = LinearLayout(context)
    list.orientation = LinearLayout.VERTICAL
    // Rounded surface on the ScrollView itself (not the scrolling content), so the
    // corners stay rounded while scrolling instead of clipping to a square window.
    val bg =
      GradientDrawable().apply {
        setColor(p.menuBg)
        cornerRadius = dpToPx(20).toFloat()
        setStroke(dpToPx(1), p.barStroke)
      }
    scroll.background = bg
    scroll.clipToOutline = true
    val listPad = dpToPx(8)
    list.setPadding(listPad, listPad, listPad, listPad)
    scroll.addView(list)

    var currentRows = rowsProvider()
    val stateUpdaters = mutableListOf<() -> Unit>()

    fun formatSize(sizeBytes: Long): String {
      val sizeMiB = sizeBytes / (1024f * 1024f)
      return if (sizeMiB < 1f) {
        "\u003c1 MB"
      } else {
        "${sizeMiB.roundToInt()} MB"
      }
    }

    fun formatPercent(downloaded: Long, total: Long): String {
      val pct =
        if (total > 0) {
          ((downloaded * 100) / total).toInt().coerceIn(0, 100)
        } else {
          0
        }
      return "$pct%"
    }

    fun applyRipple(view: View) {
      val out = TypedValue()
      if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, out, true) && out.resourceId != 0) {
        view.setBackgroundResource(out.resourceId)
      }
    }

    fun render(rowList: List<dev.davidv.translator.OverlayLanguagePickerRow>) {
      currentRows = rowList
      list.removeAllViews()
      stateUpdaters.clear()

      if (isSource && allowAuto) {
        val autoItem =
          TextView(context).apply {
            text = "\u2728  Auto-detect"
            setTextColor(p.menuText)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            val pad = dpToPx(12)
            setPadding(pad, pad, pad, pad)
            applyRipple(this)
            setOnClickListener { onPick(null) }
          }
        list.addView(autoItem)
      }

      for (row in rowList) {
        val language = row.language
        val state = downloadStateFor(language)
        val showAsInstalled = row.installed || state?.isCompleted == true
        val downloadingState = if (!showAsInstalled && state?.isDownloading == true) state else null

        val item = LinearLayout(context)
        item.orientation = LinearLayout.HORIZONTAL
        item.gravity = Gravity.CENTER_VERTICAL
        val itemPad = dpToPx(12)
        item.setPadding(itemPad, itemPad, itemPad, itemPad)
        applyRipple(item)

        val name =
          TextView(context).apply {
            text = language.localizedName()
            setTextColor(p.menuText)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            maxLines = 1
            ellipsize = TextUtils.TruncateAt.END
          }
        item.addView(name, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))

        // Trailing group: [size % / progress ring] [star] [download button]
        val trailing =
          LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
          }

        var sizeText: TextView? = null
        if (!showAsInstalled || row.installedSizeBytes > 0) {
          val label =
            TextView(context).apply {
              text =
                when {
                  downloadingState != null -> formatPercent(downloadingState.downloaded, downloadingState.totalSize)
                  showAsInstalled -> formatSize(row.installedSizeBytes)
                  else -> formatSize(row.downloadSizeBytes)
                }
              setTextColor(p.menuTextDisabled)
              setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
              gravity = Gravity.END
            }
          sizeText = label
          trailing.addView(
            label,
            LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, LinearLayout.LayoutParams.WRAP_CONTENT)
              .apply { marginEnd = dpToPx(8) },
          )
        }

        val starHolder =
          FrameLayout(context).apply {
            val starSize = dpToPx(36)
            val starPad = dpToPx(9)
            val star =
              ImageView(context).apply {
                setPadding(starPad, starPad, starPad, starPad)
              }
            fun applyStar(favorite: Boolean) {
              star.setImageResource(if (favorite) R.drawable.star_filled else R.drawable.star)
              star.setColorFilter(if (favorite) p.activeTint else p.menuTextDisabled)
              star.contentDescription = if (favorite) "Unstar ${language.displayName}" else "Star ${language.displayName}"
            }
            applyStar(row.favorite)
            addView(star, FrameLayout.LayoutParams(starSize, starSize).apply { gravity = Gravity.CENTER })
            setOnClickListener {
              val newFavorite = !(currentRows.find { it.language.code == language.code }?.favorite ?: row.favorite)
              applyStar(newFavorite)
              onToggleFavorite(language, newFavorite)
            }
          }
        trailing.addView(starHolder)

        if (!showAsInstalled) {
          val iconHolder = FrameLayout(context)
          val iconSize = dpToPx(36)
          fun setDownloadIcon() {
            iconHolder.removeAllViews()
            iconHolder.setOnClickListener(null)
            val pill =
              GradientDrawable().apply {
                setColor(p.chipColor)
                cornerRadius = dpToPx(18).toFloat()
              }
            val out = TypedValue()
            if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, out, true) && out.resourceId != 0) {
              iconHolder.background =
                android.graphics.drawable.LayerDrawable(
                  arrayOf(
                    pill,
                    context.getDrawable(out.resourceId) ?: android.graphics.drawable.ColorDrawable(Color.TRANSPARENT),
                  ),
                )
            } else {
              iconHolder.background = pill
            }
            val iconPad = dpToPx(8)
            val icon =
              ImageView(context).apply {
                setImageResource(R.drawable.download)
                setColorFilter(p.onSurface)
                setPadding(iconPad, iconPad, iconPad, iconPad)
                contentDescription = "Download ${language.displayName}"
              }
            iconHolder.addView(icon, FrameLayout.LayoutParams(iconSize, iconSize).apply { gravity = Gravity.CENTER })
            iconHolder.setOnClickListener { onDownload(row) }
          }
          fun setProgressRing(downloaded: Long, total: Long) {
            iconHolder.removeAllViews()
            iconHolder.background = null
            val ring =
              android.widget.ProgressBar(context, null, android.R.attr.progressBarStyleSmall).apply {
                isIndeterminate = false
                max = 100
                progress =
                  if (total > 0) {
                    ((downloaded * 100) / total).toInt().coerceIn(0, 100)
                  } else {
                    0
                  }
                progressTintList = android.content.res.ColorStateList.valueOf(p.activeTint)
              }
            val ringSize = dpToPx(24)
            iconHolder.addView(ring, FrameLayout.LayoutParams(ringSize, ringSize).apply { gravity = Gravity.CENTER })
            // Tapping the ring cancels the download, like the language setup screen.
            iconHolder.setOnClickListener { onCancelDownload(language) }
          }
          if (downloadingState != null) {
            setProgressRing(downloadingState.downloaded, downloadingState.totalSize)
          } else {
            setDownloadIcon()
          }
          trailing.addView(iconHolder, LinearLayout.LayoutParams(iconSize, iconSize))

          stateUpdaters.add {
            val currentState = downloadStateFor(language)
            if (currentState?.isDownloading == true) {
              setProgressRing(currentState.downloaded, currentState.totalSize)
              sizeText?.text = formatPercent(currentState.downloaded, currentState.totalSize)
            } else {
              setDownloadIcon()
              sizeText?.text = formatSize(row.downloadSizeBytes)
            }
          }
        } else if (!language.isEnglish) {
          // Installed rows: delete button (English has no downloadable assets).
          val deleteHolder =
            FrameLayout(context).apply {
              val delSize = dpToPx(36)
              val pill =
                GradientDrawable().apply {
                  setColor(p.chipColor)
                  cornerRadius = dpToPx(18).toFloat()
                }
              val out = TypedValue()
              if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, out, true) && out.resourceId != 0) {
                background =
                  android.graphics.drawable.LayerDrawable(
                    arrayOf(
                      pill,
                      context.getDrawable(out.resourceId) ?: android.graphics.drawable.ColorDrawable(Color.TRANSPARENT),
                    ),
                  )
              } else {
                background = pill
              }
              val iconPad = dpToPx(8)
              val icon =
                ImageView(context).apply {
                  setImageResource(R.drawable.delete)
                  setColorFilter(p.onSurface)
                  setPadding(iconPad, iconPad, iconPad, iconPad)
                  contentDescription = "Delete ${language.displayName}"
                }
              addView(icon, FrameLayout.LayoutParams(delSize, delSize).apply { gravity = Gravity.CENTER })
              setOnClickListener { onDelete(row) }
            }
          trailing.addView(deleteHolder, LinearLayout.LayoutParams(dpToPx(36), dpToPx(36)))
        }

        item.addView(trailing)
        if (showAsInstalled) {
          item.setOnClickListener { onPick(language) }
        }
        list.addView(item)
      }
      scroll.scrollTo(0, 0)
    }

    render(currentRows)
    return LanguagePickerHandle(
      root = scroll,
      setRows = { render(it) },
      updateDownloadStates = { stateUpdaters.forEach { update -> update() } },
    )
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
    val out = TypedValue()
    if (context.theme.resolveAttribute(android.R.attr.selectableItemBackground, out, true) && out.resourceId != 0) {
      item.setBackgroundResource(out.resourceId)
    }
    item.setOnClickListener { onClick() }
    parent.addView(item)
  }
}
