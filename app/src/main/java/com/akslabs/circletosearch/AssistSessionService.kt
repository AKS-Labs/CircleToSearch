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
            android.util.Log.d("AssistSessionService", "onShow called")
        }

        override fun onHandleAssist(data: Bundle?, structure: AssistStructure?, content: AssistContent?) {
            android.util.Log.d("AssistSessionService", "onHandleAssist called")
            val prefs = context.getSharedPreferences("app_prefs", Context.MODE_PRIVATE)
            if (!prefs.getBoolean("assistant_enabled", false)) {
                android.util.Log.d("AssistSessionService", "Assistant disabled in prefs, but continuing for debug")
            }

            // Haptic feedback
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }
            
            // Try to trigger capture via AccessibilityService (which handles screenshot & launch)
            try {
                android.util.Log.d("AssistSessionService", "Requesting triggerCapture from AccessibilityService")
                CircleToSearchAccessibilityService.triggerCapture()
            } catch (e: Exception) {
                 android.util.Log.e("AssistSessionService", "Failed to trigger AccessibilityService capture", e)
                 // Fallback: Launch overlay anyway (might be black, but better than nothing)
                 launchOverlayDirectly()
            }
            
            // We can finish the session now as the overlay takes over
            finish()
        }

        private fun launchOverlayDirectly() {
            BitmapRepository.setScreenshot(null)
            android.util.Log.d("AssistSessionService", "Launching OverlayActivity directly (fallback)")
            val intent = Intent(context, OverlayActivity::class.java)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            intent.addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
            try {
                // startVoiceActivity might fail if not allowed, try catch
                 try {
                    startVoiceActivity(intent)
                } catch (e: SecurityException) {
                    android.util.Log.w("AssistSessionService", "startVoiceActivity failed, using startActivity", e)
                    context.startActivity(intent)
                }
            } catch (e: Exception) {
                android.util.Log.e("AssistSessionService", "Failed to launch OverlayActivity", e)
            }
        }
    }
}
