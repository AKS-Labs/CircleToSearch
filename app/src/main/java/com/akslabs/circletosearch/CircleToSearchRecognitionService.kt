package com.akslabs.circletosearch

import android.content.Intent
import android.speech.RecognitionService

class CircleToSearchRecognitionService : RecognitionService() {
    override fun onStartListening(intent: Intent?, callback: Callback?) {
        // Not implemented - strictly for system compliance
    }

    override fun onStopListening(callback: Callback?) {
        // Not implemented
    }

    override fun onCancel(callback: Callback?) {
        // Not implemented
    }
}
