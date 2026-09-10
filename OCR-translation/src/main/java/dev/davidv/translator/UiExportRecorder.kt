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

import android.content.Context
import android.content.ContextWrapper
import android.content.res.Resources
import java.util.concurrent.ConcurrentHashMap

/**
 * Records resolved string-resource lookups during the UI-export instrumented run, so the SVG exporter
 * can stamp each label with the exact R.string name it came from instead of recovering it by matching
 * the rendered text (which can't tell apart two ids with the same English, and has to reverse-engineer
 * format/plural substitutions).
 *
 * Inert in production: nothing wraps and nothing records until [active] is set true, which only the
 * export test does, before the activity is created. When inactive, [wrap] returns the context unchanged.
 */
object UiExportRecorder {
  @Volatile var active = false
  private val byValue = ConcurrentHashMap<String, String>()
  private val ambiguous = ConcurrentHashMap.newKeySet<String>()

  fun reset() {
    byValue.clear()
    ambiguous.clear()
  }

  fun record(
    entry: String,
    value: String,
  ) {
    if (value.isEmpty()) return
    val prev = byValue.putIfAbsent(value, entry)
    if (prev != null && prev != entry) ambiguous.add(value)
  }

  /** The R.string name that produced [value] exactly, or null if unknown or produced by several ids. */
  fun idFor(value: String): String? = if (ambiguous.contains(value)) null else byValue[value]

  /** Wrap [base] so its Resources record every string lookup; the identity context when not [active]. */
  fun wrap(base: Context): Context =
    if (!active) {
      base
    } else {
      object : ContextWrapper(base) {
        private val res by lazy { RecordingResources(base.resources) }

        override fun getResources(): Resources = res
      }
    }
}

@Suppress("DEPRECATION")
private class RecordingResources(base: Resources) :
  Resources(base.assets, base.displayMetrics, base.configuration) {
  private fun <T : CharSequence> T.record(id: Int): T {
    if (UiExportRecorder.active) {
      try {
        UiExportRecorder.record(getResourceEntryName(id), toString())
      } catch (_: NotFoundException) {
      }
    }
    return this
  }

  override fun getString(id: Int): String = super.getString(id).record(id)

  override fun getString(
    id: Int,
    vararg formatArgs: Any?,
  ): String = super.getString(id, *formatArgs).record(id)

  override fun getText(id: Int): CharSequence = super.getText(id).record(id)

  override fun getQuantityString(
    id: Int,
    quantity: Int,
  ): String = super.getQuantityString(id, quantity).record(id)

  override fun getQuantityString(
    id: Int,
    quantity: Int,
    vararg formatArgs: Any?,
  ): String = super.getQuantityString(id, quantity, *formatArgs).record(id)

  override fun getQuantityText(
    id: Int,
    quantity: Int,
  ): CharSequence = super.getQuantityText(id, quantity).record(id)
}
