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
                // If not enabled, do nothing or show toast? For now just finish
                // Ideally we might prompt them, but sticking to existing logic
            }

            // Haptic feedback
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }
            
            // Set null screenshot to indicate we need to capture inside OverlayActivity or just show overlay
            // The previous logic was to launch OverlayActivity. 
            // Since this isn't a screenshot capture from accessibility service, we pass null.
            BitmapRepository.setScreenshot(null)

            android.util.Log.d("AssistSessionService", "Launching OverlayActivity via startVoiceActivity")
            val intent = Intent(context, OverlayActivity::class.java)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            intent.addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
            try {
                startVoiceActivity(intent)
                android.util.Log.d("AssistSessionService", "OverlayActivity launch requested")
            } catch (e: Exception) {
                android.util.Log.e("AssistSessionService", "Failed to launch OverlayActivity", e)
                // Fallback to normal start if startVoiceActivity fails (though less likely to work if background)
                try {
                    context.startActivity(intent)
                } catch (e2: Exception) {
                     android.util.Log.e("AssistSessionService", "Fallback launch also failed", e2)
                }
            }
            
            // Do not finish immediately? 
            // If we finish(), the session might be destroyed before the activity starts?
            // But usually onHandleAssist is transient.
            // Let's keep finish() but maybe delay it or rely on onHide.
            // For now, keep as is.
            // finish() // Removing finish() to ensure session stays alive long enough to launch? 
            // Actually, for "Circle to Search", we usually want the session to close so the overlay takes over.
            // But let's try keeping it open for a split second or just call finish().
            finish()
        }
    }
}
