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

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import dev.davidv.translator.Language
import dev.davidv.translator.R
import dev.davidv.translator.TranslatorMessage

// The compact source row under the input field: screen (experimental, gated),
// voice, camera, photos and documents. It absorbs what used to be the FAB's
// image-source bottom sheet plus the standalone voice button.
@Composable
fun InputSourceRow(
  input: String,
  from: Language,
  onMessage: (TranslatorMessage) -> Unit,
  actions: ImageSourceActions,
  onCameraClick: () -> Unit,
  screenTranslateEnabled: Boolean,
  isAutoSource: Boolean,
  modifier: Modifier = Modifier,
) {
  val context = LocalContext.current

  Row(
    modifier = modifier.fillMaxWidth(),
    horizontalArrangement = Arrangement.spacedBy(4.dp),
    verticalAlignment = Alignment.CenterVertically,
  ) {
    if (screenTranslateEnabled) {
      ActionPillButton(
        iconRes = R.drawable.videocam,
        contentDescription = stringResource(R.string.a11y_translate_screen_live),
        onClick = {
          if (isAutoSource) {
            Toast.makeText(context, context.getString(R.string.screen_translate_needs_source), Toast.LENGTH_LONG).show()
          } else {
            actions.startScreen()
          }
        },
      )
    }
    SpeechInputButton(input = input, from = from, onMessage = onMessage)
    ActionPillButton(
      iconRes = R.drawable.camera,
      contentDescription = stringResource(R.string.a11y_source_camera),
      onClick = onCameraClick,
    )
    ActionPillButton(
      iconRes = R.drawable.gallery,
      contentDescription = stringResource(R.string.a11y_source_photos),
      onClick = actions.pickPhotos,
    )
    ActionPillButton(
      iconRes = R.drawable.draft,
      contentDescription = stringResource(R.string.a11y_source_document),
      onClick = actions.pickDocs,
    )
  }
}
