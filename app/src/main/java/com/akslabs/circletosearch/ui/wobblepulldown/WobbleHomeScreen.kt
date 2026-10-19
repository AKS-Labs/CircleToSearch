package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateIntAsState
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.VolumeDown
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Assistant
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.TouchApp
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Widgets
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material3.ButtonGroup
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.ButtonShapes
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.ShapeDefaults
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.ToggleButton
import androidx.compose.material3.ToggleButtonDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.Canvas
import androidx.core.app.ActivityOptionsCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.akslabs.circletosearch.MainActivity
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.SupportDialogHost
import com.akslabs.circletosearch.data.ActionType
import com.akslabs.circletosearch.data.BottomEdgeConfig
import com.akslabs.circletosearch.data.GestureType
import com.akslabs.circletosearch.data.OverlayConfig
import com.akslabs.circletosearch.data.OverlayConfigurationManager
import com.akslabs.circletosearch.data.OverlaySegment
import com.akslabs.circletosearch.ui.SettingsSectionHeader
import com.akslabs.circletosearch.ui.components.UnifiedSearchMethodSelector
import com.akslabs.circletosearch.utils.UIPreferences
import kotlinx.coroutines.delay

private const val BOTTOM_EDGE_MARKER = "cts_bottom_edge"

private val HW_TRIGGER_KEYS = listOf(
    "trigger_double_vol_up",
    "trigger_double_vol_down",
    "trigger_double_power",
    "trigger_power_vol_up",
    "trigger_power_vol_down",
    "trigger_vol_up_down",
)

private val HW_TRIGGER_TITLES = listOf(
    R.string.trigger_double_vol_up_title,
    R.string.trigger_double_vol_down_title,
    R.string.trigger_double_power_title,
    R.string.trigger_power_vol_up_title,
    R.string.trigger_power_vol_down_title,
    R.string.trigger_vol_up_down_title,
)

private val HW_TRIGGER_ICONS: List<ImageVector>
    get() = listOf(
        Icons.AutoMirrored.Filled.VolumeUp,
        Icons.AutoMirrored.Filled.VolumeDown,
        Icons.Default.PowerSettingsNew,
        Icons.Default.Add,
        Icons.Default.Add,
        Icons.Default.Add,
    )

