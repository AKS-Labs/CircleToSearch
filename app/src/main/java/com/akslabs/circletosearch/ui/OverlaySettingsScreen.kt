/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui

import android.content.Context
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.akslabs.circletosearch.data.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OverlaySettingsScreen(
    onBack: () -> Unit
) {
    // Handle system back press
    androidx.activity.compose.BackHandler(onBack = onBack)
    
    val context = LocalContext.current
    val configManager = remember { OverlayConfigurationManager(context) }
    var config by remember { mutableStateOf(configManager.getConfig()) }
    
    // Save on changes - Updates in REALTIME
    fun updateConfig(newConfig: OverlayConfig) {
        config = newConfig
        configManager.saveConfig(newConfig)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Overlay Settings", fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.Default.ArrowBack, contentDescription = "Back")
                    }
                },
                actions = {
                    IconButton(onClick = { 
                        updateConfig(OverlayConfig()) // Reset
                    }) {
                        Icon(Icons.Default.Refresh, contentDescription = "Reset")
                    }
                }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(16.dp)
        ) {
            // 1. Main Toggles
            SettingsSectionHeader(title = "General")
            SettingsToggleItem(
                title = "Enable Overlay",
                subtitle = "Show trigger zone over status bar",
                icon = Icons.Default.Layers,
                checked = config.isEnabled,
                onCheckedChange = { updateConfig(config.copy(isEnabled = it)) }
            )
            Spacer(modifier = Modifier.height(12.dp))
            SettingsToggleItem(
                title = "Landscape Mode",
                subtitle = "Keep overlay active in landscape",
                icon = Icons.Default.ScreenRotation,
                checked = config.isEnabledInLandscape,
                onCheckedChange = { updateConfig(config.copy(isEnabledInLandscape = it)) }
            )
            Spacer(modifier = Modifier.height(12.dp))
            SettingsToggleItem(
                title = "Debug Visibility",
                subtitle = "Show color to adjust position",
                icon = Icons.Default.Visibility,
                checked = config.isVisible,
                onCheckedChange = { updateConfig(config.copy(isVisible = it)) }
            )

            Spacer(modifier = Modifier.height(24.dp))

            // 2. Overlays List
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                SettingsSectionHeader(title = "Overlays")
                TextButton(onClick = {
                    // Add new overlay segment
                    val currentSegments = config.segments.toMutableList()
                    currentSegments.add(OverlaySegment(xOffset = 300)) // Add with some offset so it doesn't overlap perfectly if 0
                    updateConfig(config.copy(segments = currentSegments))
                }) {
                    Icon(Icons.Default.Add, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(4.dp))
                    Text("Add Overlay")
                }
            }
            
            if (config.segments.isEmpty()) {
                Text(
                    "No overlays added. Click Add Overlay to start.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }

            config.segments.forEachIndexed { index, segment ->
                SegmentEditorItem(
                    index = index,
                    segment = segment,
                    onUpdate = { updatedSegment ->
                        val newSegments = config.segments.toMutableList()
                        newSegments[index] = updatedSegment
                        updateConfig(config.copy(segments = newSegments))
                    },
                    onDelete = {
                        val newSegments = config.segments.toMutableList()
                        newSegments.removeAt(index)
                        updateConfig(config.copy(segments = newSegments))
                    }
                )
                Spacer(modifier = Modifier.height(16.dp))
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SegmentEditorItem(
    index: Int,
    segment: OverlaySegment,
    onUpdate: (OverlaySegment) -> Unit,
    onDelete: () -> Unit
) {
    var isExpanded by remember { mutableStateOf(false) }
    var showGestureDialog by remember { mutableStateOf(false) }
    
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
                modifier = Modifier.clickable { isExpanded = !isExpanded }
            ) {
                Icon(
                    imageVector = Icons.Default.Layers,
                    contentDescription = null,
                    tint = listOf(Color.Red, Color.Blue, Color.Green, Color.Yellow, Color.Magenta)[index % 5],
                    modifier = Modifier.size(24.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text("Overlay ${index + 1}", fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.weight(1f))
                
                IconButton(onClick = onDelete) {
                     Icon(Icons.Default.Delete, contentDescription = "Delete", tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp))
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
                
                Text("Horizontal Position (X): ${segment.xOffset}px", style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.xOffset.toFloat().coerceIn(0f, maxWidth),
                    onValueChange = { onUpdate(segment.copy(xOffset = it.toInt())) },
                    valueRange = 0f..maxWidth
                )
                
                Text("Vertical Position (Y): ${segment.yOffset}px", style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.yOffset.toFloat().coerceIn(0f, maxHeight),
                    onValueChange = { onUpdate(segment.copy(yOffset = it.toInt())) },
                    valueRange = 0f..maxHeight
                )
                
                Text("Width: ${segment.width}px", style = MaterialTheme.typography.labelMedium)
                Slider(
                    value = segment.width.toFloat().coerceIn(10f, maxWidth),
                    onValueChange = { onUpdate(segment.copy(width = it.toInt())) },
                    valueRange = 10f..maxWidth
                )
                
                Text("Height: ${segment.height}px", style = MaterialTheme.typography.labelMedium)
                val hRange = 10f..400f
                val hSteps = ((hRange.endInclusive - hRange.start) / 10).toInt() - 1
                Slider(
                    value = segment.height.toFloat().coerceIn(hRange),
                    onValueChange = { onUpdate(segment.copy(height = it.toInt())) },
                    valueRange = hRange,
                    steps = if (hSteps > 0) hSteps else 0
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
                         Text("Gestures", fontWeight = FontWeight.Bold)
                         Text("Configure taps, long press & swipes", style = MaterialTheme.typography.bodySmall)
                    }
                    Spacer(modifier = Modifier.weight(1f))
                    Icon(Icons.Default.ArrowForwardIos, contentDescription = null, modifier = Modifier.size(16.dp))
                }
                
                Spacer(modifier = Modifier.height(8.dp))
                
                // Gesture Summary (first 2 non-none)
                Column {
                    segment.gestures.entries.filter { it.value != ActionType.NONE }.take(3).forEach {
                        Text(
                            "${it.key.getFriendlyName()}: ${it.value.getFriendlyName()}", 
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
                Text("Configure Gestures", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
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
                            gesture.getFriendlyName(), 
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
                                
                                var label = currentAction.getFriendlyName()
                                if (currentAction == ActionType.OPEN_APP) {
                                    val pkg = segment.gestureData[gesture]
                                    val appName = runCatching { 
                                        val info = context.packageManager.getApplicationInfo(pkg ?: "", 0)
                                        context.packageManager.getApplicationLabel(info).toString()
                                    }.getOrDefault(pkg ?: "Unknown App")
                                    label = "Open: $appName"
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
                    Text("Close")
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
                Text("Select Action", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
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
                                    action.getFriendlyName(),
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
                    Text("Cancel")
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
                Text("Select App", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(16.dp))
                
                // Search Bar
                OutlinedTextField(
                    value = searchQuery,
                    onValueChange = { searchQuery = it },
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text("Search apps") },
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
