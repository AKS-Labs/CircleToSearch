/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui

import android.content.Context
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import com.akslabs.circletosearch.data.*

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OverlaySettingsScreen(
    onBack: () -> Unit
) {
    val context = LocalContext.current
    val configManager = remember { OverlayConfigurationManager(context) }
    var config by remember { mutableStateOf(configManager.getConfig()) }
    
    // Save on changes
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
            // 1. Visualizer
            Text("PREVIEW", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
            Spacer(modifier = Modifier.height(8.dp))
            OverlayVisualizer(config = config)
            Spacer(modifier = Modifier.height(24.dp))

            // 2. Main Toggles
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
                subtitle = "Show colored overlay for testing",
                icon = Icons.Default.Visibility,
                checked = config.isVisible,
                onCheckedChange = { updateConfig(config.copy(isVisible = it)) }
            )

            Spacer(modifier = Modifier.height(24.dp))

            // 3. Dimensions
            SettingsSectionHeader(title = "Dimensions")
            Text("Height: ${config.height}px", style = MaterialTheme.typography.bodySmall)
            Slider(
                value = config.height.toFloat(),
                onValueChange = { updateConfig(config.copy(height = it.toInt())) },
                valueRange = 20f..300f
            )
            
            Text("Vertical Offset: ${config.verticalOffset}px", style = MaterialTheme.typography.bodySmall)
            Slider(
                value = config.verticalOffset.toFloat(),
                onValueChange = { updateConfig(config.copy(verticalOffset = it.toInt())) },
                valueRange = 0f..200f
            )

            Spacer(modifier = Modifier.height(24.dp))

            // 4. Segments
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically
            ) {
                SettingsSectionHeader(title = "Segments & Gestures")
                TextButton(onClick = {
                    // Add split logic: Split the last segment in half or add new one
                    val currentSegments = config.segments.toMutableList()
                     if (currentSegments.isNotEmpty()) {
                        // Redistribution logic: reset to equal parts for simplicity when adding
                        val newCount = currentSegments.size + 1
                        val newWidth = 1f / newCount
                        val newSegments = mutableListOf<OverlaySegment>()
                        for (i in 0 until newCount) {
                             newSegments.add(OverlaySegment(startFraction = i * newWidth, widthFraction = newWidth))
                        }
                        updateConfig(config.copy(segments = newSegments))
                    } else {
                        updateConfig(config.copy(segments = listOf(OverlaySegment(0f, 1f))))
                    }
                }) {
                    Text("+ Add Split")
                }
            }
            
            // Re-distribute/Clear
            if (config.segments.size > 1) {
                TextButton(
                    onClick = { updateConfig(config.copy(segments = listOf(OverlaySegment(0f, 1f)))) },
                    modifier = Modifier.align(Alignment.End)
                ) {
                    Text("Reset to Single", color = MaterialTheme.colorScheme.error)
                }
            }

            config.segments.forEachIndexed { index, segment ->
                SegmentEditorItem(
                    index = index,
                    segment = segment,
                    onUpdate = { updatedSegment ->
                        val newSegments = config.segments.toMutableList()
                        newSegments[index] = updatedSegment
                        updateConfig(config.copy(segments = newSegments))
                    }
                )
                Spacer(modifier = Modifier.height(8.dp))
            }
        }
    }
}

