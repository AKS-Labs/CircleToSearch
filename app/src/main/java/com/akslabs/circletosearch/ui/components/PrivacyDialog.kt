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

package com.akslabs.circletosearch.ui.components

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Security
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties

@Composable
fun PrivacyDialog(
    onAccept: () -> Unit
) {
    AlertDialog(
        onDismissRequest = { /* Cannot dismiss without accepting */ },
        modifier = Modifier
            .fillMaxSize()
            .padding(20.dp),
        properties = DialogProperties(
            usePlatformDefaultWidth = false,
            dismissOnBackPress = false,
            dismissOnClickOutside = false
        ),
        icon = {
            Icon(
                imageVector = Icons.Filled.Security,
                contentDescription = "Privacy",
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(48.dp)
            )
        },
        title = {
            Text(
                text = "Privacy & Data Usage Transparency",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
                textAlign = TextAlign.Center
            )
        },
        text = {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .verticalScroll(rememberScrollState())
            ) {

                // Where It Goes Section
                PrivacySection(
                    icon = Icons.Filled.Info,
                    question = "Does this app upload my searched images anywhere other than search engines?",
                    answer = "Yes! ✅\nYour images are uploaded to LitterBox and Catbox third-party services to generate image url.\n\n• Litterbox → Auto-delete image after 1 hour\n• Catbox → Used only when Litterbox fails (stores image forever\n\nImportant: Litterbox is actually part of Catbox.\n" +
                            "They belong to the same service family and may use similar infrastructure.)"
                )

                Spacer(modifier = Modifier.height(16.dp))
                // Why Upload Section
                PrivacySection(
                    icon = Icons.Filled.Info,
                    question = "Why does this app upload my image at all?",
                    answer = "Search engines don't allow apps to directly upload images Programmatically (to prevent misuse & abuse). So the app uploads your image first to create a safe, usable link/url of image."
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                

                PrivacySection(
                    icon = Icons.Filled.Warning,
                    question = "Are Catbox and Litterbox safe?",
                    answer = "They’re widely trusted for temporary file hosting because:\n\n• No account & API is required\n• Anonymous uploads are supported\n• Simple, fast and reliable\n\nThey are not open-source, so please read their official data and privacy policies to fully understand how your data is handled.",
                    isWarning = true
                )
                Spacer(modifier = Modifier.height(16.dp))
                // Warning Section
                PrivacySection(
                    icon = Icons.Filled.Warning,
                    question = "Should I search personal or private images?",
                    answer = "Please don't! 🙏\n\nAvoid uploading personal photos, private documents, IDs, or anything sensitive. Only search images that are safe and non-personal.",
                    isWarning = true
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                
                // App Storage Section
                PrivacySection(
                    icon = Icons.Filled.CheckCircle,
                    question = "Does the app store my images or links?",
                    answer = "Nope! ✅\n\nYour images and URLs are not stored, tracked, or logged by this app.",
                    isPositive = true
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                
                // Developer Access Section
                PrivacySection(
                    icon = Icons.Filled.CheckCircle,
                    question = "Can the developer see my images or links?",
                    answer = "No. Never.\n\nThis App:\n• Has no server\n• Does not store images\n• Does not store image URLs\n• Does not track what you search\n• Does not collect data\n\nYour data stays between you → Catbox/Litterbox → Search engine.",
                    isPositive = true
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                
                // Important Reminder
                Card(
                    modifier = Modifier.fillMaxWidth(),
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.3f)
                    ),
                    shape = RoundedCornerShape(12.dp)
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp)
                    ) {
                        Text(
                            text = "🔔 Important Reminder",
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.Bold,
                            color = MaterialTheme.colorScheme.error
                        )
                        Spacer(modifier = Modifier.height(8.dp))
                        Text(
                            text = "Your searched images can remain on search engine servers, Catbox servers, caches, and backups. This is why you should never upload private or personal images.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface
                        )
                    }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = onAccept,
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp)
            ) {
                Text(
                    text = "I Understand & Accept",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(8.dp)
                )
            }
        }
    )
}

@Composable
private fun PrivacySection(
    icon: ImageVector,
    question: String,
    answer: String,
    isWarning: Boolean = false,
    isPositive: Boolean = false
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = when {
                isWarning -> MaterialTheme.colorScheme.errorContainer.copy(alpha = 0.2f)
                isPositive -> MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.2f)
                else -> MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)
            }
        ),
        shape = RoundedCornerShape(12.dp)
    ) {
        Column(
            modifier = Modifier.padding(12.dp)
        ) {
            Row(
                verticalAlignment = Alignment.Top,
                horizontalArrangement = Arrangement.Start
            ) {
                Icon(
                    imageVector = icon,
                    contentDescription = null,
                    tint = when {
                        isWarning -> MaterialTheme.colorScheme.error
                        isPositive -> MaterialTheme.colorScheme.primary
                        else -> MaterialTheme.colorScheme.onSurfaceVariant
                    },
                    modifier = Modifier.size(20.dp)
                )
                Spacer(modifier = Modifier.width(8.dp))
                Column {
                    Text(
                        text = question,
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurface
                    )
                    Spacer(modifier = Modifier.height(4.dp))
                    Text(
                        text = answer,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant
                    )
                }
            }
        }
    }
}
