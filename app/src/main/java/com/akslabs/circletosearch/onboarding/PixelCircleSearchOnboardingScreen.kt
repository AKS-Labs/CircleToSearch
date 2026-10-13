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
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.CloudUpload
import androidx.compose.material.icons.filled.Gesture
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
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
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.components.BackgroundPreset
import com.akslabs.circletosearch.ui.components.ExpressiveOnboardingScaffold
import com.akslabs.circletosearch.ui.theme.M3ExpressiveSpring
import com.akslabs.circletosearch.utils.PrivacyPreferences

/**
 * Circle to Search onboarding flow.
 *
 * Two steps, styled after the homescreen preview (WobbleHomeScreen):
 * - Warm welcome with a homescreen-style hero card (variable Google Sans Flex
 *   title that settles from wide/bold, primaryContainer card, gesture pill)
 * - Privacy & data transparency (must accept to finish)
 */
@Composable
fun PixelCircleSearchOnboardingScreen(
  onFinish: () -> Unit,
  reducedMotion: Boolean = false,
  modifier: Modifier = Modifier
) {
  var currentStep by remember { mutableStateOf(1) }
  val totalSteps = 2
  val context = LocalContext.current
  val privacyPreferences = remember { PrivacyPreferences(context) }
  var privacyAccepted by remember { mutableStateOf(privacyPreferences.hasAcceptedPrivacyPolicy()) }
  val isLastStep = currentStep == totalSteps

  // Dev visual tester: cycles the animated onboarding backgrounds one by one.
  val backgroundPresets = remember { BackgroundPreset.entries.toList() }
  var bgPresetIndex by remember {
    mutableStateOf(backgroundPresets.indexOf(BackgroundPreset.CircleSearch).coerceAtLeast(0))
  }

  Box(modifier = modifier) {
  ExpressiveOnboardingScaffold(
    modifier = Modifier.testTag("circle_search_onboarding_screen"),
    backgroundPreset = backgroundPresets[bgPresetIndex],
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
        1 -> CircleSearchWelcomeStep(reducedMotion = reducedMotion)
        else -> CircleSearchPrivacyStep(
          accepted = privacyAccepted,
          onAcceptChange = { privacyAccepted = it }
        )
      }
    }
  }

    // Dev-only pill button: tap to switch to the next background animation.
    Surface(
      shape = CircleShape,
      color = MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = 0.92f),
      modifier = Modifier
        .align(Alignment.BottomEnd)
        .navigationBarsPadding()
        .padding(end = 16.dp, bottom = 96.dp)
        .clickable { bgPresetIndex = (bgPresetIndex + 1) % backgroundPresets.size }
        .testTag("bg_preset_cycle_button")
    ) {
      Row(
        modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically
      ) {
        Icon(
          imageVector = Icons.Default.Refresh,
          contentDescription = "Switch background animation",
          tint = MaterialTheme.colorScheme.onSurfaceVariant,
          modifier = Modifier.size(16.dp)
        )
        Spacer(modifier = Modifier.width(6.dp))
        Text(
          text = backgroundPresets[bgPresetIndex].name,
          style = MaterialTheme.typography.labelMedium,
          color = MaterialTheme.colorScheme.onSurfaceVariant
        )
      }
    }
  }
}

/**
 * Homescreen-style variable font: Google Sans Flex with the ROND axis, animating
 * width/weight exactly like the WobbleHomeScreen hero card.
 */
@OptIn(ExperimentalTextApi::class)
@Composable
private fun rememberHomeHeroFontFamily(width: Float, weight: Int): FontFamily =
  remember(width, weight) {
    try {
      FontFamily(
        Font(
          R.font.google_sans_flex,
          variationSettings = FontVariation.Settings(
            FontVariation.width(width),
            FontVariation.weight(weight),
            FontVariation.Setting("ROND", 100f),
          ),
        )
      )
    } catch (_: Exception) {
      FontFamily.Default
    }
  }

/**
 * Step 1: Warm welcome built like the homescreen preview — a primaryContainer
 * hero card with a variable-font title that springs from wide/bold to rest,
 * the gesture-handle pill, and homescreen-style chips below.
 */
