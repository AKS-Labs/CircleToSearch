/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.data

import android.content.Context
import android.content.SharedPreferences
import com.akslabs.circletosearch.R
import com.google.gson.Gson
import com.google.gson.annotations.SerializedName

data class OverlayConfig(
    @SerializedName("isEnabled") val isEnabled: Boolean = true,
    @SerializedName("isEnabledInLandscape") val isEnabledInLandscape: Boolean = false,
    @SerializedName("isVisible") val isVisible: Boolean = false, // Debug visibility (colored)
    @SerializedName("activeSegmentIndex") val activeSegmentIndex: Int = -1, // Currently being edited in settings
    @SerializedName("segments") val segments: List<OverlaySegment> = listOf(OverlaySegment(width = 1080)) // Default to a common large width
)

data class OverlaySegment(
    @SerializedName("width") val width: Int = 150, // Pixels
    @SerializedName("height") val height: Int = 60, // Pixels
    @SerializedName("xOffset") val xOffset: Int = 0, // Pixels from left
    @SerializedName("yOffset") val yOffset: Int = 0, // Pixels from top
    @SerializedName("gestures") val gestures: MutableMap<GestureType, ActionType> = mutableMapOf(GestureType.DOUBLE_TAP to ActionType.CTS_AUTO),
    @SerializedName("gestureData") val gestureData: MutableMap<GestureType, String> = mutableMapOf() // Stores extra data like package name for OPEN_APP
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
    CTS_AUTO,
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

fun ActionType.getFriendlyName(context: Context): String = when (this) {
    ActionType.NONE -> context.getString(R.string.action_none)
    ActionType.SCREENSHOT -> context.getString(R.string.action_screenshot)
    ActionType.FLASHLIGHT -> context.getString(R.string.action_flashlight)
    ActionType.HOME -> context.getString(R.string.action_home)
    ActionType.BACK -> context.getString(R.string.action_back)
    ActionType.RECENTS -> context.getString(R.string.action_recents)
    ActionType.LOCK_SCREEN -> context.getString(R.string.action_lock_screen)
    ActionType.OPEN_NOTIFICATIONS -> context.getString(R.string.action_open_notifications)
    ActionType.OPEN_QUICK_SETTINGS -> context.getString(R.string.action_quick_settings)
    ActionType.CTS_AUTO -> context.getString(R.string.action_cts_auto)
    ActionType.CTS_LENS -> context.getString(R.string.action_cts_lens)
    ActionType.CTS_MULTI -> context.getString(R.string.action_cts_multi)
    ActionType.SPLIT_SCREEN -> context.getString(R.string.action_split_screen)
    ActionType.OPEN_APP -> context.getString(R.string.action_open_app)
    ActionType.SCROLL_TOP -> context.getString(R.string.action_scroll_top)
    ActionType.SCROLL_BOTTOM -> context.getString(R.string.action_scroll_bottom)
    ActionType.SCREEN_OFF -> context.getString(R.string.action_screen_off)
    ActionType.TOGGLE_AUTO_ROTATE -> context.getString(R.string.action_toggle_auto_rotate)
    ActionType.MEDIA_PLAY_PAUSE -> context.getString(R.string.action_media_play_pause)
    ActionType.MEDIA_NEXT -> context.getString(R.string.action_media_next)
    ActionType.MEDIA_PREVIOUS -> context.getString(R.string.action_media_previous)
}

fun GestureType.getFriendlyName(context: Context): String = when (this) {
    GestureType.DOUBLE_TAP -> context.getString(R.string.gesture_double_tap)
    GestureType.LONG_PRESS -> context.getString(R.string.gesture_long_press)
    GestureType.TRIPLE_TAP -> context.getString(R.string.gesture_triple_tap)
    GestureType.SWIPE_UP -> context.getString(R.string.gesture_swipe_up)
    GestureType.SWIPE_DOWN -> context.getString(R.string.gesture_swipe_down)
    GestureType.SWIPE_LEFT -> context.getString(R.string.gesture_swipe_left)
    GestureType.SWIPE_RIGHT -> context.getString(R.string.gesture_swipe_right)
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
