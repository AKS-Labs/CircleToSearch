package com.akslabs.circletosearch.onboarding

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Calculate
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CloudUpload
import androidx.compose.material.icons.filled.Gesture
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.ImageSearch
import androidx.compose.material.icons.filled.ShoppingBag
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.components.BackgroundPreset
import com.akslabs.circletosearch.ui.components.ExpressiveOnboardingScaffold
import com.akslabs.circletosearch.ui.theme.M3ExpressiveShapes
import com.akslabs.circletosearch.ui.theme.M3ExpressiveSpring
import com.akslabs.circletosearch.ui.theme.m3ExpressivePress
import com.akslabs.circletosearch.utils.PrivacyPreferences

/**
 * Circle to Search onboarding flow.
 *
 * Steps:
 * - Warm welcome & introduction
 * - Hold gesture bar to activate
 * - Interactive gesture drawing canvas (finger lasso around search targets)
 * - Instant translation & equation solving
 * - Privacy & data transparency (must accept to finish)
 */
@Composable
fun PixelCircleSearchOnboardingScreen(
  onFinish: () -> Unit,
  reducedMotion: Boolean = false,
  modifier: Modifier = Modifier
) {
  var currentStep by remember { mutableStateOf(1) }
  val totalSteps = 5
  val context = LocalContext.current
  val privacyPreferences = remember { PrivacyPreferences(context) }
  var privacyAccepted by remember { mutableStateOf(privacyPreferences.hasAcceptedPrivacyPolicy()) }
  val isLastStep = currentStep == totalSteps

  ExpressiveOnboardingScaffold(
    modifier = modifier.testTag("circle_search_onboarding_screen"),
    backgroundPreset = BackgroundPreset.GeminiAura,
    reducedMotion = reducedMotion,
    totalSteps = totalSteps,
    currentStep = currentStep,
    onBackClick = {
      if (currentStep > 1) currentStep-- else onFinish()
    },
    topActionIcon = null,
    topActionDescription = null,
    onTopActionClick = null,
    secondaryActionText = if (currentStep > 1) "Back" else "Skip",
    onSecondaryActionClick = {
      if (currentStep > 1) currentStep-- else currentStep = totalSteps
    },
    primaryActionText = if (isLastStep) "Accept & Finish" else "Next",
    primaryActionEnabled = !isLastStep || privacyAccepted,
    onPrimaryActionClick = {
      if (currentStep < totalSteps) {
        currentStep++
      } else {
        privacyPreferences.setPrivacyAccepted()
        onFinish()
      }
    }
  ) {
    AnimatedContent(
      targetState = currentStep,
      transitionSpec = {
        (fadeIn() + scaleIn(initialScale = 0.94f, animationSpec = M3ExpressiveSpring.Bouncy)).togetherWith(
          fadeOut() + scaleOut(targetScale = 0.94f)
        )
      },
      label = "CircleSearchStepTransition"
    ) { step ->
      when (step) {
        1 -> CircleSearchWelcomeStep()
        2 -> CircleSearchActivateStep(reducedMotion = reducedMotion)
        3 -> CircleSearchInteractiveLassoStep(reducedMotion = reducedMotion)
        4 -> CircleSearchTranslateSolveStep(reducedMotion = reducedMotion)
        else -> CircleSearchPrivacyStep(
          accepted = privacyAccepted,
          onAcceptChange = { privacyAccepted = it }
        )
      }
    }
  }
}

/**
 * Step 1: Warm welcome and introduction.
 */
@Composable
private fun CircleSearchWelcomeStep() {
  Column(
    modifier = Modifier
      .fillMaxWidth()
      .padding(bottom = 16.dp)
  ) {
    Text(
      text = "Welcome to Circle to Search",
      style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Normal),
      color = MaterialTheme.colorScheme.onBackground
    )

    Spacer(modifier = Modifier.height(8.dp))

    Text(
      text = "Search anything on your screen — text, images, QR codes — without switching apps. Free, open source, and no account needed.",
      style = MaterialTheme.typography.bodyLarge,
      color = MaterialTheme.colorScheme.onSurfaceVariant
    )

    Spacer(modifier = Modifier.height(16.dp))

    Row(
      horizontalArrangement = Arrangement.spacedBy(10.dp),
      verticalAlignment = Alignment.CenterVertically
    ) {
      Surface(
        shape = M3ExpressiveShapes.SlantedPill,
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)
      ) {
        Text(
          text = "Free & Open Source",
          style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
          color = MaterialTheme.colorScheme.primary,
          modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp)
        )
      }

      Surface(
        shape = M3ExpressiveShapes.SlantedPill,
        color = MaterialTheme.colorScheme.secondary.copy(alpha = 0.12f)
      ) {
        Text(
          text = "No Account Needed",
          style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
          color = MaterialTheme.colorScheme.secondary,
          modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp)
        )
      }
    }
  }
}

