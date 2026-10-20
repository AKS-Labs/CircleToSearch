/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 */

package com.akslabs.circletosearch

import android.app.Activity
import android.content.Intent
import android.graphics.Bitmap
import android.os.Build
import android.os.Bundle
import android.service.voice.VoiceInteractionSession
import com.akslabs.circletosearch.data.BitmapRepository

/**
 * Entry point for system assist intents (ACTION_ASSIST / ACTION_VOICE_ASSIST) and
 * the contextual search launch (ACTION_LAUNCH_CONTEXTUAL_SEARCH) that the system
 * server targets at this package while it is the default assistant.
 *
 * The trigger is handed to our VoiceInteractionSession — the assistant path — with
 * the invocation-time screenshot the system server captured with assistant
 * privileges. The session launches the overlay from that screenshot, so the flow
 * does not depend on the accessibility service. Accessibility capture remains as
 * a fallback for invocations where no screenshot was delivered.
 */
class AssistEntryActivity : Activity() {

    companion object {
        /** Cold-start race window: starting this activity may spawn the process before
         *  the system has bound [CircleToSearchVoiceService]; retry briefly for it. */
        private const val VOICE_SERVICE_RETRY_MS = 100L
        private const val VOICE_SERVICE_RETRIES = 10
    }

    private val retryHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private var screenshot: Bitmap? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else {
            @Suppress("DEPRECATION")
            overridePendingTransition(0, 0)
        }

        android.util.Log.d("CircleToSearchAssist", "Assist intent received: ${intent?.action}")
        screenshot = extractScreenshot(intent)
        attemptHandoff(0)
    }

    private fun attemptHandoff(attempt: Int) {
        val shot = screenshot
        val voiceService = CircleToSearchVoiceService.instance
        if (voiceService != null) {
            try {
                val args = Bundle()
                if (shot != null) {
                    args.putParcelable(AssistSessionService.EXTRA_SCREENSHOT_BITMAP, shot)
                }
                // No screenshot attached? Ask the system for one through the
                // assistant pipeline instead; the session falls back if it
                // never arrives.
                val flags = if (shot == null) {
                    VoiceInteractionSession.SHOW_WITH_SCREENSHOT
                } else {
                    0
                }
                voiceService.showSession(args, flags)
                android.util.Log.d(
                    "CircleToSearchAssist",
                    "Handed trigger to voice interaction session (screenshot=${shot != null}, attempt=$attempt)"
                )
                finish()
                return
            } catch (e: Exception) {
                android.util.Log.e("CircleToSearchAssist", "showSession failed; using direct path", e)
            }
        }

        // The gesture app may have started this process fresh: the system binds the
        // voice interaction service slightly later, so retry before giving up.
        if (attempt < VOICE_SERVICE_RETRIES) {
            retryHandler.postDelayed({ attemptHandoff(attempt + 1) }, VOICE_SERVICE_RETRY_MS)
            return
        }

        // Assistant session unavailable: still launch without accessibility when
        // the system handed us its own screenshot.
        if (shot != null) {
            BitmapRepository.setScreenshot(shot)
            try {
                startActivity(Intent(this, OverlayActivity::class.java).apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
                    putExtra("triggered_by", "assistant")
                })
            } catch (e: Exception) {
                android.util.Log.e("CircleToSearchAssist", "Failed to launch OverlayActivity", e)
            }
        } else {
            CircleToSearchAccessibilityService.triggerCapture()
        }
        finish()
    }

    override fun onDestroy() {
        retryHandler.removeCallbacksAndMessages(null)
        super.onDestroy()
    }

    private fun extractScreenshot(intent: Intent?): Bitmap? {
        val raw = try {
            @Suppress("DEPRECATION")
            intent?.extras?.get("android.app.contextualsearch.extra.SCREENSHOT")
        } catch (e: Exception) {
            null
        }
        if (raw == null) return null
        if (raw !is Bitmap) {
            android.util.Log.w("CircleToSearchAssist", "Unexpected screenshot extra type: ${raw.javaClass.name}")
            return null
        }
        return try {
            // Match the accessibility capture path: OCR/native pixel access expects a
            // software ARGB_8888 bitmap; the assist screenshot may arrive as HARDWARE
            // or a wide-color config.
            if (raw.config != Bitmap.Config.ARGB_8888) {
                raw.copy(Bitmap.Config.ARGB_8888, false)
            } else {
                raw
            }
        } catch (e: Exception) {
            raw
        }
    }
}
