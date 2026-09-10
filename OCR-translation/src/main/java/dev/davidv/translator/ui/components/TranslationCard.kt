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

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

// The shared skeleton for the input and output cards: a header row with the
// source/target language label on the left and a tools cluster on the right, a
// body that fills the remaining height, and an optional footer separated from
// the body by a subtle gutter line (used by the input card for its source row).
@Composable
fun TranslationCard(
  label: String,
  modifier: Modifier = Modifier,
  tools: @Composable RowScope.() -> Unit,
  footer: (@Composable () -> Unit)? = null,
  labelContent: (@Composable () -> Unit)? = null,
  body: @Composable () -> Unit,
) {
  AppCard(modifier = modifier) {
    Column(
      modifier =
        Modifier
          .fillMaxSize()
          .padding(start = 20.dp, end = 8.dp, top = 8.dp, bottom = 12.dp),
    ) {
      Row(
        modifier = Modifier.fillMaxWidth().heightIn(min = 40.dp),
        verticalAlignment = Alignment.CenterVertically,
      ) {
        if (labelContent != null) {
          Box(modifier = Modifier.weight(1f).padding(end = 8.dp)) {
            labelContent()
          }
        } else {
          Text(
            text = label.uppercase(),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            letterSpacing = 0.8.sp,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f).padding(end = 8.dp),
          )
        }
        Row(
          horizontalArrangement = Arrangement.spacedBy(2.dp),
          verticalAlignment = Alignment.CenterVertically,
          content = tools,
        )
      }

      Box(
        modifier =
          Modifier
            .fillMaxWidth()
            .weight(1f)
            .padding(end = 12.dp, top = 4.dp),
      ) {
        body()
      }

      if (footer != null) {
        HorizontalDivider(
          modifier = Modifier.padding(end = 12.dp, top = 4.dp, bottom = 4.dp),
          color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.6f),
        )
        footer()
      }
    }
  }
}
