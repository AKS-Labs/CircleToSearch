package com.akslabs.circletosearch

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import com.akslabs.circletosearch.ui.CircleToSearchScreen
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import com.akslabs.circletosearch.utils.ImageUtils

class OverlayActivity : ComponentActivity() {
    
    private val screenshotBitmap = androidx.compose.runtime.mutableStateOf<android.graphics.Bitmap?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        android.util.Log.d("CircleToSearch", "OverlayActivity onCreate")
        
        handleIntent(intent)

        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent
                ) {
                    CircleToSearchScreen(
                        screenshot = screenshotBitmap.value,
                        onClose = { finish() }
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        android.util.Log.d("CircleToSearch", "OverlayActivity onNewIntent")
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: android.content.Intent) {
        val screenshotPath = intent.getStringExtra("SCREENSHOT_PATH")
        android.util.Log.d("CircleToSearch", "Loading screenshot from: $screenshotPath")
        if (screenshotPath != null) {
            val bitmap = ImageUtils.loadBitmap(screenshotPath)
            if (bitmap != null) {
                android.util.Log.d("CircleToSearch", "Bitmap loaded successfully. Size: ${bitmap.width}x${bitmap.height}")
                screenshotBitmap.value = bitmap
            } else {
                android.util.Log.e("CircleToSearch", "Failed to load bitmap from path")
            }
        } else {
            android.util.Log.e("CircleToSearch", "No screenshot path in intent")
        }
    }
}
