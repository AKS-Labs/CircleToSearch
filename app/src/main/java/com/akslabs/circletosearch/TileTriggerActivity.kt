/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch

import android.app.Activity
import android.os.Bundle
import android.os.Handler
import android.os.Looper

/**
 * A transparent activity launched from the Quick Settings tile.
 * Launching an activity from TileService automatically collapses the notification shade.
 */
class TileTriggerActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        // Hide from recents and UI
        // Theme is set in manifest (Translucent/Transparent)
        
        android.util.Log.d("CircleToSearchTile", "TileTriggerActivity started")
        
        // Small delay to ensure the shade is fully collapsed and system is ready
        Handler(Looper.getMainLooper()).postDelayed({
            android.util.Log.d("CircleToSearchTile", "Triggering capture from activity")
            CircleToSearchAccessibilityService.triggerCapture()
            finish()
            // Optional: overridePendingTransition(0, 0)
        }, 300)
    }
}
