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

import android.graphics.Bitmap
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.unit.dp

/**
 * State for a frozen-screenshot selection surface, shared by the assistant session and the
 * accessibility overlay's select-text mode: the screenshot with the scan animation while OCR runs,
 * then the translated image with the interactive word-selection overlay, and the flip to the
 * original. Bitmap lifetimes stay with the owner; [clear] drops references without recycling.
 */
class SelectionSurfaceState {
  val display = mutableStateOf<Bitmap?>(null)
  val original = mutableStateOf<Bitmap?>(null)
  val selection = mutableStateOf<ImageWordSelection?>(null)
  val processing = mutableStateOf(false)
  val regions = mutableStateOf<DetectedRegions?>(null)
  val showOriginal = mutableStateOf(false)

  fun showProcessing(source: Bitmap) {
    original.value = source
    display.value = source
    selection.value = null
    regions.value = null
    showOriginal.value = false
    processing.value = true
  }

  fun showResult(
    translated: Bitmap,
    source: Bitmap,
    words: ImageWordSelection,
  ) {
    processing.value = false
    regions.value = null
    original.value = source
    selection.value = words
    display.value = translated
  }

  fun toggleShowOriginal(): Boolean {
    val show = !showOriginal.value
    showOriginal.value = show
    return show
  }

  fun clear() {
    display.value = null
    original.value = null
    selection.value = null
    regions.value = null
    processing.value = false
    showOriginal.value = false
  }
}

@Composable
fun SelectionSurface(state: SelectionSurfaceState) {
  val display = state.display.value ?: return
  val configuration = LocalConfiguration.current
  ImageDisplaySection(
    displayImage = display,
    originalImage = state.original.value,
    showOriginal = state.showOriginal.value,
    isProcessing = state.processing.value,
    detectedRegions = state.regions.value,
    wordSelection = state.selection.value,
    maxHeight = configuration.screenHeightDp.dp,
    modifier = Modifier.fillMaxWidth(),
  )
}
