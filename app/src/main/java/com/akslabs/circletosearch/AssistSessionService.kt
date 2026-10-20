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

import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.app.assist.AssistContent
import android.app.assist.AssistStructure
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.Build
import com.akslabs.circletosearch.data.BitmapRepository
import dev.davidv.translator.TranslatorApplication

class AssistSessionService : VoiceInteractionSessionService() {

    companion object {
        /** Screenshot handed from AssistEntryActivity (captured by the system's assist pipeline). */
        const val EXTRA_SCREENSHOT_BITMAP = "com.akslabs.circletosearch.extra.SCREENSHOT_BITMAP"

        /**
         * Statically invoked from the in-app translate action (accessibility disabled) to show
         * the module's assistant translate overlay. This reuses the module's
         * [TranslatorVoiceInteractionSession] — the exact overlay UI/UX the accessibility path
         * uses — so the translation screen looks and behaves identically; it is driven by the
         * assistant permission rather than a separately-built in-app screen.
         */
        fun showTranslateOverlay(context: Context, screenshot: Bitmap?) {
            pendingTranslateScreenshot = screenshot
            translateMode = true
            val voiceService = CircleToSearchVoiceService.instance
            if (voiceService == null) {
                android.util.Log.e("AssistSessionService", "Voice service not ready; cannot show translate overlay")
                translateMode = false
                return
            }
            try {
                // No SHOW_WITH_SCREENSHOT: the screenshot we already hold is handed in via
                // setScreenshot, so the system doesn't need to capture one itself.
                voiceService.showSession(Bundle(), 0)
                // Our overlay activity keeps covering the screen with the frozen capture,
                // and the module session's window isn't an activity. It is closed once the
                // session has consumed the frame — see onNewSession / closeOverlayActivity.
            } catch (e: Exception) {
                android.util.Log.e("AssistSessionService", "Failed to show translate session", e)
                translateMode = false
            }
        }

        /**
         * Closes the Circle to Search overlay activity. Called once the module session has
         * been shown so its window is the one on screen; without this the frozen capture
         * drawn by the activity covers the session's toolbar/scan UI.
         */
        fun closeOverlayActivity(context: Context) {
            val activity = activityRef ?: return
            activityRef = null
            activity.runOnUiThread {
                android.util.Log.d("AssistSessionService", "Closing overlay activity so module session is visible")
                activity.finish()
            }
        }

        /** [OverlayActivity] registers itself on create so [closeOverlayActivity] can dismiss it. */
        fun registerOverlayActivity(activity: android.app.Activity) {
            activityRef = activity
        }

        private var translateMode = false
        private var pendingTranslateScreenshot: Bitmap? = null
        private var activityRef: android.app.Activity? = null

        /** Consumed once by the module session so a later plain assist isn't forced into translate. */
        internal fun consumeTranslateScreenshot(): Bitmap? =
            pendingTranslateScreenshot.also { pendingTranslateScreenshot = null }
    }

    private var langStateManager: dev.davidv.translator.LanguageStateManager? = null
    private var overlayTextTranslationHelper: dev.davidv.translator.OverlayTextTranslationHelper? = null

    override fun onCreate() {
        super.onCreate()
        android.util.Log.d("AssistSessionService", "Service onCreate")
        // Wire the module overlay's dependencies from the shared application singletons.
        // Mirrors the wiring the module's own TranslatorVoiceInteractionSessionService does,
        // so the overlay gets the same state as the accessibility path.
        val app = application as? TranslatorApplication
        if (app != null) {
            val lm = dev.davidv.translator.LanguageStateManager(
                kotlinx.coroutines.CoroutineScope(
                    kotlinx.coroutines.SupervisorJob() + kotlinx.coroutines.Dispatchers.Main
                ),
                app.filePathManager,
                null,
            )
            langStateManager = lm
            overlayTextTranslationHelper =
                dev.davidv.translator.OverlayTextTranslationHelper(lm, app.languageMetadataManager)
        }
    }

