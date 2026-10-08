package com.akslabs.circletosearch.ui.wobblepulldown

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.systemBars
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import androidx.core.app.ActivityOptionsCompat
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class WobbleHomeDemoActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            CircleToSearchTheme {
                WobbleDemoContent()
            }
        }
    }
}

@Composable
private fun WobbleDemoContent() {
    val context = LocalContext.current
    WobbleHomeScreen(
        onOpenDeviceInfo = {
            val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
            val options = ActivityOptionsCompat.makeCustomAnimation(
                context,
                R.anim.anim_slide_in_top,
                R.anim.anim_stay,
            )
            context.startActivity(intent, options.toBundle())
        },
        contentPadding = WindowInsets.systemBars.asPaddingValues(),
    )
}
