package dev.davidv.translator.assistantOverlay

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout

class BorderWaveView private constructor(
  context: Context,
  private val borderColor: Int,
  private val gradientWidthPx: Int,
) : FrameLayout(context) {
  companion object {
    private const val COLOR = "#E08050"
    private const val GRADIENT_DP = 6

    fun create(context: Context): BorderWaveView {
      val density = context.resources.displayMetrics.density
      return BorderWaveView(context, Color.parseColor(COLOR), (GRADIENT_DP * density).toInt().coerceAtLeast(1))
    }
  }

  private val topEdge = View(context)
  private val rightEdge = View(context)
  private val bottomEdge = View(context)
  private val leftEdge = View(context)

  init {
    setWillNotDraw(true)
    clipChildren = false
    clipToPadding = false

    addView(
      topEdge,
      LayoutParams(LayoutParams.MATCH_PARENT, gradientWidthPx, Gravity.TOP),
    )
    addView(
      rightEdge,
      LayoutParams(gradientWidthPx, LayoutParams.MATCH_PARENT, Gravity.END),
    )
    addView(
      bottomEdge,
      LayoutParams(LayoutParams.MATCH_PARENT, gradientWidthPx, Gravity.BOTTOM),
    )
    addView(
      leftEdge,
      LayoutParams(gradientWidthPx, LayoutParams.MATCH_PARENT, Gravity.START),
    )

    topEdge.background = edgeDrawable(GradientDrawable.Orientation.TOP_BOTTOM)
    rightEdge.background = edgeDrawable(GradientDrawable.Orientation.RIGHT_LEFT)
    bottomEdge.background = edgeDrawable(GradientDrawable.Orientation.BOTTOM_TOP)
    leftEdge.background = edgeDrawable(GradientDrawable.Orientation.LEFT_RIGHT)

    visibility = GONE
  }

  fun startAnimation() {
    visibility = VISIBLE
  }

  fun stopAnimation() {
    visibility = GONE
  }

  private fun edgeDrawable(orientation: GradientDrawable.Orientation): GradientDrawable =
    GradientDrawable(
      orientation,
      intArrayOf(withAlpha(255), withAlpha(0)),
    ).apply {
      shape = GradientDrawable.RECTANGLE
    }

  private fun withAlpha(alpha: Int): Int = (borderColor and 0x00FFFFFF) or (alpha shl 24)
}
