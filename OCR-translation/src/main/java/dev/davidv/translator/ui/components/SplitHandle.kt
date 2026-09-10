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

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.davidv.translator.R

/**
 * Drag band between the input and output cards. Compose clips hit-testing to a
 * node's reported bounds, so the band cannot overhang the cards the way a View
 * TouchDelegate would — the touch target is exactly this height.
 */
@Composable
fun SplitHandle(
  onDrag: (Float) -> Unit,
  modifier: Modifier = Modifier,
) {
  val description = stringResource(R.string.a11y_resize_split)
  Box(
    modifier =
      modifier
        .fillMaxWidth()
        .height(24.dp)
        .semantics { contentDescription = description }
        .draggable(
          orientation = Orientation.Vertical,
          state = rememberDraggableState { onDrag(it) },
        ),
    contentAlignment = Alignment.Center,
  ) {
    Box(
      modifier =
        Modifier
          .width(64.dp)
          .height(4.dp)
          .clip(RoundedCornerShape(2.dp))
          .background(MaterialTheme.colorScheme.outlineVariant),
    )
  }
}
