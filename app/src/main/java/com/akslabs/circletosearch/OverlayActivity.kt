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
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.ui.CircleToSearchScreen
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class OverlayActivity : ComponentActivity() {
    
    private val screenshotBitmap = androidx.compose.runtime.mutableStateOf<android.graphics.Bitmap?>(null)

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
                        }
                    )
                }
            }
        }
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
            // Fallback or finish? For now just log.
        }
    }
    
    override fun onDestroy() {
        super.onDestroy()
        if (isFinishing) {
             BitmapRepository.clear()
        }
    }
}
