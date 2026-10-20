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

package dev.davidv.translator.overlayChrome

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.LinearLayout
import dev.davidv.translator.R

/** Normalized `[0,1]` region (origin top-left), the shape [LiveScreenTracker.setRegion] wants. */
data class NormalizedRegion(
  val left: Float,
  val top: Float,
  val right: Float,
  val bottom: Float,
)

/**
 * Full-screen region editor: a draggable / resizable rectangle with corner and
 * edge handles (the rest of the screen dimmed), plus a confirm / reset / cancel
 * control row. Reports the chosen area in normalized coordinates so it is
 * independent of the capture resolution. Shared by the overlay services.
 */
class RegionSelectView(
  context: Context,
  private val dpToPx: (Int) -> Int,
  initial: NormalizedRegion?,
  private val onConfirm: (NormalizedRegion) -> Unit,
  private val onReset: () -> Unit,
  private val onCancel: () -> Unit,
) : FrameLayout(context) {
  private val canvas = EditCanvas(context, initial)

  init {
    addView(canvas, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))

    val controls =
      LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
        gravity = Gravity.CENTER
      }
    controls.addView(controlPill(R.drawable.cancel, context.getString(R.string.a11y_cancel)) { onCancel() })
    controls.addView(controlPill(R.drawable.delete, context.getString(R.string.a11y_region_reset)) { onReset() })
    controls.addView(
      controlPill(R.drawable.check_plain, context.getString(R.string.a11y_region_confirm)) { onConfirm(canvas.normalized()) },
    )
    addView(
      controls,
      LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT).apply {
        gravity = Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL
        bottomMargin = dpToPx(40)
      },
    )
  }

  private fun controlPill(
    iconRes: Int,
    description: String,
    onClick: () -> Unit,
  ): View {
    val size = dpToPx(52)
    val pad = dpToPx(13)
    val icon =
      ImageView(context).apply {
        setImageResource(iconRes)
        setColorFilter(Color.WHITE)
        setPadding(pad, pad, pad, pad)
        contentDescription = description
        setOnClickListener { onClick() }
      }
    val pill = OverlayChromeFactory.makePill(context, dpToPx, icon)
    pill.layoutParams = LinearLayout.LayoutParams(size, size).apply { marginEnd = dpToPx(12) }
    return pill
  }

  /** The drawing + touch surface. Coordinates are pixels within this view (full screen). */
  private inner class EditCanvas(
    context: Context,
    initial: NormalizedRegion?,
  ) : View(context) {
    private val rect = RectF()
    private var pendingInit: NormalizedRegion? = initial
    private var mode = Mode.NONE
    private var lastX = 0f
    private var lastY = 0f
    private val handleR = dpToPx(9).toFloat()
    private val hitR = dpToPx(22).toFloat()
    private val minSize = dpToPx(48).toFloat()

    private val dimPaint =
      Paint().apply {
        color = Color.parseColor("#99000000")
        style = Paint.Style.FILL
      }
    private val borderPaint =
      Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.parseColor("#F1AB7F")
        style = Paint.Style.STROKE
        strokeWidth = dpToPx(2).toFloat()
        pathEffect = DashPathEffect(floatArrayOf(dpToPx(8).toFloat(), dpToPx(6).toFloat()), 0f)
      }
    private val handleFill =
      Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.WHITE
        style = Paint.Style.FILL
      }
    private val handleStroke =
      Paint(Paint.ANTI_ALIAS_FLAG).apply {
        color = Color.parseColor("#F1AB7F")
        style = Paint.Style.STROKE
        strokeWidth = dpToPx(2).toFloat()
      }

    override fun onSizeChanged(
      w: Int,
      h: Int,
      oldw: Int,
      oldh: Int,
    ) {
      val init = pendingInit
      if (init != null) {
        rect.set(init.left * w, init.top * h, init.right * w, init.bottom * h)
      } else {
        rect.set(w * 0.15f, h * 0.25f, w * 0.85f, h * 0.7f)
      }
      pendingInit = null
    }

    fun normalized(): NormalizedRegion {
      val w = width.coerceAtLeast(1).toFloat()
      val h = height.coerceAtLeast(1).toFloat()
      return NormalizedRegion(
        (rect.left / w).coerceIn(0f, 1f),
        (rect.top / h).coerceIn(0f, 1f),
        (rect.right / w).coerceIn(0f, 1f),
        (rect.bottom / h).coerceIn(0f, 1f),
      )
    }

    override fun onDraw(c: Canvas) {
      // Dim everything outside the rect (even-odd: whole view minus the rect).
      val dim =
        Path().apply {
          fillType = Path.FillType.EVEN_ODD
          addRect(0f, 0f, width.toFloat(), height.toFloat(), Path.Direction.CW)
          addRect(rect, Path.Direction.CW)
        }
      c.drawPath(dim, dimPaint)
      c.drawRect(rect, borderPaint)
      for ((hx, hy) in handlePoints()) {
        c.drawCircle(hx, hy, handleR, handleFill)
        c.drawCircle(hx, hy, handleR, handleStroke)
      }
    }

    private fun handlePoints(): List<Pair<Float, Float>> {
      val cx = (rect.left + rect.right) / 2
      val cy = (rect.top + rect.bottom) / 2
      return listOf(
        rect.left to rect.top,
        cx to rect.top,
        rect.right to rect.top,
        rect.left to cy,
        rect.right to cy,
        rect.left to rect.bottom,
        cx to rect.bottom,
        rect.right to rect.bottom,
      )
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
      when (event.action) {
        MotionEvent.ACTION_DOWN -> {
          mode = pickMode(event.x, event.y)
          lastX = event.x
          lastY = event.y
          return mode != Mode.NONE
        }
        MotionEvent.ACTION_MOVE -> {
          val dx = event.x - lastX
          val dy = event.y - lastY
          applyDrag(dx, dy)
          lastX = event.x
          lastY = event.y
          invalidate()
          return true
        }
      }
      return false
    }

    private fun pickMode(
      x: Float,
      y: Float,
    ): Mode {
      val near = { a: Float, b: Float -> Math.abs(a - b) <= hitR }
      val onLeft = near(x, rect.left)
      val onRight = near(x, rect.right)
      val onTop = near(y, rect.top)
      val onBottom = near(y, rect.bottom)
      val inX = x >= rect.left - hitR && x <= rect.right + hitR
      val inY = y >= rect.top - hitR && y <= rect.bottom + hitR
      return when {
        onLeft && onTop -> Mode.TL
        onRight && onTop -> Mode.TR
        onLeft && onBottom -> Mode.BL
        onRight && onBottom -> Mode.BR
        onLeft && inY -> Mode.L
        onRight && inY -> Mode.R
        onTop && inX -> Mode.T
        onBottom && inX -> Mode.B
        rect.contains(x, y) -> Mode.MOVE
        else -> Mode.NONE
      }
    }

    private fun applyDrag(
      dx: Float,
      dy: Float,
    ) {
      val w = width.toFloat()
      val h = height.toFloat()
      when (mode) {
        Mode.MOVE -> {
          val nx = (rect.left + dx).coerceIn(0f, w - rect.width())
          val ny = (rect.top + dy).coerceIn(0f, h - rect.height())
          rect.offsetTo(nx, ny)
        }
        Mode.L, Mode.TL, Mode.BL -> rect.left = (rect.left + dx).coerceIn(0f, rect.right - minSize)
        Mode.R, Mode.TR, Mode.BR -> rect.right = (rect.right + dx).coerceIn(rect.left + minSize, w)
        else -> {}
      }
      when (mode) {
        Mode.T, Mode.TL, Mode.TR -> rect.top = (rect.top + dy).coerceIn(0f, rect.bottom - minSize)
        Mode.B, Mode.BL, Mode.BR -> rect.bottom = (rect.bottom + dy).coerceIn(rect.top + minSize, h)
        else -> {}
      }
    }
  }

  private enum class Mode { NONE, MOVE, L, R, T, B, TL, TR, BL, BR }
}