@Composable
private fun CircleSearchWelcomeStep(reducedMotion: Boolean) {
  val context = LocalContext.current
  val enterProgress = remember { Animatable(if (reducedMotion) 1f else 0f) }
  val heroScale = remember { Animatable(if (reducedMotion) 1f else 0.92f) }

  LaunchedEffect(Unit) {
    if (!reducedMotion) {
      heroScale.animateTo(1f, animationSpec = M3ExpressiveSpring.Bouncy)
    }
    enterProgress.animateTo(1f, animationSpec = M3ExpressiveSpring.Gentle)
  }

  // Wide/bold settles to the resting face — the pull-to-refresh feel of the home hero.
  val fontWidth = 130f - 30f * enterProgress.value
  val fontWeight = (700f - 200f * enterProgress.value).toInt()
  val heroFontFamily = rememberHomeHeroFontFamily(width = fontWidth, weight = fontWeight)

  // Soft breathing pulse on the gesture pill, like the home chevron affordance.
  val infiniteTransition = rememberInfiniteTransition(label = "WelcomePulse")
  val pillAlpha by if (reducedMotion) {
    remember { mutableFloatStateOf(1f) }
  } else {
    infiniteTransition.animateFloat(
      initialValue = 0.55f,
      targetValue = 1f,
      animationSpec = infiniteRepeatable(
        animation = tween(durationMillis = 1400, easing = LinearEasing),
        repeatMode = RepeatMode.Reverse
      ),
      label = "PillAlpha"
    )
  }

  Column(
    modifier = Modifier
      .fillMaxWidth()
      .padding(bottom = 16.dp)
  ) {
    // Hero content floats directly over the animated background — no card
    // container, so the preset animation stays fully visible behind it.
    Box(
      modifier = Modifier
        .fillMaxWidth()
        .padding(bottom = 8.dp)
        .graphicsLayer {
          scaleX = heroScale.value
          scaleY = heroScale.value
        }
        .testTag("circle_search_hero_card"),
      contentAlignment = Alignment.Center
    ) {
      Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
      ) {
        val launcherIcon = remember(context) {
          val drawable = context.getDrawable(R.mipmap.ic_launcher) ?: return@remember null
          val w = drawable.intrinsicWidth.coerceAtLeast(1)
          val h = drawable.intrinsicHeight.coerceAtLeast(1)
          val bitmap = android.graphics.Bitmap.createBitmap(
            w, h, android.graphics.Bitmap.Config.ARGB_8888
          )
          drawable.setBounds(0, 0, w, h)
          drawable.draw(android.graphics.Canvas(bitmap))
          bitmap.asImageBitmap()
        }
        if (launcherIcon != null) {
          Icon(
            bitmap = launcherIcon,
            contentDescription = stringResource(R.string.cd_app_icon),
            tint = Color.Unspecified,
            modifier = Modifier
              .size(64.dp)
              .clip(RoundedCornerShape(18.dp))
          )
        }

        Spacer(modifier = Modifier.height(16.dp))

        Text(
          text = "Circle to Search",
          style = MaterialTheme.typography.titleLarge.copy(fontFamily = heroFontFamily),
          color = MaterialTheme.colorScheme.onBackground
        )

        Spacer(modifier = Modifier.height(4.dp))

        Text(
          text = "Ready on your device",
          style = MaterialTheme.typography.bodyMedium,
          color = MaterialTheme.colorScheme.onSurfaceVariant
        )

        Spacer(modifier = Modifier.height(18.dp))

        Surface(
          shape = RoundedCornerShape(50),
          color = MaterialTheme.colorScheme.primary,
          modifier = Modifier
            .width(96.dp)
            .height(6.dp)
            .graphicsLayer { alpha = pillAlpha }
        ) {}
      }
    }

    Spacer(modifier = Modifier.height(24.dp))

    Text(
      text = "Search anything on your screen",
      style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Normal),
      color = MaterialTheme.colorScheme.onBackground
    )

    Spacer(modifier = Modifier.height(8.dp))

    Text(
      text = "Circle text, images, and QR codes without switching apps — straight from any screen with one gesture. Free, open source, and no account needed.",
      style = MaterialTheme.typography.bodyLarge,
      color = MaterialTheme.colorScheme.onSurfaceVariant
    )

    Spacer(modifier = Modifier.height(16.dp))

    Row(
      horizontalArrangement = Arrangement.spacedBy(10.dp),
      verticalAlignment = Alignment.CenterVertically
    ) {
      HomeChip(
        icon = Icons.Default.Gesture,
        label = "One-Gesture Search",
        containerColor = MaterialTheme.colorScheme.primaryContainer,
        contentColor = MaterialTheme.colorScheme.onPrimaryContainer
      )
      HomeChip(
        icon = null,
        label = "No Account Needed",
        containerColor = MaterialTheme.colorScheme.secondaryContainer,
        contentColor = MaterialTheme.colorScheme.onSecondaryContainer
      )
    }
  }
}

/** Homescreen-style pill chip (matches the "Ready" chip on the home hero card). */
@Composable
private fun HomeChip(
  icon: ImageVector?,
  label: String,
  containerColor: Color,
  contentColor: Color
) {
  Surface(
    shape = CircleShape,
    color = containerColor
  ) {
    Row(
      modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
      verticalAlignment = Alignment.CenterVertically
    ) {
      if (icon != null) {
        Icon(
          imageVector = icon,
          contentDescription = null,
          tint = contentColor,
          modifier = Modifier.size(16.dp)
        )
        Spacer(modifier = Modifier.width(6.dp))
      }
      Text(
        text = label,
        style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.Medium),
        color = contentColor
      )
    }
  }
}

/**
 * Step 2: Full privacy & data transparency, restyled in the homescreen's
 * surface language (surfaceContainerLow cards, 28dp corners). Scrollable so
 * nothing is lost. The finish button stays disabled until the accept row is ticked.
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
        .clip(RoundedCornerShape(28.dp))
        .background(MaterialTheme.colorScheme.surfaceContainerLow)
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
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.secondaryContainer,
        modifier = Modifier.size(48.dp)
      ) {
        Box(contentAlignment = Alignment.Center) {
          Icon(
            imageVector = Icons.Default.Shield,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSecondaryContainer,
            modifier = Modifier.size(24.dp)
          )
        }
      }
      Text(
        text = stringResource(R.string.title_privacy),
        style = MaterialTheme.typography.headlineSmall.copy(fontWeight = FontWeight.SemiBold),
        color = MaterialTheme.colorScheme.onSurface
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
      shape = RoundedCornerShape(28.dp),
      color = if (accepted) MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)
      else MaterialTheme.colorScheme.surfaceContainerHighest,
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
      isWarning -> MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.25f)
      isPositive -> MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.35f)
      else -> MaterialTheme.colorScheme.surfaceContainer
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
