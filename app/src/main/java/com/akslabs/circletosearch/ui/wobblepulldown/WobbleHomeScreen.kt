package com.akslabs.circletosearch.ui.wobblepulldown

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.animateIntAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
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
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.KeyboardArrowDown
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.material3.pulltorefresh.rememberPullToRefreshState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.akslabs.circletosearch.R
import kotlinx.coroutines.delay

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

    var permissionGranted by remember {
        mutableStateOf(isA11yGranted(context) || isAssistantGranted(context))
    }
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) {
                permissionGranted = isA11yGranted(context) || isAssistantGranted(context)
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    LaunchedEffect(Unit) {
        while (true) {
            permissionGranted = isA11yGranted(context) || isAssistantGranted(context)
            delay(500)
        }
    }

    LaunchedEffect(wobbleEvent) {
        if (wobbleEvent > 0) {
            WobbleHaptics.performUIHaptic(view)
            rippleTrigger++
        }
    }

    val density = LocalDensity.current
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
                permissionGranted -> MaterialTheme.colorScheme.primaryContainer
                else -> MaterialTheme.colorScheme.surfaceContainerLow
            },
            animationSpec = tween(durationMillis = 300),
            label = "containerColor",
        )

        val contentColor by animateColorAsState(
            targetValue = when {
                thresholdPassed -> MaterialTheme.colorScheme.onPrimary
                permissionGranted -> MaterialTheme.colorScheme.onPrimaryContainer
                else -> MaterialTheme.colorScheme.primary
            },
            animationSpec = tween(durationMillis = 300),
            label = "contentColor",
        )

        val borderColor by animateColorAsState(
            targetValue = if (thresholdPassed || permissionGranted) {
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
                permissionGranted -> MaterialTheme.colorScheme.onPrimaryContainer
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
                                    if (permissionGranted) {
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

            val categories = listOf(
                "Display & Ambient",
                "Sound & Vibration",
                "Notifications & Lighting",
                "Battery & Power",
                "System & Security",
            )

            items(categories.size) { index ->
                val category = categories[index]
                Column(
                    modifier = Modifier.fillMaxWidth(),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text(
                        text = category,
                        style = MaterialTheme.typography.titleMedium,
                        color = MaterialTheme.colorScheme.primary,
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
                            repeat(3) { itemIdx ->
                                Row(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 4.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                ) {
                                    Column {
                                        Text(
                                            text = "Sample Setting ${index * 3 + itemIdx + 1}",
                                            style = MaterialTheme.typography.bodyLarge,
                                            fontWeight = FontWeight.Medium,
                                        )
                                        Text(
                                            text = "Configuration options for $category",
                                            style = MaterialTheme.typography.bodySmall,
                                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                                        )
                                    }
                                    Icon(
                                        imageVector = Icons.Rounded.ChevronRight,
                                        contentDescription = null,
                                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
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
