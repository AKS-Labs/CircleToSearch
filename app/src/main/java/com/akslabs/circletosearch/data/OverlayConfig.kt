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
    @SerializedName("previewVisible") val previewVisible: Boolean = false, // true only while the settings dropdown is open
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

/**
 * Dedicated config for the bottom-edge long-press triggers (two corner arcs + one
 * bottom-centre semicircle). Stored separately from [OverlayConfig.segments] so that
 * toggling the status-bar overlay ([OverlayConfig.isEnabled]) can never add/remove them.
 */
data class BottomEdgeConfig(
    @SerializedName("leftEnabled") val leftEnabled: Boolean = true,
    @SerializedName("rightEnabled") val rightEnabled: Boolean = true,
    @SerializedName("midEnabled") val midEnabled: Boolean = false,
    @SerializedName("cornerSizeDp") val cornerSizeDp: Int = 72, // arc radius-ish size
    @SerializedName("midSizeDp") val midSizeDp: Int = 48, // semicircle size (actual overlay)
    @SerializedName("previewVisible") val previewVisible: Boolean = false, // true only while settings dropdown open
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
private const val KEY_BOTTOM_EDGE = "bottom_edge_config"
private const val KEY_BOTTOM_EDGE_MID_MIGRATED = "bottom_edge_mid_migrated_v1"
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

    fun getBottomEdgeConfig(): BottomEdgeConfig {
        val json = prefs.getString(KEY_BOTTOM_EDGE, null) ?: return BottomEdgeConfig()
        val cfg = try {
            gson.fromJson(json, BottomEdgeConfig::class.java) ?: BottomEdgeConfig()
        } catch (e: Exception) {
            return BottomEdgeConfig()
        }
        // One-time migration: the old mid-overlay defaults (96dp, later 64dp) were too
        // large for the actual on-screen semicircle — shrink them to the new 48dp default.
        if (!prefs.getBoolean(KEY_BOTTOM_EDGE_MID_MIGRATED, false) && cfg.midSizeDp >= 64) {
            val migrated = cfg.copy(midSizeDp = 48)
            prefs.edit()
                .putString(KEY_BOTTOM_EDGE, gson.toJson(migrated))
                .putBoolean(KEY_BOTTOM_EDGE_MID_MIGRATED, true)
                .apply()
            return migrated
        }
        return cfg
    }

    fun saveBottomEdgeConfig(config: BottomEdgeConfig) {
        prefs.edit().putString(KEY_BOTTOM_EDGE, gson.toJson(config)).apply()
    }
}
