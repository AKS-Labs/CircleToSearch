/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.data

import android.content.Context
import android.content.SharedPreferences
import com.google.gson.Gson
import com.akslabs.circletosearch.data.OverlaySegment

data class OverlayConfig(
    val isEnabled: Boolean = true,
    val isEnabledInLandscape: Boolean = false,
    val isVisible: Boolean = false, // Debug visibility (colored)
    val segments: List<OverlaySegment> = listOf(OverlaySegment(width = 1080)) // Default to a common large width
)

data class OverlaySegment(
    val width: Int = 150, // Pixels
    val height: Int = 60, // Pixels
    val xOffset: Int = 0, // Pixels from left
    val yOffset: Int = 0, // Pixels from top
    val gestures: MutableMap<GestureType, ActionType> = mutableMapOf(GestureType.DOUBLE_TAP to ActionType.CTS_MULTI),
    val gestureData: MutableMap<GestureType, String> = mutableMapOf() // Stores extra data like package name for OPEN_APP
)

enum class GestureType {
    DOUBLE_TAP,
    LONG_PRESS,
    TRIPLE_TAP,
    SWIPE_UP,
    SWIPE_DOWN,
    SWIPE_LEFT,
    SWIPE_RIGHT
}

enum class ActionType {
    NONE,
    SCREENSHOT,
    FLASHLIGHT,
    HOME,
    BACK,
    RECENTS,
    LOCK_SCREEN,
    OPEN_NOTIFICATIONS,
    OPEN_QUICK_SETTINGS,
    CTS_LENS,
    CTS_MULTI,
    SPLIT_SCREEN,
    OPEN_APP,
    SCROLL_TOP,
    SCROLL_BOTTOM,
    SCREEN_OFF,
    TOGGLE_AUTO_ROTATE,
    MEDIA_PLAY_PAUSE,
    MEDIA_NEXT,
    MEDIA_PREVIOUS
}

fun ActionType.getFriendlyName(): String = when (this) {
    ActionType.NONE -> "No Action"
    ActionType.SCREENSHOT -> "Take Screenshot"
    ActionType.FLASHLIGHT -> "Flashlight"
    ActionType.HOME -> "Go Home"
    ActionType.BACK -> "Go Back"
    ActionType.RECENTS -> "Recent Apps"
    ActionType.LOCK_SCREEN -> "Lock Screen"
    ActionType.OPEN_NOTIFICATIONS -> "Open Notifications"
    ActionType.OPEN_QUICK_SETTINGS -> "Quick Settings"
    ActionType.CTS_LENS -> "Google Lens Search"
    ActionType.CTS_MULTI -> "Multi-Search (All Engines)"
    ActionType.SPLIT_SCREEN -> "Split Screen"
    ActionType.OPEN_APP -> "Open Application"
    ActionType.SCROLL_TOP -> "Scroll to Top"
    ActionType.SCROLL_BOTTOM -> "Scroll to Bottom"
    ActionType.SCREEN_OFF -> "Turn Off Screen"
    ActionType.TOGGLE_AUTO_ROTATE -> "Toggle Auto Rotate"
    ActionType.MEDIA_PLAY_PAUSE -> "Media Play/Pause"
    ActionType.MEDIA_NEXT -> "Media Next"
    ActionType.MEDIA_PREVIOUS -> "Media Previous"
}

fun GestureType.getFriendlyName(): String = when (this) {
    GestureType.DOUBLE_TAP -> "Double Tap"
    GestureType.LONG_PRESS -> "Long Press"
    GestureType.TRIPLE_TAP -> "Triple Tap"
    GestureType.SWIPE_UP -> "Swipe Up"
    GestureType.SWIPE_DOWN -> "Swipe Down"
    GestureType.SWIPE_LEFT -> "Swipe Left"
    GestureType.SWIPE_RIGHT -> "Swipe Right"
}

class OverlayConfigurationManager(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("overlay_prefs", Context.MODE_PRIVATE)
    private val gson = Gson()
    
    companion object {
        private const val KEY_CONFIG = "overlay_config"
    }

    fun getConfig(): OverlayConfig {
        val json = prefs.getString(KEY_CONFIG, null)
        if (json != null) {
            try {
                val config = gson.fromJson(json, OverlayConfig::class.java)
                // Sanitize: Gson might result in null keys in maps if enum values are missing
                val sanitizedSegments = config.segments.map { segment ->
                     val cleanGestures = segment.gestures.filterKeys { it != null }.toMutableMap()
                     segment.copy(gestures = cleanGestures)
                }
                return config.copy(segments = sanitizedSegments)
            } catch (e: Exception) {
                // If deep failure, return default
                return OverlayConfig()
            }
        } else {
            return OverlayConfig()
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
