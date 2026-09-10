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

/**
 * A list zipper over the target languages: the ordered tab list is
 * `before + active + after`. Holding [active] as its own field (rather than an
 * index or a flag per element) makes the invariants structural — the list is
 * always non-empty, there is always exactly one active tab, and the active tab
 * is always a member.
 */
data class TargetTabs(
  val before: List<Language>,
  val active: Language,
  val after: List<Language>,
) {
  val tabs: List<Language> get() = before + active + after

  /**
   * Move the focus to [lang]. If [lang] is already a tab it becomes active
   * (order preserved); otherwise it replaces the active tab in place (retarget).
   */
  fun select(lang: Language): TargetTabs =
    when {
      lang == active -> this
      lang in before -> {
        val i = before.indexOf(lang)
        TargetTabs(before.take(i), lang, before.drop(i + 1) + active + after)
      }
      lang in after -> {
        val i = after.indexOf(lang)
        TargetTabs(before + active + after.take(i), lang, after.drop(i + 1))
      }
      else -> copy(active = lang)
    }

  /** Append [lang] as a new tab and focus it (a no-op switch if already present). */
  fun add(lang: Language): TargetTabs = if (lang in tabs) select(lang) else TargetTabs(before + active + after, lang, emptyList())

  /** Drop [lang]; if it was the active tab the focus moves to a neighbour. The last tab cannot be removed. */
  fun remove(lang: Language): TargetTabs =
    when {
      lang == active && after.isNotEmpty() -> TargetTabs(before, after.first(), after.drop(1))
      lang == active && before.isNotEmpty() -> TargetTabs(before.dropLast(1), before.last(), emptyList())
      lang == active -> this
      lang in before -> copy(before = before - lang)
      lang in after -> copy(after = after - lang)
      else -> this
    }

  companion object {
    fun of(lang: Language) = TargetTabs(emptyList(), lang, emptyList())
  }
}
