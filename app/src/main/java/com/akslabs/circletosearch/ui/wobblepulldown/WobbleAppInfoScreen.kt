package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.net.Uri
import android.provider.Settings
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.StringRes
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.rounded.Accessibility
import androidx.compose.material.icons.rounded.Apps
import androidx.compose.material.icons.rounded.Assistant
import androidx.compose.material.icons.rounded.TouchApp
import androidx.compose.material.icons.rounded.Verified
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.core.content.pm.PackageInfoCompat
import androidx.core.graphics.drawable.toBitmap
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.data.OverlayConfigurationManager
import com.akslabs.circletosearch.ui.components.AccessibilityDisclosureDialog
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val GITHUB_URL = "https://github.com/aks-labs"
private const val MAX_PULL_PX = 160f
private const val PULL_HAPTIC_PX = 120f

private enum class TriggerStatus { ACTIVE, NEEDS_A11Y, OFF }

private data class LaunchMethod(
    @StringRes val titleRes: Int,
    val status: TriggerStatus,
)

private data class AppInfoData(
    val versionName: String,
    val versionCode: Long,
    val isDebug: Boolean,
)

private val HW_TRIGGER_KEYS = listOf(
    "trigger_double_vol_up",
    "trigger_double_vol_down",
    "trigger_double_power",
    "trigger_power_vol_up",
    "trigger_power_vol_down",
    "trigger_vol_up_down",
)

private fun loadAppInfo(context: Context): AppInfoData {
    return try {
        val packageInfo = context.packageManager.getPackageInfo(context.packageName, 0)
        AppInfoData(
            versionName = packageInfo.versionName ?: "?",
            versionCode = PackageInfoCompat.getLongVersionCode(packageInfo),
            isDebug = ((packageInfo.applicationInfo?.flags ?: 0) and ApplicationInfo.FLAG_DEBUGGABLE) != 0,
        )
    } catch (_: Exception) {
        AppInfoData(versionName = "?", versionCode = 0L, isDebug = false)
    }
}

private fun loadLaunchMethods(context: Context): List<LaunchMethod> {
    val a11y = isA11yGranted(context)
    val overlayOn = OverlayConfigurationManager(context).getStatusBarConfig().isEnabled
    val appPrefs = context.getSharedPreferences("app_prefs", Context.MODE_PRIVATE)
    val bubbleOn = appPrefs.getBoolean("bubble_enabled", false)
    val hwCount = HW_TRIGGER_KEYS.count { appPrefs.getBoolean(it, false) }
    val assistant = isAssistantGranted(context)

    fun status(configured: Boolean): TriggerStatus = when {
        !configured -> TriggerStatus.OFF
        a11y -> TriggerStatus.ACTIVE
        else -> TriggerStatus.NEEDS_A11Y
    }

    return listOf(
        LaunchMethod(R.string.trigger_statusbar_title, status(overlayOn)),
        LaunchMethod(R.string.label_floating_bubble, status(bubbleOn)),
        LaunchMethod(R.string.info_method_hardware, status(hwCount > 0)),
        LaunchMethod(
            R.string.trigger_qs_title,
            if (a11y) TriggerStatus.ACTIVE else TriggerStatus.NEEDS_A11Y,
        ),
        LaunchMethod(
            R.string.trigger_name_assistant,
            if (assistant) TriggerStatus.ACTIVE else TriggerStatus.OFF,
        ),
    )
}

private fun openLink(context: Context, url: String) {
    runCatching {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
    }.onFailure {
        Toast.makeText(context, context.getString(R.string.toast_could_not_open_link), Toast.LENGTH_SHORT).show()
    }
}

