/*
 * Copyright (C) 2024 David V
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

package dev.davidv.translator

import android.net.Uri

enum class FromLangChange {
  KeepTarget,
  MovePreviousToTarget,
}

sealed class TranslatorMessage {
  data class TextInput(
    val text: String,
  ) : TranslatorMessage()

  data class FromLang(
    val language: Language,
    val change: FromLangChange = FromLangChange.KeepTarget,
  ) : TranslatorMessage()

  data class ToLang(
    val language: Language,
  ) : TranslatorMessage()

  data class AddTab(
    val language: Language,
  ) : TranslatorMessage()

  data class RemoveTab(
    val language: Language,
  ) : TranslatorMessage()

  data class SetImageUri(
    val uri: Uri,
    val deleteAfterLoad: Boolean = false,
  ) : TranslatorMessage()

  data class SetDocumentPath(
    val path: String,
    val displayName: String,
    val sizeBytes: Long,
    val deleteAfterLoad: Boolean = false,
  ) : TranslatorMessage()

  data object ShareTranslatedImage : TranslatorMessage()

  data object ToggleJapaneseOcrMode : TranslatorMessage()

  data object SwapLanguages : TranslatorMessage()

  data object EnableAutoSource : TranslatorMessage()

  data object ClearInput : TranslatorMessage()

  data class InitializeLanguages(
    val from: Language,
    val to: Language,
  ) : TranslatorMessage()

  data class ImageTextDetected(
    val extractedText: String,
  ) : TranslatorMessage()

  data class DictionaryLookup(
    val str: String,
    val language: Language,
  ) : TranslatorMessage()

  data class SpeakTranslatedText(
    val text: String,
    val language: Language,
  ) : TranslatorMessage()

  /**
   * Re-translate the current output forcing [forcedPrefix] as the start of the
   * translation — used when the user picks a word alternative. The source and
   * languages come from current state.
   */
  data class Steer(
    val forcedPrefix: String,
  ) : TranslatorMessage()

  data object PopDictionary : TranslatorMessage()

  data object ClearDictionaryStack : TranslatorMessage()

  data class ChangeLaunchMode(
    val newLaunchMode: LaunchMode,
  ) : TranslatorMessage()
}
