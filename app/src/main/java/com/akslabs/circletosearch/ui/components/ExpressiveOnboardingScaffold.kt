package com.akslabs.circletosearch.ui.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.fadeIn
import androidx.compose.animation.slideInVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material.icons.filled.MotionPhotosOff
import androidx.compose.material.icons.filled.MotionPhotosOn
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/**
 * Reusable scaffold for Google Pixel & Material 3 Expressive onboarding flows.
 *
 * Implements:
 * - Animated organic canvas background
 * - Full edge-to-edge window insets handling
 * - Expressive top bar with navigation, step indicators, and quick action chips
 * - Spacious content slot with staggered entrance animation
 * - Bottom action bar with secondary & primary native M3 expressive buttons
 */
@Composable
fun ExpressiveOnboardingScaffold(
  modifier: Modifier = Modifier,
  backgroundPreset: BackgroundPreset = BackgroundPreset.PixelSetup,
  reducedMotion: Boolean = false,
  totalSteps: Int = 1,
  currentStep: Int = 1,
  onBackClick: (() -> Unit)? = null,
  topActionIcon: ImageVector? = Icons.Default.LocationOn,
  topActionDescription: String? = "Location options",
  onTopActionClick: (() -> Unit)? = null,
  secondaryActionText: String? = null,
  onSecondaryActionClick: (() -> Unit)? = null,
  primaryActionText: String = "Get started",
  primaryActionEnabled: Boolean = true,
  onPrimaryActionClick: () -> Unit,
  content: @Composable () -> Unit,
) {
  // Staggered entrance animation control
  var showContent by remember(reducedMotion) { mutableStateOf(reducedMotion) }
  var showBottomBar by remember(reducedMotion) { mutableStateOf(reducedMotion) }

  LaunchedEffect(reducedMotion) {
    if (!reducedMotion) {
      delay(120)
      showContent = true
      delay(200)
      showBottomBar = true
    }
  }

  Box(
    modifier = modifier
      .fillMaxSize()
      .background(MaterialTheme.colorScheme.background)
  ) {
    // 1. Organic Vector Animated Background
    ExpressiveOnboardingBackground(
      preset = backgroundPreset,
      reducedMotion = reducedMotion
    )

    // 2. Foreground Layout with full Insets
    Column(
      modifier = Modifier
        .fillMaxSize()
        .statusBarsPadding()
        .navigationBarsPadding()
        .padding(horizontal = 24.dp),
      verticalArrangement = Arrangement.SpaceBetween
    ) {
      // Top Navigation / Action Bar
      Row(
        modifier = Modifier
          .fillMaxWidth()
          .padding(top = 12.dp, bottom = 8.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically
      ) {
        // Left: Back button or Step Indicators
        if (onBackClick != null) {
          IconButton(
            onClick = onBackClick,
            modifier = Modifier
              .size(48.dp)
              .testTag("onboarding_back_button")
              .semantics { contentDescription = "Navigate back" }
          ) {
            Icon(
              imageVector = Icons.AutoMirrored.Filled.ArrowBack,
              contentDescription = "Back",
              tint = MaterialTheme.colorScheme.onBackground
            )
          }
        } else {
          // Subtle logo or empty spacer
          Spacer(modifier = Modifier.size(48.dp))
        }

        // Center: Expressive Step Indicator Dots if multi-step
        if (totalSteps > 1) {
          Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
              .clip(CircleShape)
              .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f))
              .padding(horizontal = 12.dp, vertical = 6.dp)
              .testTag("step_indicator")
          ) {
            for (step in 1..totalSteps) {
              val isCurrent = step == currentStep
              val isPassed = step < currentStep
              Box(
                modifier = Modifier
                  .size(if (isCurrent) 18.dp else 8.dp, 8.dp)
                  .clip(CircleShape)
                  .background(
                    when {
                      isCurrent -> MaterialTheme.colorScheme.primary
                      isPassed -> MaterialTheme.colorScheme.primary.copy(alpha = 0.5f)
                      else -> MaterialTheme.colorScheme.outline.copy(alpha = 0.3f)
                    }
                  )
              )
            }
          }
        } else {
          Spacer(modifier = Modifier.weight(1f))
        }

        // Right: Pixel Signature Pill Chip (e.g. Location chip in screenshot)
        if (topActionIcon != null && onTopActionClick != null) {
          Surface(
            shape = CircleShape,
            color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.85f),
            modifier = Modifier
              .size(48.dp, 36.dp)
              .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onTopActionClick
              )
              .testTag("top_action_pill")
              .semantics {
                role = Role.Button
                contentDescription = topActionDescription ?: "Action"
              }
          ) {
            Box(
              contentAlignment = Alignment.Center,
              modifier = Modifier.fillMaxSize()
            ) {
              Icon(
                imageVector = topActionIcon,
                contentDescription = topActionDescription,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(20.dp)
              )
            }
          }
        } else {
          Spacer(modifier = Modifier.size(48.dp))
        }
      }

      // Middle: Spacious Animated Content
      AnimatedVisibility(
        visible = showContent,
        enter = if (reducedMotion) EnterTransition.None else (
          fadeIn(animationSpec = androidx.compose.animation.core.tween(350)) +
              slideInVertically(
                initialOffsetY = { it / 8 },
                animationSpec = androidx.compose.animation.core.tween(400)
              )
        ),
        modifier = Modifier
          .weight(1f)
          .fillMaxWidth()
      ) {
        Box(
          modifier = Modifier.fillMaxSize(),
          contentAlignment = Alignment.BottomStart
        ) {
          content()
        }
      }

      // Bottom: Expressive Action Bar (Matches Pixel setup layout)
      AnimatedVisibility(
        visible = showBottomBar,
        enter = if (reducedMotion) EnterTransition.None else (
          fadeIn(animationSpec = androidx.compose.animation.core.tween(300)) +
              slideInVertically(
                initialOffsetY = { it / 4 },
                animationSpec = androidx.compose.animation.core.tween(350)
              )
        ),
        modifier = Modifier.fillMaxWidth()
      ) {
        Row(
          modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 18.dp),
          horizontalArrangement = Arrangement.SpaceBetween,
          verticalAlignment = Alignment.CenterVertically
        ) {
          // Secondary text action (e.g. "Emergency call" in setup screenshot)
          if (secondaryActionText != null && onSecondaryActionClick != null) {
            TextButton(
              onClick = onSecondaryActionClick,
              modifier = Modifier
                .testTag("secondary_action_button")
                .semantics { role = Role.Button }
            ) {
              Text(
                text = secondaryActionText,
                style = MaterialTheme.typography.labelLarge.copy(
                  fontWeight = FontWeight.Medium,
                  fontSize = 15.sp
                ),
                color = MaterialTheme.colorScheme.secondary
              )
            }
          } else {
            Spacer(modifier = Modifier.weight(1f))
          }

          // Primary native pill button (e.g. "Get started" in setup screenshot)
          Button(
            onClick = onPrimaryActionClick,
            enabled = primaryActionEnabled,
            shape = RoundedCornerShape(28.dp),
            colors = ButtonDefaults.buttonColors(
              containerColor = MaterialTheme.colorScheme.primary,
              contentColor = MaterialTheme.colorScheme.onPrimary
            ),
            modifier = Modifier
              .height(52.dp)
              .testTag("primary_action_button")
              .semantics { role = Role.Button }
          ) {
            Text(
              text = primaryActionText,
              style = MaterialTheme.typography.labelLarge.copy(
                fontWeight = FontWeight.SemiBold,
                fontSize = 15.sp
              ),
              modifier = Modifier.padding(horizontal = 12.dp)
            )
          }
        }
      }
    }
  }
}