@OptIn(ExperimentalMaterial3Api::class, ExperimentalTextApi::class)
@Composable
fun WobbleHomeScreen(
    onOpenDeviceInfo: () -> Unit,
    modifier: Modifier = Modifier,
    wobbleEvent: Int = 0,
    contentPadding: PaddingValues = PaddingValues(0.dp),
    onOpenSettings: ((String) -> Unit)? = null,
) {
    val context = LocalContext.current
    val view = LocalView.current
    val density = LocalDensity.current
    val pullRefreshState = rememberPullToRefreshState()
    var isRefreshing by remember { mutableStateOf(false) }

    LaunchedEffect(isRefreshing) {
        if (isRefreshing) {
            WobbleHaptics.performUIHaptic(view)
            onOpenDeviceInfo()
            isRefreshing = false
        }
    }

    var rippleTrigger by remember { mutableIntStateOf(0) }
    var textCenterOffset by remember { mutableStateOf(Offset.Zero) }
    var lastHapticBucket by remember { mutableStateOf(0) }

    LaunchedEffect(pullRefreshState.distanceFraction) {
        val fraction = pullRefreshState.distanceFraction
        val currentBucket = (fraction * 10).toInt()

        if (fraction >= 1f && lastHapticBucket < 10) {
            WobbleHaptics.performUIHaptic(view)
            rippleTrigger++
            lastHapticBucket = 10
        } else if (fraction < 1f && currentBucket != lastHapticBucket) {
            if (currentBucket > lastHapticBucket) {
                WobbleHaptics.performSliderHaptic(view)
            }
            lastHapticBucket = currentBucket
        }

        if (fraction == 0f) {
            lastHapticBucket = 0
        }
    }

    val appPrefs = remember {
        context.getSharedPreferences("app_prefs", Context.MODE_PRIVATE)
    }
    val configManager = remember { OverlayConfigurationManager(context) }
    val uiPreferences = remember { UIPreferences(context) }
    var a11yGranted by remember { mutableStateOf(isA11yGranted(context)) }
    var assistantGranted by remember { mutableStateOf(isAssistantGranted(context)) }
    var overlayEnabled by remember { mutableStateOf(configManager.getConfig().isEnabled) }
    var overlayConfig by remember { mutableStateOf(configManager.getConfig()) }
    var bubbleEnabled by remember { mutableStateOf(appPrefs.getBoolean("bubble_enabled", false)) }
    var hwTriggers by remember {
        mutableStateOf(HW_TRIGGER_KEYS.associateWith { appPrefs.getBoolean(it, false) })
    }
    var showHwSheet by remember { mutableStateOf(false) }
    var bottomEdge by remember { mutableStateOf(configManager.getBottomEdgeConfig()) }
    var statusBarExpanded by remember { mutableStateOf(false) }
    var bottomEdgeExpanded by remember { mutableStateOf(false) }
    val bottomEdgeActive = bottomEdge.leftEnabled || bottomEdge.rightEnabled || bottomEdge.midEnabled
    val hwEnabledCount = hwTriggers.values.count { it }
    val running = assistantGranted ||
        (a11yGranted && (overlayEnabled || bubbleEnabled || bottomEdgeActive || hwEnabledCount > 0))

    val refreshHomeState = {
        a11yGranted = isA11yGranted(context)
        assistantGranted = isAssistantGranted(context)
        overlayEnabled = configManager.getConfig().isEnabled
        overlayConfig = configManager.getConfig()
        bubbleEnabled = appPrefs.getBoolean("bubble_enabled", false)
        hwTriggers = HW_TRIGGER_KEYS.associateWith { appPrefs.getBoolean(it, false) }
        bottomEdge = configManager.getBottomEdgeConfig()
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            refreshHomeState()
            if (event == Lifecycle.Event.ON_STOP) {
                // Switching away from the app closes both settings dropdowns, which
                // also drops their on-screen overlay previews.
                statusBarExpanded = false
                bottomEdgeExpanded = false
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(Unit) {
        while (true) {
            refreshHomeState()
            delay(500)
        }
    }

    val openSettingsPage: (String) -> Unit = { page ->
        val intent = Intent(context, MainActivity::class.java)
            .putExtra(MainActivity.EXTRA_SETTINGS_PAGE, page)
        val options = ActivityOptionsCompat.makeCustomAnimation(
            context,
            R.anim.anim_slide_in_top,
            R.anim.anim_stay,
        )
        context.startActivity(intent, options.toBundle())
    }

    val saveOverlayConfig: (OverlayConfig) -> Unit = { cfg ->
        configManager.saveConfig(cfg)
        overlayConfig = cfg
        overlayEnabled = cfg.isEnabled
    }

    val setBubbleEnabled: (Boolean) -> Unit = { on ->
        appPrefs.edit().putBoolean("bubble_enabled", on).apply()
        bubbleEnabled = on
    }

    val onHwToggle: (String, Boolean) -> Unit = { key, checked ->
        appPrefs.edit().putBoolean(key, checked).apply()
        hwTriggers = hwTriggers + (key to checked)
    }

    val saveBottomEdge: (BottomEdgeConfig) -> Unit = { cfg ->
        bottomEdge = cfg
        configManager.saveBottomEdgeConfig(cfg)
    }

    // Strip any legacy full-width bottom strip that older builds stored as a segment.
    LaunchedEffect(Unit) {
        val cfg = configManager.getConfig()
        if (cfg.segments.any { it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER }) {
            configManager.saveConfig(
                cfg.copy(segments = cfg.segments.filterNot {
                    it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER
                }),
            )
        }
    }

    // Clear preview when leaving the screen so overlays don't stay visible.
    DisposableEffect(Unit) {
        onDispose {
            val cur = configManager.getBottomEdgeConfig()
            if (cur.previewVisible) configManager.saveBottomEdgeConfig(cur.copy(previewVisible = false))
        }
    }

    LaunchedEffect(wobbleEvent) {
        if (wobbleEvent > 0) {
            WobbleHaptics.performUIHaptic(view)
            rippleTrigger++
        }
    }

    val topBlurHeightPx = with(density) { contentPadding.calculateTopPadding().toPx() * 1.5f }
    val bottomBlurHeightPx = with(density) { 120.dp.toPx() }

    PullToRefreshBox(
        isRefreshing = isRefreshing,
        onRefresh = { isRefreshing = true },
        state = pullRefreshState,
        indicator = { },
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .progressiveBlur(
                blurRadius = 40f,
                height = topBlurHeightPx,
                direction = BlurDirection.TOP,
            )
            .liquidRipple(
                trigger = rippleTrigger,
                origin = textCenterOffset,
                enabled = true,
                durationMillis = 2800,
                amplitudeDp = 34f,
                frequency = 12f,
                decay = 4.5f,
                speedDp = 1400f,
            ),
    ) {
        val deviceInfo = remember { WobbleDeviceUtils.getDeviceInfo(context) }
        val displayFraction = if (isRefreshing) 1f else pullRefreshState.distanceFraction
        val thresholdPassed = displayFraction >= 1f
        val statusBarPadding = contentPadding.calculateTopPadding()

        val cardExpansion by animateDpAsState(
            targetValue = 120.dp * displayFraction.coerceIn(0f, 1f),
            animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
            label = "cardExpansion",
        )

        val containerColor by animateColorAsState(
            targetValue = when {
                thresholdPassed -> MaterialTheme.colorScheme.primary
                running -> MaterialTheme.colorScheme.primaryContainer
                else -> MaterialTheme.colorScheme.surfaceContainerLow
            },
            animationSpec = tween(durationMillis = 300),
            label = "containerColor",
        )

        val contentColor by animateColorAsState(
            targetValue = when {
                thresholdPassed -> MaterialTheme.colorScheme.onPrimary
                running -> MaterialTheme.colorScheme.onPrimaryContainer
                else -> MaterialTheme.colorScheme.primary
            },
            animationSpec = tween(durationMillis = 300),
            label = "contentColor",
        )

        val borderColor by animateColorAsState(
            targetValue = if (thresholdPassed || running) {
                Color.Transparent
            } else {
                MaterialTheme.colorScheme.outlineVariant
            },
            animationSpec = tween(durationMillis = 300),
            label = "borderColor",
        )

        val chevronAlpha by animateFloatAsState(
            targetValue = if (thresholdPassed) 0f else 1f,
            animationSpec = tween(durationMillis = 300),
            label = "chevronAlpha",
        )

        val chevronWidth by animateDpAsState(
            targetValue = if (thresholdPassed) 0.dp else 24.dp + 8.dp,
            animationSpec = tween(durationMillis = 300),
            label = "chevronWidth",
        )

        val fontWeight by animateIntAsState(
            targetValue = if (thresholdPassed) 700 else 500,
            animationSpec = tween(durationMillis = 100),
            label = "fontWeight",
        )

        val textScale by animateFloatAsState(
            targetValue = if (thresholdPassed) 1.5f else 1f,
            animationSpec = tween(durationMillis = 300),
            label = "textScale",
        )

        val fontWidth = remember(displayFraction) {
            (100f + (displayFraction.coerceIn(0f, 1f) * 50f))
        }

        val fontFamily = remember(fontWidth, fontWeight) {
            try {
                FontFamily(
                    Font(
                        R.font.google_sans_flex,
                        variationSettings = FontVariation.Settings(
                            FontVariation.width(fontWidth),
                            FontVariation.weight(fontWeight),
                            FontVariation.Setting("ROND", 100f),
                        ),
                    )
                )
            } catch (_: Exception) {
                FontFamily.Default
            }
        }

        val heroStyle = MaterialTheme.typography.titleMedium.copy(fontFamily = fontFamily)
        val heroColor by animateColorAsState(
            targetValue = when {
                thresholdPassed -> MaterialTheme.colorScheme.onPrimary
                running -> MaterialTheme.colorScheme.onPrimaryContainer
                else -> MaterialTheme.colorScheme.onSurface
            },
            animationSpec = tween(durationMillis = 300),
            label = "heroColor",
        )

        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .progressiveBlur(
                    blurRadius = 40f,
                    height = bottomBlurHeightPx,
                    direction = BlurDirection.BOTTOM,
                ),
            contentPadding = PaddingValues(
                start = 16.dp,
                end = 16.dp,
                bottom = contentPadding.calculateBottomPadding() + 120.dp,
            ),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            item {
                OutlinedCard(
                    onClick = {
                        WobbleHaptics.performUIHaptic(view)
                        onOpenDeviceInfo()
                    },
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(64.dp + statusBarPadding + cardExpansion),
                    shape = RoundedCornerShape(
                        topStart = 0.dp,
                        topEnd = 0.dp,
                        bottomStart = 28.dp,
                        bottomEnd = 28.dp,
                    ),
                    colors = CardDefaults.outlinedCardColors(
                        containerColor = containerColor,
                        contentColor = heroColor,
                    ),
                    border = BorderStroke(1.dp, borderColor),
                ) {
                    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                        Column(
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.Center,
                        ) {
                            Spacer(modifier = Modifier.height(statusBarPadding))
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Box(
                                    modifier = Modifier
                                        .width(chevronWidth)
                                        .graphicsLayer { alpha = chevronAlpha },
                                ) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Icon(
                                            imageVector = Icons.Rounded.KeyboardArrowDown,
                                            contentDescription = null,
                                            modifier = Modifier
                                                .size(24.dp)
                                                .graphicsLayer {
                                                    rotationZ = (displayFraction * 180f).coerceIn(0f, 180f)
                                                },
                                            tint = contentColor,
                                        )
                                        Spacer(modifier = Modifier.width(8.dp))
                                    }
                                }
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier
                                        .onGloballyPositioned { coords ->
                                            val pos = coords.positionInRoot()
                                            val size = coords.size
                                            textCenterOffset = Offset(
                                                x = pos.x + (size.width / 2f),
                                                y = pos.y + (size.height / 2f),
                                            )
                                        }
                                        .graphicsLayer {
                                            scaleX = textScale
                                            scaleY = textScale
                                        },
                                ) {
                                    if (running) {
                                        Text(
                                            text = stringResource(R.string.home_running_title),
                                            style = heroStyle,
                                            color = heroColor,
                                        )
                                        Text(
                                            text = stringResource(R.string.home_running_state),
                                            style = heroStyle,
                                            color = heroColor,
                                        )
                                    } else {
                                        Text(
                                            text = deviceInfo.deviceName,
                                            style = heroStyle,
                                            color = heroColor,
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }

            item {
                Column {
                    SettingsSectionHeader(title = stringResource(R.string.section_launch_methods))
                    if (!a11yGranted) {
                        Text(
                            text = stringResource(R.string.trigger_hw_notice),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.padding(bottom = 8.dp),
                        )
                    }
                    Surface(
                        shape = RoundedCornerShape(28.dp),
                        color = MaterialTheme.colorScheme.surfaceContainerLow,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column {
                            HomeStatusBarSection(
                                config = overlayConfig,
                                a11yGranted = a11yGranted,
                                expanded = statusBarExpanded,
                                onExpandedChange = { statusBarExpanded = it },
                                onChange = saveOverlayConfig,
                            )
                            HomeRowDivider()
                            HomeToggleItem(
                                title = stringResource(R.string.label_floating_bubble),
                                subtitle = stringResource(R.string.trigger_bubble_subtitle),
                                icon = Icons.Default.TouchApp,
                                checked = bubbleEnabled,
                                enabled = a11yGranted,
                                onCheckedChange = { setBubbleEnabled(it) },
                            )
                            HomeRowDivider()
                            HomeBottomEdgeSection(
                                config = bottomEdge,
                                a11yGranted = a11yGranted,
                                expanded = bottomEdgeExpanded,
                                onExpandedChange = { bottomEdgeExpanded = it },
                                onChange = saveBottomEdge,
                            )
                            HomeRowDivider()
                            HomeAssistantRow(
                                assistantGranted = assistantGranted,
                                onSetUp = {
                                    WobbleHaptics.performUIHaptic(view)
                                    runCatching {
                                        context.startActivity(Intent(Settings.ACTION_VOICE_INPUT_SETTINGS))
                                    }
                                },
                            )
                            HomeRowDivider()
                            HomeLinkRow(
                                icon = Icons.AutoMirrored.Filled.VolumeUp,
                                title = stringResource(R.string.home_hw_title),
                                subtitle = stringResource(
                                    if (hwEnabledCount > 0) R.string.home_hw_count else R.string.home_hw_none,
                                    hwEnabledCount,
                                ),
                                enabled = a11yGranted,
                                onClick = { showHwSheet = true },
                            )
                            HomeRowDivider()
                            HomeLinkRow(
                                icon = Icons.Default.Widgets,
                                title = stringResource(R.string.trigger_qs_title),
                                subtitle = stringResource(R.string.trigger_qs_subtitle),
                                enabled = a11yGranted,
                            )
                        }
                    }
                }
            }

            item {
                Surface(
                    shape = RoundedCornerShape(28.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerLow,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    UnifiedSearchMethodSelector(
                        uiPreferences = uiPreferences,
                        modifier = Modifier.padding(16.dp),
                    )
                }
            }

            item {
                Column {
                    SettingsSectionHeader(title = stringResource(R.string.home_section_more))
                    Surface(
                        shape = RoundedCornerShape(28.dp),
                        color = MaterialTheme.colorScheme.surfaceContainerLow,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column {
                            HomeLinkRow(
                                icon = Icons.Default.Tune,
                                title = stringResource(R.string.title_settings),
                                subtitle = null,
                                onClick = {
                                    if (onOpenSettings != null) onOpenSettings("") else openSettingsPage("")
                                },
                            )
                        }
                    }
                }
            }
        }
    }

    if (showHwSheet) {
        HomeHwSheet(
            hwTriggers = hwTriggers,
            a11yGranted = a11yGranted,
            onToggle = onHwToggle,
            onDismiss = { showHwSheet = false },
        )
    }

    val grantedOnEntry = remember { a11yGranted || assistantGranted }
    if (grantedOnEntry) {
        val showDonateSheet = remember { mutableStateOf(false) }
        SupportDialogHost(showDonateSheet = showDonateSheet)
    }
}

@Composable
private fun HomeRowDivider() {
    HorizontalDivider(
        modifier = Modifier.padding(horizontal = 16.dp),
        thickness = 0.5.dp,
        color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.1f),
    )
}

@Composable
private fun HomeToggleItem(
    title: String,
    subtitle: String,
    icon: ImageVector,
    checked: Boolean,
    enabled: Boolean,
    onCheckedChange: (Boolean) -> Unit,
) {
    val alpha = if (enabled) 1f else 0.38f
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(enabled = enabled) { onCheckedChange(!checked) }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha),
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha),
            )
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha),
            )
        }
        Spacer(modifier = Modifier.width(8.dp))
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            enabled = enabled,
        )
    }
}

