package com.akslabs.circletosearch.ui.components

import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * Background presets representing different Google Pixel system & application onboarding aesthetics.
 */
enum class BackgroundPreset {
  PixelSetup,     // Classic Pixel Setup with organic wavy scallop loop & sweeping orchid arcs
  GeminiAura,     // Creative AI with morphing 4-point organic spark curves and fluid rings
  EcosystemOrbit, // Connected devices with orbital elliptical contours and harmonic radar waves
  MindfulWaves,   // Digital Wellbeing with calm breathing ribbons and soothing organic ripples
  CameraMagic,    // Pixel Pro Camera with iris aperture blades and floating bokeh rings
  TitanSecurity,  // Titan M2 Hardware Security with hexagonal lattice and shield radar pulses
  AuroraFlow,     // Flowing celestial ribbon curtains with undulating harmonic waves
  CosmicPulse,    // Starlit satellite orbits and planetary alignment rings
  CircleSearch,   // Google Circle to Search holographic neon contour rings & lasso aura
  PasskeysWallet, // Google Wallet & Passkeys FIDO2 biometric waves & card outlines
  AdaptiveBattery, // Android 15 Adaptive Energy flow & charging wattage curve
  FilledOrganicBlobs // Organic pastel filled blobs, butter yellow & coral waves, and floating droplet bubbles inspired by AKS Labs
}

/**
 * Reusable animated expressive background inspired by Google Pixel system UI and Material 3 Expressive.
 *
 * Renders organic vector geometry programmatically via Canvas with ultra-smooth, subtle drifting
 * and morphing motion. Allocations inside the draw loop are minimized.
 */
