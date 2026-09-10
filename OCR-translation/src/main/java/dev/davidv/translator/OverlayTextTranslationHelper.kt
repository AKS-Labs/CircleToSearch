package dev.davidv.translator

import kotlinx.coroutines.flow.first

class OverlayTextTranslationHelper(
  private val langStateManager: LanguageStateManager,
  private val languageMetadataManager: LanguageMetadataManager,
) {
  fun availableLanguages(isSource: Boolean): List<Language> {
    val metadata = languageMetadataManager.metadata.value
    return langStateManager.languageState.value
      .translatorLanguages(requireOcr = isSource)
      .sortedWith(
        compareByDescending<Language> { metadata[it]?.favorite ?: false }
          .thenBy(languageNameComparator()) { it.localizedName() },
      )
  }

  suspend fun awaitAvailableLanguages(isSource: Boolean): List<Language> {
    awaitTranslatorLanguages()
    return availableLanguages(isSource)
  }

  private suspend fun awaitTranslatorLanguages(): List<Language> {
    langStateManager.languageState.first { !it.isChecking }
    return langStateManager.languageState.value.translatorLanguages()
  }
}
