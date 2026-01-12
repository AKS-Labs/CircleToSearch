/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch

import android.app.Activity
import android.os.Build
import android.os.Bundle

/**
 * A transparent activity launched from the Quick Settings tile.
 * Launching an activity from TileService automatically collapses the notification shade.
 */
class TileTriggerActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        
        // Hide from recents and UI
        // Theme is set in manifest (Translucent/Transparent)
        
        // Disable window animations for this activity
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_OPEN, 0, 0)
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else {
            @Suppress("DEPRECATION")
            overridePendingTransition(0, 0)
        }
        
        android.util.Log.d("CircleToSearchTile", "TileTriggerActivity started")
        
        // Trigger capture immediately
        CircleToSearchAccessibilityService.triggerCapture()
        finish()
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            overrideActivityTransition(OVERRIDE_TRANSITION_CLOSE, 0, 0)
        } else {
            @Suppress("DEPRECATION")
            overridePendingTransition(0, 0)
        }
    }
}
