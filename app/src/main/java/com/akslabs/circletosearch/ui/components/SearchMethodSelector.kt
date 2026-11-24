/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.ui.components

import androidx.compose.foundation.clickable
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.utils.UIPreferences

/**
 * A unified search method selector: the segmented Multi-Search / Google Lens
 * chooser, plus two separate modes below it — "Ask every time" (a confirm dock
 * appears before anything is uploaded) and "Off" (visual search disabled; the
 * crop stays on-device and nothing leaves the phone). The two modes are
 * mutually exclusive.
 */
@Composable
fun SearchMethodSelector(
    method: String,
    askEveryTime: Boolean,
    visualSearchOff: Boolean,
    onMethodChange: (String) -> Unit,
    onAskEveryTimeChange: (Boolean) -> Unit,
    onVisualSearchOffChange: (Boolean) -> Unit,
    modifier: Modifier = Modifier
) {
    Column(modifier = modifier) {
        Text(
            text = stringResource(R.string.section_search_method),
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
                onClick = { onMethodChange("multi") },
                selected = method == "multi",
                icon = { SegmentedButtonDefaults.Icon(method == "multi") },
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
                    Text(stringResource(R.string.search_method_multi), style = MaterialTheme.typography.labelLarge)
                }
            }
            SegmentedButton(
                shape = SegmentedButtonDefaults.itemShape(index = 1, count = 2),
                onClick = { onMethodChange("lens") },
                selected = method == "lens",
                icon = { SegmentedButtonDefaults.Icon(method == "lens") },
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
                    Text(stringResource(R.string.search_method_lens), style = MaterialTheme.typography.labelLarge)
                }
            }
        }

        Spacer(modifier = Modifier.height(16.dp))

        // Ask every time: confirm before uploading. Turning it on clears Off.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { onAskEveryTimeChange(true) }
                .padding(horizontal = 4.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.search_method_ask),
                    style = MaterialTheme.typography.bodyLarge.copy(
                        fontWeight = if (askEveryTime) FontWeight.Bold else FontWeight.Normal
                    ),
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = stringResource(R.string.search_method_ask_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            Switch(
                checked = askEveryTime,
                onCheckedChange = { onAskEveryTimeChange(it) }
            )
        }

        // Off: visual search disabled. Turning it on clears Ask every time.
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clickable { onVisualSearchOffChange(true) }
                .padding(horizontal = 4.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = stringResource(R.string.search_method_off),
                    style = MaterialTheme.typography.bodyLarge.copy(
                        fontWeight = if (visualSearchOff) FontWeight.Bold else FontWeight.Normal
                    ),
                    color = MaterialTheme.colorScheme.onSurface
                )
                Text(
                    text = stringResource(R.string.search_method_off_desc),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant
                )
            }
            Switch(
                checked = visualSearchOff,
                onCheckedChange = { onVisualSearchOffChange(it) }
            )
        }
    }
}

/**
 * A reactive wrapper for SearchMethodSelector that observes UIPreferences.
 * Uses the flows as the single source of truth so that all instances
 * (Home screen and Settings sheet) are always synchronized.
 */
@Composable
fun UnifiedSearchMethodSelector(
    uiPreferences: UIPreferences,
    modifier: Modifier = Modifier
) {
    // The flows ARE the state — one reactive source drives both Home and Settings.
    // Writing via the setters triggers the SharedPreferences listeners, which emit
    // into these flows, which recompose this composable instantly.
    val method by uiPreferences.observeSearchMethod().collectAsState(
        initial = uiPreferences.getSearchMethod()
    )
    val askEveryTime by uiPreferences.observeAskEveryTime().collectAsState(
        initial = uiPreferences.isAskEveryTime()
    )
    val visualSearchOff by uiPreferences.observeVisualSearchOff().collectAsState(
        initial = uiPreferences.isVisualSearchOff()
    )

    SearchMethodSelector(
        method = method,
        askEveryTime = askEveryTime,
        visualSearchOff = visualSearchOff,
        onMethodChange = { uiPreferences.setSearchMethod(it) },
        onAskEveryTimeChange = { uiPreferences.setAskEveryTime(it) },
        onVisualSearchOffChange = { uiPreferences.setVisualSearchOff(it) },
        modifier = modifier
    )
}
