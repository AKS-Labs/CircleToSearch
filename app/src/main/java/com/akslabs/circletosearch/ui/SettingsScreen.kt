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

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.utils.UIPreferences

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(
    uiPreferences: UIPreferences,
    onDismissRequest: () -> Unit
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    
    var showFriendlyMessages by remember { mutableStateOf(uiPreferences.isShowFriendlyMessages()) }
    var useGoogleLensOnly by remember { mutableStateOf(uiPreferences.isUseGoogleLensOnly()) }
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

    LaunchedEffect(useGoogleLensOnly) {
        uiPreferences.setUseGoogleLensOnly(useGoogleLensOnly)
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
                .padding(horizontal = 24.dp)
                .padding(bottom = 32.dp)
        ) {
            Text(
                text = "Settings",
                style = MaterialTheme.typography.headlineMedium.copy(
                    fontWeight = FontWeight.ExtraBold,
                    letterSpacing = (-0.5).sp
                ),
                color = MaterialTheme.colorScheme.onSurface
            )
            
            Spacer(modifier = Modifier.height(24.dp))

            // General Section
            SettingsSectionHeader(title = "General")
            
            SettingsToggleItem(
                title = "Friendly Messages",
                subtitle = "Show random greeting messages on trigger",
                icon = Icons.Default.ChatBubbleOutline,
                checked = showFriendlyMessages,
                onCheckedChange = { showFriendlyMessages = it }
            )

            Spacer(modifier = Modifier.height(16.dp))

            // Search Method Section
            SearchMethodSelector(
                isLensOnly = useGoogleLensOnly,
                onMethodChange = { useGoogleLensOnly = it }
            )

            Spacer(modifier = Modifier.height(12.dp))
            
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
                        text = "Lens mode needs the Google App to work its magic! 🪄 Degoogled? Multi-Search mode has your back! 🚀",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSecondaryContainer,
                        lineHeight = 16.sp,
                        modifier = Modifier.weight(1f)
                    )
                }
            }

            Spacer(modifier = Modifier.height(24.dp))

            // Search Engines Section
            SettingsSectionHeader(title = "Search Engines")
            Text(
                text = "Tap arrows to change tab sequence",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(bottom = 8.dp)
            )

            Surface(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(24.dp)),
                color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.3f)
            ) {
                Column {
                    engineOrder.forEachIndexed { index, engine ->
                        EngineOrderItem(
                            engine = engine,
                            isFirst = index == 0,
                            isLast = index == engineOrder.size - 1,
                            onMoveUp = {
                                if (index > 0) {
                                    val temp = engineOrder[index]
                                    engineOrder[index] = engineOrder[index - 1]
                                    engineOrder[index - 1] = temp
                                }
                            },
                            onMoveDown = {
                                if (index < engineOrder.size - 1) {
                                    val temp = engineOrder[index]
                                    engineOrder[index] = engineOrder[index + 1]
                                    engineOrder[index + 1] = temp
                                }
                            }
                        )
                        if (index < engineOrder.size - 1) {
                            HorizontalDivider(
                                modifier = Modifier.padding(horizontal = 16.dp),
                                thickness = 0.5.dp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.1f)
                            )
                        }
                    }
                }
            }
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
            .clip(RoundedCornerShape(20.dp))
            .clickable { onCheckedChange(!checked) }
            .padding(vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Surface(
            modifier = Modifier.size(48.dp),
            shape = RoundedCornerShape(14.dp),
            color = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.4f)
        ) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                modifier = Modifier
                    .padding(12.dp)
                    .size(24.dp),
                tint = MaterialTheme.colorScheme.onPrimaryContainer
            )
        }
        
        Spacer(modifier = Modifier.width(16.dp))
        
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold)
            )
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant
            )
        }
        
        Switch(
            checked = checked,
            onCheckedChange = onCheckedChange,
            colors = SwitchDefaults.colors(
                checkedThumbColor = MaterialTheme.colorScheme.primary,
                checkedTrackColor = MaterialTheme.colorScheme.primaryContainer
            )
        )
    }
}

@Composable
fun EngineOrderItem(
    engine: SearchEngine,
    isFirst: Boolean,
    isLast: Boolean,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(16.dp),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Text(
            text = engine.displayName,
            style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
            modifier = Modifier.weight(1f)
        )
        
        IconButton(
            onClick = onMoveUp,
            enabled = !isFirst
        ) {
            Icon(
                Icons.Default.KeyboardArrowUp,
                contentDescription = "Move Up",
                tint = if (!isFirst) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f)
            )
        }
        
        IconButton(
            onClick = onMoveDown,
            enabled = !isLast
        ) {
            Icon(
                Icons.Default.KeyboardArrowDown,
                contentDescription = "Move Down",
                tint = if (!isLast) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f)
            )
        }
    }
}
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchMethodSelector(
    isLensOnly: Boolean,
    onMethodChange: (Boolean) -> Unit
) {
    Column {
        Text(
            text = "SEARCH METHOD",
            style = MaterialTheme.typography.labelLarge.copy(
                fontWeight = FontWeight.Bold,
                letterSpacing = 1.sp
            ),
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(bottom = 12.dp)
        )

        SingleChoiceSegmentedButtonRow(
            modifier = Modifier.fillMaxWidth()
        ) {
            SegmentedButton(
                shape = SegmentedButtonDefaults.itemShape(index = 0, count = 2),
                onClick = { onMethodChange(false) },
                selected = !isLensOnly,
                icon = { SegmentedButtonDefaults.Icon(!isLensOnly) },
                colors = SegmentedButtonDefaults.colors(
                    activeContainerColor = MaterialTheme.colorScheme.primaryContainer,
                    activeContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.ManageSearch, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(8.dp))
                    Text("Multi-Search", style = MaterialTheme.typography.labelLarge)
                }
            }
            SegmentedButton(
                shape = SegmentedButtonDefaults.itemShape(index = 1, count = 2),
                onClick = { onMethodChange(true) },
                selected = isLensOnly,
                icon = { SegmentedButtonDefaults.Icon(isLensOnly) },
                colors = SegmentedButtonDefaults.colors(
                    activeContainerColor = MaterialTheme.colorScheme.primaryContainer,
                    activeContentColor = MaterialTheme.colorScheme.onPrimaryContainer
                )
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Default.AutoFixHigh, contentDescription = null, modifier = Modifier.size(18.dp))
                    Spacer(modifier = Modifier.width(8.dp))
                    Text("Google Lens", style = MaterialTheme.typography.labelLarge)
                }
            }
        }
    }
}
