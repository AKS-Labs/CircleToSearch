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

import android.content.Context
import android.graphics.Canvas
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.text.Layout
import android.util.AttributeSet
import android.view.GestureDetector
import android.view.MotionEvent
import android.widget.TextView

/**
 * A selectable [TextView] that draws a subtle dotted underline under given
 * char ranges (`[start, endExclusive)` per `IntRange` built with `start until
 * end`). Used to mark translated words that have alternatives.
 *
 * In [wordTapMode] text selection is turned off and a single tap on a word
 * reports its char offset via [wordTapListener] — the fast path for swapping or
 * looking up one word after another without long-pressing each time.
 */
class DottedUnderlineTextView
  @JvmOverloads
  constructor(
    context: Context,
    attrs: AttributeSet? = null,
  ) : TextView(context, attrs) {
    var underlineRanges: List<IntRange> = emptyList()
      set(value) {
        field = value
        invalidate()
      }

    // The word whose alternatives are open, tinted behind the glyphs so the user
    // sees which word the drawer refers to (the drawer no longer repeats it).
    var highlightRange: IntRange? = null
      set(value) {
        field = value
        invalidate()
      }

    var wordHighlightColor: Int = 0
      set(value) {
        field = value
        invalidate()
      }

    var wordTapListener: ((Int) -> Unit)? = null

    var wordTapMode: Boolean = false
      set(value) {
        if (field == value) return
        field = value
        // Consume taps ourselves and drop the selection UI while the mode is on.
        isClickable = value
        setTextIsSelectable(!value)
        invalidate()
      }

    private val tapDetector =
      GestureDetector(
        context,
        object : GestureDetector.SimpleOnGestureListener() {
          override fun onSingleTapUp(e: MotionEvent): Boolean {
            val l = layout ?: return false
            val x = e.x - totalPaddingLeft + scrollX
            val y = e.y - totalPaddingTop + scrollY
            val line = l.getLineForVertical(y.toInt())
            wordTapListener?.invoke(l.getOffsetForHorizontal(line, x))
            performClick()
            return true
          }
        },
      )

    override fun onTouchEvent(event: MotionEvent): Boolean {
      if (wordTapMode) {
        tapDetector.onTouchEvent(event)
        return true
      }
      return super.onTouchEvent(event)
    }

    private val density = resources.displayMetrics.density
    private val underlinePaint =
      Paint().apply {
        style = Paint.Style.STROKE
        isAntiAlias = true
        strokeWidth = 1.5f * density
        pathEffect = DashPathEffect(floatArrayOf(2f * density, 2.5f * density), 0f)
      }
    private val highlightPaint =
      Paint().apply {
        style = Paint.Style.FILL
        isAntiAlias = true
      }

    private fun drawHighlight(
      canvas: Canvas,
      l: Layout,
      range: IntRange,
    ) {
      val len = text?.length ?: 0
      val start = range.first.coerceIn(0, len)
      val end = (range.last + 1).coerceIn(start, len)
      if (start >= end) return
      highlightPaint.color = wordHighlightColor
      val padLeft = totalPaddingLeft.toFloat()
      val padTop = totalPaddingTop.toFloat()
      val inset = 1.5f * density
      val radius = 4f * density
      val firstLine = l.getLineForOffset(start)
      val lastLine = l.getLineForOffset(end)
      for (line in firstLine..lastLine) {
        val segStart = if (line == firstLine) start else l.getLineStart(line)
        val segEnd = if (line == lastLine) end else l.getLineEnd(line)
        var x1 = l.getPrimaryHorizontal(segStart)
        var x2 = l.getPrimaryHorizontal(segEnd)
        if (x1 > x2) {
          val t = x1
          x1 = x2
          x2 = t
        }
        val top = padTop + l.getLineTop(line) + inset
        val bottom = padTop + l.getLineBottom(line) - inset
        canvas.drawRoundRect(padLeft + x1 - inset, top, padLeft + x2 + inset, bottom, radius, radius, highlightPaint)
      }
    }

    override fun onDraw(canvas: Canvas) {
      layout?.let { l -> highlightRange?.let { drawHighlight(canvas, l, it) } }
      super.onDraw(canvas)
      if (underlineRanges.isEmpty()) return
      val l = layout ?: return
      val len = text?.length ?: 0
      // ~45% of the text colour so it reads as a hint, not an edit.
      underlinePaint.color = (currentTextColor and 0x00FFFFFF) or (0x73 shl 24)
      val padLeft = totalPaddingLeft.toFloat()
      val padTop = totalPaddingTop.toFloat()
      val gap = 2.5f * density

      for (range in underlineRanges) {
        val start = range.first.coerceIn(0, len)
        val end = (range.last + 1).coerceIn(start, len)
        if (start >= end) continue
        val firstLine = l.getLineForOffset(start)
        val lastLine = l.getLineForOffset(end)
        for (line in firstLine..lastLine) {
          val segStart = if (line == firstLine) start else l.getLineStart(line)
          val segEnd = if (line == lastLine) end else l.getLineEnd(line)
          var x1 = l.getPrimaryHorizontal(segStart)
          var x2 = l.getPrimaryHorizontal(segEnd)
          if (x1 > x2) {
            val t = x1
            x1 = x2
            x2 = t
          }
          val y = padTop + l.getLineBaseline(line) + gap
          canvas.drawLine(padLeft + x1, y, padLeft + x2, y, underlinePaint)
        }
      }
    }
  }
