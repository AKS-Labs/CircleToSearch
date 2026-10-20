package dev.davidv.translator.accessibilityOverlay

import android.content.Context
import dev.davidv.translator.LanguageStateManager

interface TranslatorOverlayHost {
  val hostContext: Context
  val langStateManager: LanguageStateManager

  fun activate()

  fun deactivate()

  fun toggleSelectMode()

  fun toggleFlipOriginal()

  fun swapLanguages()

  fun showLanguagePicker(isSource: Boolean)

  fun toggleJapaneseOcrMode()

  fun showDotsMenu()

  fun handleFullScreenOcr()

  fun disableService()
}
