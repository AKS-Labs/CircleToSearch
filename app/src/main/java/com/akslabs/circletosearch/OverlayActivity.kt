package com.akslabs.circletosearch

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
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
