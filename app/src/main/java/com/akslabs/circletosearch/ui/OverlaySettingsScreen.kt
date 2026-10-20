/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material.icons.automirrored.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.core.content.ContextCompat
import android.os.Build
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.compose.ui.res.stringResource
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.CircleToSearchAccessibilityService
import com.akslabs.circletosearch.data.*
import com.akslabs.circletosearch.isAccessibilityServiceEnabled
import com.akslabs.circletosearch.ui.components.DonateBottomSheet
import com.akslabs.circletosearch.ui.components.UnifiedSearchMethodSelector
import com.akslabs.circletosearch.utils.UIPreferences
import androidx.core.app.ActivityOptionsCompat
import dev.davidv.translator.MainActivity as TranslatorMainActivity

private const val NAV_TRANSITION_MS = 320
private val NavEasing = CubicBezierEasing(0.2f, 0f, 0f, 1f)

private sealed class SettingsPage {
    object Root : SettingsPage()
    object Triggers : SettingsPage()
    object Zones : SettingsPage()
    object Search : SettingsPage()
    object General : SettingsPage()
    object About : SettingsPage()
}

private fun SettingsPage.order(): Int = when (this) {
    SettingsPage.Root -> 0
    SettingsPage.Triggers -> 1
    SettingsPage.Zones -> 2
    SettingsPage.Search -> 3
    SettingsPage.General -> 4
    SettingsPage.About -> 5
}

private fun settingsPageFromName(name: String?): SettingsPage? = when (name) {
    "triggers" -> SettingsPage.Triggers
    "zones" -> SettingsPage.Zones
    "search" -> SettingsPage.Search
    "general" -> SettingsPage.General
    "about" -> SettingsPage.About
    else -> null
}

@Stable
private class OverlaySettingsState(context: Context) {
    private val configManager = OverlayConfigurationManager(context)
    private val appPrefs = context.getSharedPreferences("app_prefs", Context.MODE_PRIVATE)

    val uiPreferences = UIPreferences(context)
    val isAccessibilityOn = isAccessibilityServiceEnabled(context)

    private val _config = mutableStateOf(configManager.getConfig())
    val config: OverlayConfig get() = _config.value
    fun updateConfig(newConfig: OverlayConfig) {
        _config.value = newConfig
        configManager.saveConfig(newConfig)
        // Direct refresh: don't rely solely on the SharedPreferences listener,
        // some devices delay it — poke the running service so changes apply live.
        CircleToSearchAccessibilityService.instance?.refreshOverlays()
    }

    private val _bubbleEnabled = mutableStateOf(appPrefs.getBoolean("bubble_enabled", false))
    val bubbleEnabled: Boolean get() = _bubbleEnabled.value
    fun setBubbleEnabled(enabled: Boolean) {
        _bubbleEnabled.value = enabled
        appPrefs.edit().putBoolean("bubble_enabled", enabled).apply()
    }

    private val _hwTriggers = mutableStateOf(
        HW_TRIGGER_KEYS.associateWith { appPrefs.getBoolean(it, false) }
    )
    val hwTriggers: Map<String, Boolean> get() = _hwTriggers.value
    fun setTrigger(key: String, enabled: Boolean) {
        _hwTriggers.value = _hwTriggers.value + (key to enabled)
        appPrefs.edit().putBoolean(key, enabled).apply()
    }

    companion object {
        val HW_TRIGGER_KEYS = listOf(
            "trigger_double_vol_up",
            "trigger_double_vol_down",
            "trigger_long_vol_up",
            "trigger_long_vol_down",
            "trigger_vol_up_down"
        )
    }
}

