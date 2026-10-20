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

package dev.davidv.translator.ui.components

// The char range of the whole word around a tap offset (an insertion index, so
// it may land just past a word or on a space — step back onto the preceding
// letter in that case). Null when the tap isn't on a word.
fun wordRangeAt(
  text: String,
  offset: Int,
): IntRange? {
  val n = text.length
  if (n == 0) return null
  var i = offset.coerceIn(0, n)
  if (i >= n || !text[i].isLetterOrDigit()) {
    if (i > 0 && text[i - 1].isLetterOrDigit()) i-- else return null
  }
  var start = i
  while (start > 0 && text[start - 1].isLetterOrDigit()) start--
  var end = i + 1
  while (end < n && text[end].isLetterOrDigit()) end++
  return start until end
}

fun wordAt(
  text: String,
  offset: Int,
): String? = wordRangeAt(text, offset)?.let { text.substring(it.first, it.last + 1) }
