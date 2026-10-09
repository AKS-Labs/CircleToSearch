package com.akslabs.circletosearch.ui.wobblepulldown

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.OnBackPressedCallback
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

class WobbleDeviceInfoActivity : ComponentActivity() {

    private val wizardMode: Boolean by lazy {
        intent.getBooleanExtra(EXTRA_SETUP_WIZARD, false)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (wizardMode) {
                    setResult(RESULT_CANCELED)
                    finish()
                } else {
                    finish()
                    @Suppress("DEPRECATION")
                    overridePendingTransition(R.anim.anim_stay, R.anim.anim_slide_out_top)
                }
            }
        })

        lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onResume(owner: LifecycleOwner) {
                maybeAutoDismiss()
            }
        })

        if (wizardMode) {
            lifecycleScope.launch {
                while (true) {
                    delay(500)
                    maybeAutoDismiss()
                }
            }
        }

        setContent {
            CircleToSearchTheme {
                if (wizardMode) {
                    WobbleSetupWizard()
                } else {
                    WobbleAppInfoScreen(
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

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) maybeAutoDismiss()
    }

    private fun maybeAutoDismiss() {
        if (wizardMode &&
            (isA11yGranted(applicationContext) || isAssistantGranted(applicationContext))
        ) {
            setResult(RESULT_OK)
            finish()
            @Suppress("DEPRECATION")
            overridePendingTransition(R.anim.anim_stay, R.anim.anim_slide_out_top)
        }
    }

    companion object {
        const val EXTRA_SETUP_WIZARD = "setup_wizard"
    }
}