@Composable
private fun HomeLinkRow(
    icon: ImageVector,
    title: String,
    subtitle: String?,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    onClick: (() -> Unit)? = null,
) {
    val alpha = if (enabled) 1f else 0.38f
    Row(
        modifier = modifier
            .fillMaxWidth()
            .let { if (onClick != null && enabled) it.clickable { onClick() } else it }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha),
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha),
            )
            if (subtitle != null) {
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (onClick != null && enabled) {
            Icon(
                imageVector = Icons.AutoMirrored.Filled.KeyboardArrowRight,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.5f),
            )
        }
    }
}

@Composable
private fun HomeAssistantRow(
    assistantGranted: Boolean,
    onSetUp: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = Icons.Default.Assistant,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = stringResource(R.string.trigger_name_assistant),
                style = MaterialTheme.typography.bodyLarge,
            )
            Text(
                text = stringResource(
                    if (assistantGranted) R.string.home_assistant_ready_subtitle
                    else R.string.home_assistant_setup_subtitle,
                ),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(modifier = Modifier.width(8.dp))
        if (assistantGranted) {
            Surface(
                shape = RoundedCornerShape(50),
                color = MaterialTheme.colorScheme.secondaryContainer,
            ) {
                Text(
                    text = stringResource(R.string.home_ready),
                    style = MaterialTheme.typography.labelMedium,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onSecondaryContainer,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                )
            }
        } else {
            FilledTonalButton(onClick = onSetUp) {
                Text(stringResource(R.string.home_set_up))
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HomeHwSheet(
    hwTriggers: Map<String, Boolean>,
    a11yGranted: Boolean,
    onToggle: (String, Boolean) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(modifier = Modifier.padding(bottom = 32.dp)) {
            Text(
                text = stringResource(R.string.home_hw_title),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
            )
            if (!a11yGranted) {
                Text(
                    text = stringResource(R.string.trigger_hw_notice),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 4.dp),
                )
            }
            HW_TRIGGER_KEYS.forEachIndexed { index, key ->
                if (index > 0) HomeRowDivider()
                HomeToggleItem(
                    title = stringResource(HW_TRIGGER_TITLES[index]),
                    subtitle = stringResource(R.string.trigger_hw_subtitle),
                    icon = HW_TRIGGER_ICONS[index],
                    checked = hwTriggers[key] ?: false,
                    enabled = a11yGranted,
                    onCheckedChange = { onToggle(key, it) },
                )
            }
        }
    }
}

/**
 * Expandable block for the status-bar overlay, mirroring [HomeBottomEdgeSection]:
 * a header row with a master switch, and a dropdown with an animated strip preview
 * plus width / height / vertical-position sliders for the top trigger strip.
 */
@Composable
private fun HomeStatusBarSection(
    config: OverlayConfig,
    a11yGranted: Boolean,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    onChange: (OverlayConfig) -> Unit,
) {
    val masterOn = config.isEnabled
    val density = LocalDensity.current
    val expandProgress by animateFloatAsState(
        targetValue = if (expanded) 1f else 0f,
        animationSpec = tween(durationMillis = 240),
        label = "statusBarExpand",
    )

    // Keep the strip on screen (as a tinted preview) while the dropdown is open so
    // slider adjustments are visible live; closing the dropdown hides it again.
    LaunchedEffect(expanded) {
        if (config.previewVisible != expanded) {
            onChange(config.copy(previewVisible = expanded))
        }
    }

    val segment = config.segments.firstOrNull() ?: OverlaySegment()
    fun saveSegment(newSegment: OverlaySegment) {
        val segments = config.segments.toMutableList()
        if (segments.isEmpty()) segments.add(newSegment) else segments[0] = newSegment
        onChange(config.copy(segments = segments))
    }

    val screenWidthDp = LocalConfiguration.current.screenWidthDp.toFloat()
    val screenWidthPx = screenWidthDp * density.density
    val widthDp = segment.width / density.density
    val heightDp = segment.height / density.density
    val yDp = segment.yOffset / density.density

    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = a11yGranted) { onExpandedChange(!expanded) }
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val rowAlpha = if (a11yGranted) 1f else 0.38f
            Icon(
                imageVector = Icons.Rounded.KeyboardArrowDown,
                contentDescription = null,
                modifier = Modifier
                    .size(24.dp)
                    .graphicsLayer {
                        alpha = rowAlpha
                        rotationZ = expandProgress * 180f
                    },
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.width(16.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.trigger_statusbar_title),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = rowAlpha),
                )
                Text(
                    text = stringResource(R.string.trigger_statusbar_subtitle),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = rowAlpha),
                )
            }
            Spacer(modifier = Modifier.width(8.dp))
            Switch(
                checked = masterOn,
                enabled = a11yGranted,
                onCheckedChange = { on ->
                    onChange(config.copy(isEnabled = on))
                    if (on) onExpandedChange(true)
                },
            )
        }

        AnimatedVisibility(
            visible = expanded && a11yGranted,
            enter = expandVertically(tween(240)) + fadeIn(tween(240)),
            exit = shrinkVertically(tween(200)) + fadeOut(tween(160)),
        ) {
            Column(modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) {
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerLow,
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(modifier = Modifier.padding(12.dp)) {
                        // Animated preview of the trigger strip on a mini phone canvas.
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(72.dp)
                                .graphicsLayer { alpha = expandProgress },
                            contentAlignment = Alignment.TopCenter,
                        ) {
                            Box(
                                modifier = Modifier
                                    .fillMaxSize()
                                    .padding(horizontal = 48.dp)
                                    .clip(RoundedCornerShape(14.dp))
                                    .background(MaterialTheme.colorScheme.surfaceContainerHighest),
                                contentAlignment = Alignment.TopCenter,
                            ) {
                                val stripWidthFraction =
                                    (segment.width / screenWidthPx).coerceIn(0.08f, 1f)
                                Box(
                                    modifier = Modifier
                                        .padding(top = yDp.dp)
                                        .fillMaxWidth(stripWidthFraction)
                                        .height(heightDp.coerceAtMost(56f).dp)
                                        .background(
                                            MaterialTheme.colorScheme.primary.copy(alpha = 0.25f),
                                        ),
                                )
                            }
                        }

                        Spacer(modifier = Modifier.height(8.dp))

                        LabelledSlider(
                            label = stringResource(R.string.home_statusbar_width),
                            value = widthDp,
                            valueRange = 60f..maxOf(screenWidthDp, widthDp),
                            valueText = "${widthDp.toInt()}dp",
                            onValueChange = {
                                saveSegment(segment.copy(width = (it * density.density).toInt()))
                            },
                        )
                        LabelledSlider(
                            label = stringResource(R.string.home_statusbar_height),
                            value = heightDp,
                            valueRange = 24f..140f,
                            valueText = "${heightDp.toInt()}dp",
                            onValueChange = {
                                saveSegment(segment.copy(height = (it * density.density).toInt()))
                            },
                        )
                        LabelledSlider(
                            label = stringResource(R.string.home_statusbar_ypos),
                            value = yDp,
                            valueRange = 0f..160f,
                            valueText = "${yDp.toInt()}dp",
                            onValueChange = {
                                saveSegment(segment.copy(yOffset = (it * density.density).toInt()))
                            },
                        )
                    }
                }
            }
        }
    }
}

