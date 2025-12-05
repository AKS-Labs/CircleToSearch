package com.akslabs.circletosearch

import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.VibrationEffect
import android.os.Vibrator
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.app.assist.AssistStructure
import android.app.assist.AssistContent
import com.akslabs.circletosearch.data.BitmapRepository

class AssistService : VoiceInteractionSessionService() {
    
    override fun onNewSession(args: Bundle?): VoiceInteractionSession {
        return CircleToSearchSession(this)
    }
    
    inner class CircleToSearchSession(context: Context) : VoiceInteractionSession(context) {
        
        override fun onHandleAssist(
            data: Bundle?,
            structure: AssistStructure?,
            content: AssistContent?
        ) {
            // Check if assistant integration is enabled
            val prefs = context.getSharedPreferences("app_prefs", Context.MODE_PRIVATE)
            if (!prefs.getBoolean("assistant_enabled", false)) {
                finish()
                return
            }
            
            // Haptic feedback
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_HEAVY_CLICK))
            } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(10, VibrationEffect.DEFAULT_AMPLITUDE))
            }
            
            // Launch overlay without screenshot
            // Note: VoiceInteractionSession doesn't have direct screenshot capability
            // User will see the overlay and can select area manually
            BitmapRepository.setScreenshot(null)
            launchOverlay()
            
            finish()
        }
        
        private fun launchOverlay() {
            val intent = Intent(context, OverlayActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
            }
            context.startActivity(intent)
        }
    }
}