@Composable
fun OverlaySettingsScreen(
    onBack: () -> Unit,
    initialPage: String? = null
) {
    val context = LocalContext.current
    val state = remember { OverlaySettingsState(context) }
    val backStack = remember {
        val startPage = settingsPageFromName(initialPage)
        if (startPage != null) {
            // Deep link (e.g. "Custom overlay" row): start directly on the target
            // page — back from it returns home, not to the settings root.
            mutableStateListOf<SettingsPage>(startPage)
        } else {
            mutableStateListOf<SettingsPage>(SettingsPage.Root)
        }
    }
    val pop: () -> Unit = {
        if (backStack.size > 1) backStack.removeAt(backStack.lastIndex) else onBack()
    }

    androidx.activity.compose.BackHandler {
        if (backStack.size > 1) pop() else onBack()
    }

    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background
    ) {
        AnimatedContent(
            targetState = backStack.last(),
            modifier = Modifier.fillMaxSize(),
            transitionSpec = {
                val forward = targetState.order() > initialState.order()
                if (forward) {
                    (slideInHorizontally(tween(NAV_TRANSITION_MS, easing = NavEasing)) { it } +
                            fadeIn(tween(240)))
                        .togetherWith(
                            slideOutHorizontally(tween(NAV_TRANSITION_MS, easing = NavEasing)) { -it } +
                                    fadeOut(tween(200))
                        )
                } else {
                    (slideInHorizontally(tween(NAV_TRANSITION_MS, easing = NavEasing)) { -it } +
                            fadeIn(tween(240)))
                        .togetherWith(
                            slideOutHorizontally(tween(NAV_TRANSITION_MS, easing = NavEasing)) { it } +
                                    fadeOut(tween(200))
                        )
                }
            },
            label = "settingsNav"
        ) { page ->
            when (page) {
                SettingsPage.Root -> SettingsRootScreen(
                    onBack = onBack,
                    onNavigate = { backStack.add(it) }
                )
                SettingsPage.Triggers -> TriggersSettingsScreen(state = state, onBack = pop)
                SettingsPage.Zones -> ZonesSettingsScreen(state = state, onBack = pop)
                SettingsPage.Search -> SearchSettingsScreen(state = state, onBack = pop)
                SettingsPage.General -> GeneralSettingsScreen(onBack = pop)
                SettingsPage.About -> AboutSettingsScreen(onBack = pop)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsDetailScaffold(
    title: String,
    onBack: () -> Unit,
    actions: @Composable RowScope.() -> Unit = {},
    content: @Composable ColumnScope.() -> Unit
) {
    Scaffold(
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            TopAppBar(
                title = { Text(title, fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = stringResource(R.string.cd_back)
                        )
                    }
                },
                actions = actions,
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.background,
                    titleContentColor = MaterialTheme.colorScheme.onBackground
                )
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp),
            content = content
        )
    }
}

@Composable
private fun SettingsRootScreen(
    onBack: () -> Unit,
    onNavigate: (SettingsPage) -> Unit
) {
    val context = LocalContext.current
    SettingsDetailScaffold(
        title = stringResource(R.string.title_settings),
        onBack = onBack
    ) {
        Spacer(modifier = Modifier.height(8.dp))
        SettingsGroup {
            SettingsCategoryRow(
                icon = Icons.Default.Info,
                title = stringResource(R.string.settings_cat_about),
                subtitle = stringResource(R.string.settings_cat_about_subtitle),
                onClick = { onNavigate(SettingsPage.About) }
            )
        }

        Spacer(modifier = Modifier.height(24.dp))
    }
}

@Composable
private fun TriggersSettingsScreen(
    state: OverlaySettingsState,
    onBack: () -> Unit
) {
    val config = state.config
    SettingsDetailScaffold(
        title = stringResource(R.string.triggers_card_title),
        onBack = onBack
    ) {
        Spacer(modifier = Modifier.height(8.dp))
        SettingsSectionHeader(title = stringResource(R.string.section_launch_methods))
        SettingsGroup {
            SettingsToggleItem(
                title = stringResource(R.string.trigger_statusbar_title),
                subtitle = stringResource(R.string.trigger_statusbar_subtitle),
                icon = Icons.Default.Layers,
                checked = config.isEnabled,
                onCheckedChange = { state.updateConfig(config.copy(isEnabled = it)) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.label_floating_bubble),
                subtitle = stringResource(R.string.trigger_bubble_subtitle),
                icon = Icons.Default.TouchApp,
                checked = state.bubbleEnabled,
                onCheckedChange = { state.setBubbleEnabled(it) }
            )
            SettingsGroupDivider()
            SettingsInfoRow(
                icon = Icons.Default.Widgets,
                title = stringResource(R.string.trigger_qs_title),
                subtitle = stringResource(R.string.trigger_qs_subtitle)
            )
        }

        Spacer(modifier = Modifier.height(24.dp))
        SettingsSectionHeader(title = stringResource(R.string.section_hardware_buttons))
        if (!state.isAccessibilityOn) {
            Text(
                text = stringResource(R.string.trigger_hw_notice),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(bottom = 8.dp)
            )
        }
        SettingsGroup {
            SettingsToggleItem(
                title = stringResource(R.string.trigger_double_vol_up_title),
                subtitle = stringResource(R.string.trigger_hw_vol_subtitle),
                icon = Icons.AutoMirrored.Filled.VolumeUp,
                checked = state.hwTriggers["trigger_double_vol_up"] ?: false,
                onCheckedChange = { state.setTrigger("trigger_double_vol_up", it) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.trigger_double_vol_down_title),
                subtitle = stringResource(R.string.trigger_hw_vol_subtitle),
                icon = Icons.AutoMirrored.Filled.VolumeDown,
                checked = state.hwTriggers["trigger_double_vol_down"] ?: false,
                onCheckedChange = { state.setTrigger("trigger_double_vol_down", it) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.trigger_long_vol_up_title),
                subtitle = stringResource(R.string.trigger_hw_long_subtitle),
                icon = Icons.AutoMirrored.Filled.VolumeUp,
                checked = state.hwTriggers["trigger_long_vol_up"] ?: false,
                onCheckedChange = { state.setTrigger("trigger_long_vol_up", it) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.trigger_long_vol_down_title),
                subtitle = stringResource(R.string.trigger_hw_long_subtitle),
                icon = Icons.AutoMirrored.Filled.VolumeDown,
                checked = state.hwTriggers["trigger_long_vol_down"] ?: false,
                onCheckedChange = { state.setTrigger("trigger_long_vol_down", it) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.trigger_vol_up_down_title),
                subtitle = stringResource(R.string.trigger_hw_subtitle),
                icon = Icons.Default.Add,
                checked = state.hwTriggers["trigger_vol_up_down"] ?: false,
                onCheckedChange = { state.setTrigger("trigger_vol_up_down", it) }
            )
        }
        Spacer(modifier = Modifier.height(24.dp))
    }
}

