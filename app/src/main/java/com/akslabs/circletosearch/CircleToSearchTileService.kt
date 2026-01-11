/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch

import android.content.Intent
import android.provider.Settings
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.widget.Toast
import android.util.Log

class CircleToSearchTileService : TileService() {

    override fun onClick() {
        super.onClick()
        
        Log.d("CircleToSearchTile", "Tile clicked. Service enabled: ${isAccessibilityServiceEnabled()}")
        
        if (isAccessibilityServiceEnabled()) {
            // Launch the trigger activity which will collapse the shade and call the capture logic
            val intent = Intent(this, TileTriggerActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivityAndCollapse(intent)
        } else {
            // Guide user to enable accessibility
            Toast.makeText(this, "Please enable Circle to Search Accessibility Service", Toast.LENGTH_LONG).show()
            val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivityAndCollapse(intent)
        }
    }

    override fun onStartListening() {
        super.onStartListening()
        updateTileState()
    }

    private fun updateTileState() {
        val tile = qsTile ?: return
        val isEnabled = isAccessibilityServiceEnabled()
        
        tile.state = if (isEnabled) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "Circle to Search"
        tile.updateTile()
    }

    private fun isAccessibilityServiceEnabled(): Boolean {
        val expectedComponentName = android.content.ComponentName(this, CircleToSearchAccessibilityService::class.java).flattenToString()
        val enabledServices = Settings.Secure.getString(contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: ""
        return enabledServices.contains(expectedComponentName)
    }
}
