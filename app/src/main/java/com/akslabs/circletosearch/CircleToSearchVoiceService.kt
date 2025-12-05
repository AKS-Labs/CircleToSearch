package com.akslabs.circletosearch

import android.service.voice.VoiceInteractionService

/**
 * Main VoiceInteractionService
 * This makes the app appear in Android's assistant selection settings.
 * The actual assist handling is done by AssistSessionService.
 */
class CircleToSearchVoiceService : VoiceInteractionService() {
    override fun onCreate() {
        super.onCreate()
        android.util.Log.d("CircleToSearchVoiceService", "VoiceService Created")
    }

    override fun onReady() {
        super.onReady()
        android.util.Log.d("CircleToSearchVoiceService", "VoiceService Ready")
    }
}
