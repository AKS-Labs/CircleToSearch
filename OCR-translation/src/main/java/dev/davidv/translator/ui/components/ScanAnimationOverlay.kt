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

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import uniffi.translator_core.OrientedRect
import kotlin.math.abs

/**
 * Detected text regions of one image (oriented rects in image-pixel space, `imageWidth` x
 * `imageHeight`), surfaced by the detect pass before recognition runs so the UI can show progress.
 */
data class DetectedRegions(
  val imageWidth: Int,
  val imageHeight: Int,
  val boxes: List<OrientedRect>,
)

private const val BASE_ALPHA = 0.14f

/**
 * Progress indicator for the detect→recognize→translate pass: transparent-white pills over each
 * detected region. A wave sweeps top-to-bottom *once* (brightening the pills it passes), then the
 * pills settle into a gentle synchronized pulse — a determinate-looking pass into an
 * indeterminate "work still happening, unknown how long" state, instead of a confusing re-loop.
 */
@Composable
fun ScanAnimationOverlay(
  regions: DetectedRegions,
  modifier: Modifier = Modifier,
) {
  val sweep = remember { Animatable(0f) }
  // `breathe` is driven from 0 only after the sweep finishes, so the pulse begins at exactly the
  // sweep's resting opacity (`BASE_ALPHA`) and rises from there — no jump into a random phase.
  val breathe = remember { Animatable(0f) }
  var sweepDone by remember { mutableStateOf(false) }
  LaunchedEffect(Unit) {
    sweep.animateTo(1f, tween(durationMillis = 1300, easing = LinearEasing))
    sweepDone = true
    breathe.animateTo(
      1f,
      infiniteRepeatable(
        animation = tween(durationMillis = 1100, easing = FastOutSlowInEasing),
        repeatMode = RepeatMode.Reverse,
      ),
    )
  }

  Canvas(modifier = modifier) {
    if (regions.boxes.isEmpty()) return@Canvas
    val t = fitTransform(size.width, size.height, regions.imageWidth, regions.imageHeight)
    if (!sweepDone) {
      val waveY = sweep.value * size.height
      val halfWidth = size.height * 0.06f
      for (box in regions.boxes) {
        val falloff = (1f - abs(t.mapY(box.cy) - waveY) / halfWidth).coerceAtLeast(0f)
        drawOrientedPill(box, t, Color(1f, 1f, 1f, BASE_ALPHA + 0.30f * falloff))
      }
    } else {
      val alpha = BASE_ALPHA + 0.12f * breathe.value
      for (box in regions.boxes) {
        drawOrientedPill(box, t, Color(1f, 1f, 1f, alpha))
      }
    }
  }
}
