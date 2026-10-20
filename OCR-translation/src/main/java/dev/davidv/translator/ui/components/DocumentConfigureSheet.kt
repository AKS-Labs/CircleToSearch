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

import android.text.format.Formatter
import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageMetadata
import dev.davidv.translator.R
import dev.davidv.translator.TxtLayoutChoice
import kotlin.math.roundToInt

private enum class LayoutMode {
  PRESERVE,
  REFLOW,
  REFLOW_WRAP,
}

@StringRes
private fun LayoutMode.labelRes(): Int =
  when (this) {
    LayoutMode.PRESERVE -> R.string.layout_preserve
    LayoutMode.REFLOW -> R.string.layout_reflow
    LayoutMode.REFLOW_WRAP -> R.string.layout_reflow_wrap
  }

private const val WRAP_MIN = 70
private const val WRAP_MAX = 120
private const val WRAP_DEFAULT = 80

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocumentConfigureSheet(
  fileName: String,
  fileSizeBytes: Long,
  extension: String,
  initialFrom: Language,
  initialTo: Language,
  availableLanguages: List<Language>,
  languageMetadata: Map<Language, LanguageMetadata>,
  defaultTranslatePdfImages: Boolean,
  onDismiss: () -> Unit,
  onTranslate: (from: Language, to: Language, txtLayout: TxtLayoutChoice, translatePdfImages: Boolean) -> Unit,
) {
  val context = LocalContext.current
  val sheetState = rememberModalBottomSheetState()

  var selectedFrom by remember { mutableStateOf(initialFrom) }
  var selectedTo by remember { mutableStateOf(initialTo) }
  var layoutMode by remember { mutableStateOf(LayoutMode.PRESERVE) }
  var wrapColumns by remember { mutableStateOf(WRAP_DEFAULT) }
  var translatePdfImages by remember { mutableStateOf(defaultTranslatePdfImages) }

  val isTxt = extension == "txt"
  val isPdf = extension == "pdf"

  ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
    Column(
      modifier =
        Modifier
          .fillMaxWidth()
          .padding(horizontal = 24.dp)
          .padding(bottom = 8.dp),
      verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
      Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
      ) {
        LanguageSelector(
          selectedLanguage = selectedFrom,
          availableLanguages = availableLanguages,
          languageMetadata = languageMetadata,
          onLanguageSelected = { selectedFrom = it },
          modifier = Modifier.weight(1f),
        )
        LanguageSwapButton(
          onClick = {
            val previousFrom = selectedFrom
            selectedFrom = selectedTo
            selectedTo = previousFrom
          },
        )
        LanguageSelector(
          selectedLanguage = selectedTo,
          availableLanguages = availableLanguages,
          languageMetadata = languageMetadata,
          onLanguageSelected = { selectedTo = it },
          modifier = Modifier.weight(1f),
        )
      }

      Column {
        Text(
          text = fileName,
          style = MaterialTheme.typography.bodyLarge,
          fontWeight = FontWeight.Medium,
        )
        Text(
          text = Formatter.formatShortFileSize(context, fileSizeBytes),
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
      }

      if (isTxt) {
        LayoutModeDropdown(
          selected = layoutMode,
          onSelected = { layoutMode = it },
        )
        if (layoutMode == LayoutMode.REFLOW_WRAP) {
          Column {
            Text(
              text = stringResource(R.string.document_wrap_columns, wrapColumns),
              style = MaterialTheme.typography.bodySmall,
            )
            Slider(
              value = wrapColumns.toFloat(),
              onValueChange = { wrapColumns = (it.roundToInt() / 10) * 10 },
              valueRange = WRAP_MIN.toFloat()..WRAP_MAX.toFloat(),
              steps = (WRAP_MAX - WRAP_MIN) / 10 - 1,
            )
          }
        }
      }

      if (isPdf) {
        Row(
          modifier = Modifier.fillMaxWidth(),
          verticalAlignment = Alignment.CenterVertically,
          horizontalArrangement = Arrangement.SpaceBetween,
        ) {
          Text(stringResource(R.string.document_translate_pdf_images))
          Switch(
            checked = translatePdfImages,
            onCheckedChange = { translatePdfImages = it },
          )
        }
      }

      Button(
        onClick = {
          val txtLayout =
            when (layoutMode) {
              LayoutMode.PRESERVE -> TxtLayoutChoice.Preserve
              LayoutMode.REFLOW -> TxtLayoutChoice.Reflow(wrapColumns = null)
              LayoutMode.REFLOW_WRAP -> TxtLayoutChoice.Reflow(wrapColumns = wrapColumns)
            }
          onTranslate(selectedFrom, selectedTo, txtLayout, translatePdfImages)
        },
        modifier = Modifier.fillMaxWidth(),
      ) {
        Text(stringResource(R.string.translate_text))
      }

      Spacer(modifier = Modifier.navigationBarsPadding())
    }
  }
}

@Composable
private fun LayoutModeDropdown(
  selected: LayoutMode,
  onSelected: (LayoutMode) -> Unit,
) {
  var expanded by remember { mutableStateOf(false) }
  Column {
    OutlinedButton(
      onClick = { expanded = true },
      modifier = Modifier.fillMaxWidth(),
    ) {
      Text(stringResource(selected.labelRes()), modifier = Modifier.weight(1f))
      Text("▾")
    }
    DropdownMenu(
      expanded = expanded,
      onDismissRequest = { expanded = false },
    ) {
      LayoutMode.entries.forEach { mode ->
        DropdownMenuItem(
          text = { Text(stringResource(mode.labelRes())) },
          onClick = {
            onSelected(mode)
            expanded = false
          },
        )
      }
    }
  }
}
