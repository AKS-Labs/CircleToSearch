package dev.davidv.translator.overlayChrome

import android.app.Activity
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageMetadataManager
import dev.davidv.translator.LanguageStateManager
import dev.davidv.translator.OverlayTextTranslationHelper
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

/**
 * The translate toolbar's language picker dialog (same [OverlayMenuManager] +
 * [OverlayChromeFactory.createLanguagePicker] views, same scrim, same download
 * rows) hosted in the running activity instead of an accessibility overlay
 * window — for screens inside the app, e.g. the live camera's language pills.
 *
 * The dialog views are added to a full-screen FrameLayout overlaying the
 * activity's content ONLY while the picker is up: [OverlayMenuHost.removeMenuChild]
 * detaches the container the moment the last child (scrim or dialog) is
 * removed, so a lingering full-screen view can never swallow camera-screen taps.
 */
class ActivityLanguagePicker(
  private val activity: Activity,
  private val languageStateManager: LanguageStateManager,
  languageMetadataManager: LanguageMetadataManager,
) {
  private val container = FrameLayout(activity)
  private val helper = OverlayTextTranslationHelper(languageStateManager, languageMetadataManager)
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private val dpToPx: (Int) -> Int = { dp ->
    (dp * activity.resources.displayMetrics.density).toInt()
  }

  private val menuManager =
    OverlayMenuManager(
      activity,
      dpToPx,
      object : OverlayMenuHost {
        override fun addDismissLayer(view: View) {
          attach()
          container.addView(
            view,
            FrameLayout.LayoutParams(
              ViewGroup.LayoutParams.MATCH_PARENT,
              ViewGroup.LayoutParams.MATCH_PARENT,
            ),
          )
        }

        override fun addMenuView(view: View) {
          attach()
          container.addView(
            view,
            FrameLayout.LayoutParams(dpToPx(180), ViewGroup.LayoutParams.WRAP_CONTENT).apply {
              gravity = Gravity.BOTTOM or Gravity.END
              bottomMargin = dpToPx(84)
              marginEnd = dpToPx(8)
            },
          )
        }

        override fun addPickerView(view: View) {
          attach()
          val (pickerW, pickerH) = OverlayChromeFactory.pickerWindowSize(activity)
          container.addView(
            view,
            FrameLayout.LayoutParams(pickerW, pickerH).apply {
              gravity = Gravity.CENTER
            },
          )
        }

        override fun removeMenuChild(view: View) {
          container.removeView(view)
          // The picker session is over once its last child is gone — drop the
          // full-screen container so touches reach the screen underneath again.
          if (container.childCount == 0) detach()
        }
      },
    )

  private fun attach() {
    if (container.parent == null) {
      activity.addContentView(
        container,
        ViewGroup.LayoutParams(
          ViewGroup.LayoutParams.MATCH_PARENT,
          ViewGroup.LayoutParams.MATCH_PARENT,
        ),
      )
    }
  }

  private fun detach() {
    (container.parent as? ViewGroup)?.removeView(container)
  }

  /**
   * Shows the picker dialog. [allowAuto] adds the ✨ Auto-detect row (source
   * side); [onPick] receives the chosen language, or null when Auto was picked.
   */
  fun show(
    isSource: Boolean,
    allowAuto: Boolean,
    onPick: (Language?) -> Unit,
  ) {
    languageStateManager.refreshLanguageAvailability()
    scope.launch {
      helper.awaitPickerRows()
      menuManager.showLanguagePicker(
        isSource = isSource,
        rowsProvider = { helper.pickerRows() },
        allowAuto = allowAuto,
        onToggleFavorite = { language, favorite -> helper.setFavorite(language, favorite) },
        onDelete = { language -> languageStateManager.deleteLanguage(language) },
        onPick = onPick,
      )
    }
  }

  fun dismiss() {
    menuManager.dismiss()
    detach()
  }

  fun destroy() {
    dismiss()
    scope.cancel()
  }
}
