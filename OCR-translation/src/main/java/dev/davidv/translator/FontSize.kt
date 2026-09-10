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

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.sp

enum class FontSize(
  val displayName: String,
) {
  SMALL("Small"),
  MEDIUM("Medium"),
  LARGE("Large"),
  ;

  @Composable
  fun getTextStyle(): TextStyle =
    when (this) {
      SMALL -> MaterialTheme.typography.bodyLarge
      MEDIUM -> MaterialTheme.typography.bodyLarge.copy(fontSize = 24.sp, lineHeight = 32.sp)
      LARGE -> MaterialTheme.typography.bodyLarge.copy(fontSize = 32.sp, lineHeight = 40.sp)
    }
}