/**
 * Compact, expandable block for the bottom-edge long-press triggers.
 * Master switch enables/disables; expanding it reveals an animated illustration plus
 * per-corner toggles, a corner-size slider, and a separate bottom-mid switch + slider.
 * While expanded, [BottomEdgeConfig.previewVisible] is forced true so the real overlays
 * appear on screen for live sizing; collapsing (or leaving) hides them again.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun HomeBottomEdgeSection(
    config: BottomEdgeConfig,
    a11yGranted: Boolean,
    expanded: Boolean,
    onExpandedChange: (Boolean) -> Unit,
    onChange: (BottomEdgeConfig) -> Unit,
) {
    val masterOn = config.leftEnabled || config.rightEnabled || config.midEnabled

    // Keep preview in sync with expansion state.
    LaunchedEffect(expanded) {
        if (config.previewVisible != expanded) {
            onChange(config.copy(previewVisible = expanded))
        }
    }

    val cornerSize by animateDpAsState(
        targetValue = config.cornerSizeDp.dp,
        animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
        label = "cornerSize",
    )
    val midSize by animateDpAsState(
        targetValue = config.midSizeDp.dp,
        animationSpec = spring(stiffness = Spring.StiffnessMediumLow),
        label = "midSize",
    )
    val expandProgress by animateFloatAsState(
        targetValue = if (expanded) 1f else 0f,
        animationSpec = tween(durationMillis = 240),
        label = "expandProgress",
    )

    Column(modifier = Modifier.fillMaxWidth()) {
        // Header row: icon + label + master switch + expand chevron.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable(enabled = a11yGranted) { onExpandedChange(!expanded) }
                .padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val rowAlpha = if (a11yGranted) 1f else 0.38f
            Icon(
                imageVector = Icons.Rounded.KeyboardArrowDown,
                contentDescription = null,
                modifier = Modifier
                    .size(24.dp)
                    .graphicsLayer {
                        alpha = rowAlpha
                        rotationZ = expandProgress * 180f
                    },
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(modifier = Modifier.width(16.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.home_bottom_edge_title),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = rowAlpha),
                )
                Text(
                    text = stringResource(R.string.home_bottom_edge_subtitle),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = rowAlpha),
                )
            }
            Spacer(modifier = Modifier.width(8.dp))
            Switch(
                checked = masterOn,
                enabled = a11yGranted,
                onCheckedChange = { on ->
                    onChange(
                        config.copy(
                            leftEnabled = on,
                            rightEnabled = on,
                            // keep mid as-is unless turning everything on for the first time
                            midEnabled = if (on) config.midEnabled else false,
                        ),
                    )
                    if (on) onExpandedChange(true)
                },
            )
        }

        AnimatedVisibility(
            visible = expanded && a11yGranted,
            enter = expandVertically(tween(240)) + fadeIn(tween(240)),
            exit = shrinkVertically(tween(200)) + fadeOut(tween(160)),
        ) {
            Column(modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp)) {
                // Compact container grouping the illustration, toggles and sliders so
                // they share one consistent width and read as a single control block.
                Surface(
                    shape = RoundedCornerShape(16.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerLow,
                    border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(modifier = Modifier.padding(12.dp)) {
                        // Animated illustration of the phone bottom edge.
                        Box(
                            modifier = Modifier
                                .fillMaxWidth()
                                .height(96.dp)
                                .graphicsLayer { alpha = expandProgress },
                            contentAlignment = Alignment.Center,
                        ) {
                            BottomEdgeIllustration(
                                leftEnabled = config.leftEnabled,
                                rightEnabled = config.rightEnabled,
                                midEnabled = config.midEnabled,
                                cornerSizeDp = config.cornerSizeDp,
                                midSizeDp = config.midSizeDp,
                                expandProgress = expandProgress,
                            )
                        }

                        Spacer(modifier = Modifier.height(8.dp))

                        // Per-shape toggles as a small M3 expressive connected button
                        // group: Left / Mid / Right. Unchecked buttons get a chip-like
                        // outline so they stay clearly visible.
                        val groupBorder = BorderStroke(1.dp, MaterialTheme.colorScheme.outline)
                        ButtonGroup(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement =
                                Arrangement.spacedBy(ButtonGroupDefaults.connectedSpaceBetween),
                            animateFraction = 0f,
                        ) {
                            val startShapes = ButtonShapes(
                                shape = ButtonGroupDefaults.connectedLeadingButtonShape,
                                pressedShape = ButtonGroupDefaults.connectedLeadingButtonPressShape,
                                checkedShape = ToggleButtonDefaults.checkedShape,
                            )
                            val middleShapes = ButtonShapes(
                                shape = ShapeDefaults.Small,
                                pressedShape = ToggleButtonDefaults.pressedShape,
                                checkedShape = ToggleButtonDefaults.checkedShape,
                            )
                            val endShapes = ButtonShapes(
                                shape = ButtonGroupDefaults.connectedTrailingButtonShape,
                                pressedShape = ButtonGroupDefaults.connectedTrailingButtonPressShape,
                                checkedShape = ToggleButtonDefaults.checkedShape,
                            )
                            ToggleButton(
                                checked = config.leftEnabled,
                                onCheckedChange = { on ->
                                    onChange(config.copy(leftEnabled = on))
                                },
                                shapes = startShapes,
                                border = groupBorder,
                                modifier = Modifier.weight(1f),
                            ) {
                                Text(
                                    text = stringResource(R.string.home_corner_left),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            ToggleButton(
                                checked = config.midEnabled,
                                onCheckedChange = { on ->
                                    onChange(config.copy(midEnabled = on))
                                },
                                shapes = middleShapes,
                                border = groupBorder,
                                modifier = Modifier.weight(1f),
                            ) {
                                Text(
                                    text = stringResource(R.string.home_bottom_mid_title),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                            ToggleButton(
                                checked = config.rightEnabled,
                                onCheckedChange = { on ->
                                    onChange(config.copy(rightEnabled = on))
                                },
                                shapes = endShapes,
                                border = groupBorder,
                                modifier = Modifier.weight(1f),
                            ) {
                                Text(
                                    text = stringResource(R.string.home_corner_right),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }

                        Spacer(modifier = Modifier.height(10.dp))

                        // Corner size slider.
                        LabelledSlider(
                            label = stringResource(R.string.home_corner_size),
                            value = config.cornerSizeDp.toFloat(),
                            valueRange = 48f..140f,
                            valueText = "${config.cornerSizeDp}dp",
                            onValueChange = { onChange(config.copy(cornerSizeDp = it.toInt())) },
                        )

                        // Mid size slider (only meaningful when mid enabled).
                        AnimatedVisibility(visible = config.midEnabled) {
                            LabelledSlider(
                                label = stringResource(R.string.home_mid_size),
                                value = config.midSizeDp.toFloat(),
                                valueRange = 48f..200f,
                                valueText = "${config.midSizeDp}dp",
                                onValueChange = { onChange(config.copy(midSizeDp = it.toInt())) },
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun LabelledSlider(
    label: String,
    value: Float,
    valueRange: ClosedFloatingPointRange<Float>,
    valueText: String,
    onValueChange: (Float) -> Unit,
) {
    Column(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                text = valueText,
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        Slider(
            value = value,
            onValueChange = onValueChange,
            valueRange = valueRange,
        )
    }
}

/**
 * A lightweight animated illustration of a phone's bottom edge showing the two corner arcs
 * and the bottom-mid semicircle, so the user can visualise what they are enabling/sizing.
 */
