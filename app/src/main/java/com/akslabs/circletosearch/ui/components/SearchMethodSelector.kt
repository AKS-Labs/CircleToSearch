/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui.components

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AutoFixHigh
import androidx.compose.material.icons.filled.ManageSearch
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.utils.UIPreferences

/**
 * A unified search method selector component that allows users to switch
 * between Multi-Search and Google Lens modes.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchMethodSelector(
    isLensOnly: Boolean,
    onMethodChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(modifier = modifier) {
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
                    Icon(
                        imageVector = Icons.Default.ManageSearch,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp)
                    )
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
                    Icon(
                        imageVector = Icons.Default.AutoFixHigh,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp)
                    )
                    Spacer(modifier = Modifier.width(8.dp))
                    Text("Google Lens", style = MaterialTheme.typography.labelLarge)
                }
            }
        }
    }
}

/**
 * A reactive wrapper for SearchMethodSelector that observes UIPreferences.
 * Uses the flow as the single source of truth so that all instances
 * (Home screen and Settings sheet) are always synchronized.
 */
@Composable
fun UnifiedSearchMethodSelector(
    uiPreferences: UIPreferences,
    modifier: Modifier = Modifier
) {
    // The flow IS the state — one reactive source drives both Home and Settings.
    // Writing via setUseGoogleLensOnly() triggers the SharedPreferences listener,
    // which emits into this flow, which recomposes this composable instantly.
    val isLensOnly by uiPreferences.observeUseGoogleLensOnly().collectAsState(
        initial = uiPreferences.isUseGoogleLensOnly()
    )

    SearchMethodSelector(
        isLensOnly = isLensOnly,
        onMethodChange = { newValue ->
            // commit() ensures the write is synchronous so the accessibility
            // service and CircleToSearchScreen both read the updated value
            // before the search is triggered.
            uiPreferences.setUseGoogleLensOnly(newValue)
        },
        modifier = modifier
    )
}
