package com.akslabs.circletosearch.ui.wobblepulldown

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class WobbleDeviceInfoActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                finish()
                @Suppress("DEPRECATION")
                overridePendingTransition(R.anim.anim_stay, R.anim.anim_slide_out_top)
            }
        })

        setContent {
            CircleToSearchTheme {
                WobbleDeviceInfoScreen(
                    onBackClick = {
                        finish()
                        @Suppress("DEPRECATION")
                        overridePendingTransition(R.anim.anim_stay, R.anim.anim_slide_out_top)
                    }
                )
            }
        }
    }
}
