package com.akslabs.circletosearch

import android.content.Intent
import android.speech.RecognitionService

class CircleToSearchRecognitionService : RecognitionService() {
    override fun onStartListening(intent: Intent?, callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onStartListening")
    }

    override fun onStopListening(callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onStopListening")
    }

    override fun onCancel(callback: Callback?) {
        android.util.Log.d("CircleToSearchRecog", "onCancel")
    }
}
