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

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import uniffi.translator_core.OrientedRect
import kotlin.math.min

/** Image->view transform for a bitmap drawn with ContentScale.Fit inside `viewSize`. */
internal data class FitTransform(val scale: Float, val offsetX: Float, val offsetY: Float) {
  fun mapX(x: Float) = offsetX + x * scale

  fun mapY(y: Float) = offsetY + y * scale

  fun unmapX(x: Float) = (x - offsetX) / scale

  fun unmapY(y: Float) = (y - offsetY) / scale
}

internal fun fitTransform(
  viewW: Float,
  viewH: Float,
  imageW: Int,
  imageH: Int,
): FitTransform {
  val scale = min(viewW / imageW, viewH / imageH)
  return FitTransform(scale, (viewW - imageW * scale) / 2f, (viewH - imageH * scale) / 2f)
}

/** Draw an oriented rect as a rounded pill, mapped through `t` and rotated to its angle. */
internal fun DrawScope.drawOrientedPill(
  rect: OrientedRect,
  t: FitTransform,
  color: Color,
) {
  val cx = t.mapX(rect.cx)
  val cy = t.mapY(rect.cy)
  val w = rect.width * t.scale
  val h = rect.height * t.scale
  rotate(degrees = Math.toDegrees(rect.angleRadians.toDouble()).toFloat(), pivot = Offset(cx, cy)) {
    drawRoundRect(
      color = color,
      topLeft = Offset(cx - w / 2f, cy - h / 2f),
      size = Size(w, h),
      cornerRadius = CornerRadius(h / 2f, h / 2f),
    )
  }
}
