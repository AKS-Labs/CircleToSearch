package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Intent
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Accessibility
import androidx.compose.material.icons.rounded.Assistant
import androidx.compose.material.icons.rounded.Verified
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.components.AccessibilityDisclosureDialog
import kotlinx.coroutines.delay

@Composable
fun WobbleSetupWizard(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val view = LocalView.current
    var a11yGranted by remember { mutableStateOf(isA11yGranted(context)) }
    var assistantGranted by remember { mutableStateOf(isAssistantGranted(context)) }
    var showDisclosure by remember { mutableStateOf(false) }

    val refreshPermissions = {
        a11yGranted = isA11yGranted(context)
        assistantGranted = isAssistantGranted(context)
    }

    val settingsLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        refreshPermissions()
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, _ ->
            refreshPermissions()
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(Unit) {
        while (true) {
            refreshPermissions()
            delay(500)
        }
    }

    if (showDisclosure) {
        AccessibilityDisclosureDialog(
            onAccept = {
                showDisclosure = false
                runCatching {
                    settingsLauncher.launch(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                }
            },
            onDismiss = { showDisclosure = false },
        )
    }

    Box(modifier = modifier.fillMaxSize()) {
        Surface(
            shape = RoundedCornerShape(bottomStart = 28.dp, bottomEnd = 28.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            modifier = Modifier
                .fillMaxWidth()
                .fillMaxHeight(0.5f)
                .align(Alignment.TopCenter),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(
                        start = 24.dp,
                        end = 24.dp,
                        top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding() + 20.dp,
                        bottom = 24.dp,
                    ),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(
                    text = stringResource(R.string.wizard_title),
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
                Text(
                    text = stringResource(R.string.wizard_hint),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                WizardPermissionRow(
                    icon = Icons.Rounded.Accessibility,
                    title = stringResource(R.string.wizard_a11y_title),
                    subtitle = stringResource(R.string.label_enable_accessibility_subtitle),
                    granted = a11yGranted,
                    onClick = {
                        WobbleHaptics.performUIHaptic(view)
                        showDisclosure = true
                    },
                )
                WizardPermissionRow(
                    icon = Icons.Rounded.Assistant,
                    title = stringResource(R.string.wizard_assistant_title),
                    subtitle = stringResource(R.string.label_set_default_assistant_subtitle),
                    granted = assistantGranted,
                    onClick = {
                        WobbleHaptics.performUIHaptic(view)
                        runCatching {
                            settingsLauncher.launch(Intent(Settings.ACTION_VOICE_INPUT_SETTINGS))
                        }
                    },
                )
            }
        }
    }
}

@Composable
fun WizardPermissionRow(
    icon: ImageVector,
    title: String,
    subtitle: String,
    granted: Boolean,
    onClick: () -> Unit,
) {
    Surface(
        shape = RoundedCornerShape(24.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(28.dp),
            )
            Spacer(modifier = Modifier.width(16.dp))
            Column(
                modifier = Modifier.weight(1f),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(modifier = Modifier.width(12.dp))
            if (granted) {
                Icon(
                    imageVector = Icons.Rounded.Verified,
                    contentDescription = stringResource(R.string.wizard_granted),
                    tint = MaterialTheme.colorScheme.primary,
                )
            } else {
                FilledTonalButton(onClick = onClick) {
                    Text(stringResource(R.string.wizard_grant))
                }
            }
        }
    }
}
