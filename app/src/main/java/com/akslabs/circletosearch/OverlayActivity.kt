package com.akslabs.circletosearch

import android.os.Build
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

    // LA CLÉ MAGIQUE ANTI-CACHE
    private val sessionKey = androidx.compose.runtime.mutableStateOf(System.currentTimeMillis())

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)

        loadScreenshot()

        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = Color.Transparent
                ) {
                    // Le bloc 'key' force l'interface à se réinitialiser à 100% à chaque nouvelle session
                    androidx.compose.runtime.key(sessionKey.value) {
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
    }


    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        loadScreenshot()
    }

    private fun loadScreenshot() {
        val bitmap = BitmapRepository.getScreenshot()
        if (bitmap != null) {
            screenshotBitmap.value = bitmap
        }
    }


    override fun finish() {
        super.finish()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else {
            @Suppress("DEPRECATION")
            overridePendingTransition(0, 0)
        }
    }
    override fun onResume() {
        super.onResume()

    }
    override fun onDestroy() {
        super.onDestroy()
        if (isFinishing) {
            BitmapRepository.clear()
        }
    }
}