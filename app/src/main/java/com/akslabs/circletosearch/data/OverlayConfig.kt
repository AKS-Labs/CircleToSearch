/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.data

import android.content.Context
import android.content.SharedPreferences
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken

data class OverlayConfig(
    val isEnabled: Boolean = true,
    val isEnabledInLandscape: Boolean = false,
    val height: Int = 100, // Pixels
    val verticalOffset: Int = 0, // Pixels from top
    val isVisible: Boolean = false, // Debug visibility (colored)
    val segments: List<OverlaySegment> = listOf(OverlaySegment(0f, 1f)) // Default single full-width segment
)

data class OverlaySegment(
    val startFraction: Float, // 0.0 to 1.0
    val widthFraction: Float, // 0.0 to 1.0
    val gestures: MutableMap<GestureType, ActionType> = mutableMapOf(GestureType.DOUBLE_TAP to ActionType.SCREENSHOT)
)

enum class GestureType {
    DOUBLE_TAP,
    LONG_PRESS,
    TRIPLE_TAP
}

enum class ActionType {
    NONE,
    SCREENSHOT,
    FLASHLIGHT,
    HOME,
    BACK,
    RECENTS,
    LOCK_SCREEN,
    OPEN_APP, // Requires extra data, maybe simple string for package name later
    OPEN_NOTIFICATIONS,
    OPEN_QUICK_SETTINGS
}

class OverlayConfigurationManager(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("overlay_prefs", Context.MODE_PRIVATE)
    private val gson = Gson()
    
    companion object {
        private const val KEY_CONFIG = "overlay_config"
    }

    fun getConfig(): OverlayConfig {
        val json = prefs.getString(KEY_CONFIG, null)
        return if (json != null) {
            try {
                gson.fromJson(json, OverlayConfig::class.java)
            } catch (e: Exception) {
                OverlayConfig()
            }
        } else {
            OverlayConfig()
        }
    }

    fun saveConfig(config: OverlayConfig) {
        val json = gson.toJson(config)
        prefs.edit().putString(KEY_CONFIG, json).apply()
    }
    
    fun resetConfig() {
        prefs.edit().remove(KEY_CONFIG).apply()
    }
}
