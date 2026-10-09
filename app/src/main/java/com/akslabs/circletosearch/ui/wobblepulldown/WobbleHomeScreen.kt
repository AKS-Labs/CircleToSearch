package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Context
import android.content.Intent
import android.provider.Settings
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateIntAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.VolumeDown
import androidx.compose.material.icons.automirrored.filled.VolumeUp
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Assistant
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Layers
import androidx.compose.material.icons.filled.PowerSettingsNew
import androidx.compose.material.icons.filled.TouchApp
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material.icons.filled.Widgets
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.app.ActivityOptionsCompat
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.akslabs.circletosearch.MainActivity
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.data.ActionType
import com.akslabs.circletosearch.data.GestureType
import com.akslabs.circletosearch.data.OverlayConfigurationManager
import com.akslabs.circletosearch.data.OverlaySegment
import com.akslabs.circletosearch.ui.SettingsSectionHeader
import com.akslabs.circletosearch.ui.SettingsToggleItem
import com.akslabs.circletosearch.ui.components.UnifiedSearchMethodSelector
import com.akslabs.circletosearch.utils.UIPreferences
import dev.davidv.translator.MainActivity as TranslatorMainActivity
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
    var bubbleEnabled by remember { mutableStateOf(appPrefs.getBoolean("bubble_enabled", false)) }
    var hwTriggers by remember {
        mutableStateOf(HW_TRIGGER_KEYS.associateWith { appPrefs.getBoolean(it, false) })
    }
    var showHwSheet by remember { mutableStateOf(false) }
    var bottomEdgeOn by remember {
        mutableStateOf(
            configManager.getConfig().segments
                .any { it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER },
        )
    }
    val hwEnabledCount = hwTriggers.values.count { it }
    val running = assistantGranted ||
        (a11yGranted && (overlayEnabled || bubbleEnabled || bottomEdgeOn || hwEnabledCount > 0))

    val refreshHomeState = {
        a11yGranted = isA11yGranted(context)
        assistantGranted = isAssistantGranted(context)
        overlayEnabled = configManager.getConfig().isEnabled
        bubbleEnabled = appPrefs.getBoolean("bubble_enabled", false)
        hwTriggers = HW_TRIGGER_KEYS.associateWith { appPrefs.getBoolean(it, false) }
        bottomEdgeOn = configManager.getConfig().segments
            .any { it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER }
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, _ -> refreshHomeState() }
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

    val setOverlayEnabled: (Boolean) -> Unit = { on ->
        configManager.saveConfig(configManager.getConfig().copy(isEnabled = on))
        overlayEnabled = on
    }

    val setBubbleEnabled: (Boolean) -> Unit = { on ->
        appPrefs.edit().putBoolean("bubble_enabled", on).apply()
        bubbleEnabled = on
    }

    val onHwToggle: (String, Boolean) -> Unit = { key, checked ->
        appPrefs.edit().putBoolean(key, checked).apply()
        hwTriggers = hwTriggers + (key to checked)
    }

    val setBottomEdge: (Boolean) -> Unit = { on ->
        val cfg = configManager.getConfig()
        val segments = cfg.segments.toMutableList()
        if (on) {
            if (segments.none { it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER }) {
                val dm = context.resources.displayMetrics
                val stripPx = with(density) { 48.dp.roundToPx() }
                segments.add(
                    OverlaySegment(
                        width = dm.widthPixels,
                        height = stripPx,
                        xOffset = 0,
                        yOffset = (dm.heightPixels - stripPx).coerceAtLeast(0),
                        gestures = mutableMapOf(GestureType.LONG_PRESS to ActionType.CTS_AUTO),
                        gestureData = mutableMapOf(GestureType.LONG_PRESS to BOTTOM_EDGE_MARKER),
                    ),
                )
            }
        } else {
            segments.removeAll { it.gestureData[GestureType.LONG_PRESS] == BOTTOM_EDGE_MARKER }
        }
        configManager.saveConfig(cfg.copy(segments = segments))
        bottomEdgeOn = on
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
                            SettingsToggleItem(
                                title = stringResource(R.string.trigger_statusbar_title),
                                subtitle = stringResource(R.string.trigger_statusbar_subtitle),
                                icon = Icons.Default.Layers,
                                checked = overlayEnabled,
                                onCheckedChange = { setOverlayEnabled(it) },
                            )
                            HomeRowDivider()
                            SettingsToggleItem(
                                title = stringResource(R.string.label_floating_bubble),
                                subtitle = stringResource(R.string.trigger_bubble_subtitle),
                                icon = Icons.Default.TouchApp,
                                checked = bubbleEnabled,
                                onCheckedChange = { setBubbleEnabled(it) },
                            )
                            HomeRowDivider()
                            SettingsToggleItem(
                                title = stringResource(R.string.home_bottom_edge_title),
                                subtitle = stringResource(R.string.home_bottom_edge_subtitle),
                                icon = Icons.Rounded.KeyboardArrowDown,
                                checked = bottomEdgeOn,
                                onCheckedChange = { setBottomEdge(it) },
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
                                onClick = { showHwSheet = true },
                            )
                            HomeRowDivider()
                            HomeLinkRow(
                                icon = Icons.Default.Widgets,
                                title = stringResource(R.string.trigger_qs_title),
                                subtitle = stringResource(R.string.trigger_qs_subtitle),
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
                                icon = Icons.Default.Translate,
                                title = stringResource(R.string.label_translator_title),
                                subtitle = stringResource(R.string.label_translator_subtitle),
                                onClick = {
                                    WobbleHaptics.performUIHaptic(view)
                                    runCatching {
                                        context.startActivity(
                                            Intent(context, TranslatorMainActivity::class.java),
                                        )
                                    }
                                },
                            )
                            HomeRowDivider()
                            HomeLinkRow(
                                icon = Icons.Default.Tune,
                                title = stringResource(R.string.title_settings),
                                subtitle = null,
                                onClick = { openSettingsPage("") },
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
private fun HomeLinkRow(
    icon: ImageVector,
    title: String,
    subtitle: String?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .let { if (onClick != null) it.clickable { onClick() } else it }
            .padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            modifier = Modifier.size(24.dp),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(modifier = Modifier.width(16.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(text = title, style = MaterialTheme.typography.bodyLarge)
            if (subtitle != null) {
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (onClick != null) {
            Icon(
                imageVector = Icons.Default.KeyboardArrowRight,
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
                SettingsToggleItem(
                    title = stringResource(HW_TRIGGER_TITLES[index]),
                    subtitle = stringResource(R.string.trigger_hw_subtitle),
                    icon = HW_TRIGGER_ICONS[index],
                    checked = hwTriggers[key] ?: false,
                    onCheckedChange = { onToggle(key, it) },
                )
            }
        }
    }
}


