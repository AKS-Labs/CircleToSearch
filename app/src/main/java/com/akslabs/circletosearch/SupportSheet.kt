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

package com.akslabs.circletosearch

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.SheetState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.toMutableStateList
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SupportSheet(
    sheetState: SheetState,
    onDismissRequest: () -> Unit
) {
    val scope = rememberCoroutineScope()
    val uriHandler = LocalUriHandler.current
    var showCryptoOptions by rememberSaveable {
        mutableStateOf(false)
    }
    val clipboard = LocalClipboardManager.current
    
    data class OptionItem(
        val text: String,
        val summary: String? = null,
        val onClick: (String) -> Unit
    )

    val mainOptions = remember {
        listOf(
            OptionItem(
                text = "PayPal",
                onClick = {
                    uriHandler.openUri("https://paypal.me/AKSLabsOfficial")
                }
            ),
            OptionItem(
                text = "Github Sponsor",
                onClick = {
                    uriHandler.openUri("https://github.com/sponsors/AKS-Labs")
                }
            ),
            OptionItem(
                text = "UPI",
                onClick = {
                    showCryptoOptions = true
                }
            )
        )
    }
    
    val cryptoOnClick: (String) -> Unit = {
        clipboard.setText(AnnotatedString(it))
    }
    
    val cryptoOptions = remember {
        mapOf(
            "UPI" to "AKSLabs@upi",

        ).map { (coin, address) ->
            OptionItem(
                text = coin,
                summary = address,
                onClick = cryptoOnClick
            )
        }
    }

    if (showCryptoOptions) {
        BackHandler {
            showCryptoOptions = false
        }
    }

    ModalBottomSheet(
        sheetState = sheetState,
        onDismissRequest = {
            if (showCryptoOptions) {
                showCryptoOptions = false
            } else {
                onDismissRequest()
            }
        },
        containerColor = MaterialTheme.colorScheme.surfaceContainerLowest,
        tonalElevation = 0.dp,
        dragHandle = { BottomSheetDefaults.DragHandle() },
        contentWindowInsets = { WindowInsets(0, 0, 0, 0) }
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 32.dp, vertical = 16.dp)
                .navigationBarsPadding()
        ) {
            Text(
                text = buildAnnotatedString {
                    withStyle(
                        style = SpanStyle(
                            color = MaterialTheme.colorScheme.onSurface,
                            fontStyle = MaterialTheme.typography.titleLarge.fontStyle,
                            fontSize = MaterialTheme.typography.titleLarge.fontSize,
                            letterSpacing = MaterialTheme.typography.titleLarge.letterSpacing
                        )
                    ) {
                        append("Support the project")
                    }
                    if (showCryptoOptions) {
                        append("\n")
                        withStyle(
                            style = SpanStyle(
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                fontStyle = MaterialTheme.typography.bodyMedium.fontStyle,
                                fontSize = MaterialTheme.typography.bodyMedium.fontSize,
                                letterSpacing = MaterialTheme.typography.bodyMedium.letterSpacing
                            )
                        ) {
                            append("Click to copy")
                        }
                    }
                },
                textAlign = TextAlign.Center,
                style = MaterialTheme.typography.titleLarge,
                color = MaterialTheme.colorScheme.onSurface,
                modifier = Modifier
                    .padding(bottom = 16.dp)
                    .fillMaxWidth()
            )
            
            val options = remember(showCryptoOptions) {
                (if (showCryptoOptions) cryptoOptions else mainOptions).toMutableStateList()
            }
//            OptionLayout(
//                modifier = Modifier.fillMaxWidth(),
//                optionList = options
//            )
            Column(modifier = Modifier.fillMaxWidth()) {
                options.forEach { option ->
                    ListItem(
                        headlineContent = { Text(option.text) },
                        supportingContent = option.summary?.let { { Text(it) } },
                        modifier = Modifier.clickable { option.onClick(option.summary ?: "") }
                    )
                    HorizontalDivider()
                }
            }
        }
    }
}