/**
 * Step 2: Hold gesture bar or home button to activate.
 */
@Composable
private fun CircleSearchActivateStep(reducedMotion: Boolean) {
  val infiniteTransition = rememberInfiniteTransition(label = "NeonPerimeterTransition")
  val borderPhase by if (reducedMotion) {
    remember { mutableStateOf(0f) }
  } else {
    infiniteTransition.animateFloat(
      initialValue = 0f,
      targetValue = 1f,
      animationSpec = infiniteRepeatable(
        animation = tween(durationMillis = 3200, easing = LinearEasing),
        repeatMode = RepeatMode.Restart
      ),
      label = "BorderPhase"
    )
  }

  val neonGlow = listOf(
    Color(0xFF4285F4),
    Color(0xFF9C27B0),
    Color(0xFFEA4335),
    Color(0xFFFBBC05),
    Color(0xFF34A853),
    Color(0xFF4285F4)
  )

  Column(
    modifier = Modifier
      .fillMaxWidth()
      .padding(bottom = 16.dp)
  ) {
    Card(
      shape = M3ExpressiveShapes.Arch,
      colors = CardDefaults.cardColors(
        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)
      ),
      modifier = Modifier
        .fillMaxWidth()
        .height(280.dp)
        .border(
          width = 3.dp,
          brush = Brush.sweepGradient(neonGlow),
          shape = M3ExpressiveShapes.Arch
        )
        .testTag("circle_search_neon_frame")
    ) {
      Box(
        modifier = Modifier
          .fillMaxSize()
          .padding(24.dp),
        contentAlignment = Alignment.Center
      ) {
        Column(
          horizontalAlignment = Alignment.CenterHorizontally,
          verticalArrangement = Arrangement.Center
        ) {
          Surface(
            shape = M3ExpressiveShapes.Clover4,
            color = MaterialTheme.colorScheme.primaryContainer,
            modifier = Modifier.size(72.dp)
          ) {
            Box(contentAlignment = Alignment.Center) {
              Icon(
                imageVector = Icons.Default.Gesture,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onPrimaryContainer,
                modifier = Modifier.size(34.dp)
              )
            }
          }

          Spacer(modifier = Modifier.height(18.dp))

          Text(
            text = "Long-press the Gesture Bar",
            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface
          )

          Spacer(modifier = Modifier.height(8.dp))

          Surface(
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier
              .width(96.dp)
              .height(6.dp)
          ) {}
        }
      }
    }

    Spacer(modifier = Modifier.height(24.dp))

    Text(
      text = "Start searching in one gesture",
      style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Normal),
      color = MaterialTheme.colorScheme.onBackground
    )

    Spacer(modifier = Modifier.height(8.dp))

    Text(
      text = "Touch and hold the gesture handle or home button from any screen. No need to open or switch apps first.",
      style = MaterialTheme.typography.bodyLarge,
      color = MaterialTheme.colorScheme.onSurfaceVariant
    )

    Spacer(modifier = Modifier.height(16.dp))

    Row(
      horizontalArrangement = Arrangement.spacedBy(10.dp),
      verticalAlignment = Alignment.CenterVertically
    ) {
      Surface(
        shape = M3ExpressiveShapes.SlantedPill,
        color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f)
      ) {
        Row(
          modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
          verticalAlignment = Alignment.CenterVertically
        ) {
          Icon(
            imageVector = Icons.Default.Gesture,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(16.dp)
          )
          Spacer(modifier = Modifier.width(6.dp))
          Text(
            text = "Zero App Switching",
            style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
            color = MaterialTheme.colorScheme.primary
          )
        }
      }

      Surface(
        shape = M3ExpressiveShapes.SlantedPill,
        color = MaterialTheme.colorScheme.secondary.copy(alpha = 0.12f)
      ) {
        Text(
          text = "Works Everywhere",
          style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
          color = MaterialTheme.colorScheme.secondary,
          modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp)
        )
      }
    }
  }
}

