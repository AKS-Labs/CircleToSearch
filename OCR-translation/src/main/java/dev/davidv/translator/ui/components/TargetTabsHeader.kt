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

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.davidv.translator.Language
import dev.davidv.translator.R
import dev.davidv.translator.localizedName

// Output-card header for multi-target mode: a scrollable strip of target tabs.
// The active tab renders like the plain single-target label; the others are
// lighter and tappable to switch. Tapping a tab only switches; "+" adds a new
// target, and the active tab gets an "×" to remove it (never the last one).
@Composable
fun TargetTabsHeader(
  tabs: List<Language>,
  active: Language,
  candidates: List<Language>,
  showAdd: Boolean,
  onSwitch: (Language) -> Unit,
  onAdd: (Language) -> Unit,
  onRemove: (Language) -> Unit,
) {
  var addExpanded by remember { mutableStateOf(false) }

  Row(
    modifier = Modifier.horizontalScroll(rememberScrollState()),
    horizontalArrangement = Arrangement.spacedBy(10.dp),
    verticalAlignment = Alignment.CenterVertically,
  ) {
    tabs.forEach { lang ->
      val selected = lang == active
      Text(
        text = lang.localizedName().uppercase(),
        style = MaterialTheme.typography.labelMedium,
        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
        letterSpacing = 0.8.sp,
        color =
          if (selected) {
            MaterialTheme.colorScheme.onSurfaceVariant
          } else {
            MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.55f)
          },
        modifier = Modifier.alignByBaseline().clickable(enabled = !selected) { onSwitch(lang) },
      )

      if (selected && tabs.size > 1) {
        Icon(
          painter = painterResource(id = R.drawable.cancel),
          contentDescription = stringResource(R.string.a11y_remove_target_language),
          tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
          modifier =
            Modifier
              // Align the glyph's visible bottom (cancel spans to 704/960) to the text baseline.
              .alignBy { (it.measuredHeight * 704 / 960f).toInt() }
              .size(16.dp)
              .clickable { onRemove(lang) },
        )
      }
    }

    if (showAdd) {
      Icon(
        painter = painterResource(id = R.drawable.add_material),
        contentDescription = stringResource(R.string.a11y_add_target_language),
        tint = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier =
          Modifier
            // Align the glyph's visible bottom (the plus spans to 760/960) to the text baseline.
            .alignBy { (it.measuredHeight * 760 / 960f).toInt() }
            .size(16.dp)
            .clickable(enabled = candidates.isNotEmpty()) { addExpanded = true },
      )
      DropdownMenu(
        expanded = addExpanded,
        onDismissRequest = { addExpanded = false },
      ) {
        candidates.forEach { lang ->
          DropdownMenuItem(
            text = { Text(lang.localizedName()) },
            onClick = {
              onAdd(lang)
              addExpanded = false
            },
          )
        }
      }
    }
  }
}
