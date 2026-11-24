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

package com.akslabs.circletosearch.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import kotlin.math.roundToInt
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.zIndex
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.utils.UIPreferences
import com.akslabs.circletosearch.ui.components.UnifiedSearchMethodSelector

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    uiPreferences: UIPreferences,
    onDismissRequest: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    val context = LocalContext.current
    
    var showFriendlyMessages by remember { mutableStateOf(uiPreferences.isShowFriendlyMessages()) }
    val initialOrderString = uiPreferences.getSearchEngineOrder()
    val allEngines = SearchEngine.values()
    
    // Parse order from preferences or use default
    val engineOrder = remember {
        val preferredNames = initialOrderString?.split(",") ?: emptyList()
        val ordered = mutableListOf<SearchEngine>()
        
        // Add existing engines in preferred order
        preferredNames.forEach { name ->
            allEngines.find { it.name == name }?.let { ordered.add(it) }
        }
        
        // Add any remaining engines
        allEngines.forEach { engine ->
            if (!ordered.contains(engine)) ordered.add(engine)
        }
        mutableStateListOf(*ordered.toTypedArray())
    }

    LaunchedEffect(showFriendlyMessages) {
        uiPreferences.setShowFriendlyMessages(showFriendlyMessages)
    }

    LaunchedEffect(engineOrder.toList()) {
        uiPreferences.setSearchEngineOrder(engineOrder.joinToString(",") { it.name })
    }

    ModalBottomSheet(
        onDismissRequest = onDismissRequest,
        sheetState = sheetState,
        containerColor = MaterialTheme.colorScheme.surface,
        dragHandle = { BottomSheetDefaults.DragHandle() }
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp)
                .padding(bottom = 32.dp)
        ) {
            Text(
                text = stringResource(R.string.settings),
                style = MaterialTheme.typography.headlineMedium.copy(
                    fontWeight = FontWeight.ExtraBold,
                    letterSpacing = (-0.5).sp
                ),
                color = MaterialTheme.colorScheme.onSurface
            )
            
            Spacer(modifier = Modifier.height(24.dp))

            // General Section
            SettingsSectionHeader(title = stringResource(R.string.general))
            
            SettingsToggleItem(
                title = stringResource(R.string.setting_friendly_messages_title),
                subtitle = stringResource(R.string.setting_friendly_messages_subtitle),
                icon = Icons.Default.ChatBubbleOutline,
                checked = showFriendlyMessages,
                onCheckedChange = { showFriendlyMessages = it }
            )

            Spacer(modifier = Modifier.height(16.dp))

            UnifiedSearchMethodSelector(uiPreferences = uiPreferences)

            Spacer(modifier = Modifier.height(24.dp))
            
            // Explanatory Note
            Surface(
                color = MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.3f),
                shape = RoundedCornerShape(16.dp),
                modifier = Modifier.fillMaxWidth()
            ) {
                Row(
                    modifier = Modifier.padding(12.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Icon(
                        imageVector = Icons.Default.Info,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.secondary,
                        modifier = Modifier.size(20.dp)
                    )
                    Spacer(modifier = Modifier.width(12.dp))
                    Text(
                        text = stringResource(R.string.info_lens_mode_note),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSecondaryContainer,
                        lineHeight = 16.sp,
                        modifier = Modifier.weight(1f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(24.dp))

            // Search Engines Section
            SettingsSectionHeader(title = stringResource(R.string.section_search_engines))
            Text(
                text = stringResource(R.string.hint_tab_sequence),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = 8.dp)
            )

            EngineOrderChipsRow(
                engineOrder,
                { it.getDisplayName(context) },
                { it.iconRes },
            )
        }
    }
}

@Composable
fun SettingsSectionHeader(title: String) {
    Text(
        text = title.uppercase(),
        style = MaterialTheme.typography.labelLarge.copy(
            fontWeight = FontWeight.Bold,
            letterSpacing = 1.sp
        ),
        color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.padding(bottom = 12.dp)
    )
}

@Composable
fun SettingsToggleItem(
    title: String,
    subtitle: String,
    icon: ImageVector,
    checked: Boolean,
    onCheckedChange: (Boolean) -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onCheckedChange(!checked) }
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

        Column(modifier = Modifier.weight(1f)) {
            Text(text = title, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }

        Spacer(modifier = Modifier.width(8.dp))

        Switch(checked = checked, onCheckedChange = onCheckedChange)
    }
}

/** Compact engine-order editor: favicon chips laid out as a grid (4 per row; a single
 *  row when there are ≤4 engines), left→right, top→bottom = tab order. Long-press a
 *  chip and drag freely — the target cell follows the drag and the item is re-inserted
 *  there live. ALL chips live in ONE Box keyed by item and are placed by offset, so
 *  the gesture node never reparents and survives moves across rows (separate Row
 *  containers would dispose the dragged node and kill the gesture). Keep the grid
 *  outside any clipping container or the dragged chip is cut off at its edges. */