/**
 * Step 3: Interactive gesture lasso canvas where users drag finger to circle target.
 */
@Composable
private fun CircleSearchInteractiveLassoStep(reducedMotion: Boolean) {
  val strokePoints = remember { mutableStateListOf<Offset>() }
  var hasCircled by remember { mutableStateOf(false) }
  val resultCardScale = remember { Animatable(0.85f) }

  LaunchedEffect(hasCircled) {
    if (hasCircled) {
      resultCardScale.animateTo(
        targetValue = 1f,
        animationSpec = M3ExpressiveSpring.Bouncy
      )
    }
  }

  Column(
    modifier = Modifier
      .fillMaxWidth()
      .padding(bottom = 16.dp)
  ) {
    Card(
      shape = RoundedCornerShape(24.dp),
      colors = CardDefaults.cardColors(
        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f)
      ),
      modifier = Modifier
        .fillMaxWidth()
        .height(280.dp)
        .testTag("interactive_lasso_canvas_card")
    ) {
      Box(modifier = Modifier.fillMaxSize()) {
        Column(
          modifier = Modifier
            .fillMaxSize()
            .padding(16.dp),
          horizontalAlignment = Alignment.CenterHorizontally,
          verticalArrangement = Arrangement.Center
        ) {
          Surface(
            shape = M3ExpressiveShapes.AsymmetricCard,
            color = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.8f),
            modifier = Modifier.size(90.dp)
          ) {
            Box(contentAlignment = Alignment.Center) {
              Icon(
                imageVector = Icons.Default.ShoppingBag,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onPrimaryContainer,
                modifier = Modifier.size(42.dp)
              )
            }
          }

          Spacer(modifier = Modifier.height(12.dp))

          Text(
            text = if (!hasCircled) "Draw a circle around this item" else "Item Identified!",
            style = MaterialTheme.typography.titleSmall.copy(fontWeight = FontWeight.SemiBold),
            color = MaterialTheme.colorScheme.onSurface
          )
        }

        val strokeColor = MaterialTheme.colorScheme.primary
        Canvas(
          modifier = Modifier
            .fillMaxSize()
            .pointerInput(Unit) {
              detectDragGestures(
                onDragStart = { startOffset ->
                  strokePoints.clear()
                  strokePoints.add(startOffset)
                  hasCircled = false
                },
                onDrag = { change, _ ->
                  change.consume()
                  strokePoints.add(change.position)
                  if (strokePoints.size > 25) {
                    hasCircled = true
                  }
                },
                onDragEnd = {
                  if (strokePoints.size > 15) {
                    hasCircled = true
                  }
                }
              )
            }
        ) {
          if (strokePoints.size > 1) {
            val lassoPath = Path()
            lassoPath.moveTo(strokePoints.first().x, strokePoints.first().y)
            for (pt in strokePoints) {
              lassoPath.lineTo(pt.x, pt.y)
            }
            drawPath(
              path = lassoPath,
              color = strokeColor,
              style = Stroke(width = 3.5.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
            )
          }
        }

        androidx.compose.animation.AnimatedVisibility(
          visible = hasCircled,
          enter = fadeIn() + scaleIn(initialScale = 0.8f, animationSpec = M3ExpressiveSpring.Bouncy),
          modifier = Modifier
            .align(Alignment.BottomCenter)
            .padding(12.dp)
        ) {
          Surface(
            shape = RoundedCornerShape(16.dp),
            color = MaterialTheme.colorScheme.surface,
            shadowElevation = 6.dp,
            modifier = Modifier.fillMaxWidth()
          ) {
            Row(
              modifier = Modifier.padding(12.dp),
              verticalAlignment = Alignment.CenterVertically
            ) {
              Surface(
                shape = CircleShape,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(36.dp)
              ) {
                Icon(
                  imageVector = Icons.Default.Check,
                  contentDescription = null,
                  tint = MaterialTheme.colorScheme.onPrimary,
                  modifier = Modifier.padding(8.dp)
                )
              }
              Spacer(modifier = Modifier.width(12.dp))
              Column {
                Text(
                  text = "Visual Search Result",
                  style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.Bold),
                  color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                  text = "Retro Leather Duffle • Available nearby",
                  style = MaterialTheme.typography.bodySmall,
                  color = MaterialTheme.colorScheme.onSurfaceVariant
                )
              }
            }
          }
        }
      }
    }

    Spacer(modifier = Modifier.height(24.dp))

    Text(
      text = "Circle, scribble, or tap",
      style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Normal),
      color = MaterialTheme.colorScheme.onBackground
    )

    Spacer(modifier = Modifier.height(8.dp))

    Text(
      text = "Draw a circle with your finger around anything you want to know more about: clothes, shoes, landmarks, or formulas. Try it above!",
      style = MaterialTheme.typography.bodyLarge,
      color = MaterialTheme.colorScheme.onSurfaceVariant
    )
  }
}