@Composable
private fun ZonesSettingsScreen(
    state: OverlaySettingsState,
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val config = state.config
    val overlayPalette = rememberOverlayPalette(context)

    // Overlay strips can only render while the accessibility service is running.
    // Re-check on resume so enabling the service elsewhere updates this screen.
    var a11yOn by remember { mutableStateOf(isAccessibilityServiceEnabled(context)) }
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) {
                a11yOn = isAccessibilityServiceEnabled(context)
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    // Clear the editing preview when leaving the screen (deep-link back goes
    // straight home, so no later screen reset can happen).
    DisposableEffect(Unit) {
        onDispose {
            val cfg = state.config
            if (cfg.previewVisible || cfg.activeSegmentIndex != -1) {
                state.updateConfig(cfg.copy(previewVisible = false, activeSegmentIndex = -1))
            }
        }
    }

    SettingsDetailScaffold(
        title = stringResource(R.string.settings_cat_zones),
        onBack = onBack,
        actions = {
            IconButton(onClick = { state.updateConfig(OverlayConfig()) }) {
                Icon(Icons.Default.Refresh, contentDescription = stringResource(R.string.cd_reset))
            }
        }
    ) {
        Spacer(modifier = Modifier.height(8.dp))

        if (!a11yOn) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                color = MaterialTheme.colorScheme.errorContainer,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(
                    modifier = Modifier.padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        Icons.Default.Warning,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.onErrorContainer
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = stringResource(R.string.overlay_a11y_off_title),
                            style = MaterialTheme.typography.bodyLarge,
                            color = MaterialTheme.colorScheme.onErrorContainer
                        )
                        Text(
                            text = stringResource(R.string.overlay_a11y_off_subtitle),
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onErrorContainer
                        )
                    }
                    Spacer(modifier = Modifier.width(12.dp))
                    TextButton(onClick = {
                        context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS))
                    }) {
                        Text(stringResource(R.string.overlay_a11y_off_enable))
                    }
                }
            }
            Spacer(modifier = Modifier.height(12.dp))
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            SettingsSectionHeader(title = stringResource(R.string.section_overlays))
            TextButton(onClick = {
                val currentSegments = config.segments.toMutableList()
                currentSegments.add(OverlaySegment(xOffset = 300))
                state.updateConfig(config.copy(segments = currentSegments))
            }) {
                Icon(Icons.Default.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                Spacer(modifier = Modifier.width(4.dp))
                Text(stringResource(R.string.btn_add_overlay))
            }
        }

        if (config.segments.isEmpty()) {
            Text(
                text = stringResource(R.string.label_no_overlays),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }

        config.segments.forEachIndexed { index, segment ->
            SegmentEditorItem(
                index = index,
                segment = segment,
                overlayPalette = overlayPalette,
                isExpanded = config.activeSegmentIndex == index,
                onExpandToggle = {
                    val nextIndex = if (config.activeSegmentIndex == index) -1 else index
                    // While a segment editor is open, force the real overlay strips
                    // visible so position/size changes can be seen live.
                    state.updateConfig(config.copy(activeSegmentIndex = nextIndex, previewVisible = nextIndex != -1))
                },
                onUpdate = { updatedSegment ->
                    val newSegments = config.segments.toMutableList()
                    newSegments[index] = updatedSegment
                    state.updateConfig(config.copy(segments = newSegments))
                },
                onDelete = {
                    val newSegments = config.segments.toMutableList()
                    newSegments.removeAt(index)
                    val nextActiveIndex = if (config.activeSegmentIndex == index) -1
                    else if (config.activeSegmentIndex > index) config.activeSegmentIndex - 1
                    else config.activeSegmentIndex
                    state.updateConfig(config.copy(segments = newSegments, activeSegmentIndex = nextActiveIndex, previewVisible = nextActiveIndex != -1))
                }
            )
            Spacer(modifier = Modifier.height(16.dp))
        }

        Spacer(modifier = Modifier.height(8.dp))
        SettingsSectionHeader(title = stringResource(R.string.section_behavior))
        SettingsGroup {
            SettingsToggleItem(
                title = stringResource(R.string.overlay_setting_landscape_title),
                subtitle = stringResource(R.string.overlay_setting_landscape_subtitle),
                icon = Icons.Default.ScreenRotation,
                checked = config.isEnabledInLandscape,
                onCheckedChange = { state.updateConfig(config.copy(isEnabledInLandscape = it)) }
            )
            SettingsGroupDivider()
            SettingsToggleItem(
                title = stringResource(R.string.overlay_setting_show_title),
                subtitle = stringResource(R.string.overlay_setting_show_subtitle),
                icon = Icons.Default.Visibility,
                checked = config.isVisible,
                onCheckedChange = { state.updateConfig(config.copy(isVisible = it)) }
            )
        }
        Spacer(modifier = Modifier.height(24.dp))
    }
}

