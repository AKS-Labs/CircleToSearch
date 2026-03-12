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

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.ui.CircleToSearchScreen
import com.akslabs.circletosearch.ui.components.CopyTextOverlayManager
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class OverlayActivity : ComponentActivity() {
    
    private val screenshotBitmap = androidx.compose.runtime.mutableStateOf<android.graphics.Bitmap?>(null)
    private var copyTextManager: CopyTextOverlayManager? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        android.util.Log.d("CircleToSearch", "OverlayActivity onCreate")
        
        loadScreenshot()

        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent
                ) {
                    CircleToSearchScreen(
                        screenshot = screenshotBitmap.value,
                        onClose = { 
                            BitmapRepository.clear()
                            finish() 
                        },
                        onCopyText = { activateCopyText() }
                    )
                }
            }
        }
    }

    /**
     * Activates the Copy Text overlay.
     * Uses the accessibility service's root node as the source for text scanning.
     */
    private fun activateCopyText() {
        if (copyTextManager != null) return // Already active
        android.util.Log.d("CircleToSearch", "Activating Copy Text overlay")

        val wm = getSystemService(WINDOW_SERVICE) as WindowManager

        copyTextManager = CopyTextOverlayManager(
            context = this,
            windowManager = wm,
            getRoot = {
                // Delegate to accessibility service for the current window's node tree
                CircleToSearchAccessibilityService.getRootNode()
            }
        )

        copyTextManager?.show(onDismiss = {
            android.util.Log.d("CircleToSearch", "Copy Text overlay dismissed")
            copyTextManager = null
            // Forward scroll-rescan binding is automatic via accessibility event forwarding
        })
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        android.util.Log.d("CircleToSearch", "OverlayActivity onNewIntent")
        setIntent(intent)
        loadScreenshot()
    }

    private fun loadScreenshot() {
        val bitmap = BitmapRepository.getScreenshot()
        if (bitmap != null) {
            android.util.Log.d("CircleToSearch", "Bitmap loaded from Repository. Size: ${bitmap.width}x${bitmap.height}")
            screenshotBitmap.value = bitmap
        } else {
            android.util.Log.e("CircleToSearch", "No bitmap in Repository")
        }
    }
    
    override fun onDestroy() {
        super.onDestroy()
        copyTextManager?.dismiss()
        copyTextManager = null
        if (isFinishing) {
             BitmapRepository.clear()
        }
    }
}