/**
 * Step 4: Instant text translation & math equation solving.
 */
@Composable
private fun CircleSearchTranslateSolveStep(reducedMotion: Boolean) {
  var activeMode by remember { mutableStateOf(0) }

  Column(
    modifier = Modifier
      .fillMaxWidth()
      .padding(bottom = 16.dp)
  ) {
    Card(
      shape = M3ExpressiveShapes.AsymmetricCard,
      colors = CardDefaults.cardColors(
        containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)
      ),
      modifier = Modifier
        .fillMaxWidth()
        .height(280.dp)
    ) {
      Column(
        modifier = Modifier
          .fillMaxSize()
          .padding(20.dp),
        verticalArrangement = Arrangement.SpaceBetween
      ) {
        Row(
          horizontalArrangement = Arrangement.spacedBy(8.dp),
          modifier = Modifier.fillMaxWidth()
        ) {
          Surface(
            shape = M3ExpressiveShapes.SlantedPill,
            color = if (activeMode == 0) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface,
            modifier = Modifier
              .weight(1f)
              .m3ExpressivePress { activeMode = 0 }
              .testTag("mode_translate_pill")
          ) {
            Row(
              modifier = Modifier.padding(vertical = 8.dp),
              horizontalArrangement = Arrangement.Center,
              verticalAlignment = Alignment.CenterVertically
            ) {
              Icon(
                imageVector = Icons.Default.Translate,
                contentDescription = null,
                tint = if (activeMode == 0) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.size(16.dp)
              )
              Spacer(modifier = Modifier.width(6.dp))
              Text(
                text = "Translate",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                color = if (activeMode == 0) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
              )
            }
          }

          Surface(
            shape = M3ExpressiveShapes.SlantedPill,
            color = if (activeMode == 1) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface,
            modifier = Modifier
              .weight(1f)
              .m3ExpressivePress { activeMode = 1 }
              .testTag("mode_math_pill")
          ) {
            Row(
              modifier = Modifier.padding(vertical = 8.dp),
              horizontalArrangement = Arrangement.Center,
              verticalAlignment = Alignment.CenterVertically
            ) {
              Icon(
                imageVector = Icons.Default.Calculate,
                contentDescription = null,
                tint = if (activeMode == 1) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                modifier = Modifier.size(16.dp)
              )
              Spacer(modifier = Modifier.width(6.dp))
              Text(
                text = "Solve Math",
                style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                color = if (activeMode == 1) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
              )
            }
          }
        }

        AnimatedContent(
          targetState = activeMode,
          transitionSpec = {
            (fadeIn() + scaleIn(initialScale = 0.95f, animationSpec = M3ExpressiveSpring.Snappy)).togetherWith(
              fadeOut() + scaleOut(targetScale = 0.95f)
            )
          },
          label = "ModeContentTransition"
        ) { mode ->
          if (mode == 0) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
              Surface(
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.surface,
                modifier = Modifier.fillMaxWidth()
              ) {
                Column(modifier = Modifier.padding(14.dp)) {
                  Text(
                    text = "Original text: Bonjour le monde!",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                  )
                  Spacer(modifier = Modifier.height(4.dp))
                  Text(
                    text = "Translated: Hello world!",
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                    color = MaterialTheme.colorScheme.primary
                  )
                }
              }
            }
          } else {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
              Surface(
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.surface,
                modifier = Modifier.fillMaxWidth()
              ) {
                Column(modifier = Modifier.padding(14.dp)) {
                  Text(
                    text = "Equation: ∫ 2x dx",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                  )
                  Spacer(modifier = Modifier.height(4.dp))
                  Text(
                    text = "Step-by-step Solution: x² + C",
                    style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                    color = MaterialTheme.colorScheme.tertiary
                  )
                }
              }
            }
          }
        }

        Row(verticalAlignment = Alignment.CenterVertically) {
          Icon(
            imageVector = Icons.Default.ImageSearch,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.primary,
            modifier = Modifier.size(16.dp)
          )
          Spacer(modifier = Modifier.width(6.dp))
          Text(
            text = "Multi-search: add text to refine image results",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant
          )
        }
      }
    }

    Spacer(modifier = Modifier.height(24.dp))

    Text(
      text = "Translate & Solve Instantly",
      style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Normal),
      color = MaterialTheme.colorScheme.onBackground
    )

    Spacer(modifier = Modifier.height(8.dp))

    Text(
      text = "Circle foreign menus, street signs, or tricky math problems to get live translations and step-by-step explanations.",
      style = MaterialTheme.typography.bodyLarge,
      color = MaterialTheme.colorScheme.onSurfaceVariant
    )
  }
}

