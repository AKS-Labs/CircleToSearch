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
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
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
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Shield
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.components.BackgroundPreset
import com.akslabs.circletosearch.ui.components.ExpressiveOnboardingScaffold
import com.akslabs.circletosearch.ui.theme.M3ExpressiveSpring
import com.akslabs.circletosearch.utils.PrivacyPreferences

/**
 * Circle to Search onboarding flow.
 *
 * Two steps, styled after the homescreen preview (WobbleHomeScreen):
 * - Warm welcome with the app icon, a variable Google Sans Flex title that
 *   settles from wide/bold, and the gesture pill — floating directly over the
 *   animated background (default preset: PixelSetup)
 * - Privacy & data transparency as a professional grouped list (must accept
 *   to finish)
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
    mutableStateOf(backgroundPresets.indexOf(BackgroundPreset.PixelSetup).coerceAtLeast(0))
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

    // Invisible dev-only hotspot (top-right): cycles background animations and
    // toasts the active preset name. No visible UI — only the developer knows.
    Box(
      modifier = Modifier
        .align(Alignment.TopEnd)
        .statusBarsPadding()
        .padding(top = 8.dp, end = 8.dp)
        .size(48.dp)
        .clickable(
          interactionSource = remember { MutableInteractionSource() },
          indication = null
        ) {
          bgPresetIndex = (bgPresetIndex + 1) % backgroundPresets.size
          android.widget.Toast
            .makeText(
              context,
              "BG: ${backgroundPresets[bgPresetIndex].name}",
              android.widget.Toast.LENGTH_SHORT
            )
            .show()
        }
    )
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
 * Step 2: Full privacy & data transparency as a professional grouped list —
 * one homescreen-style card with hairline dividers, neutral rows, and a
 * restrained accent palette. Scrollable so nothing is lost. The finish button
 * stays disabled until the accept row is ticked.
 */
