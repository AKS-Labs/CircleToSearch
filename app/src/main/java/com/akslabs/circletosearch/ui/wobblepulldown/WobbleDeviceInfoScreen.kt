package com.sameerasw.essentials.wobblepulldown

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.EaseOut
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
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
import androidx.compose.material.icons.rounded.Memory
import androidx.compose.material.icons.rounded.Storage
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.sameerasw.essentials.R
import kotlinx.coroutines.launch

@Composable
fun WobbleDeviceInfoScreen(
    onBackClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val view = LocalView.current
    val scope = rememberCoroutineScope()
    val deviceInfo = remember { WobbleDeviceUtils.getDeviceInfo(context) }
    val scrollState = rememberScrollState()
    val overscrollOffset = remember { Animatable(0f) }

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
                    if (prevValue >= 350f) return Offset.Zero

                    val newValue = (prevValue + available.y * 0.5f).coerceAtMost(350f)
                    scope.launch { overscrollOffset.snapTo(newValue) }

                    if (prevValue < 300f && newValue >= 300f) {
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

    val statusBarHeightPx = with(LocalDensity.current) {
        WindowInsets.statusBars.asPaddingValues().calculateTopPadding().toPx()
    }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.surfaceContainer)
            .progressiveBlur(
                blurRadius = 40f,
                height = statusBarHeightPx * 1.5f,
                direction = BlurDirection.TOP,
            ),
    ) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .progressiveBlur(
                    blurRadius = 40f,
                    height = with(LocalDensity.current) { 120.dp.toPx() },
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
                    top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding() + 16.dp,
                    bottom = 120.dp,
                    start = 16.dp,
                    end = 16.dp,
                ),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
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
                Spacer(modifier = Modifier.width(16.dp))
                Text(
                    text = "Device Info",
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }

            WobbleDeviceHeroCard(
                deviceInfo = deviceInfo,
                contentAlpha = { contentAlphaState },
                contentOffset = { contentOffsetState },
                overscrollOffset = overscrollOffset.value,
            )

            Text(
                text = "Hardware Details",
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.primary,
                modifier = Modifier.padding(top = 8.dp),
            )

            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Card(
                    modifier = Modifier.weight(1f),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                    shape = RoundedCornerShape(24.dp),
                ) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Icon(
                            imageVector = Icons.Rounded.Storage,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = WobbleDeviceUtils.formatHardwareSize(deviceInfo.totalStorage),
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            text = "Total Storage",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }

                Card(
                    modifier = Modifier.weight(1f),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                    shape = RoundedCornerShape(24.dp),
                ) {
                    Column(modifier = Modifier.padding(16.dp)) {
                        Icon(
                            imageVector = Icons.Rounded.Memory,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.primary,
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = WobbleDeviceUtils.formatHardwareSize(deviceInfo.totalRam),
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                        )
                        Text(
                            text = "Total RAM",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }

            Text(
                text = "System Information",
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
                    SpecItem(label = "Android Version", value = "Android ${deviceInfo.androidVersion} (API ${deviceInfo.sdkInt})")
                    SpecItem(label = "Security Patch", value = deviceInfo.securityPatch)
                    SpecItem(label = "Manufacturer", value = deviceInfo.manufacturer.replaceFirstChar { it.uppercase() })
                    SpecItem(label = "Model / Hardware", value = "${deviceInfo.model} (${deviceInfo.hardware})")
                    SpecItem(label = "Display Build", value = deviceInfo.display)
                }
            }
        }
    }
}

@Composable
private fun WobbleDeviceHeroCard(
    deviceInfo: WobbleDeviceInfo,
    contentAlpha: () -> Float,
    contentOffset: () -> Dp,
    overscrollOffset: Float,
) {
    val androidLogoRes = WobbleDeviceImageMapper.getAndroidLogo(deviceInfo)
    val density = LocalDensity.current
    val extraSize = with(density) { overscrollOffset.toDp() }

    val fontFamily = remember {
        try {
            FontFamily(
                Font(
                    R.font.google_sans_flex,
                    variationSettings = FontVariation.Settings(
                        FontVariation.width(150f),
                        FontVariation.weight(FontWeight.Normal.weight),
                        FontVariation.Setting("ROND", 100f),
                    ),
                )
            )
        } catch (_: Exception) {
            FontFamily.Default
        }
    }

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 12.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        if (androidLogoRes != 0) {
            Icon(
                painter = painterResource(id = androidLogoRes),
                contentDescription = null,
                tint = Color.Unspecified,
                modifier = Modifier
                    .graphicsLayer {
                        alpha = contentAlpha()
                        translationY = contentOffset().toPx()
                        scaleX = 1f + (overscrollOffset / 1000f)
                        scaleY = 1f + (overscrollOffset / 1000f)
                    }
                    .size(180.dp + extraSize)
                    .padding(bottom = 24.dp),
            )
        }

        Text(
            text = deviceInfo.deviceName,
            modifier = Modifier.graphicsLayer {
                alpha = contentAlpha()
                translationY = contentOffset().toPx()
            },
            style = MaterialTheme.typography.headlineMedium.copy(fontFamily = fontFamily),
            color = MaterialTheme.colorScheme.onSurface,
            fontWeight = FontWeight.Bold,
        )

        Spacer(modifier = Modifier.height(4.dp))

        Text(
            text = "${deviceInfo.manufacturer.replaceFirstChar { it.uppercase() }} ${deviceInfo.model} (${deviceInfo.hardware})",
            modifier = Modifier.graphicsLayer {
                alpha = contentAlpha()
                translationY = contentOffset().toPx()
            },
            style = MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun SpecItem(label: String, value: String) {
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