@Composable
private fun SearchSettingsScreen(
    state: OverlaySettingsState,
    onBack: () -> Unit
) {
    SettingsDetailScaffold(
        title = stringResource(R.string.settings_cat_search),
        onBack = onBack
    ) {
        Spacer(modifier = Modifier.height(8.dp))
        SettingsGroup {
            Column(modifier = Modifier.padding(16.dp)) {
                UnifiedSearchMethodSelector(uiPreferences = state.uiPreferences)
                Spacer(modifier = Modifier.height(12.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        imageVector = Icons.Default.Info,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.secondary,
                        modifier = Modifier.size(16.dp)
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Text(
                        text = stringResource(R.string.info_lens_needs_google),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }
        Spacer(modifier = Modifier.height(24.dp))
    }
}

@Composable
private fun GeneralSettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    SettingsDetailScaffold(
        title = stringResource(R.string.general),
        onBack = onBack
    ) {
        Spacer(modifier = Modifier.height(8.dp))
        SettingsGroup {
            SettingsLinkRow(
                icon = Icons.Default.Translate,
                title = stringResource(R.string.label_translator_title),
                subtitle = stringResource(R.string.label_translator_subtitle),
                onClick = {
                    try {
                        context.startActivity(Intent(context, TranslatorMainActivity::class.java))
                    } catch (e: Exception) {
                        android.widget.Toast.makeText(context, "Could not launch translator: ${e.message}", android.widget.Toast.LENGTH_LONG).show()
                    }
                }
            )
        }
        Spacer(modifier = Modifier.height(24.dp))
    }
}

@Composable
private fun AboutSettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    var showDonateSheet by remember { mutableStateOf(false) }
    val versionName = remember(context) {
        runCatching {
            context.packageManager.getPackageInfo(context.packageName, 0).versionName
        }.getOrNull().orEmpty()
    }

    SettingsDetailScaffold(
        title = stringResource(R.string.settings_cat_about),
        onBack = onBack
    ) {
        Spacer(modifier = Modifier.height(8.dp))
        SettingsGroup {
            SettingsValueRow(
                icon = Icons.Default.Android,
                title = stringResource(R.string.about_version),
                value = versionName
            )
            SettingsGroupDivider()
            SettingsLinkRow(
                icon = Icons.Default.Code,
                title = stringResource(R.string.about_source_code),
                subtitle = stringResource(R.string.about_source_subtitle),
                onClick = {
                    try {
                        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs")))
                    } catch (e: Exception) {
                        android.widget.Toast.makeText(context, context.getString(R.string.toast_could_not_open_link), android.widget.Toast.LENGTH_SHORT).show()
                    }
                }
            )
            SettingsGroupDivider()
            SettingsLinkRow(
                icon = Icons.Default.FavoriteBorder,
                title = stringResource(R.string.btn_donate),
                subtitle = stringResource(R.string.about_donate_subtitle),
                onClick = { showDonateSheet = true }
            )
        }
        Spacer(modifier = Modifier.height(24.dp))
    }

    if (showDonateSheet) {
        DonateBottomSheet(
            onDismiss = { showDonateSheet = false },
            onDonateOptionSelected = { url ->
                try {
                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                } catch (e: Exception) {
                    android.widget.Toast.makeText(context, context.getString(R.string.toast_could_not_open_link), android.widget.Toast.LENGTH_SHORT).show()
                }
                showDonateSheet = false
            }
        )
    }
}

@Composable
private fun SettingsGroup(
    modifier: Modifier = Modifier,
    content: @Composable ColumnScope.() -> Unit
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow
    ) {
        Column(content = content)
    }
}

@Composable
private fun SettingsGroupDivider() {
    HorizontalDivider(modifier = Modifier.padding(start = 56.dp))
}

@Composable
private fun SettingsCategoryRow(
    icon: ImageVector,
    title: String,
    subtitle: String,
    onClick: () -> Unit,
    iconTint: Color = MaterialTheme.colorScheme.primary
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = iconTint
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(text = title, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        Spacer(modifier = Modifier.width(8.dp))
        Icon(
            imageVector = Icons.AutoMirrored.Filled.ArrowForwardIos,
            contentDescription = null,
            modifier = Modifier.size(14.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f)
        )
    }
}

@Composable
private fun SettingsLinkRow(
    icon: ImageVector,
    title: String,
    subtitle: String,
    onClick: () -> Unit
) {
    SettingsCategoryRow(
        icon = icon,
        title = title,
        subtitle = subtitle,
        onClick = onClick,
        iconTint = MaterialTheme.colorScheme.onSurfaceVariant
    )
}

@Composable
private fun SettingsInfoRow(
    icon: ImageVector,
    title: String,
    subtitle: String
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column {
            Text(text = title, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
    }
}

@Composable
private fun SettingsValueRow(
    icon: ImageVector,
    title: String,
    value: String
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant
        )
        Spacer(modifier = Modifier.width(16.dp))
        Text(
            text = title,
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.weight(1f)
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SegmentEditorItem(
    index: Int,
    segment: OverlaySegment,
    overlayPalette: List<Color>,
    isExpanded: Boolean,
    onExpandToggle: () -> Unit,
    onUpdate: (OverlaySegment) -> Unit,
    onDelete: () -> Unit
) {
    var showGestureDialog by remember { mutableStateOf(false) }
    val context = LocalContext.current
    
    // Get screen dimensions for sliders
    val configuration = androidx.compose.ui.platform.LocalConfiguration.current
    val screenWidth = configuration.screenWidthDp.toFloat() * 3 // Rough dp to px. 
    val screenHeight = configuration.screenHeightDp.toFloat() * 3
    // Use actual resource metrics if possible, but LocalConfiguration is easiest in Compose. 
    // To be safer and strictly follow "screen boundaries", we can clamp. 
    // But since pixels vary by density, let's allow a generous but bounded range based on config.
    // 3.0 density is common (XXHDPI). 
    // We will just use a reasonably high cap matching probable max resolution (e.g. 1440p width -> ~1500, height -> ~3000)
    // Actually, user said "limit sliders to screen boundaries".
    // I should probably pass exact screen metrics from MainActivity or Context.
    val metrics = LocalContext.current.resources.displayMetrics
    val maxWidth = metrics.widthPixels.toFloat()
    val maxHeight = metrics.heightPixels.toFloat()

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha=0.3f)),
        shape = RoundedCornerShape(16.dp)
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            // Header
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.clickable { onExpandToggle() }
            ) {
                Icon(
                    imageVector = Icons.Default.Layers,
                    contentDescription = null,
                    tint = overlayPalette[index % overlayPalette.size],
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(stringResource(R.string.label_overlay_number, index + 1), fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.weight(1f))
                
                IconButton(onClick = onDelete) {
                     Icon(Icons.Default.Delete, contentDescription = stringResource(R.string.cd_delete), tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp))
                }
                Icon(
                    if (isExpanded) Icons.Default.ExpandLess else Icons.Default.ExpandMore,
                    contentDescription = null
                )
            }
            
            if (isExpanded) {
                Spacer(modifier = Modifier.height(16.dp))
                
                // Dimensions Sliders - Limited to Screen
                // Dimensions Sliders - Limited to Screen
                // Only Height is discrete (10px steps) per user request
                
                Text(stringResource(R.string.label_x_position, segment.xOffset), style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.xOffset.toFloat().coerceIn(0f, maxWidth),
                    onValueChange = { onUpdate(segment.copy(xOffset = it.toInt())) },
                    valueRange = 0f..maxWidth
                )
                
                Text(stringResource(R.string.label_y_position, segment.yOffset), style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.yOffset.toFloat().coerceIn(0f, maxHeight),
                    onValueChange = { onUpdate(segment.copy(yOffset = it.toInt())) },
                    valueRange = 0f..maxHeight
                )
                
                Text(stringResource(R.string.label_width, segment.width), style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.width.toFloat().coerceIn(10f, maxWidth),
                    onValueChange = { onUpdate(segment.copy(width = it.toInt())) },
                    valueRange = 10f..maxWidth
                )
                
                Text(stringResource(R.string.label_height, segment.height), style = MaterialTheme.typography.labelMedium)
                val hRange = 10f..3000f
                val hSteps = ((hRange.endInclusive - hRange.start) / 10).toInt() - 1
                Slider(
                    value = segment.height.toFloat().coerceIn(10f, maxHeight),
                    onValueChange = { onUpdate(segment.copy(height = it.toInt())) },
                    valueRange = 10f..maxHeight
                )

                
                Spacer(modifier = Modifier.height(16.dp))
                Divider(color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha=0.1f))
                Spacer(modifier = Modifier.height(12.dp))
                
                // Gestures
                Row(
                    modifier = Modifier.fillMaxWidth().clickable { showGestureDialog = true },
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(Icons.Default.TouchApp, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                    Spacer(modifier = Modifier.width(12.dp))
                    Column {
                         Text(stringResource(R.string.section_gestures), fontWeight = FontWeight.Bold)
                         Text(stringResource(R.string.gestures_subtitle), style = MaterialTheme.typography.bodySmall)
                    }
                    Spacer(modifier = Modifier.weight(1f))
                    Icon(Icons.Default.ArrowForwardIos, contentDescription = null, modifier = Modifier.size(16.dp))
                }
                
                Spacer(modifier = Modifier.height(8.dp))
                
                // Gesture Summary (first 2 non-none)
                Column {
                    segment.gestures.entries.filter { it.value != ActionType.NONE }.take(3).forEach {
                        Text(
                            "${it.key.getFriendlyName(context)}: ${it.value.getFriendlyName(context)}", 
                            style = MaterialTheme.typography.bodySmall, 
                            color = MaterialTheme.colorScheme.secondary
                        )
                    }
                }
            }
        }
    }
    
    if (showGestureDialog) {
        GestureConfigDialog(
            segment = segment,
            onDismiss = { showGestureDialog = false },
            onUpdate = onUpdate
        )
    }
}

