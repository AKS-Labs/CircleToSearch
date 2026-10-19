/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *  *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *  *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *  *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 */


package com.akslabs.circletosearch

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.systemBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.onboarding.PixelCircleSearchOnboardingScreen
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import com.akslabs.circletosearch.ui.wobblepulldown.WobbleDeviceInfoActivity
import com.akslabs.circletosearch.ui.wobblepulldown.WobbleHomeScreen
import com.akslabs.circletosearch.ui.wobblepulldown.isA11yGranted
import com.akslabs.circletosearch.ui.wobblepulldown.isAssistantGranted
import com.akslabs.circletosearch.ui.components.DonateBottomSheet
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private var pendingSettingsPage by mutableStateOf<String?>(null)
    private var navTick by mutableStateOf(0)
    private var forceOnboarding by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        pendingSettingsPage = intent.getStringExtra(EXTRA_SETTINGS_PAGE)
        forceOnboarding = intent.getBooleanExtra(EXTRA_SHOW_ONBOARDING, false)
        Toast.makeText(this, getString(R.string.toast_getting_started), Toast.LENGTH_LONG).show()
        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    val context = LocalContext.current
                    val appPrefs = remember {
                        context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
                    }
                    var showOnboarding by remember {
                        mutableStateOf(
                            forceOnboarding || !appPrefs.getBoolean("onboarding_completed", false)
                        )
                    }
                    if (showOnboarding) {
                        PixelCircleSearchOnboardingScreen(
                            onFinish = {
                                appPrefs.edit().putBoolean("onboarding_completed", true).apply()
                                showOnboarding = false
                            }
                        )
                    } else {
                        var currentScreen by remember {
                            mutableStateOf(if (pendingSettingsPage != null) "settings" else "home")
                        }
                        LaunchedEffect(navTick) {
                            if (navTick > 0 && pendingSettingsPage != null) {
                                currentScreen = "settings"
                            }
                        }
                        androidx.compose.animation.Crossfade(targetState = currentScreen) { screen ->
                            when (screen) {
                                "settings" -> key(navTick) {
                                    com.akslabs.circletosearch.ui.OverlaySettingsScreen(
                                        onBack = {
                                            pendingSettingsPage = null
                                            currentScreen = "home"
                                        },
                                        initialPage = pendingSettingsPage,
                                    )
                                }
                                else -> {
                                    var wobbleTick by remember { mutableStateOf(0) }
                                    var wizardBackdrop by remember { mutableStateOf(false) }
                                    val wizardScope = rememberCoroutineScope()
                                    val wizardLauncher = rememberLauncherForActivityResult(
                                        ActivityResultContracts.StartActivityForResult()
                                    ) { result ->
                                        if (result.resultCode == Activity.RESULT_OK) {
                                            wobbleTick++
                                        }
                                        wizardScope.launch {
                                            delay(400)
                                            wizardBackdrop = false
                                        }
                                    }
                                    LaunchedEffect(Unit) {
                                        val granted = isA11yGranted(context) || isAssistantGranted(context)
                                        if (!granted) {
                                            wizardBackdrop = true
                                            val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
                                                .putExtra(WobbleDeviceInfoActivity.EXTRA_SETUP_WIZARD, true)
                                            val options = androidx.core.app.ActivityOptionsCompat.makeCustomAnimation(
                                                context,
                                                R.anim.anim_slide_in_top,
                                                R.anim.anim_stay,
                                            )
                                            wizardLauncher.launch(intent, options)
                                        }
                                    }
                                    Box(modifier = Modifier.fillMaxSize()) {
                                        WobbleHomeScreen(
                                            onOpenDeviceInfo = {
                                                val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
                                                val options = androidx.core.app.ActivityOptionsCompat.makeCustomAnimation(
                                                    context,
                                                    R.anim.anim_slide_in_top,
                                                    R.anim.anim_stay,
                                                )
                                                context.startActivity(intent, options.toBundle())
                                            },
                                            onOpenSettings = { page ->
                                                pendingSettingsPage = page
                                                currentScreen = "settings"
                                            },
                                            wobbleEvent = wobbleTick,
                                            contentPadding = WindowInsets.systemBars.asPaddingValues(),
                                        )
                                        AnimatedVisibility(
                                            visible = wizardBackdrop,
                                            enter = EnterTransition.None,
                                            exit = fadeOut(animationSpec = tween(durationMillis = 300)),
                                        ) {
                                            Box(
                                                modifier = Modifier
                                                    .fillMaxSize()
                                                    .background(Color.Black.copy(alpha = 0.10f)),
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        pendingSettingsPage = intent.getStringExtra(EXTRA_SETTINGS_PAGE)
        navTick = navTick + 1
    }

    companion object {
        const val EXTRA_SETTINGS_PAGE = "settings_page"
        /** Dev/testing: re-show the onboarding flow without clearing app data. */
        const val EXTRA_SHOW_ONBOARDING = "show_onboarding"
    }
}

// Helper Functions
fun isAccessibilityServiceEnabled(context: android.content.Context): Boolean {
    val expectedComponentName = android.content.ComponentName(context, CircleToSearchAccessibilityService::class.java)
    val enabledServicesSetting = Settings.Secure.getString(
        context.contentResolver,
        Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES
    ) ?: return false
    
    val colonSplitter = android.text.TextUtils.SimpleStringSplitter(':')
    colonSplitter.setString(enabledServicesSetting)
    
    while (colonSplitter.hasNext()) {
        val componentNameString = colonSplitter.next()
        val enabledComponent = android.content.ComponentName.unflattenFromString(componentNameString)
        if (enabledComponent != null && enabledComponent == expectedComponentName)
            return true
    }
    return false
}

fun isDefaultAssistant(context: android.content.Context): Boolean {
    // Basic check: triggers the settings intent, but actual "is default" check is complex
    // on some Android versions. For simplified UI, we might relay on user return, 
    // or use RoleManager on Android 10+. 
    // For now, let's keep it simple or check specific secure settings if possible.
    // A reliable check is to see if we are arguably the voice interaction service.
    
    val assistant = Settings.Secure.getString(context.contentResolver, "voice_interaction_service")
    val component = android.content.ComponentName(context, CircleToSearchVoiceService::class.java)
    val myComponentString = component.flattenToString()
    
    // Check if our VoiceInteractionService is the one currently set as default
    return assistant == myComponentString
}

@Composable
fun SupportDialog(
    showCount: Int,
    onDismiss: () -> Unit,
    onDonate: () -> Unit,
    dontShowAgain: MutableState<Boolean>
) {
    androidx.compose.ui.window.Dialog(
        onDismissRequest = onDismiss,
        properties = androidx.compose.ui.window.DialogProperties(
            usePlatformDefaultWidth = false
        )
    ) {
        androidx.compose.material3.Surface(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp)
                .clip(androidx.compose.foundation.shape.RoundedCornerShape(28.dp)),
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 6.dp
        ) {
            // Outer column: wraps content naturally but caps height at 85% screen height
            // using BoxWithConstraints so the scroll only kicks in when needed
            androidx.compose.foundation.layout.BoxWithConstraints(
                modifier = Modifier.fillMaxWidth()
            ) {
                val maxDialogHeight = maxHeight * 0.85f
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = maxDialogHeight),
                    horizontalAlignment = Alignment.CenterHorizontally
                ) {
                    // ── Fixed header (always visible) ──────────────────────────
                    Column(
                        modifier = Modifier.padding(start = 24.dp, end = 24.dp, top = 24.dp),
                        horizontalAlignment = Alignment.CenterHorizontally
                    ) {
                        Icon(
                            painter = painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(48.dp)
                        )
                        Spacer(modifier = Modifier.height(16.dp))
                        Text(
                            text = stringResource(R.string.support_title),
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.ExtraBold,
                            textAlign = TextAlign.Center,
                            lineHeight = 28.sp,
                            modifier = Modifier.fillMaxWidth()
                        )
                        Spacer(modifier = Modifier.height(16.dp))
                    }

                    // ── Scrollable body ────────────────────────────────────────
                    Column(
                        modifier = Modifier
                            .weight(1f, fill = false)
                            .verticalScroll(rememberScrollState())
                            .padding(horizontal = 24.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        Text(
                            text = stringResource(R.string.support_body),
                            style = MaterialTheme.typography.bodyLarge,
                            lineHeight = 24.sp,
                            textAlign = TextAlign.Center,
                            fontWeight = FontWeight.Medium,
                            modifier = Modifier.fillMaxWidth()
                        )

                        if (showCount >= 7) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(androidx.compose.foundation.shape.RoundedCornerShape(8.dp))
                                    .clickable { dontShowAgain.value = !dontShowAgain.value }
                                    .padding(vertical = 4.dp)
                            ) {
                                Checkbox(
                                    checked = dontShowAgain.value,
                                    onCheckedChange = { dontShowAgain.value = it }
                                )
                                Text(
                                    text = stringResource(R.string.label_dont_show_again),
                                    style = MaterialTheme.typography.bodySmall,
                                    modifier = Modifier.padding(start = 8.dp)
                                )
                            }
                        }
                        Spacer(modifier = Modifier.height(4.dp))
                    }

                    // ── Fixed bottom buttons (always visible) ──────────────────
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 24.dp, vertical = 16.dp),
                        horizontalArrangement = Arrangement.End,
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        TextButton(onClick = onDismiss) {
                            Text(stringResource(R.string.close))
                        }
                        Spacer(modifier = Modifier.width(8.dp))
                        Button(
                            onClick = onDonate,
                            shape = androidx.compose.foundation.shape.RoundedCornerShape(14.dp),
                            contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp)
                        ) {
                            Text(stringResource(R.string.btn_donate), fontWeight = FontWeight.Bold)
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun SupportDialogHost(showDonateSheet: MutableState<Boolean>) {
    val context = LocalContext.current
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    val showSupportDialog = remember { mutableStateOf(!prefs.getBoolean("support_dialog_dismissed", false)) }
    val dontShowAgain = remember { mutableStateOf(false) }
    val showCount = remember { prefs.getInt("support_dialog_show_count", 0) }

    LaunchedEffect(showSupportDialog.value) {
        if (showSupportDialog.value) {
            prefs.edit().putInt("support_dialog_show_count", showCount + 1).apply()
        }
    }

    if (showSupportDialog.value) {
        SupportDialog(
            showCount = showCount + 1,
            onDismiss = {
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            onDonate = {
                showDonateSheet.value = true
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            dontShowAgain = dontShowAgain
        )
    }

    if (showDonateSheet.value) {
        DonateBottomSheet(
            onDismiss = { showDonateSheet.value = false },
            onDonateOptionSelected = { url ->
                try {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
                    context.startActivity(intent)
                } catch (e: Exception) {
                    android.widget.Toast.makeText(context, context.getString(R.string.toast_could_not_open_link), android.widget.Toast.LENGTH_SHORT).show()
                }
                showDonateSheet.value = false
            }
        )
    }
}

