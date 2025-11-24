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
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        val screenshotPath = intent.getStringExtra("SCREENSHOT_PATH")
        val bitmap = screenshotPath?.let { ImageUtils.loadBitmap(it) }

        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent // Important for the overlay effect
                ) {
                    CircleToSearchScreen(
                        screenshot = bitmap,
                        onClose = { finish() }
                    )
                }
            }
        }
    }
}
