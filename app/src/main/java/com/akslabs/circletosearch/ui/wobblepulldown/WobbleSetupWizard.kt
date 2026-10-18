package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
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
import androidx.compose.ui.text.style.TextAlign
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
    var showAssistantSheet by remember { mutableStateOf(false) }

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

    if (showAssistantSheet) {
        AssistantSetupSheet(
            onLaunch = { intent -> settingsLauncher.launch(intent) },
            onDismiss = { showAssistantSheet = false },
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
                    textAlign = TextAlign.Center,
                    modifier = Modifier.fillMaxWidth(),
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
                        if (a11yGranted) {
                            runCatching {
                                settingsLauncher.launch(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                            }
                        } else {
                            showDisclosure = true
                        }
                    },
                )
                WizardPermissionRow(
                    icon = Icons.Rounded.Assistant,
                    title = stringResource(R.string.wizard_assistant_title),
                    subtitle = stringResource(R.string.label_set_default_assistant_subtitle),
                    granted = assistantGranted,
                    onClick = {
                        WobbleHaptics.performUIHaptic(view)
                        showAssistantSheet = true
                    },
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun WizardPermissionRow(
    icon: ImageVector,
    title: String,
    subtitle: String,
    granted: Boolean,
    onClick: () -> Unit,
) {
    // The WHOLE row is the click target (ripple over everything), in both states:
    // un-granted opens the permission flow, granted still opens the system screen so
    // the user can review or revoke the permission.
    Surface(
        onClick = onClick,
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

/**
 * Bottom sheet for assistant setup: explains that CircleToSearch can act as the device
 * assistant (so the normal assistant trigger — long-press Home, gesture handle, etc. —
 * opens this app) and offers the picker that makes that happen.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AssistantSetupSheet(
    onLaunch: (Intent) -> Unit,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current

    // Try the canonical assistant-role picker first, then generic fallbacks that exist
    // on different OEM builds.
    fun launchFirst(vararg intents: Intent?): Boolean {
        for (intent in intents) {
            if (intent == null) continue
            val ok = runCatching { onLaunch(intent); true }.getOrDefault(false)
            if (ok) return true
        }
        return false
    }

    val roleManager = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        runCatching { context.getSystemService(android.app.role.RoleManager::class.java) }.getOrNull()
    } else {
        null
    }
    // If we already hold the assistant role the request intent is a no-op (launches
    // nothing visible), so skip it and go straight to the settings screen instead.
    val alreadyAssistant = roleManager?.isRoleHeld(android.app.role.RoleManager.ROLE_ASSISTANT) == true
    val roleIntent: Intent? = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q && !alreadyAssistant) {
        runCatching {
            roleManager?.createRequestRoleIntent(android.app.role.RoleManager.ROLE_ASSISTANT)
        }.getOrNull()
    } else {
        null
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 24.dp, end = 24.dp, bottom = 32.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = stringResource(R.string.assistant_sheet_title),
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = stringResource(R.string.assistant_sheet_body),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            Button(
                onClick = {
                    if (launchFirst(
                            // Same screen the permission row's grant action used to open.
                            Intent(Settings.ACTION_VOICE_INPUT_SETTINGS),
                            roleIntent,
                            Intent(Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS),
                            Intent(Settings.ACTION_SETTINGS),
                        )
                    ) {
                        onDismiss()
                    }
                },
                modifier = Modifier.fillMaxWidth(),
                contentPadding = PaddingValues(horizontal = 20.dp, vertical = 14.dp),
            ) {
                Icon(
                    imageVector = Icons.Rounded.Assistant,
                    contentDescription = null,
                    modifier = Modifier.size(20.dp),
                )
                Spacer(modifier = Modifier.width(10.dp))
                Text(
                    text = stringResource(R.string.assistant_opt_set_title),
                    fontWeight = FontWeight.Bold,
                )
            }
        }
    }
}
