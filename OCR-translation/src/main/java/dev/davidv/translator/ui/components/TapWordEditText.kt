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
import android.graphics.Rect
import android.util.AttributeSet
import android.view.GestureDetector
import android.view.MotionEvent
import android.view.inputmethod.InputMethodManager
import android.widget.EditText

/**
 * An [EditText] that, in [wordTapMode], stops being editable — keyboard hidden,
 * cursor gone, taps no longer move the caret — and instead reports the char
 * offset of a tapped word via [wordTapListener], for tap-to-look-up. Toggling the
 * mode off restores normal editing.
 */
class TapWordEditText
  @JvmOverloads
  constructor(
    context: Context,
    attrs: AttributeSet? = null,
  ) : EditText(context, attrs) {
    var wordTapListener: ((Int) -> Unit)? = null

    /**
     * Reports the caret line's bounds, in this view's own coordinates, whenever the
     * caret moves. The view is measured [ViewGroup.LayoutParams.WRAP_CONTENT] inside a
     * Compose scroll container, so it never scrolls itself and its native
     * bringPointIntoView never fires — the container has to do the scrolling instead.
     */
    var cursorRectListener: ((Rect) -> Unit)? = null

    private var cursorRectPending = false

    var wordTapMode: Boolean = false
      set(value) {
        if (field == value) return
        field = value
        isFocusableInTouchMode = !value
        isCursorVisible = !value
        if (value) {
          clearFocus()
          val imm = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
          imm?.hideSoftInputFromWindow(windowToken, 0)
        }
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

    override fun onSelectionChanged(
      selStart: Int,
      selEnd: Int,
    ) {
      super.onSelectionChanged(selStart, selEnd)
      cursorRectPending = true
      if (!isLayoutRequested) {
        emitCursorRect()
      }
    }

    override fun onLayout(
      changed: Boolean,
      left: Int,
      top: Int,
      right: Int,
      bottom: Int,
    ) {
      super.onLayout(changed, left, top, right, bottom)
      if (cursorRectPending) {
        emitCursorRect()
      }
    }

    private fun emitCursorRect() {
      val listener = cursorRectListener ?: return
      val l = layout ?: return
      cursorRectPending = false
      val line = l.getLineForOffset(selectionEnd)
      listener(
        Rect(
          0,
          totalPaddingTop + l.getLineTop(line),
          width,
          totalPaddingTop + l.getLineBottom(line),
        ),
      )
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
      if (wordTapMode) {
        tapDetector.onTouchEvent(event)
        return true
      }
      return super.onTouchEvent(event)
    }
  }