@Composable
fun OverlayVisualizer(config: OverlayConfig) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .height(60.dp) // Scaled down verification
            .clip(RoundedCornerShape(8.dp))
            .background(Color.Black)
            .border(1.dp, Color.DarkGray, RoundedCornerShape(8.dp))
    ) {
        // Draw Status Bar hints
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 12.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text("9:41", color = Color.White, fontSize = 10.sp)
            Row {
                Icon(Icons.Default.Wifi, contentDescription = null, tint = Color.White, modifier = Modifier.size(10.dp))
                Spacer(modifier = Modifier.width(4.dp))
                Icon(Icons.Default.BatteryFull, contentDescription = null, tint = Color.White, modifier = Modifier.size(10.dp))
            }
        }
        
        // Draw Overlay Segments
        Box(modifier = Modifier.fillMaxSize()) {
            config.segments.forEachIndexed { index, segment ->
                Box(
                    modifier = Modifier
                        .fillMaxHeight()
                        .fillMaxWidth(fraction = segment.widthFraction)
                        // Align based on start fraction. 
                        // Note: fillMaxWidth(fraction) takes fraction of PARENT. 
                        // To allow arbitrary positioning we might need offsets or a Row with weights.
                        // Or easier: Custom Layout or simple Offset.
                        // Since segments are usually sequential, Row is best visually. But we support config.
                        // Let's use alignment bias or Offset.
                ) 
            }
            
             // Better Visualizer: Canvas
             Canvas(modifier = Modifier.fillMaxSize()) {
                 val totalWidth = size.width
                 val height = if (config.height > size.height) size.height else config.height.toFloat() // Clamp for visual
                 val yOffset = 0f // In visualizer, always top
                 
                 config.segments.forEachIndexed { index, segment ->
                     val x = totalWidth * segment.startFraction
                     val w = totalWidth * segment.widthFraction
                     
                     // Colors
                     val colors = listOf(Color.Red, Color.Blue, Color.Green, Color.Yellow, Color.Magenta)
                     val color = colors[index % colors.size].copy(alpha = 0.5f)
                     
                     drawRect(
                         color = color,
                         topLeft = Offset(x, yOffset),
                         size = androidx.compose.ui.geometry.Size(w, 20f) // Fixed visual height
                     )
                     
                     // Draw Borders
                 }
             }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SegmentEditorItem(
    index: Int,
    segment: OverlaySegment,
    onUpdate: (OverlaySegment) -> Unit
) {
    var showGestureDialog by remember { mutableStateOf(false) }

    Card(
        modifier = Modifier.fillMaxWidth().clickable { showGestureDialog = true },
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant.copy(alpha=0.5f))
    ) {
        Column(modifier = Modifier.padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    modifier = Modifier.size(16.dp).background(
                         listOf(Color.Red, Color.Blue, Color.Green, Color.Yellow, Color.Magenta)[index % 5],
                         shape = androidx.compose.foundation.shape.CircleShape
                    )
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text("Segment ${index + 1}", fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.weight(1f))
                Icon(Icons.Default.Edit, contentDescription = "Edit", modifier = Modifier.size(16.dp))
            }
            
            Spacer(modifier = Modifier.height(8.dp))
            
            // Readout of gestures
            segment.gestures.forEach { (type, action) ->
                Text(
                    "${type.name}: ${action.name}", 
                    style = MaterialTheme.typography.bodySmall, 
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
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
    // We only support configuring specific gestures.
    // Let's just list them.
    
    Dialog(onDismissRequest = onDismiss) {
        Card(
            shape = RoundedCornerShape(24.dp),
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface)
        ) {
            Column(
                modifier = Modifier.padding(24.dp).verticalScroll(rememberScrollState())
            ) {
                Text("Configure Gestures", style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                Spacer(modifier = Modifier.height(16.dp))
                
                GestureType.values().forEach { gesture ->
                    val currentAction = segment.gestures[gesture] ?: ActionType.NONE
                    var expanded by remember { mutableStateOf(false) }
                    
                    Text(gesture.name.replace("_", " "), fontWeight = FontWeight.Medium)
                    
                    Box(modifier = Modifier.fillMaxWidth()) {
                        OutlinedButton(
                            onClick = { expanded = true },
                            modifier = Modifier.fillMaxWidth()
                        ) {
                            Text(currentAction.name)
                            Spacer(modifier = Modifier.weight(1f))
                            Icon(Icons.Default.ArrowDropDown, contentDescription = null)
                        }
                        
                        DropdownMenu(
                            expanded = expanded,
                            onDismissRequest = { expanded = false }
                        ) {
                            ActionType.values().forEach { action ->
                                DropdownMenuItem(
                                    text = { Text(action.name) },
                                    onClick = {
                                        val newGestures = segment.gestures.toMutableMap()
                                        newGestures[gesture] = action
                                        onUpdate(segment.copy(gestures = newGestures))
                                        expanded = false
                                    }
                                )
                            }
                        }
                    }
                    Spacer(modifier = Modifier.height(12.dp))
                }
                
                Spacer(modifier = Modifier.height(16.dp))
                Button(
                    onClick = onDismiss,
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Text("Done")
                }
            }
        }
    }
}