@Composable
fun GestureConfigDialog(
    segment: OverlaySegment,
    onDismiss: () -> Unit,
    onUpdate: (OverlaySegment) -> Unit
) {
    var showAppPicker by remember { mutableStateOf<GestureType?>(null) }
    val context = LocalContext.current
    
    if (showAppPicker != null) {
        AppPickerDialog(
            onDismiss = { showAppPicker = null },
            onAppSelected = { pkg -> 
                 val gesture = showAppPicker!!
                 val newGestures = segment.gestures.toMutableMap()
                 newGestures[gesture] = ActionType.OPEN_APP
                 
                 val newData = segment.gestureData.toMutableMap()
                 newData[gesture] = pkg
                 
                 onUpdate(segment.copy(gestures = newGestures, gestureData = newData))
                 showAppPicker = null
            }
        )
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false) // Allow full width control
    ) {
        Card(
            shape = RoundedCornerShape(24.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp) // Screen padding
                .fillMaxHeight(0.85f)
        ) {
            Column(
                modifier = Modifier.padding(24.dp).verticalScroll(rememberScrollState())
            ) {
                Text(stringResource(R.string.title_configure_gestures), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(16.dp))
                
                GestureType.values().forEach { gesture ->
                    val currentAction = segment.gestures[gesture] ?: ActionType.NONE
                    var showPicker by remember { mutableStateOf(false) }
                    
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 4.dp)
                    ) {
                        Text(
                            gesture.getFriendlyName(context), 
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(start = 4.dp, bottom = 4.dp)
                        )
                        
                        Surface(
                            onClick = { showPicker = true },
                            shape = RoundedCornerShape(12.dp),
                            color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f),
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Row(
                                modifier = Modifier.padding(16.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                val actionIcon = getActionIcon(currentAction)
                                Icon(actionIcon, contentDescription = null, modifier = Modifier.size(20.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                                Spacer(modifier = Modifier.width(12.dp))
                                
                                var label = currentAction.getFriendlyName(context)
                                if (currentAction == ActionType.OPEN_APP) {
                                    val pkg = segment.gestureData[gesture]
                                    val appName = runCatching { 
                                        val info = context.packageManager.getApplicationInfo(pkg ?: "", 0)
                                        context.packageManager.getApplicationLabel(info).toString()
                                    }.getOrDefault(pkg ?: context.getString(R.string.unknown_app))
                                    label = stringResource(R.string.label_open_app, appName)
                                }
                                
                                Text(
                                    label, 
                                    style = MaterialTheme.typography.bodyLarge,
                                    maxLines = 1, 
                                    overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f)
                                )
                                
                                Icon(Icons.AutoMirrored.Filled.ArrowForwardIos, contentDescription = null, modifier = Modifier.size(16.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f))
                            }
                        }
                    }

                    if (showPicker) {
                        ActionPickerDialog(
                            currentAction = currentAction,
                            onDismiss = { showPicker = false },
                            onActionSelected = { action ->
                                if (action == ActionType.OPEN_APP) {
                                    showAppPicker = gesture
                                } else {
                                    val newGestures = segment.gestures.toMutableMap()
                                    newGestures[gesture] = action
                                    onUpdate(segment.copy(gestures = newGestures))
                                }
                                showPicker = false
                            }
                        )
                    }
                }
                
                Spacer(modifier = Modifier.height(16.dp))
                Button(
                    onClick = onDismiss,
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Text(stringResource(R.string.btn_close))
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionPickerDialog(
    currentAction: ActionType,
    onDismiss: () -> Unit,
    onActionSelected: (ActionType) -> Unit
) {
    val context = LocalContext.current
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Card(
            shape = RoundedCornerShape(24.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp)
                .fillMaxHeight(0.8f)
        ) {
            Column(modifier = Modifier.padding(24.dp)) {
                Text(stringResource(R.string.title_select_action), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(16.dp))
                
                androidx.compose.foundation.lazy.LazyColumn(modifier = Modifier.weight(1f)) {
                    items(ActionType.values().size) { index ->
                        val action = ActionType.values()[index]
                        val isSelected = action == currentAction
                        
                        Surface(
                            onClick = { onActionSelected(action) },
                            shape = RoundedCornerShape(12.dp),
                            color = if (isSelected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent,
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Row(
                                modifier = Modifier.padding(16.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                Icon(
                                    getActionIcon(action), 
                                    contentDescription = null, 
                                    modifier = Modifier.size(24.dp),
                                    tint = if (isSelected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
                                )
                                Spacer(modifier = Modifier.width(16.dp))
                                Text(
                                    action.getFriendlyName(context),
                                    style = MaterialTheme.typography.bodyLarge,
                                    fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                                    color = if (isSelected) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurface
                                )
                                if (isSelected) {
                                    Spacer(modifier = Modifier.weight(1f))
                                    Icon(Icons.Default.Check, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
                                }
                            }
                        }
                    }
                }
                
                Spacer(modifier = Modifier.height(16.dp))
                TextButton(onClick = onDismiss, modifier = Modifier.align(Alignment.End)) {
                    Text(stringResource(R.string.btn_cancel))
                }
            }
        }
    }
}

fun getActionIcon(action: ActionType): ImageVector = when (action) {
    ActionType.NONE -> Icons.Default.Block
    ActionType.SCREENSHOT -> Icons.Default.Screenshot
    ActionType.FLASHLIGHT -> Icons.Default.FlashlightOn
    ActionType.HOME -> Icons.Default.Home
    ActionType.BACK -> Icons.Default.ArrowBack
    ActionType.RECENTS -> Icons.Default.History
    ActionType.LOCK_SCREEN -> Icons.Default.Lock
    ActionType.OPEN_NOTIFICATIONS -> Icons.Default.Notifications
    ActionType.OPEN_QUICK_SETTINGS -> Icons.Default.Settings
    ActionType.CTS_LENS -> Icons.Default.Search
    ActionType.CTS_MULTI -> Icons.Default.TravelExplore
    ActionType.SPLIT_SCREEN -> Icons.Default.VerticalSplit
    ActionType.OPEN_APP -> Icons.Default.Apps
    ActionType.SCROLL_TOP -> Icons.Default.VerticalAlignTop
    ActionType.SCROLL_BOTTOM -> Icons.Default.VerticalAlignBottom
    ActionType.SCREEN_OFF -> Icons.Default.PowerSettingsNew
    ActionType.CTS_AUTO -> Icons.Default.SettingsSuggest
    ActionType.TOGGLE_AUTO_ROTATE -> Icons.Default.ScreenRotation
    ActionType.MEDIA_PLAY_PAUSE -> Icons.Default.PlayArrow
    ActionType.MEDIA_NEXT -> Icons.Default.SkipNext
    ActionType.MEDIA_PREVIOUS -> Icons.Default.SkipPrevious
}

@Composable
fun AppPickerDialog(onDismiss: () -> Unit, onAppSelected: (String) -> Unit) {
    val context = LocalContext.current
    data class AppItem(val label: String, val packageName: String)
    
    // Full list
    var allApps by remember { mutableStateOf<List<AppItem>>(emptyList()) }
    var isLoading by remember { mutableStateOf(true) }
    
    // Search
    var searchQuery by remember { mutableStateOf("") }
    
    // Filtered list
    val filteredApps = remember(allApps, searchQuery) {
        if (searchQuery.isEmpty()) allApps
        else allApps.filter { 
            it.label.contains(searchQuery, ignoreCase = true) || 
            it.packageName.contains(searchQuery, ignoreCase = true) 
        }
    }
    
    LaunchedEffect(Unit) {
        kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
            val pm = context.packageManager
            val allPackages = pm.getInstalledPackages(0)
            val list = allPackages.mapNotNull { pkg ->
                val intent = pm.getLaunchIntentForPackage(pkg.packageName)
                if (intent != null) {
                    val label = pkg.applicationInfo?.loadLabel(pm).toString()
                    AppItem(label, pkg.packageName)
                } else null
            }.sortedBy { it.label.lowercase() }
            
            allApps = list
            isLoading = false
        }
    }

    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(usePlatformDefaultWidth = false)
    ) {
        Card(
            shape = RoundedCornerShape(24.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            modifier = Modifier
                .fillMaxWidth()
                .padding(24.dp)
                .fillMaxHeight(0.85f)
        ) {
            Column(modifier = Modifier.padding(24.dp)) {
                Text(stringResource(R.string.title_select_app), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(16.dp))
                
                // Search Bar
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.hint_search_apps)) },
                    leadingIcon = { Icon(Icons.Default.Search, contentDescription = null) },
                    singleLine = true,
                    shape = RoundedCornerShape(12.dp)
                )
                
                Spacer(modifier = Modifier.height(12.dp))
                
                if (isLoading) {
                    Box(modifier = Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                         CircularProgressIndicator()
                    }
                } else {
                    androidx.compose.foundation.lazy.LazyColumn(modifier = Modifier.weight(1f)) {
                        items(filteredApps.size) { index ->
                            val app = filteredApps[index]
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable { onAppSelected(app.packageName) }
                                    .padding(vertical = 12.dp),
                                verticalAlignment = Alignment.CenterVertically
                            ) {
                                // App Icon using AndroidView for performance (avoids Bitmap conversion lag in Compose)
                                androidx.compose.ui.viewinterop.AndroidView(
                                    factory = { ctx ->
                                        android.widget.ImageView(ctx).apply {
                                            scaleType = android.widget.ImageView.ScaleType.FIT_CENTER
                                        }
                                    },
                                    update = { imageView ->
                                        // Load icon
                                        try {
                                            val icon = context.packageManager.getApplicationIcon(app.packageName)
                                            imageView.setImageDrawable(icon)
                                        } catch (e: Exception) {
                                            imageView.setImageResource(android.R.drawable.sym_def_app_icon)
                                        }
                                    },
                                    modifier = Modifier.size(40.dp)
                                )
                                
                                Spacer(modifier = Modifier.width(16.dp))
                                
                                Column {
                                    Text(app.label, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium)
                                    Text(app.packageName, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                            Divider(color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha=0.5f))
                        }
                    }
                }
            }
        }
    }
}
@Composable
fun rememberOverlayPalette(context: Context): List<Color> {
    return remember(context) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            listOf(
                android.R.color.system_accent1_200,
                android.R.color.system_accent2_200,
                android.R.color.system_accent3_200,
                android.R.color.system_neutral1_200,
                android.R.color.system_neutral2_200
            ).map { 
                Color(ContextCompat.getColor(context, it))
            }
        } else {
            listOf(
                Color.Red,
                Color.Green,
                Color.Blue,
                Color.Yellow,
                Color.Magenta
            )
        }
    }
}
