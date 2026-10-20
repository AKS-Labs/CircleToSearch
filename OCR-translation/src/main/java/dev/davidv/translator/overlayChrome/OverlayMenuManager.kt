package dev.davidv.translator.overlayChrome

import android.content.Context
import android.graphics.Color
import android.view.View
import dev.davidv.translator.DownloadEvent
import dev.davidv.translator.DownloadService
import dev.davidv.translator.Language
import dev.davidv.translator.OverlayLanguagePickerRow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch

interface OverlayMenuHost {
  fun addDismissLayer(view: View)

  fun addMenuView(view: View)

  fun addPickerView(view: View)

  fun removeMenuChild(view: View)
}

class OverlayMenuManager(
  private val context: Context,
  private val dpToPx: (Int) -> Int,
  private val host: OverlayMenuHost,
) {
  private var dismissLayer: View? = null
  private var menuView: View? = null
  private val pickerScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
  private var pickerObservationJob: Job? = null
  private var pendingDownloadCode: String? = null
  private var pinnedDownloadCode: String? = null
  private val favoriteOverrides = mutableMapOf<String, Boolean>()

  /** Languages deleted from this picker session: rows flip to "download" state
   *  instantly, without waiting for the availability refresh to land. */
  private val deletedThisSession = mutableSetOf<String>()

  fun showDotsMenu(menuItems: List<Pair<String, () -> Unit>>) {
    dismiss()

    val dismissView =
      View(context).apply {
        setBackgroundColor(Color.TRANSPARENT)
        setOnClickListener { dismiss() }
      }
    host.addDismissLayer(dismissView)
    dismissLayer = dismissView

    val menuContainer = OverlayChromeFactory.createMenuContainer(context, dpToPx)
    for ((label, action) in menuItems) {
      OverlayChromeFactory.addMenuItem(context, menuContainer, dpToPx, label) {
        dismiss()
        action()
      }
    }
    host.addMenuView(menuContainer)
    menuView = menuContainer
  }

  /**
   * Language picker over every catalog language. Rows supplied by [rowsProvider]
   * (re-queried when a download completes so the row flips to installed);
   * not-installed rows carry a size label, a star toggle, and an inline download
   * button showing determinate progress (tap the ring to cancel). A language
   * downloaded from this picker is pinned to the top of the list on completion
   * so the user can pick it directly; favorites (starred rows) sort first.
   */
  fun showLanguagePicker(
    isSource: Boolean,
    rowsProvider: () -> List<OverlayLanguagePickerRow>,
    allowAuto: Boolean = true,
    onToggleFavorite: (Language, Boolean) -> Unit,
    onDelete: (Language) -> Unit,
    onPick: (Language?) -> Unit,
  ) {
    dismiss()

    val dismissView =
      View(context).apply {
        setBackgroundColor(Color.parseColor("#80000000"))
        setOnClickListener { dismiss() }
      }
    host.addDismissLayer(dismissView)
    dismissLayer = dismissView

    pendingDownloadCode = null
    pinnedDownloadCode = null
    favoriteOverrides.clear()
    deletedThisSession.clear()

    // Live rows: favorite overrides apply instantly (persistence flows through
    // the metadata store); languages deleted this session flip to their
    // download state immediately; a language downloaded here pins to the top.
    val liveRowsProvider = {
      rowsProvider()
        .map { row ->
          val base =
            if (row.language.code in deletedThisSession && row.installed) {
              row.copy(
                installed = false,
                installedSizeBytes = 0L,
                downloadSizeBytes = row.installedSizeBytes,
                pendingDictionaryInfo = null,
              )
            } else {
              row
            }
          favoriteOverrides[base.language.code]?.let { base.copy(favorite = it) } ?: base
        }.let { rows ->
          pinnedDownloadCode?.let { pin ->
            rows.sortedBy { it.language.code != pin }
          } ?: rows
        }
    }

    var handle: LanguagePickerHandle? = null
    handle =
      OverlayChromeFactory.createLanguagePicker(
        context = context,
        dpToPx = dpToPx,
        isSource = isSource,
        rowsProvider = liveRowsProvider,
        allowAuto = allowAuto,
        // Ignore stale completed states for languages deleted this session.
        downloadStateFor = { language ->
          if (language.code in deletedThisSession) {
            null
          } else {
            DownloadService.pickerStates.value[language]
          }
        },
        onPick = { language ->
          onPick(language)
          dismiss()
        },
        onDownload = { row ->
          deletedThisSession.remove(row.language.code)
          pendingDownloadCode = row.language.code
          DownloadService.startDownload(context, row.language)
          row.pendingDictionaryInfo?.let { dictInfo ->
            DownloadService.startDictDownload(context, row.language, dictInfo)
          }
          handle?.updateDownloadStates()
        },
        onCancelDownload = { language ->
          if (pendingDownloadCode == language.code) {
            pendingDownloadCode = null
          }
          DownloadService.cancelDownload(context, language)
          DownloadService.cancelDictDownload(context, language)
        },
        onDelete = { row ->
          deletedThisSession.add(row.language.code)
          if (pendingDownloadCode == row.language.code) {
            pendingDownloadCode = null
          }
          onDelete(row.language)
          handle?.setRows(liveRowsProvider())
        },
        onToggleFavorite = { language, favorite ->
          favoriteOverrides[language.code] = favorite
          onToggleFavorite(language, favorite)
        },
      )
    host.addPickerView(handle.root)
    menuView = handle.root

    pickerObservationJob?.cancel()
    pickerObservationJob =
      pickerScope.launch {
        launch {
          DownloadService.pickerStates.collect {
            handle.updateDownloadStates()
          }
        }
        launch {
          DownloadService.pickerEvents.collect { event ->
            when (event) {
              is DownloadEvent.NewTranslationAvailable -> {
                val code = event.language.code
                if (pendingDownloadCode == code) {
                  pendingDownloadCode = null
                  pinnedDownloadCode = code
                }
                handle.setRows(liveRowsProvider())
              }

              is DownloadEvent.DownloadError -> {
                pendingDownloadCode = null
                handle.updateDownloadStates()
              }

              else -> Unit
            }
          }
        }
      }
  }

  fun dismiss() {
    pickerObservationJob?.cancel()
    pickerObservationJob = null
    dismissLayer?.let {
      host.removeMenuChild(it)
      dismissLayer = null
    }
    menuView?.let {
      host.removeMenuChild(it)
      menuView = null
    }
  }
}