@Composable
fun <T> EngineOrderChipsRow(
    items: MutableList<T>,
    label: (T) -> String,
    iconRes: (T) -> Int,
) {
    val density = LocalDensity.current
    val gapDp = 8.dp
    val cellHeightDp = 68.dp
    val rowGapDp = 8.dp
    val columns = if (items.size <= 4) items.size.coerceAtLeast(1) else 4
    val rowCount = if (items.isEmpty()) 0 else (items.size + columns - 1) / columns
    var dragging by remember { mutableStateOf<Any?>(null) }
    var dragOffset by remember { mutableStateOf(Offset.Zero) }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        val slotWidthDp = if (items.isEmpty()) {
            0.dp
        } else {
            (maxWidth - gapDp * (columns - 1)) / columns
        }
        val slotPitchDp = slotWidthDp + gapDp
        val rowPitchDp = cellHeightDp + rowGapDp
        val gridHeightDp = if (rowCount == 0) 0.dp else rowPitchDp * rowCount - rowGapDp

        Box(modifier = Modifier.fillMaxWidth().height(gridHeightDp)) {
            items.forEach { item ->
                key(item) {
                    val isDragging = dragging == item
                    val i = items.indexOf(item)
                    val col = i % columns
                    val row = i / columns
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        modifier = Modifier
                            .offset {
                                IntOffset(
                                    (col * slotPitchDp.toPx()).roundToInt(),
                                    (row * rowPitchDp.toPx()).roundToInt(),
                                )
                            }
                            .width(slotWidthDp)
                            .height(cellHeightDp)
                            .pointerInput(item) {
                                detectDragGesturesAfterLongPress(
                                    onDragStart = {
                                        dragging = item
                                        dragOffset = Offset.Zero
                                    },
                                    onDrag = { change, amount ->
                                        change.consume()
                                        dragOffset += amount
                                        val from = items.indexOf(item)
                                        if (from < 0 || slotPitchDp <= 0.dp) return@detectDragGesturesAfterLongPress
                                        val fromCol = from % columns
                                        val fromRow = from / columns
                                        val slotPitchPx = slotPitchDp.toPx()
                                        val rowPitchPx = rowPitchDp.toPx()
                                        val maxRow = (items.size - 1) / columns
                                        val targetCol = (fromCol + (dragOffset.x / slotPitchPx).roundToInt())
                                            .coerceIn(0, columns - 1)
                                        val targetRow = (fromRow + (dragOffset.y / rowPitchPx).roundToInt())
                                            .coerceIn(0, maxRow)
                                        var target = targetRow * columns + targetCol
                                        if (target >= items.size) target = items.size - 1
                                        if (target != from) {
                                            val moved = items.removeAt(from)
                                            items.add(target, moved)
                                            // Re-anchor the drag so the chip stays under the
                                            // finger: its cell moved, the remainder of the drag
                                            // is measured from the new cell.
                                            val newI = items.indexOf(item)
                                            dragOffset = Offset(
                                                dragOffset.x - ((newI % columns) - fromCol) * slotPitchPx,
                                                dragOffset.y - ((newI / columns) - fromRow) * rowPitchPx,
                                            )
                                        }
                                    },
                                    onDragEnd = {
                                        dragging = null
                                        dragOffset = Offset.Zero
                                    },
                                    onDragCancel = {
                                        dragging = null
                                        dragOffset = Offset.Zero
                                    },
                                )
                            }
                            .graphicsLayer {
                                translationX = if (isDragging) dragOffset.x else 0f
                                translationY = if (isDragging) dragOffset.y else 0f
                                val scale = if (isDragging) 1.15f else 1f
                                scaleX = scale
                                scaleY = scale
                            }
                            .zIndex(if (isDragging) 1f else 0f),
                    ) {
                        // Bare favicon + name — no clip, no background, no shadow frames.
                        // Drag feedback = the chip lifts, scales up and its name turns primary.
                        Image(
                            painter = painterResource(iconRes(item)),
                            contentDescription = null,
                            modifier = Modifier.size(30.dp),
                        )
                        Spacer(modifier = Modifier.height(4.dp))
                        Text(
                            text = label(item),
                            style = MaterialTheme.typography.labelSmall,
                            color = if (isDragging) {
                                MaterialTheme.colorScheme.primary
                            } else {
                                MaterialTheme.colorScheme.onSurface
                            },
                            textAlign = TextAlign.Center,
                        )
                    }
                }
            }
        }
    }
}
