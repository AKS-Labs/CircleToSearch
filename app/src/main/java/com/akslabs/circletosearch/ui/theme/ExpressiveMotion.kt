package com.akslabs.circletosearch.ui.theme

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.SpringSpec
import androidx.compose.animation.core.spring
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import kotlinx.coroutines.launch

/**
 * Google Material 3 Expressive Motion Physics System.
 *
 * M3 Expressive shifts from static duration/cubic-bezier curves to physical spring forces.
 * Expressive motion features intentional overshoot, snappy acceleration, and organic bounce,
 * creating interfaces that feel alive, responsive, and tactile.
 */
object M3ExpressiveSpring {
  /**
   * Expressive Bouncy (Material 3 Expressive Default):
   * Noticeable overshoot and pleasant bounce. Used for hero components, FABs, expanding cards,
   * step transitions, and successful completion states.
   */
  val Bouncy: SpringSpec<Float> = spring(
    dampingRatio = 0.54f,
    stiffness = 450f
  )

  /**
   * Expressive Snappy:
   * Low damping with quick settling. Used for interactive controls, toggles, chips,
   * and immediate user-driven feedback.
   */
  val Snappy: SpringSpec<Float> = spring(
    dampingRatio = 0.72f,
    stiffness = 850f
  )

  /**
   * Expressive Gentle:
   * Smooth, organic motion with minimal overshoot. Used for background shapes,
   * subtle floating items, and continuous breathing animations.
   */
  val Gentle: SpringSpec<Float> = spring(
    dampingRatio = 0.88f,
    stiffness = 260f
  )

  /**
   * Expressive Hero:
   * Majestic spatial entry with prominent spring physics for large full-screen elements,
   * modals, and primary onboarding focal graphics.
   */
  val Hero: SpringSpec<Float> = spring(
    dampingRatio = 0.58f,
    stiffness = 200f
  )

  /**
   * Fast Spatial:
   * High stiffness for immediate spatial responses like drag release and magnetic snaps.
   */
  val FastSpatial: SpringSpec<Float> = spring(
    dampingRatio = 0.65f,
    stiffness = 1200f
  )

  /**
   * Damping & Stiffness presets corresponding to Material 3 Expressive tokens.
   */
  const val DampingBouncy = 0.54f
  const val DampingSnappy = 0.72f
  const val DampingGentle = 0.88f

  const val StiffnessLow = 200f
  const val StiffnessMedium = 450f
  const val StiffnessHigh = 850f
  const val StiffnessExtraHigh = 1200f
}

/**
 * Material 3 Expressive Press modifier.
 *
 * Applies realistic spring physics: when pressed down, the element squashes smoothly (e.g. to 0.94f);
 * upon release, it expands back with an expressive spring overshoot bounce.
 */
fun Modifier.m3ExpressivePress(
  pressedScale: Float = 0.93f,
  enabled: Boolean = true,
  onClick: (() -> Unit)? = null
): Modifier = composed {
  if (!enabled) return@composed this

  val scale = remember { Animatable(1f) }
  val scope = rememberCoroutineScope()

  val clickModifier = if (onClick != null) {
    Modifier.clickable(
      interactionSource = remember { MutableInteractionSource() },
      indication = null,
      onClick = onClick
    )
  } else Modifier

  this
    .then(clickModifier)
    .graphicsLayer {
      scaleX = scale.value
      scaleY = scale.value
    }
    .pointerInput(enabled) {
      while (true) {
        awaitPointerEventScope {
          awaitFirstDown(requireUnconsumed = false)
          scope.launch {
            scale.animateTo(
              targetValue = pressedScale,
              animationSpec = spring(
                dampingRatio = Spring.DampingRatioNoBouncy,
                stiffness = Spring.StiffnessMedium
              )
            )
          }

          waitForUpOrCancellation()
          scope.launch {
            scale.animateTo(
              targetValue = 1f,
              animationSpec = M3ExpressiveSpring.Bouncy
            )
          }
        }
      }
    }
}

/**
 * Material 3 Expressive Interactive 3D tilt modifier with spring physics recovery.
 */
fun Modifier.m3ExpressiveTilt(
  maxTiltDegrees: Float = 8f
): Modifier = composed {
  val tiltX = remember { Animatable(0f) }
  val tiltY = remember { Animatable(0f) }
  val scope = rememberCoroutineScope()

  this
    .graphicsLayer {
      rotationX = tiltX.value
      rotationY = tiltY.value
      cameraDistance = 16f * density
    }
    .pointerInput(Unit) {
      while (true) {
        awaitPointerEventScope {
          val down = awaitFirstDown(requireUnconsumed = false)
          val centerW = size.width / 2f
          val centerH = size.height / 2f
          val dx = (down.position.x - centerW) / centerW
          val dy = (down.position.y - centerH) / centerH

          scope.launch {
            tiltY.animateTo(dx * maxTiltDegrees, M3ExpressiveSpring.Snappy)
            tiltX.animateTo(-dy * maxTiltDegrees, M3ExpressiveSpring.Snappy)
          }

          waitForUpOrCancellation()
          scope.launch {
            tiltY.animateTo(0f, M3ExpressiveSpring.Bouncy)
            tiltX.animateTo(0f, M3ExpressiveSpring.Bouncy)
          }
        }
      }
    }
}
