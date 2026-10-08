package com.akslabs.circletosearch.ui.wobblepulldown

import android.app.Activity
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.systemBars
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.app.ActivityOptionsCompat
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.isAccessibilityServiceEnabled
import com.akslabs.circletosearch.isDefaultAssistant
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class WobbleHomeDemoActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            CircleToSearchTheme {
                WobbleDemoContent(onExit = { finish() })
            }
        }
    }
}

@Composable
private fun WobbleDemoContent(onExit: () -> Unit) {
    val context = LocalContext.current
    var wobbleTick by remember { mutableStateOf(0) }

    val wizardLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            wobbleTick++
        } else {
            onExit()
        }
    }

    LaunchedEffect(Unit) {
        val granted = isAccessibilityServiceEnabled(context) || isDefaultAssistant(context)
        if (!granted) {
            val intent = Intent(context, WobbleDeviceInfoActivity::class.java)
                .putExtra(WobbleDeviceInfoActivity.EXTRA_SETUP_WIZARD, true)
            val options = ActivityOptionsCompat.makeCustomAnimation(
                context,
                R.anim.anim_slide_in_top,
                R.anim.anim_stay,
            )
            wizardLauncher.launch(intent, options)
        }
    }

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
        wobbleEvent = wobbleTick,
        contentPadding = WindowInsets.systemBars.asPaddingValues(),
    )
}
