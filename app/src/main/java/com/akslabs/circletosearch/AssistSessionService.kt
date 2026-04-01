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
import android.os.Bundle
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.app.assist.AssistContent
import android.app.assist.AssistStructure
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.Build
import com.akslabs.circletosearch.data.BitmapRepository

class AssistSessionService : VoiceInteractionSessionService() {

    override fun onCreate() {
        super.onCreate()
        android.util.Log.d("AssistSessionService", "Service onCreate")
    }

    override fun onNewSession(args: Bundle?): VoiceInteractionSession {
        android.util.Log.d("AssistSessionService", "onNewSession created")
        return CircleToSearchSession(this)
    }

    inner class CircleToSearchSession(context: Context) : VoiceInteractionSession(context) {

        override fun onShow(args: Bundle?, showFlags: Int) {
            super.onShow(args, showFlags)
            android.util.Log.d("AssistSessionService", "onShow called with flags: $showFlags")
            
            // Haptic feedback to acknowledge the trigger
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }
        }

        override fun onHandleAssist(data: Bundle?, structure: AssistStructure?, content: AssistContent?) {
            super.onHandleAssist(data, structure, content)
            android.util.Log.d("AssistSessionService", "onHandleAssist called")
            // This is called for text/structure. We mainly care about the screenshot,
            // but this is a good place to ensure the overlay starts if onHandleScreenshot is delayed.
        }

        override fun onHandleScreenshot(screenshot: android.graphics.Bitmap?) {
            super.onHandleScreenshot(screenshot)
            android.util.Log.d("AssistSessionService", "onHandleScreenshot received, bitmap null? ${screenshot == null}")
            
            // 1. Save screenshot to repository for the overlay to use
            BitmapRepository.setScreenshot(screenshot)
            
            // 2. Launch the search overlay activity independently of Accessibility permission
            launchOverlayDirectly()
            
            // 3. We can finish the session now as the overlay takes over
            finish()
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
