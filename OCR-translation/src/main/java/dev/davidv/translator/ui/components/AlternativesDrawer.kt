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
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import dev.davidv.translator.R
import kotlinx.coroutines.delay

private val WS = Regex("\\s+")

/**
 * A window of `[before]` words, the word at [boldIndex] in bold, and `[after]`
 * words — with ellipses when the sentence extends past the window. Only the
 * single swapped word is bolded; any downstream cascade is shown plain.
 */
private fun window(
  sentence: String,
  boldIndex: Int,
  before: Int = 3,
  after: Int = 3,
): AnnotatedString {
  val words = sentence.split(WS).filter { it.isNotBlank() }
  if (boldIndex !in words.indices) return AnnotatedString(sentence)
  val start = (boldIndex - before).coerceAtLeast(0)
  val end = (boldIndex + after).coerceAtMost(words.size - 1)
  return buildAnnotatedString {
    if (start > 0) append("… ")
    for (i in start until boldIndex) append(words[i] + " ")
    withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(words[boldIndex]) }
    for (i in boldIndex + 1..end) append(" " + words[i])
    if (end < words.size - 1) append(" …")
  }
}

/** The last `[before]` words of the confirmed prefix, shown before anything is
 * typed so the box has context instead of a bare ellipsis. */
private fun headContext(
  head: String,
  before: Int = 3,
): AnnotatedString {
  val words = head.split(WS).filter { it.isNotBlank() }
  val start = (words.size - before).coerceAtLeast(0)
  return buildAnnotatedString {
    if (start > 0) append("… ")
    for (i in start until words.size) append(words[i] + " ")
  }
}

/**
 * A non-blocking bottom drawer for the tapped word. It sits over the bottom of
 * the screen without a scrim, so the sentence stays visible and other words stay
 * tappable — tapping another word just re-points the drawer. Each alternative is
 * run to the end of the sentence (a steer with the forced prefix) so the option
 * shows the *actual* re-decoded sentence, windowed around the swap. Tapping an
 * option commits it; "Type your own…" opens a box to force a hand-typed word.
 * [onCommit] takes the forced prefix (confirmed text up to and including the new
 * word); the close button dismisses without leaving the mode.
 */
@Composable
fun AlternativesDrawer(
  target: AlternativesTarget,
  fullText: String,
  steerPreview: suspend (String) -> String?,
  onCommit: (String) -> Unit,
  onDismiss: () -> Unit,
) {
  val begin = target.wordBegin.coerceIn(0, fullText.length)
  val head = fullText.substring(0, begin)
  // The swapped word sits right after the confirmed prefix, so its word index
  // (in both the original and every re-decoded sentence) is the prefix's word
  // count.
  val boldIndex = head.split(WS).filter { it.isNotBlank() }.size

  var showCustom by remember(target) { mutableStateOf(false) }

  // option text -> re-decoded sentence (null while loading / on failure).
  val previews = remember(target) { mutableStateMapOf<String, String?>() }
  LaunchedEffect(target) {
    for (option in target.options) {
      previews[option.text] = steerPreview(head + option.text)
    }
  }

  Popup(
    alignment = Alignment.BottomCenter,
    properties = PopupProperties(focusable = false),
  ) {
    Surface(
      modifier = Modifier.fillMaxWidth(),
      shape = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp),
      color = MaterialTheme.colorScheme.surface,
      tonalElevation = 8.dp,
      shadowElevation = 8.dp,
    ) {
      Column(
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp).fillMaxWidth(),
      ) {
        Row(
          modifier = Modifier.fillMaxWidth(),
          horizontalArrangement = Arrangement.End,
        ) {
          IconButton(onClick = onDismiss, modifier = Modifier.size(28.dp)) {
            Icon(
              painterResource(id = R.drawable.cancel),
              contentDescription = stringResource(R.string.a11y_close_alternatives),
              tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
          }
        }
        Column(
          modifier = Modifier.heightIn(max = 280.dp).verticalScroll(rememberScrollState()),
          verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
          for (option in target.options) {
            val preview = previews[option.text]
            Text(
              // Before the steer returns, show the swap in place with no tail.
              text = window(preview ?: (head + option.text), boldIndex),
              style = MaterialTheme.typography.bodyLarge,
              color =
                if (preview == null) {
                  MaterialTheme.colorScheme.onSurfaceVariant
                } else {
                  MaterialTheme.colorScheme.onSurface
                },
              modifier =
                Modifier
                  .fillMaxWidth()
                  .clickable { onCommit(head + option.text) }
                  .padding(vertical = 12.dp),
            )
          }
          Text(
            text = stringResource(R.string.alt_custom_entry),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.primary,
            textAlign = TextAlign.Center,
            modifier =
              Modifier
                .fillMaxWidth()
                .clickable { showCustom = true }
                .padding(vertical = 12.dp),
          )
        }
      }
    }
  }

  if (showCustom) {
    CustomAlternativeDialog(
      head = head,
      boldIndex = boldIndex,
      steerPreview = steerPreview,
      onCommit = onCommit,
      onDismiss = { showCustom = false },
    )
  }
}

/**
 * A small modal to force a hand-typed word at the tapped position. The preview
 * re-decodes the sentence from `head + typed` (debounced) so the user sees the
 * actual result before committing. OK forces exactly what was typed.
 */
@Composable
private fun CustomAlternativeDialog(
  head: String,
  boldIndex: Int,
  steerPreview: suspend (String) -> String?,
  onCommit: (String) -> Unit,
  onDismiss: () -> Unit,
) {
  var typed by remember { mutableStateOf("") }
  var preview by remember { mutableStateOf<String?>(null) }
  LaunchedEffect(typed) {
    if (typed.isBlank()) {
      preview = null
      return@LaunchedEffect
    }
    preview = null
    delay(300)
    preview = steerPreview(head + typed)
  }

  Dialog(onDismissRequest = onDismiss) {
    Surface(
      shape = MaterialTheme.shapes.large,
      color = MaterialTheme.colorScheme.surface,
      tonalElevation = 6.dp,
    ) {
      Column(
        modifier = Modifier.padding(16.dp).fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(12.dp),
      ) {
        OutlinedTextField(
          value = typed,
          onValueChange = { typed = it },
          singleLine = true,
          label = { Text(stringResource(R.string.alt_custom_label)) },
          modifier = Modifier.fillMaxWidth(),
        )
        Text(
          text = stringResource(R.string.alt_custom_preview_label),
          style = MaterialTheme.typography.labelMedium,
          color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
          text =
            if (typed.isBlank()) {
              headContext(head)
            } else {
              window(preview ?: (head + typed), boldIndex)
            },
          style = MaterialTheme.typography.bodyLarge,
          color =
            if (preview == null) {
              MaterialTheme.colorScheme.onSurfaceVariant
            } else {
              MaterialTheme.colorScheme.onSurface
            },
        )
        Row(
          modifier = Modifier.fillMaxWidth(),
          horizontalArrangement = Arrangement.End,
        ) {
          TextButton(onClick = onDismiss) {
            Text(stringResource(R.string.common_cancel))
          }
          TextButton(
            onClick = { onCommit(head + typed) },
            enabled = typed.isNotBlank(),
          ) {
            Text(stringResource(R.string.common_ok))
          }
        }
      }
    }
  }
}
