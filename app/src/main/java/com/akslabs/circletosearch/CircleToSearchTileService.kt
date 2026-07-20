/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch

import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.provider.Settings
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import android.util.Log
import android.widget.Toast
import com.akslabs.circletosearch.utils.SearchTrigger

class CircleToSearchTileService : TileService() {

    override fun onClick() {
        super.onClick()

        Log.d("CircleToSearchTile", "Tile clicked. Service enabled: ${SearchTrigger.isAccessibilityEnabled(this)}")

        if (SearchTrigger.isAccessibilityEnabled(this)) {
            // Launch the trigger activity which will collapse the shade and call the capture logic
            collapseAndStart(SearchTrigger.createTriggerIntent(this, collapseShade = true))
        } else {
            // Guide user to enable accessibility
            Toast.makeText(
                this,
                "Please enable Circle to Search Accessibility Service",
                Toast.LENGTH_LONG
            ).show()
            val settingsIntent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            collapseAndStart(settingsIntent)
        }
    }

    override fun onStartListening() {
        super.onStartListening()
        updateTileState()
    }

    private fun collapseAndStart(intent: Intent) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            val pendingIntent = PendingIntent.getActivity(
                this, 0, intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )
            startActivityAndCollapse(pendingIntent)
        } else {
            @Suppress("DEPRECATION")
            startActivityAndCollapse(intent)
        }
    }

    private fun updateTileState() {
        val tile = qsTile ?: return
        val isEnabled = SearchTrigger.isAccessibilityEnabled(this)

        tile.state = if (isEnabled) Tile.STATE_ACTIVE else Tile.STATE_INACTIVE
        tile.label = "Circle to Search"
        tile.updateTile()
    }
}