@Composable
fun ExpressiveOnboardingBackground(
  modifier: Modifier = Modifier,
  preset: BackgroundPreset = BackgroundPreset.PixelSetup,
  reducedMotion: Boolean = false,
  customPrimaryColor: Color? = null,
  customSecondaryColor: Color? = null,
) {
  val colorScheme = MaterialTheme.colorScheme
  val isDark = colorScheme.background.red < 0.5f

  // Colors derived gracefully from Material 3 colorScheme
  val primaryStroke = customPrimaryColor ?: if (isDark) {
    colorScheme.primary.copy(alpha = 0.85f)
  } else {
    colorScheme.primary.copy(alpha = 0.65f)
  }

  val secondaryStroke = customSecondaryColor ?: if (isDark) {
    colorScheme.tertiary.copy(alpha = 0.75f)
  } else {
    colorScheme.tertiary.copy(alpha = 0.55f)
  }

  val subtleAccent = if (isDark) {
    colorScheme.secondary.copy(alpha = 0.35f)
  } else {
    colorScheme.secondary.copy(alpha = 0.25f)
  }

  // Infinite transitions for subtle, calm, non-distracting organic breathing
  val transition = rememberInfiniteTransition(label = "BackgroundMotion")

  val driftPhase by if (reducedMotion) {
    remember { androidx.compose.runtime.mutableFloatStateOf(0f) }
  } else {
    transition.animateFloat(
      initialValue = 0f,
      targetValue = (2 * PI).toFloat(),
      animationSpec = infiniteRepeatable(
        animation = tween(durationMillis = 18000, easing = LinearEasing),
        repeatMode = RepeatMode.Restart
      ),
      label = "DriftPhase"
    )
  }

  val morphProgress by if (reducedMotion) {
    remember { androidx.compose.runtime.mutableFloatStateOf(0.5f) }
  } else {
    transition.animateFloat(
      initialValue = 0f,
      targetValue = 1f,
      animationSpec = infiniteRepeatable(
        animation = tween(durationMillis = 10000, easing = FastOutSlowInEasing),
        repeatMode = RepeatMode.Reverse
      ),
      label = "MorphProgress"
    )
  }

  val secondaryDrift by if (reducedMotion) {
    remember { androidx.compose.runtime.mutableFloatStateOf(0f) }
  } else {
    transition.animateFloat(
      initialValue = 0f,
      targetValue = (2 * PI).toFloat(),
      animationSpec = infiniteRepeatable(
        animation = tween(durationMillis = 24000, easing = LinearEasing),
        repeatMode = RepeatMode.Restart
      ),
      label = "SecondaryDrift"
    )
  }

  Canvas(
    modifier = modifier.fillMaxSize()
  ) {
    val width = size.width
    val height = size.height

    when (preset) {
      BackgroundPreset.PixelSetup -> {
        drawPixelSetupBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          secondaryDrift = secondaryDrift,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.GeminiAura -> {
        drawGeminiAuraBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.EcosystemOrbit -> {
        drawEcosystemOrbitBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.MindfulWaves -> {
        drawMindfulWavesBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.CameraMagic -> {
        drawCameraMagicBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.TitanSecurity -> {
        drawTitanSecurityBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.AuroraFlow -> {
        drawAuroraFlowBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.CosmicPulse -> {
        drawCosmicPulseBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.CircleSearch -> {
        drawCircleSearchBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.PasskeysWallet -> {
        drawPasskeysWalletBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.AdaptiveBattery -> {
        drawAdaptiveBatteryBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
      BackgroundPreset.FilledOrganicBlobs -> {
        drawFilledOrganicBlobsBackground(
          width = width,
          height = height,
          driftPhase = driftPhase,
          morphProgress = morphProgress,
          secondaryDrift = secondaryDrift,
          primaryColor = primaryStroke,
          secondaryColor = secondaryStroke,
          accentColor = subtleAccent
        )
      }
    }
  }
}

/**
 * Pixel Setup signature composition.
 */
private fun DrawScope.drawPixelSetupBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  secondaryDrift: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * (0.68f + 0.03f * sin(driftPhase))
  val centerY = -height * 0.05f + (height * 0.02f * cos(driftPhase))
  val baseRadius = width * (0.42f + 0.03f * morphProgress)
  val waveCount = 14
  val waveAmplitude = width * (0.042f + 0.012f * sin(driftPhase * 1.5f))

  val scallopPath = Path()
  val numSteps = 120
  for (i in 0..numSteps) {
    val angle = (i.toFloat() / numSteps) * 2f * PI.toFloat()
    val radiusMod = baseRadius + waveAmplitude * sin(angle * waveCount + driftPhase * 0.7f)
    val x = centerX + radiusMod * cos(angle)
    val y = centerY + radiusMod * sin(angle)
    if (i == 0) scallopPath.moveTo(x, y) else scallopPath.lineTo(x, y)
  }
  scallopPath.close()

  drawPath(
    path = scallopPath,
    color = primaryColor,
    style = Stroke(width = 2.4.dp.toPx(), cap = StrokeCap.Round)
  )

  val sweepPath = Path()
  val startX = -width * 0.15f
  val startY = height * (0.36f + 0.02f * sin(secondaryDrift))
  val control1X = width * (0.35f + 0.04f * cos(secondaryDrift))
  val control1Y = height * (0.33f + 0.03f * sin(secondaryDrift * 0.8f))
  val control2X = width * (0.75f + 0.03f * sin(driftPhase))
  val control2Y = height * (0.46f + 0.02f * cos(driftPhase))
  val endX = width * 1.25f
  val endY = height * (0.62f + 0.03f * sin(driftPhase))

  sweepPath.moveTo(startX, startY)
  sweepPath.cubicTo(control1X, control1Y, control2X, control2Y, endX, endY)

  drawPath(
    path = sweepPath,
    color = secondaryColor,
    style = Stroke(width = 2.0.dp.toPx(), cap = StrokeCap.Round)
  )

  val bottomArcPath = Path()
  val bStartX = -width * 0.2f
  val bStartY = height * (0.82f + 0.02f * cos(driftPhase))
  val bCtrl1X = width * (0.28f + 0.03f * sin(secondaryDrift))
  val bCtrl1Y = height * (0.94f + 0.02f * morphProgress)
  val bCtrl2X = width * (0.65f + 0.03f * cos(driftPhase * 0.9f))
  val bCtrl2Y = height * (0.96f - 0.02f * morphProgress)
  val bEndX = width * 1.15f
  val bEndY = height * (0.78f + 0.02f * sin(secondaryDrift))

  bottomArcPath.moveTo(bStartX, bStartY)
  bottomArcPath.cubicTo(bCtrl1X, bCtrl1Y, bCtrl2X, bCtrl2Y, bEndX, bEndY)

  drawPath(
    path = bottomArcPath,
    color = secondaryColor.copy(alpha = secondaryColor.alpha * 0.8f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )

  val filamentPath = Path()
  filamentPath.moveTo(-width * 0.05f, height * 0.08f)
  filamentPath.quadraticTo(
    width * (0.12f + 0.02f * sin(driftPhase)),
    height * (0.16f + 0.02f * cos(driftPhase)),
    width * 0.04f,
    height * 0.26f
  )

  drawPath(
    path = filamentPath,
    color = accentColor,
    style = Stroke(width = 1.4.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Creative Gemini Aura background with morphing 4-point organic fluid curves.
 */
private fun DrawScope.drawGeminiAuraBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * (0.52f + 0.03f * sin(driftPhase * 0.8f))
  val centerY = height * (0.28f + 0.02f * cos(driftPhase * 0.6f))
  val maxRadius = width * (0.48f + 0.04f * morphProgress)
  val minRadius = width * (0.22f + 0.03f * (1f - morphProgress))

  val starPath = Path()
  val points = 80
  for (i in 0..points) {
    val angle = (i.toFloat() / points) * 2f * PI.toFloat()
    val r = minRadius + (maxRadius - minRadius) * (0.5f + 0.5f * cos(angle * 4f + driftPhase))
    val x = centerX + r * cos(angle)
    val y = centerY + r * sin(angle)
    if (i == 0) starPath.moveTo(x, y) else starPath.lineTo(x, y)
  }
  starPath.close()

  drawPath(
    path = starPath,
    color = primaryColor,
    style = Stroke(width = 2.2.dp.toPx(), cap = StrokeCap.Round)
  )

  val outerAura = Path()
  val outerRadius = width * (0.65f + 0.03f * sin(driftPhase))
  for (i in 0..60) {
    val angle = (i.toFloat() / 60) * 2f * PI.toFloat()
    val r = outerRadius + (width * 0.05f * sin(angle * 3f - driftPhase))
    val x = centerX + r * cos(angle)
    val y = centerY + r * sin(angle)
    if (i == 0) outerAura.moveTo(x, y) else outerAura.lineTo(x, y)
  }
  outerAura.close()

  drawPath(
    path = outerAura,
    color = secondaryColor,
    style = Stroke(width = 1.6.dp.toPx(), cap = StrokeCap.Round)
  )

  val streamPath = Path()
  streamPath.moveTo(-width * 0.1f, height * 0.72f)
  streamPath.cubicTo(
    width * 0.3f, height * (0.68f + 0.03f * cos(driftPhase)),
    width * 0.7f, height * (0.82f - 0.03f * sin(driftPhase)),
    width * 1.15f, height * 0.76f
  )
  drawPath(
    path = streamPath,
    color = accentColor,
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Ecosystem & Watch orbital background with concentric elliptical contours.
 */
private fun DrawScope.drawEcosystemOrbitBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * 0.75f
  val centerY = height * 0.22f

  val rx1 = width * (0.55f + 0.02f * sin(driftPhase))
  val ry1 = height * (0.24f + 0.015f * cos(driftPhase))
  val angleOffset = driftPhase * 0.1f

  val orbit1 = Path()
  for (i in 0..72) {
    val theta = (i.toFloat() / 72) * 2f * PI.toFloat()
    val unrotatedX = rx1 * cos(theta)
    val unrotatedY = ry1 * sin(theta)
    val rot = 0.35f + angleOffset
    val x = centerX + (unrotatedX * cos(rot) - unrotatedY * sin(rot))
    val y = centerY + (unrotatedX * sin(rot) + unrotatedY * cos(rot))
    if (i == 0) orbit1.moveTo(x, y) else orbit1.lineTo(x, y)
  }
  orbit1.close()

  drawPath(
    path = orbit1,
    color = primaryColor,
    style = Stroke(width = 2.0.dp.toPx(), cap = StrokeCap.Round)
  )

  val rx2 = width * (0.85f + 0.03f * cos(driftPhase))
  val ry2 = height * (0.38f + 0.02f * sin(driftPhase))
  val orbit2 = Path()
  for (i in 0..72) {
    val theta = (i.toFloat() / 72) * 2f * PI.toFloat()
    val unrotatedX = rx2 * cos(theta)
    val unrotatedY = ry2 * sin(theta)
    val rot = 0.25f - angleOffset * 0.5f
    val x = centerX + (unrotatedX * cos(rot) - unrotatedY * sin(rot))
    val y = centerY + (unrotatedX * sin(rot) + unrotatedY * cos(rot))
    if (i == 0) orbit2.moveTo(x, y) else orbit2.lineTo(x, y)
  }
  orbit2.close()

  drawPath(
    path = orbit2,
    color = secondaryColor,
    style = Stroke(width = 1.6.dp.toPx(), cap = StrokeCap.Round)
  )

  val link = Path()
  link.moveTo(-width * 0.1f, height * 0.88f)
  link.cubicTo(
    width * 0.4f, height * (0.82f + 0.02f * sin(driftPhase)),
    width * 0.8f, height * (0.94f + 0.02f * cos(driftPhase)),
    width * 1.1f, height * 0.84f
  )
  drawPath(
    path = link,
    color = accentColor,
    style = Stroke(width = 1.4.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Digital Wellbeing mindful breathing curves: organic undulating waves.
 */
private fun DrawScope.drawMindfulWavesBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val wave1 = Path()
  wave1.moveTo(-width * 0.15f, height * 0.18f)
  wave1.cubicTo(
    width * 0.25f, height * (0.08f + 0.04f * sin(driftPhase)),
    width * 0.70f, height * (0.28f + 0.04f * cos(driftPhase)),
    width * 1.15f, height * 0.14f
  )
  drawPath(
    path = wave1,
    color = primaryColor,
    style = Stroke(width = 2.2.dp.toPx(), cap = StrokeCap.Round)
  )

  val wave2 = Path()
  wave2.moveTo(-width * 0.1f, height * 0.48f)
  wave2.cubicTo(
    width * 0.35f, height * (0.54f + 0.03f * cos(driftPhase * 0.8f)),
    width * 0.65f, height * (0.42f - 0.03f * sin(driftPhase * 0.8f)),
    width * 1.15f, height * 0.52f
  )
  drawPath(
    path = wave2,
    color = secondaryColor,
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )

  val puddle = Path()
  val pX = width * 0.3f
  val pY = height * 0.92f
  val pr = width * (0.35f + 0.02f * morphProgress)
  for (i in 0..50) {
    val a = (i.toFloat() / 50) * 2f * PI.toFloat()
    val r = pr + (width * 0.03f * sin(a * 4f + driftPhase))
    val x = pX + r * cos(a)
    val y = pY + r * 0.6f * sin(a)
    if (i == 0) puddle.moveTo(x, y) else puddle.lineTo(x, y)
  }
  puddle.close()

  drawPath(
    path = puddle,
    color = accentColor,
    style = Stroke(width = 1.5.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Pixel Camera Magic: Iris aperture petals and floating optical bokeh circles.
 */
private fun DrawScope.drawCameraMagicBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * (0.70f + 0.02f * sin(driftPhase))
  val centerY = height * (0.24f + 0.02f * cos(driftPhase))
  val apertureRadius = width * (0.34f + 0.03f * morphProgress)
  val blades = 7

  // Interlocking aperture blades
  for (i in 0 until blades) {
    val angle = (i.toFloat() / blades) * 2f * PI.toFloat() + driftPhase * 0.2f
    val startRadius = apertureRadius * 0.45f
    val endRadius = apertureRadius * 1.15f
    val p1x = centerX + startRadius * cos(angle)
    val p1y = centerY + startRadius * sin(angle)
    val p2x = centerX + endRadius * cos(angle + 0.55f)
    val p2y = centerY + endRadius * sin(angle + 0.55f)

    val bladePath = Path()
    bladePath.moveTo(p1x, p1y)
    bladePath.quadraticTo(
      centerX + apertureRadius * cos(angle + 0.25f),
      centerY + apertureRadius * sin(angle + 0.25f),
      p2x,
      p2y
    )

    drawPath(
      path = bladePath,
      color = primaryColor.copy(alpha = 0.7f),
      style = Stroke(width = 2.0.dp.toPx(), cap = StrokeCap.Round)
    )
  }

  // Optical Bokeh Circles
  drawCircle(
    color = secondaryColor.copy(alpha = 0.25f),
    radius = width * (0.22f + 0.02f * sin(driftPhase)),
    center = Offset(width * 0.25f, height * 0.72f),
    style = Stroke(width = 1.6.dp.toPx())
  )

  drawCircle(
    color = accentColor.copy(alpha = 0.35f),
    radius = width * (0.14f + 0.015f * cos(driftPhase)),
    center = Offset(width * 0.38f, height * 0.65f),
    style = Stroke(width = 1.4.dp.toPx())
  )

  // Floating lens flare arc
  val flarePath = Path()
  flarePath.moveTo(-width * 0.1f, height * 0.40f)
  flarePath.cubicTo(
    width * 0.3f, height * (0.35f + 0.03f * sin(driftPhase)),
    width * 0.7f, height * (0.55f + 0.03f * cos(driftPhase)),
    width * 1.15f, height * 0.48f
  )
  drawPath(
    path = flarePath,
    color = secondaryColor.copy(alpha = 0.6f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Titan M2 Security: Hexagonal lattice matrix and expanding protective pulse shields.
 */
private fun DrawScope.drawTitanSecurityBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * 0.5f
  val centerY = height * (0.26f + 0.02f * sin(driftPhase * 0.8f))

  // Concentric Hexagonal Shields
  for (shieldIndex in 1..3) {
    val radius = width * (0.22f * shieldIndex + 0.02f * sin(driftPhase + shieldIndex))
    val hexPath = Path()
    for (i in 0..6) {
      val angle = (i.toFloat() / 6) * 2f * PI.toFloat() - (PI / 2).toFloat()
      val x = centerX + radius * cos(angle)
      val y = centerY + radius * sin(angle)
      if (i == 0) hexPath.moveTo(x, y) else hexPath.lineTo(x, y)
    }
    hexPath.close()

    val strokeAlpha = (1f - (shieldIndex * 0.22f)).coerceIn(0.2f, 0.9f)
    drawPath(
      path = hexPath,
      color = if (shieldIndex % 2 == 1) primaryColor.copy(alpha = strokeAlpha) else secondaryColor.copy(alpha = strokeAlpha),
      style = Stroke(width = (2.4 - shieldIndex * 0.4).dp.toPx(), cap = StrokeCap.Round)
    )
  }

  // Security Pulse Radar lines
  val sweepLine = Path()
  sweepLine.moveTo(centerX, centerY)
  val radarAngle = driftPhase * 0.9f
  sweepLine.lineTo(
    centerX + width * 0.65f * cos(radarAngle),
    centerY + width * 0.65f * sin(radarAngle)
  )
  drawPath(
    path = sweepLine,
    color = accentColor.copy(alpha = 0.4f),
    style = Stroke(width = 1.5.dp.toPx(), cap = StrokeCap.Round)
  )

  // Lower encryption node wave
  val wave = Path()
  wave.moveTo(-width * 0.1f, height * 0.82f)
  wave.cubicTo(
    width * 0.35f, height * (0.76f + 0.03f * cos(driftPhase)),
    width * 0.65f, height * (0.88f - 0.03f * sin(driftPhase)),
    width * 1.15f, height * 0.80f
  )
  drawPath(
    path = wave,
    color = secondaryColor.copy(alpha = 0.5f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Aurora Flow: Celestial flowing ribbons with harmonic organic curves.
 */
private fun DrawScope.drawAuroraFlowBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  for (layer in 0..2) {
    val ribbon = Path()
    val yOffset = height * (0.15f + layer * 0.22f)
    ribbon.moveTo(-width * 0.15f, yOffset)
    ribbon.cubicTo(
      width * (0.25f + 0.04f * sin(driftPhase + layer)),
      yOffset + height * 0.12f * sin(driftPhase * 1.2f + layer),
      width * (0.75f - 0.04f * cos(driftPhase + layer)),
      yOffset - height * 0.10f * cos(driftPhase * 0.9f + layer),
      width * 1.15f,
      yOffset + height * 0.05f * sin(driftPhase)
    )

    val color = when (layer) {
      0 -> primaryColor
      1 -> secondaryColor
      else -> accentColor
    }

    drawPath(
      path = ribbon,
      color = color.copy(alpha = 0.65f - layer * 0.12f),
      style = Stroke(width = (2.2 - layer * 0.3).dp.toPx(), cap = StrokeCap.Round)
    )
  }
}

/**
 * Cosmic Pulse: Starlit celestial orbit rings and alignment curves.
 */
private fun DrawScope.drawCosmicPulseBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * 0.62f
  val centerY = height * 0.32f

  // Elliptical planetary rings
  val ring1 = Path()
  val rx = width * (0.42f + 0.03f * morphProgress)
  val ry = width * (0.18f + 0.02f * sin(driftPhase))
  for (i in 0..60) {
    val a = (i.toFloat() / 60) * 2f * PI.toFloat()
    val x = centerX + rx * cos(a)
    val y = centerY + ry * sin(a)
    if (i == 0) ring1.moveTo(x, y) else ring1.lineTo(x, y)
  }
  ring1.close()
  drawPath(
    path = ring1,
    color = primaryColor,
    style = Stroke(width = 2.0.dp.toPx(), cap = StrokeCap.Round)
  )

  // Satellite orbit trajectory
  val arc = Path()
  arc.moveTo(-width * 0.1f, height * 0.15f)
  arc.cubicTo(
    width * 0.35f, height * 0.65f,
    width * 0.85f, height * 0.25f,
    width * 1.15f, height * 0.85f
  )
  drawPath(
    path = arc,
    color = secondaryColor.copy(alpha = 0.6f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )

  // Pulsing core beacon circle
  drawCircle(
    color = accentColor.copy(alpha = 0.5f),
    radius = width * (0.06f + 0.015f * sin(driftPhase * 2f)),
    center = Offset(centerX, centerY),
    style = Stroke(width = 1.6.dp.toPx())
  )
}

/**
 * Circle to Search: Holographic neon boundary, target crosshairs, and multi-search radiant aura.
 */
private fun DrawScope.drawCircleSearchBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * (0.50f + 0.02f * sin(driftPhase))
  val centerY = height * (0.35f + 0.02f * cos(driftPhase))

  // Pulsing search lasso loop
  val lassoRadius = width * (0.32f + 0.03f * morphProgress)
  val lassoPath = Path()
  val points = 72
  for (i in 0..points) {
    val angle = (i.toFloat() / points) * 2f * PI.toFloat()
    val r = lassoRadius + (width * 0.025f * sin(angle * 3f + driftPhase * 1.5f))
    val x = centerX + r * cos(angle)
    val y = centerY + r * sin(angle)
    if (i == 0) lassoPath.moveTo(x, y) else lassoPath.lineTo(x, y)
  }
  lassoPath.close()

  drawPath(
    path = lassoPath,
    color = primaryColor,
    style = Stroke(width = 2.4.dp.toPx(), cap = StrokeCap.Round)
  )

  // Outer radiant search aura
  val outerAura = Path()
  val outerRadius = width * (0.48f + 0.02f * sin(driftPhase))
  for (i in 0..60) {
    val angle = (i.toFloat() / 60) * 2f * PI.toFloat()
    val r = outerRadius + (width * 0.04f * cos(angle * 4f - driftPhase))
    val x = centerX + r * cos(angle)
    val y = centerY + r * sin(angle)
    if (i == 0) outerAura.moveTo(x, y) else outerAura.lineTo(x, y)
  }
  outerAura.close()

  drawPath(
    path = outerAura,
    color = secondaryColor.copy(alpha = 0.55f),
    style = Stroke(width = 1.6.dp.toPx(), cap = StrokeCap.Round)
  )

  // Subtle gesture scan arc
  val scanArc = Path()
  scanArc.moveTo(-width * 0.1f, height * 0.78f)
  scanArc.cubicTo(
    width * 0.35f, height * (0.72f + 0.03f * sin(driftPhase)),
    width * 0.70f, height * (0.86f - 0.03f * cos(driftPhase)),
    width * 1.15f, height * 0.75f
  )
  drawPath(
    path = scanArc,
    color = accentColor.copy(alpha = 0.6f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Passkeys & Wallet: Biometric FIDO2 waves, contactless NFC ripples, and card outline geometry.
 */
private fun DrawScope.drawPasskeysWalletBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * 0.5f
  val centerY = height * (0.28f + 0.02f * sin(driftPhase * 0.7f))

  // Concentric contactless NFC radio arc waves
  for (i in 1..4) {
    val r = width * (0.16f * i + 0.02f * sin(driftPhase + i))
    val arcPath = Path()
    val startAngle = -PI.toFloat() * 0.75f
    val sweepAngle = PI.toFloat() * 1.5f
    val steps = 36
    for (s in 0..steps) {
      val angle = startAngle + (s.toFloat() / steps) * sweepAngle
      val x = centerX + r * cos(angle)
      val y = centerY + r * sin(angle)
      if (s == 0) arcPath.moveTo(x, y) else arcPath.lineTo(x, y)
    }
    drawPath(
      path = arcPath,
      color = if (i % 2 == 1) primaryColor.copy(alpha = 0.75f / i) else secondaryColor.copy(alpha = 0.65f / i),
      style = Stroke(width = 2.0.dp.toPx(), cap = StrokeCap.Round)
    )
  }

  // Floating biometric biometric fingerprint loop contour
  val fpPath = Path()
  fpPath.moveTo(width * 0.2f, height * 0.75f)
  fpPath.cubicTo(
    width * 0.4f, height * (0.68f + 0.03f * cos(driftPhase)),
    width * 0.6f, height * (0.84f - 0.03f * sin(driftPhase)),
    width * 0.85f, height * 0.72f
  )
  drawPath(
    path = fpPath,
    color = accentColor.copy(alpha = 0.55f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Adaptive Battery: Tensor AI energy flow, battery waveform, and power pulse.
 */
private fun DrawScope.drawAdaptiveBatteryBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val centerX = width * 0.5f
  val centerY = height * (0.30f + 0.02f * sin(driftPhase * 0.8f))

  // Energy cell battery shape
  val cellWidth = width * 0.58f
  val cellHeight = width * 0.78f
  val cellLeft = centerX - cellWidth / 2f
  val cellTop = centerY - cellHeight / 2f

  // Dynamic wattage pulse wave inside cell
  val wavePath = Path()
  val waveY = centerY + width * 0.12f
  wavePath.moveTo(cellLeft, waveY)
  val wSteps = 40
  for (i in 0..wSteps) {
    val x = cellLeft + (cellWidth * (i.toFloat() / wSteps))
    val y = waveY + width * (0.04f + 0.015f * morphProgress) * sin((i.toFloat() / wSteps) * 4f * PI.toFloat() + driftPhase * 2f)
    wavePath.lineTo(x, y)
  }
  drawPath(
    path = wavePath,
    color = primaryColor,
    style = Stroke(width = 2.4.dp.toPx(), cap = StrokeCap.Round)
  )

  // Floating orbital energy ring
  val ring = Path()
  val rx = width * (0.42f + 0.02f * sin(driftPhase))
  val ry = width * (0.24f + 0.02f * cos(driftPhase))
  for (i in 0..50) {
    val a = (i.toFloat() / 50) * 2f * PI.toFloat()
    val x = centerX + rx * cos(a)
    val y = centerY + ry * sin(a)
    if (i == 0) ring.moveTo(x, y) else ring.lineTo(x, y)
  }
  ring.close()
  drawPath(
    path = ring,
    color = secondaryColor.copy(alpha = 0.5f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )

  // Energy transmission arc
  val arc = Path()
  arc.moveTo(-width * 0.1f, height * 0.82f)
  arc.cubicTo(
    width * 0.35f, height * (0.75f + 0.03f * cos(driftPhase)),
    width * 0.70f, height * (0.90f - 0.03f * sin(driftPhase)),
    width * 1.15f, height * 0.80f
  )
  drawPath(
    path = arc,
    color = accentColor.copy(alpha = 0.6f),
    style = Stroke(width = 1.8.dp.toPx(), cap = StrokeCap.Round)
  )
}

/**
 * Renders genuine Material 3 Expressive shapes (12-lobe Scallop cookie, Heart,
 * horizontal Stadium pill, and Clover) on a soft pastel lilac canvas.
 */
private fun DrawScope.drawFilledOrganicBlobsBackground(
  width: Float,
  height: Float,
  driftPhase: Float,
  morphProgress: Float,
  secondaryDrift: Float,
  primaryColor: Color,
  secondaryColor: Color,
  accentColor: Color
) {
  val m3Lilac = Color(0xFFF7EFF9)
  val m3ScallopLavender = Color(0xFFBA9BF8)
  val m3DeepPlum = Color(0xFF493870)
  val m3SoftPill = Color(0xFFE2D3F8)

  // 1. Soft pastel lilac canvas backdrop fill
  drawRect(color = m3Lilac)

  // 2. Floating 12-lobed Scallop cookie shape (top-left)
  val scallopCx = width * (0.28f + 0.02f * cos(driftPhase * 0.8f))
  val scallopCy = height * (0.24f + 0.02f * sin(driftPhase * 0.7f))
  val baseR = width * (0.22f + 0.01f * morphProgress)
  val scallopAmp = width * 0.028f
  val scallopPath = Path()
  val steps = 96
  for (i in 0..steps) {
    val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
    val r = baseR + scallopAmp * cos(theta * 12f)
    val x = scallopCx + r * cos(theta)
    val y = scallopCy + r * sin(theta)
    if (i == 0) scallopPath.moveTo(x, y) else scallopPath.lineTo(x, y)
  }
  scallopPath.close()
  drawPath(path = scallopPath, color = m3ScallopLavender.copy(alpha = 0.45f))

  // 3. Floating Material 3 Expressive Heart shape (top-right)
  val heartW = width * 0.42f
  val heartH = heartW * 0.95f
  val heartLeft = width * (0.58f - 0.02f * sin(driftPhase * 0.9f))
  val heartTop = height * (0.16f + 0.02f * cos(driftPhase * 0.8f))
  val heartPath = Path()
  heartPath.moveTo(heartLeft + heartW * 0.5f, heartTop + heartH * 0.30f)
  heartPath.cubicTo(
    heartLeft + heartW * 0.44f, heartTop + heartH * 0.10f,
    heartLeft + heartW * 0.08f, heartTop + heartH * 0.10f,
    heartLeft + heartW * 0.08f, heartTop + heartH * 0.42f
  )
  heartPath.cubicTo(
    heartLeft + heartW * 0.08f, heartTop + heartH * 0.65f,
    heartLeft + heartW * 0.30f, heartTop + heartH * 0.82f,
    heartLeft + heartW * 0.50f, heartTop + heartH * 0.95f
  )
  heartPath.cubicTo(
    heartLeft + heartW * 0.70f, heartTop + heartH * 0.82f,
    heartLeft + heartW * 0.92f, heartTop + heartH * 0.65f,
    heartLeft + heartW * 0.92f, heartTop + heartH * 0.42f
  )
  heartPath.cubicTo(
    heartLeft + heartW * 0.92f, heartTop + heartH * 0.10f,
    heartLeft + heartW * 0.56f, heartTop + heartH * 0.10f,
    heartLeft + heartW * 0.50f, heartTop + heartH * 0.30f
  )
  heartPath.close()
  drawPath(path = heartPath, color = m3DeepPlum.copy(alpha = 0.32f))

  // 4. Floating Horizontal Stadium Capsule (center-bottom)
  val pillW = width * 0.74f
  val pillH = height * 0.09f
  val pillX = width * (0.13f + 0.015f * sin(secondaryDrift))
  val pillY = height * (0.42f + 0.015f * cos(driftPhase * 0.7f))
  val pillPath = Path()
  val cr = pillH * 0.5f
  pillPath.moveTo(pillX + cr, pillY)
  pillPath.lineTo(pillX + pillW - cr, pillY)
  pillPath.arcTo(
    rect = androidx.compose.ui.geometry.Rect(pillX + pillW - pillH, pillY, pillX + pillW, pillY + pillH),
    startAngleDegrees = -90f,
    sweepAngleDegrees = 180f,
    forceMoveTo = false
  )
  pillPath.lineTo(pillX + cr, pillY + pillH)
  pillPath.arcTo(
    rect = androidx.compose.ui.geometry.Rect(pillX, pillY, pillX + pillH, pillY + pillH),
    startAngleDegrees = 90f,
    sweepAngleDegrees = 180f,
    forceMoveTo = false
  )
  pillPath.close()
  drawPath(path = pillPath, color = m3DeepPlum.copy(alpha = 0.28f))

  // 5. Lower Decorative M3 4-Leaf Clover
  val cloverCx = width * 0.25f
  val cloverCy = height * (0.68f + 0.02f * sin(secondaryDrift * 0.8f))
  val cloverBaseR = width * 0.14f
  val cloverAmp = width * 0.035f
  val cloverPath = Path()
  for (i in 0..72) {
    val theta = (i.toFloat() / 72) * 2f * PI.toFloat()
    val r = cloverBaseR + cloverAmp * cos(theta * 4f)
    val x = cloverCx + r * cos(theta)
    val y = cloverCy + r * sin(theta)
    if (i == 0) cloverPath.moveTo(x, y) else cloverPath.lineTo(x, y)
  }
  cloverPath.close()
  drawPath(path = cloverPath, color = m3ScallopLavender.copy(alpha = 0.35f))

  // 6. Lower Right M3 Arch Dome
  val archW = width * 0.34f
  val archH = height * 0.16f
  val archX = width * 0.65f
  val archY = height * (0.62f - 0.02f * cos(secondaryDrift * 0.7f))
  val archPath = Path()
  archPath.moveTo(archX, archY + archH)
  archPath.lineTo(archX, archY + archW * 0.5f)
  archPath.arcTo(
    rect = androidx.compose.ui.geometry.Rect(archX, archY, archX + archW, archY + archW),
    startAngleDegrees = 180f,
    sweepAngleDegrees = 180f,
    forceMoveTo = false
  )
  archPath.lineTo(archX + archW, archY + archH)
  archPath.close()
  drawPath(path = archPath, color = m3SoftPill.copy(alpha = 0.50f))
}