@Composable
private fun BottomEdgeIllustration(
    leftEnabled: Boolean,
    rightEnabled: Boolean,
    midEnabled: Boolean,
    cornerSizeDp: Int,
    midSizeDp: Int,
    expandProgress: Float,
) {
    val primary = MaterialTheme.colorScheme.primary
    val outline = MaterialTheme.colorScheme.outlineVariant

    // Subtle idle pulse so the enabled shapes feel alive.
    val infiniteTransition = rememberInfiniteTransition(label = "edgePulse")
    val pulse by infiniteTransition.animateFloat(
        initialValue = 0.85f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1100), RepeatMode.Reverse),
        label = "pulse",
    )

    Canvas(
        modifier = Modifier
            .fillMaxWidth()
            .height(96.dp),
    ) {
        val w = size.width
        val h = size.height
        // Phone outline: rounded rect occupying the lower portion (wide so the
        // shapes have room and read as small against it).
        val phoneTop = h * 0.14f
        val phoneBottom = h * 0.92f
        val phoneLeft = w * 0.06f
        val phoneRight = w * 0.94f
        val cornerRadius = 18.dp.toPx()

        drawRoundRect(
            color = outline.copy(alpha = 0.35f * expandProgress),
            topLeft = Offset(phoneLeft, phoneTop),
            size = Size(phoneRight - phoneLeft, phoneBottom - phoneTop),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(cornerRadius, cornerRadius),
        )

        // Scale dp sizes to fit the illustration box, kept tiny and clamped so a
        // shape can never grow outside the phone outline. Fractions of the slider
        // ranges keep the illustration device-independent.
        val phoneW = phoneRight - phoneLeft
        val phoneH = phoneBottom - phoneTop
        val maxCorridor = phoneW / 2f
        val maxRadius = minOf(maxCorridor * 0.45f, phoneH * 0.85f)
        val cornerFrac = ((cornerSizeDp - 48f) / 92f).coerceIn(0f, 1f)
        val midFrac = ((midSizeDp - 48f) / 152f).coerceIn(0f, 1f)
        val cornerScale = (0.35f + 0.65f * cornerFrac) * maxRadius
        val midScale = (0.35f + 0.65f * midFrac) * maxRadius

        fun drawArcShape(isLeft: Boolean, radius: Float) {
            val cx = if (isLeft) phoneLeft else phoneRight
            val cy = phoneBottom
            val path = Path().apply {
                moveTo(cx, cy)
                if (isLeft) {
                    lineTo(cx, cy - radius)
                    arcTo(
                        Rect(cx - radius, cy - radius, cx + radius, cy + radius),
                        startAngleDegrees = 270f,
                        sweepAngleDegrees = 90f,
                        forceMoveTo = false,
                    )
                } else {
                    lineTo(cx, cy - radius)
                    arcTo(
                        Rect(cx - radius, cy - radius, cx + radius, cy + radius),
                        startAngleDegrees = 270f,
                        sweepAngleDegrees = -90f,
                        forceMoveTo = false,
                    )
                }
                close()
            }
            drawPath(path, color = primary.copy(alpha = 0.35f * pulse * expandProgress))
            drawPath(path, color = primary.copy(alpha = 0.8f * expandProgress), style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
        }

        if (leftEnabled) drawArcShape(true, cornerScale)
        if (rightEnabled) drawArcShape(false, cornerScale)

        if (midEnabled) {
            val cx = (phoneLeft + phoneRight) / 2f
            val cy = phoneBottom
            val r = midScale
            val path = Path().apply {
                moveTo(cx - r, cy)
                arcTo(
                    Rect(cx - r, cy - r, cx + r, cy + r),
                    startAngleDegrees = 180f,
                    sweepAngleDegrees = 180f,
                    forceMoveTo = false,
                )
                close()
            }
            drawPath(path, color = primary.copy(alpha = 0.35f * pulse * expandProgress))
            drawPath(path, color = primary.copy(alpha = 0.8f * expandProgress), style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
        }
    }
}


