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

import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.DialogProperties
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import com.akslabs.circletosearch.utils.SearchTrigger

/**
 * Transparent activity that triggers Circle-to-Search capture.
 * Used by App Shortcuts, gesture apps, Tasker, and the Quick Settings tile.
 */
class SearchTriggerActivity : ComponentActivity() {

    private val handler = Handler(Looper.getMainLooper())
    private var showSetupDialog by mutableStateOf(false)
    private var setupMessage by mutableStateOf("")

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Disable window animations for this activity
        disableTransitions()
        scheduleTrigger(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        // singleInstance reuse: clear any prior setup dialog and retry capture
        handler.removeCallbacksAndMessages(null)
        showSetupDialog = false
        scheduleTrigger(intent)
    }

    private fun scheduleTrigger(triggerIntent: Intent) {
        val modeOverride = SearchTrigger.parseSearchModeOverride(triggerIntent)
        val collapseShade = SearchTrigger.shouldCollapseShade(triggerIntent)
        val delayMs = if (collapseShade) SHADE_COLLAPSE_DELAY_MS else 0L

        // Wait when collapse_shade is set so the QS panel collapses cleanly across OEM skins (OneUI, MIUI/HyperOS, ColorOS)
        handler.postDelayed({
            attemptTrigger(modeOverride, retryCount = 0)
        }, delayMs)
    }

    override fun onDestroy() {
        handler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    private fun attemptTrigger(modeOverride: Boolean?, retryCount: Int) {
        when (val result = SearchTrigger.triggerSearch(this, modeOverride)) {
            SearchTrigger.Result.Triggered -> {
                finishQuietly()
            }
            SearchTrigger.Result.ServiceNotConnected -> {
                // Service may still be binding after the user just enabled it
                if (SearchTrigger.isAccessibilityEnabled(this) && retryCount < MAX_CONNECT_RETRIES) {
                    handler.postDelayed({
                        attemptTrigger(modeOverride, retryCount + 1)
                    }, CONNECT_RETRY_DELAY_MS)
                } else if (!SearchTrigger.isAccessibilityEnabled(this)) {
                    showAccessibilitySetup(
                        "Accessibility is required to capture the screen for Circle to Search."
                    )
                } else {
                    showAccessibilitySetup(
                        "Circle to Search accessibility service is not running yet. Enable it in Settings, then try again."
                    )
                }
            }
            SearchTrigger.Result.AccessibilityDisabled -> {
                showAccessibilitySetup(
                    "Accessibility is required to capture the screen for Circle to Search. This is a one-time setup."
                )
            }
        }
    }

    private fun showAccessibilitySetup(message: String) {
        setupMessage = message
        showSetupDialog = true
        setContent {
            CircleToSearchTheme {
                if (showSetupDialog) {
                    AlertDialog(
                        onDismissRequest = { finishQuietly() },
                        properties = DialogProperties(
                            dismissOnBackPress = true,
                            dismissOnClickOutside = true
                        ),
                        shape = RoundedCornerShape(24.dp),
                        title = {
                            Text(
                                text = "Enable Accessibility",
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.Bold
                            )
                        },
                        text = {
                            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text(
                                    text = setupMessage,
                                    style = MaterialTheme.typography.bodyMedium,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                                Spacer(modifier = Modifier.height(4.dp))
                                Text(
                                    text = "Gesture apps, shortcuts, and the Quick Settings tile all use this path. Default assistant is optional.",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        },
                        confirmButton = {
                            Button(
                                onClick = {
                                    startActivity(
                                        Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
                                            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                                        }
                                    )
                                    finishQuietly()
                                },
                                modifier = Modifier.fillMaxWidth()
                            ) {
                                Text("Open Accessibility Settings")
                            }
                        },
                        dismissButton = {
                            TextButton(
                                onClick = { finishQuietly() },
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(bottom = 4.dp)
                            ) {
                                Text("Cancel")
                            }
                        }
                    )
                }
            }
        }
    }

    private fun finishQuietly() {
        finish()
        disableTransitions()
    }

    private fun disableTransitions() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else {
            @Suppress("DEPRECATION")
            overridePendingTransition(0, 0)
        }
    }

    companion object {
        private const val SHADE_COLLAPSE_DELAY_MS = 500L
        private const val CONNECT_RETRY_DELAY_MS = 150L
        private const val MAX_CONNECT_RETRIES = 6
    }
}