@Composable
private fun CircleSearchPrivacyStep(
  accepted: Boolean,
  onAcceptChange: (Boolean) -> Unit
) {
  // Full-height list: everything is visible immediately from the top; it only
  // scrolls when the content genuinely overflows the available area.
  Column(
    modifier = Modifier
      .fillMaxSize()
      .verticalScroll(rememberScrollState())
      .padding(bottom = 16.dp)
      .testTag("circle_search_privacy_card"),
    verticalArrangement = Arrangement.spacedBy(12.dp)
  ) {
    Row(
      verticalAlignment = Alignment.CenterVertically,
      horizontalArrangement = Arrangement.spacedBy(12.dp)
    ) {
      Surface(
        shape = RoundedCornerShape(14.dp),
        color = MaterialTheme.colorScheme.secondaryContainer,
        modifier = Modifier.size(44.dp)
      ) {
        Box(contentAlignment = Alignment.Center) {
          Icon(
            imageVector = Icons.Default.Shield,
            contentDescription = null,
            tint = MaterialTheme.colorScheme.onSecondaryContainer,
            modifier = Modifier.size(22.dp)
          )
        }
      }
      Column {
        Text(
          text = stringResource(R.string.title_privacy),
          style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.SemiBold),
          color = MaterialTheme.colorScheme.onSurface
        )
        Text(
          text = "How your searches are handled",
          style = MaterialTheme.typography.bodySmall,
          color = MaterialTheme.colorScheme.onSurfaceVariant
        )
      }
    }

    // One grouped card, homescreen-style: rows separated by hairline dividers.
    Surface(
      shape = RoundedCornerShape(28.dp),
      color = MaterialTheme.colorScheme.surfaceContainerLow,
      modifier = Modifier.fillMaxWidth()
    ) {
      Column {
        PrivacyRow(
          icon = Icons.Default.Lock,
          title = stringResource(R.string.privacy_q_device),
          body = stringResource(R.string.privacy_a_device),
          tone = PrivacyRowTone.Positive
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.CloudUpload,
          title = stringResource(R.string.privacy_q_upload),
          body = stringResource(R.string.privacy_a_upload),
          links = mapOf(
            "ImageDB" to "https://imgdb.io/privacy",
            "Litterbox" to "https://catbox.moe/legal.php"
          )
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.Info,
          title = stringResource(R.string.privacy_q_why_upload),
          body = stringResource(R.string.privacy_a_why_upload)
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.Info,
          title = stringResource(R.string.privacy_q_safe),
          body = stringResource(R.string.privacy_a_safe),
          links = mapOf(
            "ImageDB" to "https://imgdb.io/privacy",
            "Litterbox" to "https://catbox.moe/legal.php"
          )
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.Warning,
          title = stringResource(R.string.privacy_q_personal),
          body = stringResource(R.string.privacy_a_personal),
          tone = PrivacyRowTone.Warning
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.CheckCircle,
          title = stringResource(R.string.privacy_q_store),
          body = stringResource(R.string.privacy_a_store),
          tone = PrivacyRowTone.Positive
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.CheckCircle,
          title = stringResource(R.string.privacy_q_developer),
          body = stringResource(R.string.privacy_a_developer),
          tone = PrivacyRowTone.Positive
        )
        PrivacyDivider()
        PrivacyRow(
          icon = Icons.Default.Warning,
          title = stringResource(R.string.privacy_reminder_title),
          body = stringResource(R.string.privacy_reminder_body),
          tone = PrivacyRowTone.Warning
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

private enum class PrivacyRowTone { Neutral, Warning, Positive }

@Composable
private fun PrivacyRow(
  icon: ImageVector,
  title: String,
  body: String,
  tone: PrivacyRowTone = PrivacyRowTone.Neutral,
  links: Map<String, String> = emptyMap()
) {
  val linkColor = MaterialTheme.colorScheme.primary
  // Wraps every occurrence of a link name (e.g. "ImageDB", "Litterbox") in a
  // clickable URL span that opens the service's privacy policy page.
  val bodyText = remember(body, links, linkColor) {
    buildAnnotatedString {
      if (links.isEmpty()) {
        append(body)
      } else {
        var i = 0
        while (i <= body.length) {
          val next = links.entries
            .mapNotNull { (name, url) ->
              val idx = body.indexOf(name, i)
              if (idx >= 0) Triple(idx, name, url) else null
            }
            .minByOrNull { it.first }
          if (next == null) {
            if (i < body.length) append(body.substring(i))
            break
          }
          val (idx, name, url) = next
          append(body.substring(i, idx))
          withLink(LinkAnnotation.Url(url)) {
            withStyle(
              SpanStyle(
                color = linkColor,
                fontWeight = FontWeight.Medium,
                textDecoration = TextDecoration.Underline
              )
            ) {
              append(name)
            }
          }
          i = idx + name.length
        }
      }
    }
  }
  Row(
    modifier = Modifier
      .fillMaxWidth()
      .padding(horizontal = 16.dp, vertical = 12.dp),
    verticalAlignment = Alignment.Top
  ) {
    Icon(
      imageVector = icon,
      contentDescription = null,
      tint = when (tone) {
        PrivacyRowTone.Warning -> MaterialTheme.colorScheme.error
        PrivacyRowTone.Positive -> MaterialTheme.colorScheme.primary
        PrivacyRowTone.Neutral -> MaterialTheme.colorScheme.onSurfaceVariant
      },
      modifier = Modifier.size(22.dp)
    )
    Spacer(modifier = Modifier.width(16.dp))
    Column {
      Text(
        text = title,
        style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.SemiBold),
        color = MaterialTheme.colorScheme.onSurface
      )
      Spacer(modifier = Modifier.height(4.dp))
      Text(
        text = bodyText,
        style = MaterialTheme.typography.bodySmall,
        lineHeight = 18.sp,
        color = MaterialTheme.colorScheme.onSurfaceVariant
      )
    }
  }
}

@Composable
private fun PrivacyDivider() {
  HorizontalDivider(
    modifier = Modifier.padding(horizontal = 16.dp),
    thickness = 0.5.dp,
    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.1f)
  )
}