/**
 * Step 5: Full privacy & data transparency from the dialog,
 * restyled in the same expressive language. Scrollable so nothing is lost.
 * The finish button stays disabled until the user ticks the accept row.
 */
@Composable
private fun CircleSearchPrivacyStep(
  accepted: Boolean,
  onAcceptChange: (Boolean) -> Unit
) {
  BoxWithConstraints(
    modifier = Modifier
      .fillMaxSize()
      .padding(bottom = 16.dp),
    contentAlignment = Alignment.BottomStart
  ) {
    val density = LocalDensity.current
    val collapsedMaxPx = with(density) { (maxHeight * 0.52f).toPx() }
    val fullMaxPx = with(density) { maxHeight.toPx() }
    var expandPx by remember { mutableFloatStateOf(0f) }
    val privacyScroll = rememberScrollState()
    val expandConnection = remember(collapsedMaxPx, fullMaxPx) {
      object : NestedScrollConnection {
        override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
          val dy = available.y
          val maxExpand = fullMaxPx - collapsedMaxPx
          if (dy < 0f && expandPx < maxExpand) {
            val take = minOf(-dy, maxExpand - expandPx)
            expandPx += take
            return Offset(0f, -take)
          }
          if (dy > 0f && expandPx > 0f && privacyScroll.value == 0) {
            val take = minOf(dy, expandPx)
            expandPx -= take
            return Offset(0f, take)
          }
          return Offset.Zero
        }
      }
    }
    Column(
      modifier = Modifier
        .fillMaxWidth()
        .heightIn(max = with(density) { (collapsedMaxPx + expandPx).toDp() })
        .background(
          color = MaterialTheme.colorScheme.background,
          shape = RoundedCornerShape(24.dp)
        )
        .nestedScroll(expandConnection)
        .verticalScroll(privacyScroll)
        .padding(16.dp)
        .testTag("circle_search_privacy_card"),
      verticalArrangement = Arrangement.spacedBy(12.dp)
    ) {
    Row(
      verticalAlignment = Alignment.CenterVertically,
      horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
      Surface(
        shape = M3ExpressiveShapes.Clover4,
        color = MaterialTheme.colorScheme.primaryContainer,
        modifier = Modifier.size(48.dp)
      ) {
        Box(contentAlignment = Alignment.Center) {
          Icon(
            imageVector = Icons.Default.Shield,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onPrimaryContainer,
            modifier = Modifier.size(24.dp)
          )
        }
      }
      Text(
        text = stringResource(R.string.title_privacy),
        style = MaterialTheme.typography.headlineSmall.copy(fontWeight = FontWeight.SemiBold),
        color = MaterialTheme.colorScheme.onBackground
      )
    }

    PrivacyBullet(
      icon = Icons.Default.CloudUpload,
      title = stringResource(R.string.privacy_q_upload),
      body = stringResource(R.string.privacy_a_upload)
    )
    PrivacyBullet(
      icon = Icons.Default.Info,
      title = stringResource(R.string.privacy_q_why_upload),
      body = stringResource(R.string.privacy_a_why_upload)
    )
    PrivacyBullet(
      icon = Icons.Default.Info,
      title = stringResource(R.string.privacy_q_safe),
      body = stringResource(R.string.privacy_a_safe),
      isWarning = true
    )
    PrivacyBullet(
      icon = Icons.Default.Warning,
      title = stringResource(R.string.privacy_q_personal),
      body = stringResource(R.string.privacy_a_personal),
      isWarning = true
    )
    PrivacyBullet(
      icon = Icons.Default.CheckCircle,
      title = stringResource(R.string.privacy_q_store),
      body = stringResource(R.string.privacy_a_store),
      isPositive = true
    )
    PrivacyBullet(
      icon = Icons.Default.CheckCircle,
      title = stringResource(R.string.privacy_q_developer),
      body = stringResource(R.string.privacy_a_developer),
      isPositive = true
    )

    Surface(
      shape = RoundedCornerShape(16.dp),
      color = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.3f),
      modifier = Modifier.fillMaxWidth()
    ) {
      Column(modifier = Modifier.padding(12.dp)) {
        Text(
          text = stringResource(R.string.privacy_reminder_title),
          style = MaterialTheme.typography.titleSmall,
          fontWeight = FontWeight.Bold,
          color = MaterialTheme.colorScheme.error
        )
        Spacer(modifier = Modifier.height(8.dp))
        Text(
          text = stringResource(R.string.privacy_reminder_body),
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurface
        )
      }
    }

    Surface(
      shape = M3ExpressiveShapes.SlantedPill,
      color = if (accepted) MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)
      else MaterialTheme.colorScheme.surfaceVariant,
      modifier = Modifier
        .fillMaxWidth()
        .clickable(
          role = Role.Checkbox,
          onClick = { onAcceptChange(!accepted) }
        )
        .testTag("privacy_accept_row")
    ) {
      Row(
        modifier = Modifier.padding(horizontal = 14.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically
      ) {
        Surface(
          shape = CircleShape,
          color = if (accepted) MaterialTheme.colorScheme.primary
          else MaterialTheme.colorScheme.outline.copy(alpha = 0.4f),
          modifier = Modifier.size(24.dp)
        ) {
          if (accepted) {
            Icon(
              imageVector = Icons.Default.Check,
              contentDescription = null,
              tint = MaterialTheme.colorScheme.onPrimary,
              modifier = Modifier.padding(5.dp)
            )
          }
        }
        Spacer(modifier = Modifier.width(12.dp))
        Text(
          text = "I understand how my data is handled",
          style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.Medium),
          color = MaterialTheme.colorScheme.onSurface
        )
      }
    }
    }
  }
}

@Composable
private fun PrivacyBullet(
  icon: ImageVector,
  title: String,
  body: String,
  isWarning: Boolean = false,
  isPositive: Boolean = false
) {
  Surface(
    shape = RoundedCornerShape(16.dp),
    color = when {
      isWarning -> MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.2f)
      isPositive -> MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.2f)
      else -> MaterialTheme.colorScheme.surface
    },
    modifier = Modifier.fillMaxWidth()
  ) {
    Row(
      modifier = Modifier.padding(12.dp),
      verticalAlignment = Alignment.Top
    ) {
      Icon(
        imageVector = icon,
        contentDescription = null,
        tint = when {
          isWarning -> MaterialTheme.colorScheme.error
          isPositive -> MaterialTheme.colorScheme.primary
          else -> MaterialTheme.colorScheme.onSurfaceVariant
        },
        modifier = Modifier.size(20.dp)
      )
      Spacer(modifier = Modifier.width(10.dp))
      Column {
        Text(
          text = title,
          style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.Bold),
          color = MaterialTheme.colorScheme.onSurface
        )
        Spacer(modifier = Modifier.height(2.dp))
        Text(
          text = body,
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurfaceVariant
        )
      }
    }
  }
}
