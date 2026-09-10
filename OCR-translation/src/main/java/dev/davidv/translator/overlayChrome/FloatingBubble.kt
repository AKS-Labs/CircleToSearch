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

import android.animation.ValueAnimator
import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.widget.FrameLayout
import android.widget.ImageView
import dev.davidv.translator.R

/**
 * Draggable, auto-semi-hidden control bubble for the overlay services. Lives in
 * its own touchable overlay window (the render overlay stays pass-through), so a
 * single implementation serves whichever service supplies the [windowManager] and
 * [overlayType]. Idle behaviour: snap to the nearest horizontal edge, then slide
 * half off-screen and dim; any touch restores it, a tap (no drag) fires [onTap].
 */
class FloatingBubble(
  private val context: Context,
  private val windowManager: WindowManager,
  private val overlayType: Int,
  private val dpToPx: (Int) -> Int,
  private val onTap: () -> Unit,
) {
  private val handler = Handler(Looper.getMainLooper())
  private var view: View? = null
  private var params: WindowManager.LayoutParams? = null
  private var hidden = false
  private var snapAnimator: ValueAnimator? = null

  private val idleRunnable = Runnable { hideToEdge() }

  fun show() {
    if (view != null) return

    val size = dpToPx(48)
    val pad = dpToPx(10)
    val icon =
      ImageView(context).apply {
        setImageResource(R.drawable.ic_translate_button)
        setPadding(pad, pad, pad, pad)
      }
    val bg =
      GradientDrawable().apply {
        shape = GradientDrawable.OVAL
        setColor(Color.parseColor("#F1AB7F"))
      }
    val container =
      FrameLayout(context).apply {
        background = bg
        addView(icon, FrameLayout.LayoutParams(size, size))
      }

    val lp =
      WindowManager.LayoutParams(
        size,
        size,
        overlayType,
        // NO_LIMITS so the idle bubble can slide half past the screen edge —
        // without it the window is clamped on-screen and only dims in place.
        WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
          WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
        PixelFormat.TRANSLUCENT,
      )
    lp.gravity = Gravity.TOP or Gravity.START
    lp.x = screenWidth() - size - dpToPx(8)
    lp.y = dpToPx(120)
    params = lp

    var initialX = 0
    var initialY = 0
    var touchX = 0f
    var touchY = 0f
    var moved = false

    container.setOnTouchListener { _, event ->
      when (event.action) {
        MotionEvent.ACTION_DOWN -> {
          cancelIdle()
          restore()
          initialX = lp.x
          initialY = lp.y
          touchX = event.rawX
          touchY = event.rawY
          moved = false
          true
        }
        MotionEvent.ACTION_MOVE -> {
          val dx = (event.rawX - touchX).toInt()
          val dy = (event.rawY - touchY).toInt()
          if (Math.abs(dx) > dpToPx(4) || Math.abs(dy) > dpToPx(4)) moved = true
          lp.x = initialX + dx
          lp.y = initialY + dy
          windowManager.updateViewLayout(container, lp)
          true
        }
        MotionEvent.ACTION_UP -> {
          if (!moved) {
            onTap()
          } else {
            snapToEdge()
          }
          scheduleIdle()
          true
        }
        else -> false
      }
    }

    windowManager.addView(container, lp)
    view = container
    scheduleIdle()
  }

  fun remove() {
    cancelIdle()
    snapAnimator?.cancel()
    view?.let { runCatching { windowManager.removeView(it) } }
    view = null
    params = null
  }

  /** True when the bubble sits on the right half of the screen — the menu opens on
   *  that side so it doesn't overlap the bubble. */
  fun isOnRightSide(): Boolean {
    val lp = params ?: return true
    return lp.x + lp.width / 2 >= screenWidth() / 2
  }

  /** The bubble's current top in px, for vertically aligning the menu near it. */
  fun anchorTop(): Int = params?.y ?: dpToPx(120)

  /** Hide/show the bubble view without losing its dragged position — used while
   *  the expanded toolbar is open (the toolbar is the un-collapsed form). */
  fun setShown(shown: Boolean) {
    view?.visibility = if (shown) View.VISIBLE else View.GONE
  }

  /** Bring the bubble fully back on-screen at full opacity (e.g. when the menu opens). */
  fun restore() {
    val v = view ?: return
    val lp = params ?: return
    if (!hidden) return
    hidden = false
    v.alpha = 1f
    lp.x = lp.x.coerceIn(0, maxX())
    windowManager.updateViewLayout(v, lp)
  }

  private fun scheduleIdle() {
    cancelIdle()
    handler.postDelayed(idleRunnable, IDLE_DELAY_MS)
  }

  private fun cancelIdle() = handler.removeCallbacks(idleRunnable)

  private fun snapToEdge() {
    val v = view ?: return
    val lp = params ?: return
    val target = if (lp.x + lp.width / 2 < screenWidth() / 2) 0 else maxX()
    lp.y = lp.y.coerceIn(0, maxY())
    animateX(v, lp, target)
  }

  private fun hideToEdge() {
    val v = view ?: return
    val lp = params ?: return
    hidden = true
    val onRight = lp.x + lp.width / 2 >= screenWidth() / 2
    val target = if (onRight) screenWidth() - lp.width / 2 else -lp.width / 2
    v.alpha = 0.6f
    animateX(v, lp, target)
  }

  private fun animateX(
    v: View,
    lp: WindowManager.LayoutParams,
    target: Int,
  ) {
    snapAnimator?.cancel()
    val from = lp.x
    if (from == target) return
    snapAnimator =
      ValueAnimator.ofInt(from, target).apply {
        duration = 180
        addUpdateListener {
          lp.x = it.animatedValue as Int
          runCatching { windowManager.updateViewLayout(v, lp) }
        }
        start()
      }
  }

  private fun screenWidth(): Int = context.resources.displayMetrics.widthPixels

  private fun screenHeight(): Int = context.resources.displayMetrics.heightPixels

  private fun maxX(): Int = (screenWidth() - (params?.width ?: 0) - dpToPx(8)).coerceAtLeast(0)

  private fun maxY(): Int = (screenHeight() - (params?.height ?: 0) - dpToPx(8)).coerceAtLeast(0)

  companion object {
    private const val IDLE_DELAY_MS = 1250L
  }
}
