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

import java.text.Collator
import java.util.Locale

data class Language(
  val code: String,
  val displayName: String,
  val shortDisplayName: String,
  val script: Script,
  val writingSystem: WritingSystem,
  val dictionaryCode: String,
) {
  val isEnglish: Boolean get() = code == "en"

  override fun equals(other: Any?): Boolean = other is Language && code == other.code

  override fun hashCode(): Int = code.hashCode()

  override fun toString(): String = "Language($code)"
}

fun Language.localizedName(locale: Locale = Locale.getDefault()): String {
  val tag =
    when (code) {
      "zh" -> "zh-Hans"
      "zh_hant" -> "zh-Hant"
      else -> code
    }
  val raw = Locale.forLanguageTag(tag).getDisplayName(locale)
  // forLanguageTag echoes the subtag back verbatim when CLDR has no entry for it
  val name = if (raw.isBlank() || raw.equals(tag, ignoreCase = true)) displayName else raw
  // CLDR lowercases language names in many locales (e.g. fr "persan"); title-case for list display
  return name.replaceFirstChar { if (it.isLowerCase()) it.titlecase(locale) else it.toString() }
}

fun languageNameComparator(locale: Locale = Locale.getDefault()): Comparator<String> {
  val collator = Collator.getInstance(locale).apply { strength = Collator.SECONDARY }
  return Comparator { a, b -> collator.compare(a, b) }
}
