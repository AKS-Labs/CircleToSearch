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
import androidx.compose.ui.unit.dp
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.ui.CircleToSearchScreen
import com.akslabs.circletosearch.ui.components.CopyTextOverlayManager
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class OverlayActivity : ComponentActivity() {
    
    private val screenshotBitmap = androidx.compose.runtime.mutableStateOf<android.graphics.Bitmap?>(null)
    private val copyTextManager = androidx.compose.runtime.mutableStateOf<CopyTextOverlayManager?>(null)
    private val searchModeOverride = androidx.compose.runtime.mutableStateOf<Boolean?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        window.setBackgroundDrawable(android.graphics.drawable.ColorDrawable(0))
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        android.util.Log.d("CircleToSearch", "OverlayActivity onCreate")
        
        // Ensure the activity can receive touches and focus properly
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL)
        window.addFlags(android.view.WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN)
        
        loadScreenshot()
        updateOverride(intent)

        // Initialize manager for Activity-based layout
        copyTextManager.value = CopyTextOverlayManager(
            context = this,
            screenshotBitmap = screenshotBitmap.value
        )
        CircleToSearchAccessibilityService.setCopyTextManager(copyTextManager.value)

        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent,
                    tonalElevation = 0.dp
                ) {
                    CircleToSearchScreen(
                        screenshot = screenshotBitmap.value,
                        searchModeOverride = searchModeOverride.value,
                        onClose = { 
                            BitmapRepository.clear()
                            com.akslabs.circletosearch.data.AssistDataRepository.clear()
                            finish() 
                        },
                        copyTextManager = copyTextManager.value,
                        onExitCopyMode = { 
                            // Copy Mode exited
                        }
                    )
                }
            }
        }
    }


    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        android.util.Log.d("CircleToSearch", "OverlayActivity onNewIntent - Resetting state")
        setIntent(intent)
        
        // IMMEDIATE NULLING to prevent flash of previous screen
        screenshotBitmap.value = null
        copyTextManager.value?.dismiss()
        copyTextManager.value = null
        searchModeOverride.value = null
        
        loadScreenshot()
        updateOverride(intent)
        
        // Recreate manager with new screenshot
        copyTextManager.value = CopyTextOverlayManager(
            context = this,
            screenshotBitmap = screenshotBitmap.value
        )
        CircleToSearchAccessibilityService.setCopyTextManager(copyTextManager.value)
    }

    private fun updateOverride(intent: android.content.Intent) {
        if (intent.hasExtra("EXTRA_SEARCH_MODE_OVERRIDE")) {
            searchModeOverride.value = intent.getBooleanExtra("EXTRA_SEARCH_MODE_OVERRIDE", false)
        } else {
            searchModeOverride.value = null
        }
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
        copyTextManager.value?.dismiss()
        copyTextManager.value = null
        if (isFinishing) {
             com.akslabs.circletosearch.data.BitmapRepository.clear()
             com.akslabs.circletosearch.data.AssistDataRepository.clear()
             com.akslabs.circletosearch.utils.StorageUtils.clearAppCache(this)
        }
    }
}