@OptIn(ExperimentalTextApi::class)
@Composable
fun WobbleAppInfoScreen(
    onBackClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current
    val appInfo = remember { loadAppInfo(context) }
    val appName = stringResource(R.string.app_name)
    val scrollState = rememberScrollState()
    val overscrollOffset = remember { Animatable(0f) }

    var a11yGranted by remember { mutableStateOf(isA11yGranted(context)) }
    var assistantGranted by remember { mutableStateOf(isAssistantGranted(context)) }
    var launchMethods by remember { mutableStateOf(loadLaunchMethods(context)) }
    var showDisclosure by remember { mutableStateOf(false) }
    var showAssistantSheet by remember { mutableStateOf(false) }

    val refreshPermissions = {
        a11yGranted = isA11yGranted(context)
        assistantGranted = isAssistantGranted(context)
        launchMethods = loadLaunchMethods(context)
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
                }.onFailure {
                    Toast.makeText(
                        context,
                        context.getString(R.string.toast_accessibility_settings_error),
                        Toast.LENGTH_LONG,
                    ).show()
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

    val grantedCount = (if (a11yGranted) 1 else 0) + (if (assistantGranted) 1 else 0)
    val activeTriggerCount = launchMethods.count { it.status == TriggerStatus.ACTIVE }

    val nestedScrollConnection = remember {
        object : NestedScrollConnection {
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (available.y < 0 && overscrollOffset.value > 0) {
                    val toConsume = if (overscrollOffset.value + available.y >= 0) available.y else -overscrollOffset.value
                    scope.launch { overscrollOffset.snapTo(overscrollOffset.value + toConsume) }
                    return Offset(0f, toConsume)
                }
                return Offset.Zero
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (available.y > 0 && scrollState.value == 0) {
                    val prevValue = overscrollOffset.value
                    if (prevValue >= MAX_PULL_PX) return Offset.Zero

                    val newValue = (prevValue + available.y * 0.5f).coerceAtMost(MAX_PULL_PX)
                    scope.launch { overscrollOffset.snapTo(newValue) }

                    if (prevValue < PULL_HAPTIC_PX && newValue >= PULL_HAPTIC_PX) {
                        WobbleHaptics.performUIHaptic(view)
                    }

                    return Offset(0f, available.y)
                }
                return super.onPostScroll(consumed, available, source)
            }
        }
    }

    var isStartupAnimDone by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        isStartupAnimDone = true
    }

    val contentAlphaState by animateFloatAsState(
        targetValue = if (isStartupAnimDone) 1f else 0f,
        animationSpec = tween(durationMillis = 400, easing = EaseOut),
        label = "contentAlpha",
    )

    val contentOffsetState by animateDpAsState(
        targetValue = if (isStartupAnimDone) 0.dp else 40.dp,
        animationSpec = tween(durationMillis = 400, easing = FastOutSlowInEasing),
        label = "contentOffset",
    )

    val statusBarHeightPx = with(density) {
        WindowInsets.statusBars.asPaddingValues().calculateTopPadding().toPx()
    }

    val collapsePx = with(density) { 160.dp.toPx() }
    val progress = remember(collapsePx) {
        derivedStateOf { (scrollState.value / collapsePx).coerceIn(0f, 1f) }
    }

    val textMeasurer = rememberTextMeasurer()
    val nameFontFamily = remember {
        try {
            FontFamily(
                Font(
                    R.font.google_sans_flex,
                    variationSettings = FontVariation.Settings(
                        FontVariation.weight(FontWeight.Bold.weight),
                        FontVariation.Setting("ROND", 100f),
                    ),
                )
            )
        } catch (_: Exception) {
            FontFamily.Default
        }
    }
    val nameStyle = MaterialTheme.typography.headlineMedium.copy(
        fontFamily = nameFontFamily,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.onSurface,
    )
    val nameSize = remember(appName, nameStyle) {
        textMeasurer.measure(appName, nameStyle).size
    }

    BoxWithConstraints(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .progressiveBlur(
                blurRadius = 40f,
                height = statusBarHeightPx * 1.5f,
                direction = BlurDirection.TOP,
            ),
    ) {
        val contentWidth = maxWidth

        val padH = 16.dp
        val headerY = WindowInsets.statusBars.asPaddingValues().calculateTopPadding() + 16.dp
        val headerH = 48.dp
        val heroTop = headerY + headerH + 16.dp
        val iconExpanded = 96.dp
        val iconCollapsed = 36.dp
        val nameGap = 12.dp

        val nameHeightDp = with(density) { nameSize.height.toDp() }
        val heroBottom = heroTop + 12.dp + iconExpanded + 24.dp + nameHeightDp + 4.dp + 21.dp + 12.dp
        val contentTop = heroBottom + 16.dp
        val headerBottom = headerY + headerH + 8.dp

        val slotPos = remember { mutableStateOf(Offset.Zero) }

        val maxWpx = with(density) { contentWidth.toPx() }
        val naturalIconX = with(density) { ((contentWidth - iconExpanded) / 2).toPx() }
        val naturalIconY = with(density) { (heroTop + 12.dp).toPx() }
        val naturalNameX = (maxWpx - nameSize.width) / 2f
        val naturalNameY = with(density) { (heroTop + 12.dp + iconExpanded + 24.dp).toPx() }
        val iconExpandedPx = with(density) { iconExpanded.toPx() }
        val iconCollapsedPx = with(density) { iconCollapsed.toPx() }
        val collapsedNameX = slotPos.value.x + iconCollapsedPx + with(density) { nameGap.toPx() }
        val iconScale = (with(density) { iconCollapsed.toPx() }) / (with(density) { iconExpanded.toPx() })
        val nameScale = 16f / 28f
        val collapsedNameY = slotPos.value.y + (iconCollapsedPx - nameSize.height * nameScale) / 2f

        val pullScale = 1f + overscrollOffset.value / 1000f

        val coverHeight = heroBottom + (headerBottom - heroBottom) * progress.value
        val coverAlpha = (progress.value * 4f).coerceIn(0f, 1f)

        Column(
            modifier = Modifier
                .fillMaxSize()
                .progressiveBlur(
                    blurRadius = 40f,
                    height = with(density) { 120.dp.toPx() },
                    direction = BlurDirection.BOTTOM,
                )
                .nestedScroll(nestedScrollConnection)
                .pointerInput(Unit) {
                    awaitPointerEventScope {
                        while (true) {
                            val event = awaitPointerEvent()
                            if (event.type == PointerEventType.Release && overscrollOffset.value > 0) {
                                scope.launch {
                                    overscrollOffset.animateTo(
                                        0f,
                                        spring(stiffness = Spring.StiffnessMediumLow),
                                    )
                                }
                            }
                        }
                    }
                }
                .verticalScroll(scrollState)
                .padding(
                    top = contentTop,
                    bottom = 120.dp,
                    start = padH,
                    end = padH,
                ),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                text = stringResource(R.string.section_permissions),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 8.dp),
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

            Text(
                text = stringResource(R.string.section_launch_methods),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 8.dp),
            )

            Surface(
                shape = RoundedCornerShape(24.dp),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(
                    modifier = Modifier.padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    launchMethods.forEach { method ->
                        TriggerRow(method = method)
                    }
                }
            }

            Text(
                text = stringResource(R.string.info_section_about),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 8.dp),
            )

            Surface(
                shape = RoundedCornerShape(24.dp),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    InfoSpecItem(
                        label = stringResource(R.string.info_package),
                        value = context.packageName,
                    )
                    InfoSpecItem(
                        label = stringResource(R.string.info_build),
                        value = stringResource(
                            if (appInfo.isDebug) R.string.info_build_debug else R.string.info_build_release,
                        ),
                    )
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(8.dp))
                            .clickable { openLink(context, GITHUB_URL) }
                            .padding(vertical = 2.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Row(
                            verticalAlignment = Alignment.CenterVertically,
                            modifier = Modifier.weight(1f),
                        ) {
                            Icon(
                                imageVector = Icons.Default.Code,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.size(18.dp),
                            )
                            Spacer(modifier = Modifier.width(8.dp))
                            Text(
                                text = stringResource(R.string.about_source_code),
                                style = MaterialTheme.typography.bodyMedium,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Text(
                            text = stringResource(R.string.about_source_subtitle),
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onSurface,
                        )
                    }
                }
            }

            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .clickable { openLink(context, GITHUB_URL) }
                    .padding(vertical = 8.dp),
                horizontalArrangement = Arrangement.Center,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(
                    painter = painterResource(R.drawable.github),
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.size(20.dp),
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text(
                    text = stringResource(R.string.info_made_by_akslabs),
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.primary,
                )
            }
        }

        Box(modifier = Modifier.matchParentSize()) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(coverHeight)
                    .graphicsLayer { alpha = coverAlpha }
                    .background(MaterialTheme.colorScheme.surfaceContainer),
            )

            Row(
                modifier = Modifier
                    .offset(x = padH, y = headerY)
                    .fillMaxWidth()
                    .height(headerH),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(
                    onClick = {
                        WobbleHaptics.performUIHaptic(view)
                        onBackClick()
                    },
                    modifier = Modifier
                        .clip(CircleShape)
                        .background(MaterialTheme.colorScheme.surfaceContainerHigh),
                ) {
                    Icon(
                        imageVector = Icons.AutoMirrored.Rounded.ArrowBack,
                        contentDescription = "Back",
                        tint = MaterialTheme.colorScheme.onSurface,
                    )
                }
                Spacer(modifier = Modifier.width(nameGap))
                Box(
                    modifier = Modifier
                        .size(iconCollapsed)
                        .onGloballyPositioned { coordinates ->
                            slotPos.value = coordinates.positionInRoot()
                        },
                )
            }

            Column(
                modifier = Modifier
                    .offset(x = 0.dp, y = heroTop)
                    .fillMaxWidth()
                    .padding(horizontal = padH)
                    .padding(vertical = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                val iconModifier = Modifier
                    .size(iconExpanded)
                    .graphicsLayer {
                        val p = progress.value
                        val s = 1f + (iconScale - 1f) * p
                        transformOrigin = TransformOrigin(0f, 0f)
                        scaleX = s * pullScale
                        scaleY = s * pullScale
                        translationX = (slotPos.value.x - naturalIconX) * p -
                            iconExpandedPx * (pullScale - 1f) / 2f
                        translationY = (slotPos.value.y - naturalIconY) * p + contentOffsetState.toPx() -
                            iconExpandedPx * (pullScale - 1f) / 2f
                        alpha = contentAlphaState
                    }

                val appIcon = remember(context, density) {
                    runCatching {
                        val drawable = context.packageManager.getApplicationIcon(context.packageName)
                        val sizePx = if (drawable.intrinsicWidth > 0) {
                            drawable.intrinsicWidth
                        } else {
                            with(density) { 192.dp.toPx().toInt() }.coerceAtLeast(1)
                        }
                        drawable.toBitmap(sizePx, sizePx).asImageBitmap()
                    }.getOrNull()
                }

                if (appIcon != null) {
                    Image(
                        bitmap = appIcon,
                        contentDescription = null,
                        modifier = iconModifier,
                    )
                } else {
                    Icon(
                        imageVector = Icons.Rounded.Apps,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = iconModifier,
                    )
                }

                Spacer(modifier = Modifier.height(24.dp))

                Text(
                    text = appName,
                    style = nameStyle,
                    modifier = Modifier.graphicsLayer {
                        val p = progress.value
                        val s = 1f + (nameScale - 1f) * p
                        transformOrigin = TransformOrigin(0f, 0f)
                        scaleX = s * pullScale
                        scaleY = s * pullScale
                        translationX = (collapsedNameX - naturalNameX) * p -
                            nameSize.width * (pullScale - 1f) / 2f
                        translationY = (collapsedNameY - naturalNameY) * p + contentOffsetState.toPx() -
                            nameSize.height * (pullScale - 1f) / 2f
                        alpha = contentAlphaState
                    },
                )

                Spacer(modifier = Modifier.height(4.dp))

                Text(
                    text = "${appInfo.versionName} (${appInfo.versionCode})",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.graphicsLayer {
                        val fade = (1f - progress.value * 2.5f).coerceIn(0f, 1f)
                        alpha = contentAlphaState * fade
                        translationY = contentOffsetState.toPx()
                    },
                )
            }
        }
    }
}

@Composable
private fun TriggerRow(method: LaunchMethod) {
    val value: String
    val color: Color
    when (method.status) {
        TriggerStatus.ACTIVE -> {
            value = stringResource(R.string.info_status_active)
            color = MaterialTheme.colorScheme.primary
        }
        TriggerStatus.NEEDS_A11Y -> {
            value = stringResource(R.string.info_status_needs_a11y)
            color = MaterialTheme.colorScheme.error
        }
        TriggerStatus.OFF -> {
            value = stringResource(R.string.info_status_off)
            color = MaterialTheme.colorScheme.onSurfaceVariant
        }
    }

    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = stringResource(method.titleRes),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = color,
        )
    }
}

@Composable
private fun InfoSpecItem(label: String, value: String) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}
