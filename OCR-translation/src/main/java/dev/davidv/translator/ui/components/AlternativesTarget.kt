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

import dev.davidv.translator.WordAlternative

// A tapped word the alternatives drawer acts on. Carries the word's char range
// in the current translation plus the model's alternatives (empty when the
// model had none — only "Type your own…" is offered then).
data class AlternativesTarget(
  val wordBegin: Int,
  val wordEnd: Int,
  val options: List<WordAlternative>,
)
