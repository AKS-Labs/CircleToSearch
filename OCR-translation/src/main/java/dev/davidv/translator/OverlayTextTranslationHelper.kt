package dev.davidv.translator

import kotlinx.coroutines.flow.first

/**
 * One language row for the overlay language picker: every catalog language is
 * listed; languages without their translation assets installed show a download
 * size and can be downloaded inline from the picker.
 */
data class OverlayLanguagePickerRow(
  val language: Language,
  val installed: Boolean,
  /** Favorited via the picker's star (sorts to the top of the list). */
  val favorite: Boolean,
  /** Remaining download bytes for translation + dictionary (0 when installed). */
  val downloadSizeBytes: Long,
  /** On-disk size of the installed translation + dictionary assets (0 when missing). */
  val installedSizeBytes: Long,
  /** Dictionary pack not yet installed, if this language has one. */
  val pendingDictionaryInfo: DictionaryInfo?,
)

/** Build the full-catalog picker rows (favorites first, then alphabetical). */
fun buildOverlayPickerRows(
  langStateManager: LanguageStateManager,
  metadata: Map<Language, LanguageMetadata> = emptyMap(),
): List<OverlayLanguagePickerRow> {
  val state = langStateManager.languageState.value
  val catalog = langStateManager.catalog.value ?: return emptyList()
  return state.availableLanguages
    .map { entry ->
      val language = entry.language
      val availability = entry.availability
      val installed = availability.translatorFiles
      val dictionaryInfo = catalog.dictionaryInfoFor(language)
      val installedDictionaryBytes =
        if (availability.dictionaryFiles) {
          dictionaryInfo?.size ?: 0L
        } else {
          0L
        }
      OverlayLanguagePickerRow(
        language = language,
        installed = installed,
        favorite = metadata[language]?.favorite ?: false,
        downloadSizeBytes =
          if (installed || language.isEnglish) {
            0L
          } else {
            catalog.sizeBytesForFeature(language.code, Feature.CORE) +
              if (availability.dictionaryFiles) 0L else dictionaryInfo?.size ?: 0L
          },
        installedSizeBytes =
          // English is always built-in; the catalog still lists ~2.4GB of packs
          // under "en", which are never downloaded — never show a size for it.
          if (installed && !language.isEnglish) {
            catalog.sizeBytesForFeature(language.code, Feature.CORE) + installedDictionaryBytes
          } else {
            0L
          },
        pendingDictionaryInfo = if (availability.dictionaryFiles) null else dictionaryInfo,
      )
    }.sortedWith(
      compareByDescending<OverlayLanguagePickerRow> { metadata[it.language]?.favorite ?: false }
        .thenBy(languageNameComparator()) { it.language.localizedName() },
    )
}

class OverlayTextTranslationHelper(
  private val langStateManager: LanguageStateManager,
  private val languageMetadataManager: LanguageMetadataManager,
) {
  /** All catalog languages (favorites first, then alphabetical), for the picker. */
  fun pickerRows(): List<OverlayLanguagePickerRow> =
    buildOverlayPickerRows(langStateManager, languageMetadataManager.metadata.value)

  fun setFavorite(
    language: Language,
    favorite: Boolean,
  ) {
    val current = languageMetadataManager.metadata.value[language] ?: LanguageMetadata()
    languageMetadataManager.updateLanguage(language, current.copy(favorite = favorite))
  }

  suspend fun awaitPickerRows(): List<OverlayLanguagePickerRow> {
    awaitTranslatorLanguages()
    return pickerRows()
  }

  private suspend fun awaitTranslatorLanguages() {
    langStateManager.languageState.first { !it.isChecking }
  }
}
