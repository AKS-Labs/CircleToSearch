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

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.DialogProperties
import com.akslabs.circletosearch.onboarding.PixelCircleSearchOnboardingScreen
import com.akslabs.circletosearch.ui.components.PrivacyDialog
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import com.akslabs.circletosearch.ui.theme.M3ExpressiveShapes
import com.akslabs.circletosearch.ui.theme.m3ExpressivePress
import com.akslabs.circletosearch.ui.wobblepulldown.WobbleDeviceInfoActivity
import com.akslabs.circletosearch.ui.wobblepulldown.WobbleHomeScreen
import com.akslabs.circletosearch.utils.PrivacyPreferences
import dev.davidv.translator.MainActivity as TranslatorMainActivity
import com.akslabs.circletosearch.ui.components.DonateBottomSheet
import com.akslabs.circletosearch.ui.components.AccessibilityDisclosureDialog
import com.akslabs.circletosearch.ui.components.UnifiedSearchMethodSelector
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {

    private var pendingSettingsPage by mutableStateOf<String?>(null)
    private var navTick by mutableStateOf(0)
    private var forceOnboarding by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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
                                "setup" -> SetupScreen(
                                    onSettingsClick = {
                                        pendingSettingsPage = null
                                        currentScreen = "settings"
                                    },
                                    onBack = { currentScreen = "home" },
                                )
                                else -> WobbleHomeScreen(
                                    onOpenDeviceInfo = {
                                        val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
                                        val options = androidx.core.app.ActivityOptionsCompat.makeCustomAnimation(
                                            context,
                                            R.anim.anim_slide_in_top,
                                            R.anim.anim_stay,
                                        )
                                        context.startActivity(intent, options.toBundle())
                                    },
                                    onOpenSetup = { currentScreen = "setup" },
                                    onOpenSettings = {
                                        pendingSettingsPage = null
                                        currentScreen = "settings"
                                    },
                                )
                            }
                        }
                        BackHandler(enabled = currentScreen == "setup") {
                            currentScreen = "home"
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetupScreen(onSettingsClick: () -> Unit, onBack: (() -> Unit)? = null) {
    val context = LocalContext.current
    val lifecycleOwner = androidx.lifecycle.compose.LocalLifecycleOwner.current
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    
    // Privacy Dialog State
    val privacyPreferences = remember { PrivacyPreferences(context) }
    var showPrivacyDialog by remember { mutableStateOf(!privacyPreferences.hasAcceptedPrivacyPolicy()) }
    
    // Permission States
    var isAccessibilityEnabled by remember { mutableStateOf(isAccessibilityServiceEnabled(context)) }
    var isDefaultAssistant by remember { mutableStateOf(isDefaultAssistant(context)) }
    var showAccessibilityDisclosure by remember { mutableStateOf(false) }

    // Summary of enabled triggers for the home card (refreshed on resume)
    var triggerSummary by remember { mutableStateOf("") }
    fun refreshTriggerSummary() {
        val names = mutableListOf<String>()
        val overlayConfig = runCatching {
            com.akslabs.circletosearch.data.OverlayConfigurationManager(context).getConfig()
        }.getOrNull()
        if (overlayConfig?.isEnabled == true) names.add(context.getString(R.string.trigger_name_statusbar))
        if (prefs.getBoolean("bubble_enabled", false)) names.add(context.getString(R.string.trigger_name_bubble))
        if (prefs.getBoolean("trigger_double_vol_up", false)) names.add(context.getString(R.string.trigger_double_vol_up_title))
        if (prefs.getBoolean("trigger_double_vol_down", false)) names.add(context.getString(R.string.trigger_double_vol_down_title))
        if (prefs.getBoolean("trigger_double_power", false)) names.add(context.getString(R.string.trigger_double_power_title))
        if (prefs.getBoolean("trigger_power_vol_up", false)) names.add(context.getString(R.string.trigger_power_vol_up_title))
        if (prefs.getBoolean("trigger_power_vol_down", false)) names.add(context.getString(R.string.trigger_power_vol_down_title))
        if (prefs.getBoolean("trigger_vol_up_down", false)) names.add(context.getString(R.string.trigger_vol_up_down_title))
        if (isDefaultAssistant(context)) names.add(context.getString(R.string.trigger_name_assistant))
        val shown = names.take(3)
        val hidden = names.size - shown.size
        triggerSummary = when {
            names.isEmpty() -> context.getString(R.string.triggers_card_subtitle_none)
            hidden > 0 -> shown.joinToString(" \u00B7 ") + " \u00B7 " + context.getString(R.string.triggers_more, hidden)
            else -> shown.joinToString(" \u00B7 ")
        }
    }
    LaunchedEffect(Unit) { refreshTriggerSummary() }

    // Check permissions on Resume
    DisposableEffect(lifecycleOwner) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == androidx.lifecycle.Lifecycle.Event.ON_RESUME) {
                isAccessibilityEnabled = isAccessibilityServiceEnabled(context)
                isDefaultAssistant = isDefaultAssistant(context)
                refreshTriggerSummary()
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    val showSupportDialog = remember { mutableStateOf(!prefs.getBoolean("support_dialog_dismissed", false)) }
    val dontShowAgain = remember { mutableStateOf(false) }
    
    // Manage Support Dialog Show Count
    val showCount = remember { prefs.getInt("support_dialog_show_count", 0) }
    LaunchedEffect(showSupportDialog.value) {
        if (showSupportDialog.value) {
            prefs.edit().putInt("support_dialog_show_count", showCount + 1).apply()
        }
    }
    
    var showDonateSheet by remember { mutableStateOf(false) }
    val sheetState = rememberModalBottomSheetState()
    val scope = rememberCoroutineScope()
    
    // Show Privacy Dialog First - User must accept before seeing main UI
    if (showPrivacyDialog) {
        PrivacyDialog(
            onAccept = {
                privacyPreferences.setPrivacyAccepted()
                showPrivacyDialog = false
            }
        )
        return // Don't show main UI until accepted
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            // App icon + title on the left, settings gear on the right
            TopAppBar(
                navigationIcon = {
                    if (onBack != null) {
                        IconButton(onClick = onBack) {
                            Icon(
                                imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                                contentDescription = stringResource(R.string.cd_back),
                                tint = MaterialTheme.colorScheme.onSurfaceVariant
                            )
                        }
                    }
                },
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        val launcherIcon = remember {
                            val drawable = context.getDrawable(R.mipmap.ic_launcher)
                            requireNotNull(drawable) { "ic_launcher drawable missing" }
                            val w = drawable.intrinsicWidth.coerceAtLeast(1)
                            val h = drawable.intrinsicHeight.coerceAtLeast(1)
                            val bitmap = android.graphics.Bitmap.createBitmap(
                                w, h, android.graphics.Bitmap.Config.ARGB_8888
                            )
                            drawable.setBounds(0, 0, w, h)
                            drawable.draw(android.graphics.Canvas(bitmap))
                            bitmap.asImageBitmap()
                        }
                        Icon(
                            bitmap = launcherIcon,
                            contentDescription = stringResource(R.string.cd_app_icon),
                            tint = Color.Unspecified,
                            modifier = Modifier
                                .size(34.dp)
                                .clip(androidx.compose.foundation.shape.RoundedCornerShape(9.dp))
                        )
                        Spacer(modifier = Modifier.width(12.dp))
                        Text(
                            text = stringResource(R.string.header_title),
                            style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold),
                            color = MaterialTheme.colorScheme.onBackground
                        )
                    }
                },
                actions = {
                    IconButton(onClick = onSettingsClick) {
                        Icon(
                            imageVector = Icons.Default.Settings,
                            contentDescription = stringResource(R.string.cd_overlay_settings),
                            tint = MaterialTheme.colorScheme.onSurfaceVariant
                        )
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                    titleContentColor = MaterialTheme.colorScheme.onBackground,
                    actionIconContentColor = MaterialTheme.colorScheme.onSurfaceVariant
                )
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(modifier = Modifier.height(8.dp))

            Spacer(modifier = Modifier.height(16.dp))

            Surface(
                shape = M3ExpressiveShapes.SlantedPill,
                color = (if (isAccessibilityEnabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error).copy(alpha = 0.12f),
                modifier = Modifier.align(Alignment.Start)
            ) {
                Text(
                    text = stringResource(R.string.label_required),
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                    color = if (isAccessibilityEnabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 7.dp)
                )
            }
            Spacer(modifier = Modifier.height(10.dp))
            
            if (isAccessibilityEnabled) {
                Card(
                     colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.6f)
                    ),
                    shape = M3ExpressiveShapes.AsymmetricCard,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Surface(
                            shape = M3ExpressiveShapes.Clover4,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(48.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.CheckCircle,
                                    contentDescription = stringResource(R.string.cd_granted),
                                    tint = MaterialTheme.colorScheme.onPrimary,
                                    modifier = Modifier.size(24.dp)
                                )
                            }
                        }
                        Spacer(modifier = Modifier.width(16.dp))
                        Column {
                            Text(
                                text = stringResource(R.string.label_accessibility_active),
                                style = MaterialTheme.typography.titleMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            Text(
                                text = stringResource(R.string.label_accessibility_active_subtitle),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            } else {
                Card(
                    onClick = {
                        showAccessibilityDisclosure = true
                    },
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer
                    ),
                    shape = M3ExpressiveShapes.AsymmetricCard,
                    modifier = Modifier
                        .fillMaxWidth()
                        .m3ExpressivePress()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Surface(
                            shape = M3ExpressiveShapes.Clover4,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.size(48.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.Accessibility,
                                    contentDescription = null,
                                    modifier = Modifier.size(24.dp),
                                    tint = MaterialTheme.colorScheme.onError
                                )
                            }
                        }
                        Spacer(modifier = Modifier.width(16.dp))
                        Column {
                            Text(
                                text = stringResource(R.string.label_enable_accessibility),
                                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                                color = MaterialTheme.colorScheme.onErrorContainer
                            )
                             Text(
                                text = stringResource(R.string.label_enable_accessibility_subtitle),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onErrorContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            }
            
            Spacer(modifier = Modifier.height(24.dp))

            Surface(
                shape = M3ExpressiveShapes.SlantedPill,
                color = MaterialTheme.colorScheme.secondary.copy(alpha = 0.12f),
                modifier = Modifier.align(Alignment.Start)
            ) {
                Text(
                    text = stringResource(R.string.label_optional_assistant),
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                    color = MaterialTheme.colorScheme.secondary,
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 7.dp)
                )
            }
            Spacer(modifier = Modifier.height(10.dp))

            if (isDefaultAssistant) {
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.6f)
                    ),
                    shape = M3ExpressiveShapes.AsymmetricCard,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Surface(
                            shape = M3ExpressiveShapes.Clover4,
                            color = MaterialTheme.colorScheme.secondary,
                            modifier = Modifier.size(48.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.CheckCircle,
                                    contentDescription = stringResource(R.string.cd_active),
                                    tint = MaterialTheme.colorScheme.onSecondary,
                                    modifier = Modifier.size(24.dp)
                                )
                            }
                        }
                        Spacer(modifier = Modifier.width(16.dp))
                         Text(
                            text = stringResource(R.string.label_default_assistant_set),
                            style = MaterialTheme.typography.titleMedium,
                             color = MaterialTheme.colorScheme.onSecondaryContainer
                        )
                    }
                }
            } else {
                Card(
                    onClick = {
                        val intent = Intent(Settings.ACTION_VOICE_INPUT_SETTINGS)
                        context.startActivity(intent)
                    },
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.primaryContainer
                    ),
                    shape = M3ExpressiveShapes.AsymmetricCard,
                    modifier = Modifier
                        .fillMaxWidth()
                        .m3ExpressivePress()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Surface(
                            shape = M3ExpressiveShapes.Clover4,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(48.dp)
                        ) {
                            Box(contentAlignment = Alignment.Center) {
                                Icon(
                                    imageVector = Icons.Default.TouchApp,
                                    contentDescription = null,
                                    modifier = Modifier.size(24.dp),
                                    tint = MaterialTheme.colorScheme.onPrimary
                                )
                            }
                        }
                        Spacer(modifier = Modifier.width(16.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = stringResource(R.string.label_set_default_assistant),
                                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            Text(
                                text = stringResource(R.string.label_set_default_assistant_subtitle),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            }

            Spacer(modifier = Modifier.height(24.dp))

            Surface(
                shape = M3ExpressiveShapes.SlantedPill,
                color = MaterialTheme.colorScheme.secondary.copy(alpha = 0.12f),
                modifier = Modifier.align(Alignment.Start)
            ) {
                Text(
                    text = stringResource(R.string.label_triggers),
                    style = MaterialTheme.typography.labelMedium.copy(fontWeight = FontWeight.SemiBold),
                    color = MaterialTheme.colorScheme.secondary,
                    modifier = Modifier.padding(horizontal = 14.dp, vertical = 7.dp)
                )
            }
            Spacer(modifier = Modifier.height(10.dp))

            // Quick glance at what's configured — tap to manage all triggers
            Card(
                onClick = onSettingsClick,
                shape = M3ExpressiveShapes.AsymmetricCard,
                colors = CardDefaults.cardColors(
                    containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)
                ),
                modifier = Modifier
                    .fillMaxWidth()
                    .m3ExpressivePress()
            ) {
                Row(
                    modifier = Modifier.padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Surface(
                        shape = M3ExpressiveShapes.Clover4,
                        color = MaterialTheme.colorScheme.tertiaryContainer,
                        modifier = Modifier.size(48.dp)
                    ) {
                        Box(contentAlignment = Alignment.Center) {
                            Icon(
                                imageVector = Icons.Default.SettingsSuggest,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onTertiaryContainer,
                                modifier = Modifier.size(24.dp)
                            )
                        }
                    }
                    Spacer(modifier = Modifier.width(16.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = stringResource(R.string.triggers_card_title),
                            style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                            color = MaterialTheme.colorScheme.onSurface
                        )
                        Text(
                            text = triggerSummary,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 2,
                            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis
                        )
                    }
                    Icon(
                        Icons.Default.KeyboardArrowRight,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
//            Spacer(modifier = Modifier.height(25.dp))
//
//            // Privacy Note
//            Text(
//                text = "That’s it. No more permissions.\n We’re not trying to adopt your phone.",
//                style = MaterialTheme.typography.bodySmall,
//                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
//                textAlign = TextAlign.Center
//            )

            Spacer(modifier = Modifier.height(22.dp))


            // 5. Footer
            SocialLinksRow(
                context = context,
                onDonateClick = { showDonateSheet = true }
            )
            Spacer(modifier = Modifier.height(20.dp))
            Surface(
                shape = M3ExpressiveShapes.SlantedPill,
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
                modifier = Modifier.m3ExpressivePress(onClick = {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs"))
                    context.startActivity(intent)
                })
            ) {
                Text(
                    text = stringResource(R.string.label_from_akslabs),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.8f),
                    textAlign = TextAlign.Center,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(horizontal = 18.dp, vertical = 10.dp)
                )
            }
            Spacer(modifier = Modifier.height(2.dp))
        }
    }

    // Support Dialog & Sheet
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
                showDonateSheet = true
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            dontShowAgain = dontShowAgain
        )
    }
    
    if (showDonateSheet) {
        DonateBottomSheet(
            onDismiss = { showDonateSheet = false },
            onDonateOptionSelected = { url ->
                try {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
                    context.startActivity(intent)
                } catch (e: Exception) {
                    android.widget.Toast.makeText(context, context.getString(R.string.toast_could_not_open_link), android.widget.Toast.LENGTH_SHORT).show()
                }
                showDonateSheet = false
            }
        )
    }

    if (showAccessibilityDisclosure) {
        AccessibilityDisclosureDialog(
            onAccept = {
                showAccessibilityDisclosure = false
                try {
                    val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
                    context.startActivity(intent)
                } catch (e: Exception) {
                    android.widget.Toast.makeText(context, context.getString(R.string.toast_accessibility_settings_error), android.widget.Toast.LENGTH_LONG).show()
                }
            },
            onDismiss = {
                showAccessibilityDisclosure = false
            }
        )
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

// ... SocialLinksRow, SupportDialog, BubbleSwitch same as before ...
@Composable
fun SocialLinksRow(
    context: android.content.Context,
    onDonateClick: () -> Unit
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        IconButton(
            onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs"))
                context.startActivity(intent)
            }, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.github),
                contentDescription = stringResource(R.string.cd_github),
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp)
            )
        }
        
        IconButton(
            onClick = onDonateClick, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                contentDescription = stringResource(R.string.cd_donate),
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp)
            )
        }
        
        IconButton(
            onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://t.me/akslabs"))
                context.startActivity(intent)
            }, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.telegram),
                contentDescription = stringResource(R.string.cd_telegram),
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp).size(23.dp)
            )
        }
    }
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
                            textAlign = TextAlign.Center
                        )
                        Spacer(modifier = Modifier.height(16.dp))
                    }

                    // ── Scrollable body ────────────────────────────────────────
                    var isExpanded by remember { mutableStateOf(false) }
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
                            lineHeight = 22.sp,
                            textAlign = TextAlign.Center,
                            fontWeight = FontWeight.Medium
                        )

                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(androidx.compose.foundation.shape.RoundedCornerShape(16.dp))
                                .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f))
                                .padding(16.dp)
                                .animateContentSize()
                        ) {
                            Row(
                                modifier = Modifier.fillMaxWidth().clickable { isExpanded = !isExpanded },
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.SpaceBetween
                            ) {
                                Text(
                                    text = stringResource(R.string.support_planned_features),
                                    style = MaterialTheme.typography.titleSmall,
                                    fontWeight = FontWeight.Bold,
                                    color = MaterialTheme.colorScheme.primary
                                )
                                TextButton(onClick = { isExpanded = !isExpanded }, contentPadding = PaddingValues(0.dp)) {
                                    Icon(
                                        imageVector = if (isExpanded) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown,
                                        contentDescription = stringResource(if (isExpanded) R.string.cd_collapse else R.string.cd_expand),
                                        tint = MaterialTheme.colorScheme.primary
                                    )
                                }
                            }

                            val allFeatures = listOf(
                                stringResource(R.string.feature_1),
                                stringResource(R.string.feature_2),
                                stringResource(R.string.feature_3),
                                stringResource(R.string.feature_4),
                                stringResource(R.string.feature_5),
                                stringResource(R.string.feature_6),
                                stringResource(R.string.feature_7),
                                stringResource(R.string.feature_8),
                                stringResource(R.string.feature_9),
                                stringResource(R.string.feature_10),
                                stringResource(R.string.feature_11),
                                stringResource(R.string.feature_12),
                                stringResource(R.string.feature_13),
                            )

                            val features = if (isExpanded) allFeatures else allFeatures.take(5)

                            Spacer(modifier = Modifier.height(12.dp))
                            features.forEach { feature ->
                                Row(
                                    verticalAlignment = Alignment.CenterVertically,
                                    modifier = Modifier.padding(vertical = 4.dp)
                                ) {
                                    Icon(
                                        Icons.Default.Check, null,
                                        modifier = Modifier.size(16.dp),
                                        tint = MaterialTheme.colorScheme.primary
                                    )
                                    Spacer(modifier = Modifier.width(12.dp))
                                    Text(text = feature, style = MaterialTheme.typography.bodyMedium)
                                }
                            }
                        }

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
fun BubbleSwitch(context: android.content.Context) {
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    val isBubbleEnabled = remember { mutableStateOf(prefs.getBoolean("bubble_enabled", false)) }

    Card(
        shape = M3ExpressiveShapes.AsymmetricCard,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.55f)
        ),
        modifier = Modifier.fillMaxWidth()
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Surface(
                shape = M3ExpressiveShapes.Clover4,
                color = MaterialTheme.colorScheme.tertiaryContainer,
                modifier = Modifier.size(48.dp)
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Icon(
                        imageVector = Icons.Default.TouchApp,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onTertiaryContainer,
                        modifier = Modifier.size(24.dp)
                    )
                }
            }
            Spacer(modifier = Modifier.width(16.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.label_floating_bubble),
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = stringResource(R.string.label_floating_bubble_subtitle),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            Switch(
                checked = isBubbleEnabled.value,
                onCheckedChange = { enabled ->
                    isBubbleEnabled.value = enabled
                    prefs.edit().putBoolean("bubble_enabled", enabled).apply()
                }
            )
        }
    }
}