    override fun onNewSession(args: Bundle?): VoiceInteractionSession {
        android.util.Log.d("AssistSessionService", "onNewSession created")
        // Translate mode shows the module's assistant overlay — the same UI/UX as the
        // accessibility path — instead of the search overlay. This is how the translation
        // screen is reused rather than reimplemented in-app.
        if (translateMode) {
            translateMode = false
            val app = application as? TranslatorApplication
            val lm = langStateManager
            val helper = overlayTextTranslationHelper
            if (app != null && lm != null && helper != null) {
                android.util.Log.d("AssistSessionService", "Returning module translator session")
                val session = dev.davidv.translator.assistantOverlay.TranslatorVoiceInteractionSession(
                    this,
                    app.settingsManager,
                    app.translationCoordinator,
                    helper,
                    lm,
                )
                // Close our overlay once the session has actually taken up the frame, so
                // dismissing our activity can't race the hand-off (onShow clears a pending
                // capture, and the first layout is what triggers consumePendingScreenshot).
                session.onScreenshotConsumed = { closeOverlayActivity(applicationContext) }
                // Hand the already-captured screenshot to the session's existing path.
                val shot = consumeTranslateScreenshot()
                if (shot != null) session.setScreenshot(shot)
                return session
            }
            android.util.Log.e("AssistSessionService", "Translate deps unavailable; falling back")
        }
        return CircleToSearchSession(this)
    }

    inner class CircleToSearchSession(context: Context) : VoiceInteractionSession(context) {

        private var captureLaunched = false
        private val sessionHandler = android.os.Handler(android.os.Looper.getMainLooper())

        override fun onShow(args: Bundle?, showFlags: Int) {
            super.onShow(args, showFlags)
            android.util.Log.d("AssistSessionService", "onShow called with flags: $showFlags")

            // Ensure our session window doesn't intercept target app touches
            window?.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE)
            window?.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE)

            // Haptic feedback to acknowledge the trigger
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }

            // Screenshot captured by the system's assist pipeline (assistant privileges)
            // and passed through the entry activity. Using it means the whole flow works
            // without the accessibility service.
            val systemScreenshot: android.graphics.Bitmap? = args?.let {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    it.getParcelable(EXTRA_SCREENSHOT_BITMAP, android.graphics.Bitmap::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    it.getParcelable(EXTRA_SCREENSHOT_BITMAP)
                }
            }
            if (systemScreenshot != null && !systemScreenshot.isRecycled) {
                android.util.Log.d(
                    "AssistSessionService",
                    "Using system-provided screenshot ${systemScreenshot.width}x${systemScreenshot.height}"
                )
                BitmapRepository.setScreenshot(systemScreenshot)
                captureLaunched = true
                launchOverlayDirectly()
                return
            }

            // No screenshot in args (e.g. plain assist invocation). Modern Android versions
            // may never deliver onHandleScreenshot, so kick off a capture shortly after
            // showing; whichever source (the fallback or the system screenshot) arrives
            // first wins. Without the accessibility service wait a little longer for the
            // system screenshot requested via SHOW_WITH_SCREENSHOT.
            captureLaunched = false
            val a11yAvailable = CircleToSearchAccessibilityService.instance != null
            val fallbackDelayMs = if (a11yAvailable) 250L else 1200L
            sessionHandler.postDelayed({
                if (captureLaunched) return@postDelayed
                captureLaunched = true
                if (CircleToSearchAccessibilityService.instance != null) {
                    android.util.Log.d("AssistSessionService", "Using accessibility capture fallback")
                    CircleToSearchAccessibilityService.triggerCapture()
                } else {
                    android.util.Log.d("AssistSessionService", "No capture source available; launching overlay directly")
                    launchOverlayDirectly()
                }
            }, fallbackDelayMs)
        }

        override fun onHandleAssist(data: Bundle?, structure: AssistStructure?, content: AssistContent?) {
            super.onHandleAssist(data, structure, content)
            android.util.Log.d("AssistSessionService", "onHandleAssist called")
        }

        override fun onHandleScreenshot(screenshot: android.graphics.Bitmap?) {
            super.onHandleScreenshot(screenshot)
            android.util.Log.d("AssistSessionService", "onHandleScreenshot received, bitmap null? ${screenshot == null}")

            if (captureLaunched) {
                android.util.Log.d("AssistSessionService", "Screenshot arrived after fallback already launched; ignoring")
                return
            }
            captureLaunched = true

            // 1. Save screenshot to repository
            BitmapRepository.setScreenshot(screenshot)

            // 2. Launch the search overlay
            launchOverlayDirectly()
        }

        private fun launchOverlayDirectly() {
            android.util.Log.d("AssistSessionService", "Launching OverlayActivity")
            val intent = Intent(context, OverlayActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
                // Signal that this was triggered by the assistant session
                putExtra("triggered_by", "assistant")
            }
            
            try {
                // Using startActivity as an assistant session is allowed to start activities
                context.startActivity(intent)
            } catch (e: Exception) {
                android.util.Log.e("AssistSessionService", "Failed to launch OverlayActivity", e)
            }
        }
    }
}
