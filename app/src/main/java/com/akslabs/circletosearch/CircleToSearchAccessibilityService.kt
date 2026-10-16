/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *  *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *  *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *  *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 */

package com.akslabs.circletosearch

import android.accessibilityservice.AccessibilityService
import android.annotation.SuppressLint
import android.content.Context
import android.animation.ValueAnimator
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Outline
import android.graphics.PixelFormat
import android.graphics.Rect
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.hardware.camera2.CameraManager
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.BitmapShader
import android.graphics.Shader
import android.graphics.Matrix
import androidx.core.graphics.drawable.toBitmap
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.view.Display
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.ViewOutlineProvider
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityNodeInfo
import android.view.animation.DecelerateInterpolator
import com.akslabs.circletosearch.R
import com.akslabs.circletosearch.data.ActionType
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.data.GestureType
import com.akslabs.circletosearch.data.OverlayConfigurationManager
import com.akslabs.circletosearch.data.OverlaySegment
import com.akslabs.circletosearch.utils.ImageUtils
import java.util.concurrent.Executor
import java.util.concurrent.Executors
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.spring
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MotionScheme
import androidx.compose.runtime.MonotonicFrameClock
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlin.coroutines.resume
import kotlin.coroutines.suspendCoroutine

/**
 * Corner radius for pinned stickers: never smaller than 16dp, and proportional
 * to the sticker size (12.7% of the short side) so enlarged stickers keep a
 * visually equivalent roundness instead of reading as square.
 */
private fun stickerCornerRadius(width: Int, height: Int, density: Float): Float =
    maxOf(16f * density, minOf(width, height) * 0.127f)

// --- Hardware button trigger prefs (all in app_prefs, default off) ---
private const val KEY_TRIGGER_DOUBLE_VOL_UP = "trigger_double_vol_up"

/** Opacity of the floating bubble while it is docked (half hidden behind a screen edge). */
private const val DOCKED_BUBBLE_ALPHA = 0.45f
private const val KEY_TRIGGER_DOUBLE_VOL_DOWN = "trigger_double_vol_down"
private const val KEY_TRIGGER_DOUBLE_POWER = "trigger_double_power"
private const val KEY_TRIGGER_POWER_VOL_UP = "trigger_power_vol_up"
private const val KEY_TRIGGER_POWER_VOL_DOWN = "trigger_power_vol_down"
private const val KEY_TRIGGER_VOL_UP_DOWN = "trigger_vol_up_down"

// Two presses of a single button within this window count as a double-tap
private const val KEY_DOUBLE_TAP_MS = 350L
// After a trigger fires, ignore presses for this long so one gesture can't
// fire twice (e.g. the second tap of a double-tap counting again later)
private const val KEY_TRIGGER_COOLDOWN_MS = 900L

// Shape variants for the bottom-edge long-press overlays.
private enum class BottomEdgeShape { ARC_LEFT, ARC_RIGHT, SEMICIRCLE }

class CircleToSearchAccessibilityService : AccessibilityService() {

    private var windowManager: WindowManager? = null
    private val overlayViews = mutableListOf<View>() // Track all added segment views
    private val bottomEdgeViews = mutableListOf<BottomEdgeOverlayView>() // Track bottom-edge corner/mid overlays
    private val executor: Executor = Executors.newSingleThreadExecutor()
    private lateinit var configManager: OverlayConfigurationManager

    private var translatorBridge: TranslatorOverlayBridge? = null
    
    // Bubble related - Keeping existing logic but refactoring slightly if needed
    // For now, keeping bubble separate as requested in prompt "statusbar overlay customization... but it should work normally like now"
    // The prompt asks to disable statusbar overlay in landscape but keep it working normally.
    
    private var bubbleView: View? = null
    private val prefs by lazy { getSharedPreferences("app_prefs", Context.MODE_PRIVATE) }
    private val overlayPrefs by lazy { getSharedPreferences("overlay_prefs", Context.MODE_PRIVATE) } // Watch overlay prefs too
    
    private val prefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == "bubble_enabled") {
            updateBubbleState()
        }
    }
    
    private val overlayPrefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, _ ->
        // On any overlay config change, rebuild the overlay
        updateOverlay()
        updateBottomEdgeOverlays()
    }

    override fun onServiceConnected() {
        super.onServiceConnected()
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        configManager = OverlayConfigurationManager(this)
        
        // node info from other windows without touching AndroidManifest.xml.
        // Also enable enhanced web accessibility for better WebView coverage.
        val info = serviceInfo
        info.flags = info.flags or 
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS or
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_REQUEST_ENHANCED_WEB_ACCESSIBILITY or
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_INCLUDE_NOT_IMPORTANT_VIEWS or
            // Route volume/power keys through onKeyEvent so hardware triggers work
            android.accessibilityservice.AccessibilityServiceInfo.FLAG_REQUEST_FILTER_KEY_EVENTS
        serviceInfo = info
        
        prefs.registerOnSharedPreferenceChangeListener(prefsListener)
        overlayPrefs.registerOnSharedPreferenceChangeListener(overlayPrefsListener)

        val wm = windowManager
        if (wm != null) {
            runCatching {
                TranslatorOverlayBridge(this, wm).also {
                    translatorBridge = it
                    it.start()
                }
            }.onFailure {
                android.util.Log.e("CircleToSearch", "Translator overlay bridge failed to start", it)
                translatorBridge = null
            }
        }

        updateBubbleState()
        updateOverlay()
        updateBottomEdgeOverlays()
    }
    
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        updateOverlay()
        updateBottomEdgeOverlays()
    }

    private fun updateBubbleState() {
        if (prefs.getBoolean("bubble_enabled", false)) {
            showBubble()
        } else {
            hideBubble()
        }
    }

    private fun showBubble() {
        if (bubbleView != null) return // Already shown

        val params = WindowManager.LayoutParams(
            100, 100,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        )
        params.gravity = Gravity.TOP or Gravity.START
        // Start docked at the left edge: half the bubble hidden, dimmed — it only
        // takes half its footprint of visible screen space while at rest.
        params.x = -(params.width / 2)
        params.y = 200

        bubbleView = View(this).apply {
            setBackgroundResource(R.mipmap.ic_launcher)
            elevation = 10f
            alpha = DOCKED_BUBBLE_ALPHA

            var initialX = 0
            var initialY = 0
            var initialTouchX = 0f
            var initialTouchY = 0f
            var dragged = false
            var screenW = 0
            var screenH = 0
            var dockAnimator: ValueAnimator? = null

            fun screenBounds(): Pair<Int, Int> {
                val dm = resources.displayMetrics
                val bounds = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                    windowManager?.maximumWindowMetrics?.bounds
                } else {
                    null
                }
                return (bounds?.width() ?: dm.widthPixels) to (bounds?.height() ?: dm.heightPixels)
            }

            fun updateLayout() {
                runCatching { windowManager?.updateViewLayout(this, params) }
            }

            // Slide to the nearest side edge, half hidden behind it, dimmed.
            fun dockToNearestEdge(animate: Boolean) {
                if (screenW <= 0) {
                    val (w, h) = screenBounds()
                    screenW = w
                    screenH = h
                }
                val half = params.width / 2
                val targetX = if (params.x + params.width / 2 < screenW / 2) -half else screenW - half
                val targetY = params.y.coerceIn(0, (screenH - params.height).coerceAtLeast(0))
                alpha = DOCKED_BUBBLE_ALPHA
                dockAnimator?.cancel()
                if (!animate || (params.x == targetX && params.y == targetY)) {
                    params.x = targetX
                    params.y = targetY
                    updateLayout()
                    return
                }
                val startX = params.x
                val startY = params.y
                dockAnimator = ValueAnimator.ofFloat(0f, 1f).apply {
                    duration = 180
                    interpolator = DecelerateInterpolator()
                    addUpdateListener {
                        params.x = (startX + (targetX - startX) * it.animatedFraction).toInt()
                        params.y = (startY + (targetY - startY) * it.animatedFraction).toInt()
                        updateLayout()
                    }
                    start()
                }
            }

            @SuppressLint("ClickableViewAccessibility")
            setOnTouchListener { _, event ->
                when (event.action) {
                    MotionEvent.ACTION_DOWN -> {
                        dockAnimator?.cancel()
                        initialX = params.x
                        initialY = params.y
                        initialTouchX = event.rawX
                        initialTouchY = event.rawY
                        dragged = false
                        val (w, h) = screenBounds()
                        screenW = w
                        screenH = h
                        alpha = 1f
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        val dx = event.rawX - initialTouchX
                        val dy = event.rawY - initialTouchY
                        if (!dragged && (Math.abs(dx) >= 10 || Math.abs(dy) >= 10)) dragged = true
                        val half = params.width / 2
                        params.x = (initialX + dx.toInt()).coerceIn(-half, screenW - half)
                        params.y = (initialY + dy.toInt())
                            .coerceIn(0, (screenH - params.height).coerceAtLeast(0))
                        updateLayout()
                        true
                    }
                    MotionEvent.ACTION_UP -> {
                        if (!dragged) {
                            performCapture()
                        }
                        // Never leave it floating mid-screen: after every release it
                        // ends up docked at the nearest side edge (half hidden, dimmed).
                        dockToNearestEdge(animate = true)
                        true
                    }
                    else -> false
                }
            }
        }

        try {
            windowManager?.addView(bubbleView, params)
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun hideBubble() {
        if (bubbleView != null) {
            try {
                windowManager?.removeView(bubbleView)
            } catch (e: Exception) {
                e.printStackTrace()
            }
            bubbleView = null
        }
    }

    private fun updateOverlay() {
        val config = configManager.getConfig()
        
        // Keep strips alive while their settings dropdown is open (preview), even
        // when the master switch is off, and tint them so slider adjustments are
        // visible live (normal running strips are fully transparent).
        val previewing = config.previewVisible
        if (!config.isEnabled && !config.previewVisible) {
            overlayViews.forEach { 
                try { windowManager?.removeView(it) } catch(e: Exception) {} 
            }
            overlayViews.clear()
            return
        }

        // --- Resolution of Colors ---
        val primaryColor = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            getColor(android.R.color.system_accent1_500)
        } else {
            Color.parseColor("#FF6200EE") // Default Material Primary
        }

        val themePalette = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            listOf(
                android.R.color.system_accent1_200,
                android.R.color.system_accent2_200,
                android.R.color.system_accent3_200,
                android.R.color.system_neutral1_200,
                android.R.color.system_neutral2_200
            ).map { 
                val color = getColor(it)
                // Set alpha to 40% (approx 102/255) for subtle blending
                Color.argb(102, Color.red(color), Color.green(color), Color.blue(color))
            }
        } else {
            // Legacy fallbacks with 40% opacity
            listOf(
                Color.parseColor("#66FF0000"), // Red
                Color.parseColor("#6600FF00"), // Green
                Color.parseColor("#660000FF"), // Blue
                Color.parseColor("#66FFFF00"), // Yellow
                Color.parseColor("#66FF00FF")  // Magenta
            )
        }
        
        // Landscape check
        val currentOrientation = resources.configuration.orientation
        if (currentOrientation == Configuration.ORIENTATION_LANDSCAPE && !config.isEnabledInLandscape) {
             overlayViews.forEach { 
                try { windowManager?.removeView(it) } catch(e: Exception) {} 
            }
            overlayViews.clear()
            return
        }

        // --- OPTIMIZATION: Diff Update to prevent flashing ---
        // If the number of segments matches, we try to update existing views' LayoutParams
        // If not, we rebuild.
        
        if (overlayViews.size == config.segments.size) {
            // Update mode
            config.segments.forEachIndexed { index, segment ->
                val view = overlayViews[index]
                val params = view.layoutParams as WindowManager.LayoutParams
                
                // Update params
                var changed = false
                if (params.width != segment.width) { params.width = segment.width; changed = true }
                if (params.height != segment.height) { params.height = segment.height; changed = true }
                if (params.x != segment.xOffset) { params.x = segment.xOffset; changed = true }
                // Keep strips that sit at the very top (status-bar trigger) 3dp below the edge.
                val targetY = segment.yOffset +
                        if (segment.yOffset == 0) (3 * resources.displayMetrics.density).toInt() else 0
                if (params.y != targetY) { params.y = targetY; changed = true }
                
                if (changed) {
                    try {
                        windowManager?.updateViewLayout(view, params)
                    } catch (e: Exception) {
                        // Fallback implies view might be detached, shouldn't happen commonly
                    }
                }
                
                // Update Color (Debug / dropdown preview)
                if (config.isVisible || previewing) {
                    val color = themePalette[index % themePalette.size]
                    val drawable = GradientDrawable().apply {
                        setColor(color)
                        // Only show solid primary border for the expanded/active overlay
                        if (index == config.activeSegmentIndex) {
                            setStroke(3, primaryColor)
                        }
                    }
                    view.background = drawable
                    view.elevation = 0f
                } else {
                    view.background = null
                    view.setBackgroundColor(Color.TRANSPARENT)
                    view.elevation = 0f
                }
                
                // Update gesture listener
                // Since we created the detector in the loop, we can't easily "update" its inner logic if it closes over the *old* segment.
                // WE MUST re-attach the listener or make the listener dynamic.
                // The cleanest way is to just attach a NEW listener wrapper that reads the LATEST segment config.
                // But `segment` here is from the new config.
                // Creating a new detector is cheap.
                attachTouchListener(view, segment, index)
            }
        } else {
            // Rebuild mode (Count changed)
            overlayViews.forEach { 
                try { windowManager?.removeView(it) } catch(e: Exception) {} 
            }
            overlayViews.clear()
            
            config.segments.forEachIndexed { index, segment ->
                val view = View(this)
                val params = WindowManager.LayoutParams(
                    segment.width,
                    segment.height,
                    WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                    WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                            WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                            WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                            WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                    PixelFormat.TRANSLUCENT
                )
                
                params.gravity = Gravity.TOP or Gravity.START
                params.x = segment.xOffset
                // Keep strips that sit at the very top (status-bar trigger) 3dp below the edge.
                params.y = segment.yOffset +
                        if (segment.yOffset == 0) (3 * resources.displayMetrics.density).toInt() else 0
                
                 if (config.isVisible || previewing) {
                    val color = themePalette[index % themePalette.size]
                    val drawable = GradientDrawable().apply {
                        setColor(color)
                        // Only show solid primary border for the expanded/active overlay
                        if (index == config.activeSegmentIndex) {
                            setStroke(3, primaryColor)
                        }
                    }
                    view.background = drawable
                    view.elevation = 0f
                } else {
                    view.background = null
                    view.setBackgroundColor(Color.TRANSPARENT)
                    view.elevation = 0f
                }

                attachTouchListener(view, segment, index)
                
                try {
                    windowManager?.addView(view, params)
                    overlayViews.add(view)
                } catch (e: Exception) {
                    e.printStackTrace()
                }
            }
        }
    }

    /**
     * Bottom-edge long-press triggers (two corner arcs + optional bottom-centre semicircle).
     * Deliberately kept in their own list and NOT gated by [OverlayConfig.isEnabled], so
     * toggling the status-bar overlay can never add/remove them (previously they shared the
     * segment list and got wiped together).
     */
    private fun updateBottomEdgeOverlays() {
        val wm = windowManager ?: return
        val cfg = configManager.getBottomEdgeConfig()

        // Drop everything and rebuild: simplest correct approach for max 3 views.
        bottomEdgeViews.forEach { view ->
            try { wm.removeView(view) } catch (_: Exception) {}
        }
        bottomEdgeViews.clear()

        val dm = resources.displayMetrics
        val density = dm.density
        // Full display bounds (displayMetrics can exclude bars, which made the overlays
        // float or sit off-screen, clipping the right corner's border).
        val bounds = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            wm.maximumWindowMetrics.bounds
        } else {
            null
        }
        val screenW = bounds?.width() ?: dm.widthPixels
        val screenH = bounds?.height() ?: dm.heightPixels

        val accent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            getColor(android.R.color.system_accent1_400)
        } else {
            Color.parseColor("#FF6200EE")
        }

        fun addView(view: BottomEdgeOverlayView, w: Int, h: Int, x: Int, y: Int) {
            val params = WindowManager.LayoutParams(
                w, h,
                WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                        WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                        WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                        WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
                PixelFormat.TRANSLUCENT
            )
            params.gravity = Gravity.TOP or Gravity.START
            params.x = x
            params.y = y
            try {
                wm.addView(view, params)
                bottomEdgeViews.add(view)
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }

        val cornerSizePx = (cfg.cornerSizeDp * density).toInt().coerceAtLeast(48)
        val midSizePx = (cfg.midSizeDp * density).toInt().coerceAtLeast(48)
        // Flush with the very bottom of the screen (no nav-bar inset).
        val bottomBase = screenH

        if (cfg.leftEnabled) {
            addView(
                BottomEdgeOverlayView(this, BottomEdgeShape.ARC_LEFT, cfg.previewVisible, accent),
                cornerSizePx, cornerSizePx, 0, bottomBase - cornerSizePx,
            )
        }
        if (cfg.rightEnabled) {
            addView(
                BottomEdgeOverlayView(this, BottomEdgeShape.ARC_RIGHT, cfg.previewVisible, accent),
                cornerSizePx, cornerSizePx, screenW - cornerSizePx, bottomBase - cornerSizePx,
            )
        }
        if (cfg.midEnabled) {
            addView(
                BottomEdgeOverlayView(this, BottomEdgeShape.SEMICIRCLE, cfg.previewVisible, accent),
                midSizePx * 2, midSizePx, (screenW - midSizePx * 2) / 2, bottomBase - midSizePx,
            )
        }
    }
    
    @SuppressLint("ClickableViewAccessibility")
    private fun attachTouchListener(view: View, segment: OverlaySegment, segmentIndex: Int) {
        // Slow drags and taps on an overlay must still reach the system UI below
        // (notification shade / quick settings on the status bar, the app under a strip).
        var lastScrollForwardAt = 0L
        var scrollForwardedThisGesture = false
        val gestureDetector = GestureDetector(this, object : GestureDetector.SimpleOnGestureListener() {
            override fun onDoubleTap(e: MotionEvent): Boolean {
                val action = segment.gestures[GestureType.DOUBLE_TAP] ?: ActionType.NONE
                if (action != ActionType.NONE) { performAction(action, segment); return true }
                return false
            }
            
            override fun onLongPress(e: MotionEvent) {
                val action = segment.gestures[GestureType.LONG_PRESS] ?: ActionType.NONE
                if (action != ActionType.NONE) performAction(action, segment)
            }

            override fun onScroll(e1: MotionEvent?, e2: MotionEvent, distanceX: Float, distanceY: Float): Boolean {
                if (e1 == null) return false
                val dx = e2.rawX - e1.rawX
                val dy = e2.rawY - e1.rawY
                // Only treat this as a real drag once the finger has clearly moved — tiny
                // jitter during a tap/double-tap must NOT be forwarded (forwarding briefly
                // makes the window untouchable and swallows the second tap).
                val minForwardDist = 40 * resources.displayMetrics.density
                if (Math.abs(dx) < minForwardDist && Math.abs(dy) < minForwardDist) return false
                val mostlyVertical = Math.abs(dy) > Math.abs(dx)
                val direction = when {
                    mostlyVertical && dy > 0 -> GestureType.SWIPE_DOWN
                    mostlyVertical && dy < 0 -> GestureType.SWIPE_UP
                    dx > 0 -> GestureType.SWIPE_RIGHT
                    else -> GestureType.SWIPE_LEFT
                }
                // A configured custom action wins: don't forward, let the fling handler trigger it.
                val custom = segment.gestures[direction]
                if (custom != null && custom != ActionType.NONE) return false
                // Otherwise forward the drag so the system below still tracks it
                // (e.g. pulling down the notification shade / quick settings on the status bar).
                val now = android.os.SystemClock.uptimeMillis()
                if (now - lastScrollForwardAt < 180L) return true
                lastScrollForwardAt = now
                scrollForwardedThisGesture = true
                val path = android.graphics.Path().apply {
                    moveTo(e1.rawX, e1.rawY)
                    lineTo(e2.rawX, e2.rawY)
                }
                forwardPassthroughGesture(view, path, 140)
                return true
            }

            override fun onSingleTapConfirmed(e: MotionEvent): Boolean {
                 // User wants "buttons behind to be clickable" 
                 // We temporarily disable touch on our window and dispatch the click through.
                 propagateSingleTap(view, e.rawX, e.rawY)
                 return false
            }
            
            override fun onFling(e1: MotionEvent?, e2: MotionEvent, velocityX: Float, velocityY: Float): Boolean {
                if (e1 == null) return false
                // This drag was already forwarded to the system below; don't also fire
                // a global action (would open the shade a second time).
                if (scrollForwardedThisGesture) return true
                val diffY = e2.y - e1.y
                val diffX = e2.x - e1.x
                
                if (Math.abs(diffX) > Math.abs(diffY)) {
                    if (Math.abs(diffX) > 100 && Math.abs(velocityX) > 100) {
                        if (diffX > 0) {
                             // Swipe Right
                             val action = segment.gestures[GestureType.SWIPE_RIGHT] ?: ActionType.NONE
                             if (action != ActionType.NONE) { performAction(action, segment); return true }
                        } else {
                            // Swipe Left
                            val action = segment.gestures[GestureType.SWIPE_LEFT] ?: ActionType.NONE
                             if (action != ActionType.NONE) { performAction(action, segment); return true }
                        }
                    }
                } else {
                    // Reduced threshold for vertical swipes to work with smaller overlay heights
                    if (Math.abs(diffY) > 50 && Math.abs(velocityY) > 100) {
                        if (diffY > 0) {
                             // Swipe Down
                             android.util.Log.d("CTS_Swipe", "Swipe DOWN detected - segmentIndex=$segmentIndex, diffY=$diffY, velocityY=$velocityY")
                             val action = segment.gestures[GestureType.SWIPE_DOWN] ?: ActionType.NONE
                             if (action != ActionType.NONE) {
                                 android.util.Log.d("CTS_Swipe", "Custom action assigned: $action")
                                 performAction(action, segment) 
                             } else {
                                 // Smart Swipe Logic: Only apply for first overlay (index 0) when it's full width
                                 val screenWidth = resources.displayMetrics.widthPixels
                                 val isFirstOverlay = segmentIndex == 0
                                 val isFullWidth = segment.width >= screenWidth
                                 
                                 android.util.Log.d("CTS_Swipe", "Smart swipe check - isFirstOverlay=$isFirstOverlay, isFullWidth=$isFullWidth (width=${segment.width}, screenWidth=$screenWidth)")
                                 
                                 if (isFirstOverlay && isFullWidth) {
                                     // Smart logic: Check where user actually swiped (touch X position)
                                     // Left half of screen = Notifications, Right half = Quick Settings
                                     val touchX = e1.rawX
                                     android.util.Log.d("CTS_Swipe", "Smart swipe active - touchX=$touchX, screenWidth/2=${screenWidth/2}")
                                     if (touchX < (screenWidth / 2)) {
                                         android.util.Log.d("CTS_Swipe", "Opening NOTIFICATIONS (left half)")
                                         performGlobalAction(GLOBAL_ACTION_NOTIFICATIONS)
                                     } else {
                                         android.util.Log.d("CTS_Swipe", "Opening QUICK_SETTINGS (right half)")
                                         performGlobalAction(GLOBAL_ACTION_QUICK_SETTINGS)
                                     }
                                 } else {
                                     // Default: Always open notification shade
                                     android.util.Log.d("CTS_Swipe", "Default behavior - Opening NOTIFICATIONS")
                                     performGlobalAction(GLOBAL_ACTION_NOTIFICATIONS)
                                 }
                             }
                             return true
                        } else {
                            // Swipe Up
                             val action = segment.gestures[GestureType.SWIPE_UP] ?: ActionType.NONE
                             if (action != ActionType.NONE) { performAction(action, segment); return true }
                        }
                    }
                }
                return false
            }
        }).apply {
             setOnDoubleTapListener(object : GestureDetector.OnDoubleTapListener {
                override fun onSingleTapConfirmed(e: MotionEvent): Boolean {
                    // Forward single taps so status-bar taps still reach the system UI below.
                    propagateSingleTap(view, e.rawX, e.rawY)
                    return false
                }
                override fun onDoubleTap(e: MotionEvent): Boolean {
                    val action = segment.gestures[GestureType.DOUBLE_TAP] ?: ActionType.NONE
                    if (action != ActionType.NONE) { performAction(action, segment); return true }
                    return false
                }
                override fun onDoubleTapEvent(e: MotionEvent): Boolean = false
            })
        }
        
        var lastTapTime: Long = 0
        var tapCount = 0
        
        view.setOnTouchListener { _, event ->
             if (event.action == MotionEvent.ACTION_DOWN) {
                scrollForwardedThisGesture = false
                val currentTime = System.currentTimeMillis()
                if (currentTime - lastTapTime < 400) {
                    tapCount++
                } else {
                    tapCount = 1
                }
                lastTapTime = currentTime
                
                if (tapCount == 3) {
                     val action = segment.gestures[GestureType.TRIPLE_TAP] ?: ActionType.NONE
                     if (action != ActionType.NONE) {
                         performAction(action, segment)
                         tapCount = 0 
                         return@setOnTouchListener true
                     }
                }
            }
            gestureDetector.onTouchEvent(event)
            true
        }
    }
    
    private fun performAction(action: ActionType, segment: OverlaySegment) {
        if (action == ActionType.NONE) return
        
        // Haptic feedback for action trigger
        val vibrator = getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            vibrator.vibrate(VibrationEffect.createPredefined(VibrationEffect.EFFECT_CLICK))
        } else {
             @Suppress("DEPRECATION")
            vibrator.vibrate(10)
        }

        when(action) {
            ActionType.SCREENSHOT -> performGlobalAction(GLOBAL_ACTION_TAKE_SCREENSHOT)
            ActionType.FLASHLIGHT -> toggleFlashlight()
            ActionType.HOME -> performGlobalAction(GLOBAL_ACTION_HOME)
            ActionType.BACK -> performGlobalAction(GLOBAL_ACTION_BACK)
            ActionType.RECENTS -> performGlobalAction(GLOBAL_ACTION_RECENTS)
            ActionType.LOCK_SCREEN -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                performGlobalAction(GLOBAL_ACTION_LOCK_SCREEN)
            }
            ActionType.OPEN_NOTIFICATIONS -> performGlobalAction(GLOBAL_ACTION_NOTIFICATIONS)
            ActionType.OPEN_QUICK_SETTINGS -> performGlobalAction(GLOBAL_ACTION_QUICK_SETTINGS)
            ActionType.OPEN_APP -> {
                // Open App Logic
                val packageName = segment.gestureData[findGestureForAction(segment, ActionType.OPEN_APP)]
                if (!packageName.isNullOrEmpty()) {
                    val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
                    if (launchIntent != null) {
                        launchIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                        startActivity(launchIntent)
                    }
                }
            }
            ActionType.CTS_AUTO -> {
                 // Respect Global Preference
                 performCapture(null)
            }
            ActionType.CTS_LENS -> {
                 // Force Lens Mode for this session only
                 performCapture(true)
            }
            ActionType.CTS_MULTI -> {
                 // Force Multi Mode for this session only
                 performCapture(false)
            }
            ActionType.SPLIT_SCREEN -> {
                 val success = performGlobalAction(GLOBAL_ACTION_TOGGLE_SPLIT_SCREEN)
                 if (!success) {
                     android.widget.Toast.makeText(this, getString(R.string.toast_split_screen_failed), android.widget.Toast.LENGTH_SHORT).show()
                 }
            }
            ActionType.SCROLL_TOP -> performScroll(true)
            ActionType.SCROLL_BOTTOM -> performScroll(false)
            ActionType.SCREEN_OFF -> {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                    performGlobalAction(GLOBAL_ACTION_LOCK_SCREEN)
                } else {
                     android.widget.Toast.makeText(this, getString(R.string.toast_screen_off_requires_api), android.widget.Toast.LENGTH_SHORT).show()
                }
            }
            ActionType.TOGGLE_AUTO_ROTATE -> toggleAutoRotate()
            ActionType.MEDIA_PLAY_PAUSE -> injectMediaKey(android.view.KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE)
            ActionType.MEDIA_NEXT -> injectMediaKey(android.view.KeyEvent.KEYCODE_MEDIA_NEXT)
            ActionType.MEDIA_PREVIOUS -> injectMediaKey(android.view.KeyEvent.KEYCODE_MEDIA_PREVIOUS)
            else -> {}
        }
    }
    
    // Helpers for new actions
    
    private fun performScroll(toTop: Boolean) {
        android.util.Log.d("CTS_Scroll", "performScroll called - toTop=$toTop")
        
        // We simulate multiple quick swipes instead of one long one
        // This is more reliable and less likely to be cancelled
        val displayMetrics = resources.displayMetrics
        val centerX = displayMetrics.widthPixels / 2f
        
        // Perform 3 quick swipes with delays
        val handler = android.os.Handler(android.os.Looper.getMainLooper())
        
        for (i in 0..2) {
            handler.postDelayed({
                // Scroll To Top = Swipe DOWN (drag content down, revealing top)
                // Scroll To Bottom = Swipe UP (drag content up, revealing bottom)
                val startY = if (toTop) displayMetrics.heightPixels * 0.3f else displayMetrics.heightPixels * 0.7f
                val endY = if (toTop) displayMetrics.heightPixels * 0.7f else displayMetrics.heightPixels * 0.3f
                
                android.util.Log.d("CTS_Scroll", "Scroll swipe #${i+1} - toTop=$toTop, centerX=$centerX, startY=$startY, endY=$endY")
                
                val path = android.graphics.Path().apply {
                    moveTo(centerX, startY)
                    lineTo(centerX, endY)
                }
                // Shorter, faster swipes (200ms each)
                val stroke = android.accessibilityservice.GestureDescription.StrokeDescription(path, 0, 200)
                val gesture = android.accessibilityservice.GestureDescription.Builder().addStroke(stroke).build()
                
                val success = dispatchGesture(gesture, object : AccessibilityService.GestureResultCallback() {
                    override fun onCompleted(gestureDescription: android.accessibilityservice.GestureDescription?) {
                        android.util.Log.d("CTS_Scroll", "Scroll swipe #${i+1} COMPLETED")
                    }
                    
                    override fun onCancelled(gestureDescription: android.accessibilityservice.GestureDescription?) {
                        android.util.Log.e("CTS_Scroll", "Scroll swipe #${i+1} CANCELLED")
                    }
                }, null)
                
                android.util.Log.d("CTS_Scroll", "Scroll swipe #${i+1} dispatched: $success")
            }, i * 250L) // 250ms delay between each swipe
        }
    }
    
    private fun injectMediaKey(keyCode: Int) {
        val audioManager = getSystemService(Context.AUDIO_SERVICE) as android.media.AudioManager
        val eventTime = android.os.SystemClock.uptimeMillis()
        
        val downEvent = android.view.KeyEvent(eventTime, eventTime, android.view.KeyEvent.ACTION_DOWN, keyCode, 0)
        val upEvent = android.view.KeyEvent(eventTime, eventTime, android.view.KeyEvent.ACTION_UP, keyCode, 0)
        
        audioManager.dispatchMediaKeyEvent(downEvent)
        audioManager.dispatchMediaKeyEvent(upEvent)
    }
    
    private fun toggleAutoRotate() {
        if (android.provider.Settings.System.canWrite(this)) {
            val current = android.provider.Settings.System.getInt(contentResolver, android.provider.Settings.System.ACCELEROMETER_ROTATION, 0)
            val next = if (current == 1) 0 else 1
            android.provider.Settings.System.putInt(contentResolver, android.provider.Settings.System.ACCELEROMETER_ROTATION, next)
            android.widget.Toast.makeText(this, getString(R.string.toast_auto_rotate, if (next == 1) getString(R.string.status_on) else getString(R.string.status_off)), android.widget.Toast.LENGTH_SHORT).show()
        } else {
             android.widget.Toast.makeText(this, getString(R.string.toast_auto_rotate_permission), android.widget.Toast.LENGTH_SHORT).show()
             val intent = android.content.Intent(android.provider.Settings.ACTION_MANAGE_WRITE_SETTINGS).apply {
                 data = android.net.Uri.parse("package:$packageName")
                 addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
             }
             startActivity(intent)
        }
    }
    
    private fun findGestureForAction(segment: OverlaySegment, action: ActionType): GestureType {
        return segment.gestures.entries.firstOrNull { it.value == action }?.key ?: GestureType.DOUBLE_TAP
    }
    
    // Pass-through Logic for Single Tap
    // Temporarily makes the window untouchable, injects the tap, and (via
    // forwardPassthroughGesture) guarantees the window is restored even if the
    // dispatch fails — a missing restore would leave the overlay dead.
    private fun propagateSingleTap(view: View, x: Float, y: Float) {
        val path = android.graphics.Path().apply { moveTo(x, y) }
        forwardPassthroughGesture(view, path, 50)
    }
    
    private fun toggleFlashlight() {
         try {
            val cameraManager = getSystemService(Context.CAMERA_SERVICE) as CameraManager
            val cameraId = cameraManager.cameraIdList[0]
            // This is tricky because we don't know current state easily without callback.
            // For now, let's assume valid flash.
            // A robust implementation needs a callback to track state.
            // We'll just try to turn it on for a second for testing or we need a tracked state.
            // Let's implement a simple tracking using static var or prefs?
            // Or just ignore toggle for now and just turn ON? No, user expects toggle.
            // Let's use a static state?
            if (isFlashlightOn) {
                cameraManager.setTorchMode(cameraId, false)
                isFlashlightOn = false
            } else {
                cameraManager.setTorchMode(cameraId, true)
                isFlashlightOn = true
            }
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun performCapture(searchModeOverride: Boolean? = null) {
        android.util.Log.d("CircleToSearch", "performCapture called. hasWindowManager=${windowManager != null}")
        
        // Clear repository at the source to prevent any "ghost" flash of old data
        BitmapRepository.clear()
        
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            takeScreenshot(
                Display.DEFAULT_DISPLAY,
                executor,
                object : TakeScreenshotCallback {
                    override fun onSuccess(screenshot: ScreenshotResult) {
                         try {
                            val hardwareBuffer = screenshot.hardwareBuffer
                            val colorSpace = screenshot.colorSpace
                            
                            val bitmap = Bitmap.wrapHardwareBuffer(hardwareBuffer, colorSpace)
                            if (bitmap == null) {
                                hardwareBuffer.close()
                                return
                            }

                            // Copy to software bitmap
                            val copy = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                            hardwareBuffer.close() // Close buffer after copy

                            if (copy == null) {
                                return
                            }
                            
                            // Store in Repository (In-Memory)
                            BitmapRepository.setScreenshot(copy)
                            
                            // Launch Overlay Immediately
                            launchOverlay(searchModeOverride)
                            
                        } catch (e: Exception) {
                            e.printStackTrace()
                        }
                    }

                    override fun onFailure(errorCode: Int) {
                        android.util.Log.e("CircleToSearch", "Screenshot failed with error code: $errorCode")
                    }
                }
            )
        }
    }

    fun launchOverlay(searchModeOverride: Boolean? = null) {
        android.util.Log.d("CircleToSearchAccess", "AccessibilityService launching OverlayActivity")
        val intent = Intent(this, OverlayActivity::class.java).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION) // Disable animation for faster feel
            searchModeOverride?.let { putExtra("EXTRA_SEARCH_MODE_OVERRIDE", it) }
        }
        startActivity(intent)
    }

    // Custom ImageView that clips to rounded corners on the Canvas level.
    // Draws the sticker into a content rect inset from the view bounds: the window
    // carries headroom margin during transforms (hold/drag/pinch/entrance springs)
    // so the fixed window surface never square-crops the rounded corners; at rest
    // the content rect equals the full window.
    private class RoundedImageView(context: android.content.Context) : android.widget.ImageView(context) {
        private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
        private val rect = RectF()
        private val matrix = Matrix()

        // Content size (px) drawn centered in the view. Derived at draw time from
        // the view's own bounds — never a stored offset — so a window relayout and
        // a redraw can never disagree: grow/shrink preserves the center, which makes
        // every intermediate draw pixel-identical to the settled state.
        var contentWidth = -1
        var contentHeight = -1

        fun setContentSize(w: Int, h: Int) {
            contentWidth = w
            contentHeight = h
            invalidateOutline()
            invalidate()
        }

        private fun contentLeft() = (width - minOf(contentWidth, width)) / 2
        private fun contentTop() = (height - minOf(contentHeight, height)) / 2
        private fun contentDrawW() = if (contentWidth <= 0) width else minOf(contentWidth, width)
        private fun contentDrawH() = if (contentHeight <= 0) height else minOf(contentHeight, height)

        override fun onDraw(canvas: android.graphics.Canvas) {
            val drawable = drawable ?: return
            if (contentWidth <= 0 || contentHeight <= 0) return
            val bitmap = try {
                drawable.toBitmap()
            } catch (e: Exception) {
                return
            }

            val cw = contentDrawW()
            val ch = contentDrawH()
            val left = contentLeft()
            val top = contentTop()

            val shader = BitmapShader(bitmap, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)

            matrix.reset()
            matrix.setScale(cw.toFloat() / bitmap.width, ch.toFloat() / bitmap.height)
            matrix.postTranslate(left.toFloat(), top.toFloat())
            shader.setLocalMatrix(matrix)

            paint.shader = shader
            rect.set(
                left.toFloat(),
                top.toFloat(),
                (left + cw).toFloat(),
                (top + ch).toFloat()
            )

            val radius = stickerCornerRadius(cw, ch, context.resources.displayMetrics.density)
            canvas.drawRoundRect(rect, radius, radius, paint)
        }
    }

    private val stickerAnimScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    private val stickerFrameClock = object : MonotonicFrameClock {
        override suspend fun <R> withFrameNanos(onFrame: (Long) -> R): R =
            suspendCoroutine { cont ->
                android.view.Choreographer.getInstance().postFrameCallback { frameTimeNanos ->
                    cont.resume(onFrame(frameTimeNanos))
                }
            }
    }

    /**
     * Drives Compose animation springs on plain Views (no Composition needed): the
     * Choreographer-backed frame clock lets [animate] run against WindowManager views.
     * [expressive] uses the M3 MotionScheme.expressive() spatial spring spec.
     */
    @OptIn(ExperimentalMaterial3ExpressiveApi::class)
    private fun springAnim(
        from: Float,
        to: Float,
        dampingRatio: Float,
        stiffness: Float,
        expressive: Boolean = false,
        onEnd: (() -> Unit)? = null,
        onFrame: (Float) -> Unit
    ): Job =
        stickerAnimScope.launch(stickerFrameClock) {
            val spec: androidx.compose.animation.core.AnimationSpec<Float> =
                if (expressive) MotionScheme.expressive().defaultSpatialSpec()
                else spring(dampingRatio = dampingRatio, stiffness = stiffness)
            animate(from, to, animationSpec = spec) { value, _ -> onFrame(value) }
            onEnd?.invoke()
        }

    /**
     * Bakes rounded corners into the sticker bitmap itself so the crop survives
     * any expansion (grab/pinch springs) or renderer-side clipping: the corners
     * are transparent pixels, not a per-frame drawing decision.
     */
    private fun cropBitmapToRoundedCorners(src: Bitmap, radiusPx: Float): Bitmap {
        if (radiusPx <= 0f) return src
        return try {
            val out = Bitmap.createBitmap(src.width, src.height, Bitmap.Config.ARGB_8888)
            val canvas = android.graphics.Canvas(out)
            val paint = Paint(Paint.ANTI_ALIAS_FLAG)
            paint.shader = BitmapShader(src, Shader.TileMode.CLAMP, Shader.TileMode.CLAMP)
            canvas.drawRoundRect(
                RectF(0f, 0f, src.width.toFloat(), src.height.toFloat()),
                radiusPx, radiusPx, paint
            )
            out
        } catch (e: Exception) {
            src
        }
    }

    /**
     * Headroom (px) to add around the sticker content so scale/rotation transforms
     * (grab spring, drag tilt, entrance/pinch overshoot) stay fully inside the window
     * surface instead of being square-cropped at its fixed bounds.
     */
    private fun stickerMargins(cw: Int, ch: Int, sMax: Float, thetaDeg: Float): Pair<Int, Int> {
        if (sMax <= 0f) return 0 to 0
        val th = Math.toRadians(thetaDeg.toDouble())
        val ct = Math.abs(Math.cos(th)).toFloat()
        val st = Math.abs(Math.sin(th)).toFloat()
        val ex = sMax * (cw / 2f * ct + ch / 2f * st)
        val ey = sMax * (cw / 2f * st + ch / 2f * ct)
        val mx = (ex - cw / 2f).toInt() + 8
        val my = (ey - ch / 2f).toInt() + 8
        return mx.coerceAtLeast(0) to my.coerceAtLeast(0)
    }

    private fun showPinnedArea(bitmap: Bitmap, rect: android.graphics.Rect) {
        android.util.Log.d("CircleToSearch", "showPinnedArea called for rect: $rect")
        
        val displayMetrics = resources.displayMetrics
        val screenWidth = displayMetrics.widthPixels
        val screenHeight = displayMetrics.heightPixels

        // Initial Position: Center of the selection
        val centerX = rect.centerX()
        val centerY = rect.centerY()
        
        // --- Phase 32: 1:1 Scaling ---
        // Use natural dimensions of the cropped bitmap (match what user saw in lens)
        val width = bitmap.width
        val height = bitmap.height

        val params = WindowManager.LayoutParams(
            width, height,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT
        )
        params.gravity = Gravity.TOP or Gravity.START
        params.x = centerX - width / 2
        params.y = centerY - height / 2

        var entranceScaleXJob: Job? = null
        var entranceScaleYJob: Job? = null
        var scaleJob: Job? = null
        var rotJob: Job? = null
        var alphaJob: Job? = null
        var lastTiltRawX = 0f
        var lastTiltTime = 0L
        var contentW = width
        var contentH = height
        var targetTilt = 0f
        var tiltSmoothJob: Job? = null

        // --- Two-window architecture ---
        // The window surface clips at its fixed bounds, and any window RESIZE
        // during motion start/stop flashes on this OEM (surface transition). So
        // the sticker is split into two windows that are created once and then
        // only ever moved together:
        //   V (visual) = RoundedImageView grown with transform headroom,
        //                FLAG_NOT_TOUCHABLE, added first; carries all springs.
        //   T (touch)  = plain content-sized transparent View that owns every
        //                touch handler, added on top; exactly the sticker
        //                content, so there is no invisible ring around it.
        // Drag start / release / fling start-end perform ZERO window resizes —
        // the only resize left is a live pinch, which the user is driving anyway.
        val headroomScale = 1.35f
        val headroomTilt = 15f
        var marginX = 0
        var marginY = 0
        run {
            val (mx, my) = stickerMargins(width, height, headroomScale, headroomTilt)
            marginX = mx
            marginY = my
        }

        val visualParams = WindowManager.LayoutParams(
            width + 2 * marginX,
            height + 2 * marginY,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT
        )
        visualParams.gravity = Gravity.TOP or Gravity.START
        visualParams.x = params.x - marginX
        visualParams.y = params.y - marginY

        lateinit var pinnedView: RoundedImageView
        lateinit var touchView: View

        // V is always centered on T: one call keeps both windows in sync.
        fun moveWindows(x: Int, y: Int) {
            params.x = x
            params.y = y
            visualParams.x = x - marginX
            visualParams.y = y - marginY
            try {
                if (touchView.isAttachedToWindow) windowManager?.updateViewLayout(touchView, params)
                if (pinnedView.isAttachedToWindow) windowManager?.updateViewLayout(pinnedView, visualParams)
            } catch (e: Exception) {}
        }

        // Crop corners into the bitmap (view is 1:1 with bitmap, so radius maps directly)
        val stickerRadiusPx = stickerCornerRadius(bitmap.width, bitmap.height, displayMetrics.density)
        val stickerBitmap = cropBitmapToRoundedCorners(bitmap, stickerRadiusPx)

        pinnedView = RoundedImageView(this).apply {
            setImageBitmap(stickerBitmap)
            setContentSize(bitmap.width, bitmap.height)
            scaleType = android.widget.ImageView.ScaleType.FIT_CENTER
            elevation = 0f
            // Round-rect clip on the content rect (not the window): with headroom
            // margin the clip transforms with the view, so no expansion can square it.
            outlineProvider = object : android.view.ViewOutlineProvider() {
                override fun getOutline(v: View, o: android.graphics.Outline) {
                    if (v.width <= 0 || v.height <= 0 || contentWidth <= 0 || contentHeight <= 0) return
                    val cw = minOf(contentWidth, v.width)
                    val ch = minOf(contentHeight, v.height)
                    val l = (v.width - cw) / 2
                    val t = (v.height - ch) / 2
                    o.setRoundRect(
                        l, t, l + cw, t + ch,
                        stickerCornerRadius(cw, ch, resources.displayMetrics.density)
                    )
                }
            }
            clipToOutline = true
        }

        // --- Touch window (T): content-sized, transparent, owns every gesture ---
        touchView = View(this).apply {
            var initialX = 0
            var initialY = 0
            var initialTouchX = 0f
            var initialTouchY = 0f
            var isDragging = false
            var isScaling = false
            var currentMenu: View? = null
            
            // --- Phase 45: Sticker Physics ---
            var velocityTracker: android.view.VelocityTracker? = null
            var flingAnimator: android.animation.ValueAnimator? = null
            
            val stopFling = {
                flingAnimator?.cancel()
                flingAnimator = null
            }

            // --- Phase 33: ScaleGestureDetector for pinch zoom ---
            val scaleDetector = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
                override fun onScale(detector: ScaleGestureDetector): Boolean {
                    isScaling = true
                    val scaleFactor = detector.scaleFactor

                    val newContentW = (contentW * scaleFactor).toInt()
                    val newContentH = (contentH * scaleFactor).toInt()

                    // Constraints: 15% to 95% of screen
                    val minDim = (screenWidth * 0.15f).toInt()
                    val maxDim = (screenWidth * 0.95f).toInt()

                    if (newContentW in minDim..maxDim && newContentH in minDim..maxDim) {
                        // Keep the point under the pinch focus fixed on screen
                        val focusX = detector.focusX
                        val focusY = detector.focusY
                        val oldW = params.width
                        val oldH = params.height

                        val (nmx, nmy) = stickerMargins(newContentW, newContentH, headroomScale, headroomTilt)

                        params.x -= ((newContentW - oldW) * (focusX / oldW)).toInt()
                        params.y -= ((newContentH - oldH) * (focusY / oldH)).toInt()

                        contentW = newContentW
                        contentH = newContentH
                        marginX = nmx
                        marginY = nmy
                        params.width = newContentW
                        params.height = newContentH
                        visualParams.width = newContentW + 2 * nmx
                        visualParams.height = newContentH + 2 * nmy
                        visualParams.x = params.x - marginX
                        visualParams.y = params.y - marginY
                        pinnedView.setContentSize(newContentW, newContentH)
                        try {
                            windowManager?.updateViewLayout(this@apply, params)
                            windowManager?.updateViewLayout(pinnedView, visualParams)
                        } catch (e: Exception) {}
                    }
                    return true
                }
            })

            // --- Phase 31: GestureDetector for reliable long-press ---
            val gestureDetector = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
                override fun onLongPress(e: MotionEvent) {
                    if (!isDragging) {
                        // Show actions
                        if (currentMenu == null) {
                            showPinnedActions(pinnedView, touchView, bitmap, params, params.y, contentH) { menu ->
                                currentMenu = menu
                            }
                        }
                    }
                }
            })

            // Dead zone: the sticker only moves/tilts once the finger exceeds the
            // system touch slop, so a trembling hold still reaches the long-press
            // menu instead of creeping or entering drag mode.
            val dragSlop = android.view.ViewConfiguration.get(context).scaledTouchSlop

            @SuppressLint("ClickableViewAccessibility")
            setOnTouchListener { v, event ->
                // Feed both detectors
                scaleDetector.onTouchEvent(event)
                gestureDetector.onTouchEvent(event)
                
                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN -> {
                        tiltSmoothJob?.cancel()
                        tiltSmoothJob = null
                        targetTilt = 0f
                        // A bare touch must be completely inert: no window resize,
                        // no scale pop, no position change. Every reaction starts
                        // only after the drag dead zone is crossed; the long-press
                        // menu comes from the gesture detector.
                        initialX = params.x
                        initialY = params.y
                        initialTouchX = event.rawX
                        initialTouchY = event.rawY
                        lastTiltRawX = event.rawX
                        lastTiltTime = event.eventTime
                        isDragging = false
                        isScaling = false
                        stopFling()
                        velocityTracker?.recycle()
                        velocityTracker = android.view.VelocityTracker.obtain()
                        velocityTracker?.addMovement(event)
                        true
                    }
                    MotionEvent.ACTION_POINTER_DOWN -> {
                        isScaling = true
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        if (isScaling) return@setOnTouchListener true

                        val dx = (event.rawX - initialTouchX).toInt()
                        val dy = (event.rawY - initialTouchY).toInt()
                        if (!isDragging && (Math.abs(dx) > dragSlop || Math.abs(dy) > dragSlop)) {
                            isDragging = true
                            // Dead zone crossed: rebase so the sticker doesn't jump
                            // by the slop amount when the drag actually starts.
                            // No window operations here at all — V already carries
                            // permanent headroom, so motion start can't flicker.
                            initialX = params.x
                            initialY = params.y
                            initialTouchX = event.rawX
                            initialTouchY = event.rawY
                            // If dragging, dismiss menu
                            currentMenu?.let { try { windowManager?.removeView(it) } catch(e: Exception) {} }
                            currentMenu = null
                            // Take over from any entrance springs (UP settles scale
                            // back to 1). No grab-pop: the sticker must not visually
                            // jump when a drag starts or when it's released.
                            entranceScaleXJob?.cancel()
                            entranceScaleYJob?.cancel()
                            scaleJob?.cancel()
                            rotJob?.cancel()
                        }

                        velocityTracker?.addMovement(event)
                        if (isDragging) {
                            moveWindows(
                                initialX + (event.rawX - initialTouchX).toInt(),
                                initialY + (event.rawY - initialTouchY).toInt()
                            )
                            // Playful lean toward the drag direction. The per-event
                            // velocity is noisy, so it only nudges a target value
                            // (low-pass); a per-frame smoothing loop eases rotation
                            // toward that target so the tilt flows instead of flickers.
                            val tiltDt = (event.eventTime - lastTiltTime).coerceAtLeast(1L)
                            val tiltVx = (event.rawX - lastTiltRawX) / tiltDt * 1000f
                            lastTiltRawX = event.rawX
                            lastTiltTime = event.eventTime
                            val rawTilt = (tiltVx * 0.006f).coerceIn(-8f, 8f)
                            targetTilt += (rawTilt - targetTilt) * 0.3f
                            if (tiltSmoothJob == null) {
                                tiltSmoothJob = stickerAnimScope.launch(stickerFrameClock) {
                                    while (true) {
                                        stickerFrameClock.withFrameNanos {
                                            // Finger resting: ease back toward upright
                                            if (android.os.SystemClock.uptimeMillis() - lastTiltTime > 150) {
                                                targetTilt *= 0.82f
                                            }
                                            pinnedView.rotation += (targetTilt - pinnedView.rotation) * 0.3f
                                        }
                                    }
                                }
                            }
                        }
                        true
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                        isScaling = false
                        tiltSmoothJob?.cancel()
                        tiltSmoothJob = null
                        // Settle: bouncy springs back to rest (scale + tilt).
                        // No window resize on release — V keeps its headroom forever.
                        scaleJob?.cancel()
                        rotJob?.cancel()
                        scaleJob = springAnim(pinnedView.scaleX, 1f, 0.5f, 300f) {
                            pinnedView.scaleX = it
                            pinnedView.scaleY = it
                        }
                        rotJob = springAnim(pinnedView.rotation, 0f, 0.4f, 260f) { pinnedView.rotation = it }
                        if (isDragging) {
                            velocityTracker?.computeCurrentVelocity(1000)
                            val vx = velocityTracker?.xVelocity ?: 0f
                            val vy = velocityTracker?.yVelocity ?: 0f
                            
                            if (Math.abs(vx) > 150 || Math.abs(vy) > 150) {
                                // Physics fling: real-time friction + bouncy edge restitution
                                flingAnimator = android.animation.ValueAnimator.ofFloat(0f, 1f).apply {
                                    duration = 4000
                                    interpolator = android.view.animation.LinearInterpolator()
                                    var lastPlayMs = 0L
                                    var posXf = params.x.toFloat()
                                    var posYf = params.y.toFloat()
                                    var currVx = vx * 1.5f
                                    var currVy = vy * 1.5f
                                    val display = resources.displayMetrics

                                    addUpdateListener { anim ->
                                        val nowMs = anim.currentPlayTime
                                        val dt = ((nowMs - lastPlayMs).coerceIn(0L, 50L)) / 1000f
                                        lastPlayMs = nowMs

                                        posXf += currVx * dt
                                        posYf += currVy * dt

                                        // Bounce off screen edges (restitution), but not forever
                                        if (posXf < 0f) {
                                            posXf = -posXf
                                            currVx = if (Math.abs(currVx) < 180f) 0f else -currVx * 0.8f
                                        } else if (posXf + contentW > display.widthPixels) {
                                            posXf = 2f * (display.widthPixels - contentW) - posXf
                                            currVx = if (Math.abs(currVx) < 180f) 0f else -currVx * 0.8f
                                        }
                                        if (posYf < 0f) {
                                            posYf = -posYf
                                            currVy = if (Math.abs(currVy) < 180f) 0f else -currVy * 0.8f
                                        } else if (posYf + contentH > display.heightPixels) {
                                            posYf = 2f * (display.heightPixels - contentH) - posYf
                                            currVy = if (Math.abs(currVy) < 180f) 0f else -currVy * 0.8f
                                        }

                                        // Air friction (frame-rate independent)
                                        val decay = Math.exp(-1.1 * dt).toFloat()
                                        currVx *= decay
                                        currVy *= decay

                                        moveWindows(posXf.toInt(), posYf.toInt())

                                        if (Math.hypot(currVx.toDouble(), currVy.toDouble()) < 50.0) {
                                            anim.cancel()
                                            flingAnimator = null
                                        }
                                    }
                                    addListener(object : android.animation.AnimatorListenerAdapter() {
                                        override fun onAnimationEnd(animation: android.animation.Animator) {
                                            flingAnimator = null
                                        }
                                        override fun onAnimationCancel(animation: android.animation.Animator) {
                                            flingAnimator = null
                                        }
                                    })
                                    start()
                                }
                            }
                        }
                        velocityTracker?.recycle()
                        velocityTracker = null
                        true
                    }
                    else -> false
                }
            }
        }

        try {
            // V first (below), T on top: V carries permanent headroom so the
            // entrance springs (overshoot + -15° rotation) are never square-cropped;
            // T is the exact content size so touches never land on an invisible ring.
            windowManager?.addView(pinnedView, visualParams)
            windowManager?.addView(touchView, params)

            // --- Beautiful Pin Animation: expressive springs (jelly squash & stretch) ---
            pinnedView.scaleX = 0f
            pinnedView.scaleY = 0f
            pinnedView.rotation = -15f
            pinnedView.alpha = 0f
            entranceScaleXJob = springAnim(0f, 1f, 0.55f, 240f, expressive = true) { pinnedView.scaleX = it }
            entranceScaleYJob = springAnim(0f, 1f, 0.5f, 430f, expressive = true) { pinnedView.scaleY = it }
            rotJob = springAnim(0f, 1f, 0.5f, 300f, expressive = true) { pinnedView.rotation = -15f + 15f * it }
            alphaJob = springAnim(0f, 1f, 1f, 1200f) { pinnedView.alpha = it.coerceIn(0f, 1f) }

        } catch (e: Exception) {
            android.util.Log.e("CircleToSearch", "Failed to add pinned view", e)
        }
    }

    private fun showPinnedActions(anchorView: View, touchView: View, bitmap: Bitmap, anchorParams: WindowManager.LayoutParams, contentTop: Int, contentHeight: Int, onMenuCreated: (View) -> Unit) {
        val displayMetrics = resources.displayMetrics
        val menuPadding = (10 * displayMetrics.density).toInt()
        val cornerRadius = 32f * displayMetrics.density

        // --- Matches the Share/Save/Pin selection toolbar: pill surface + plain text buttons ---
        val isNight = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

        val toolbarBgColor = try { getColor(if (isNight) android.R.color.system_surface_container_dark else android.R.color.system_surface_container_light) } catch(e: Exception) { if (isNight) Color.parseColor("#FF1C1C1C") else Color.parseColor("#FFF3EDF7") }
        val contentColor = if (isNight) Color.parseColor("#FFE6E0E9") else Color.parseColor("#FF1C1B1F")
        val rippleColor = android.content.res.ColorStateList.valueOf(if (isNight) 0x2FFFFFFF else 0x1F000000)

        val menuLayout = android.widget.LinearLayout(this).apply {
            orientation = android.widget.LinearLayout.HORIZONTAL
            // Breathing room at start/end so labels aren't flush with the pill edge
            val edgePad = (8 * displayMetrics.density).toInt()
            setPadding(edgePad, 0, edgePad, 0)
            elevation = 24f
            
            val background = GradientDrawable().apply {
                setColor(toolbarBgColor)
                setCornerRadius(cornerRadius)
            }
            setBackground(background)
            
            outlineProvider = object : ViewOutlineProvider() {
                override fun getOutline(view: View, outline: android.graphics.Outline) {
                    outline.setRoundRect(0, 0, view.width, view.height, cornerRadius)
                }
            }
            clipToOutline = true
        }

        fun createTextActionButton(label: String, onClick: () -> Unit) = android.widget.Button(this).apply {
            text = label
            transformationMethod = null
            setTextColor(contentColor)
            setTypeface(Typeface.DEFAULT, Typeface.NORMAL)
            textSize = 14f
            gravity = Gravity.CENTER
            // Platform Button style enforces ~64dp minWidth/minHeight which stretches
            // WRAP_CONTENT buttons and blows up the gaps between labels.
            minWidth = 0
            minHeight = 0
            minimumWidth = 0
            minimumHeight = 0

            // Transparent background with a shaped ripple, like the selection toolbar buttons
            background = android.graphics.drawable.RippleDrawable(
                rippleColor,
                null,
                GradientDrawable().apply { setCornerRadius(24f * displayMetrics.density) }
            )
            stateListAnimator = null
            elevation = 0f

            setPadding((8 * displayMetrics.density).toInt(), 0, (8 * displayMetrics.density).toInt(), 0)

            layoutParams = android.widget.LinearLayout.LayoutParams(
                android.widget.LinearLayout.LayoutParams.WRAP_CONTENT,
                (48 * displayMetrics.density).toInt()
            )
            setOnClickListener { onClick() }
        }

        // --- Action: Share ---
        menuLayout.addView(createTextActionButton(getString(R.string.pinned_btn_share)) {
            try {
                val fileName = "share_pin_${java.util.UUID.randomUUID()}.png"
                val path = ImageUtils.saveBitmap(this@CircleToSearchAccessibilityService, bitmap, fileName)
                val file = java.io.File(path)
                val uri = androidx.core.content.FileProvider.getUriForFile(this@CircleToSearchAccessibilityService, "com.akslabs.circletosearch.fileprovider", file)
                val shareIntent = Intent(Intent.ACTION_SEND).apply {
                    type = "image/png"
                    putExtra(Intent.EXTRA_STREAM, uri)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(Intent.createChooser(shareIntent, getString(R.string.share_chooser_pin)).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) })
            } catch (e: Exception) {
                android.util.Log.e("CircleToSearch", "Failed to share pinned image", e)
            }
            try { windowManager?.removeView(menuLayout) } catch (e: Exception) {}
        })

        // --- Action: Save ---
        menuLayout.addView(createTextActionButton(getString(R.string.pinned_btn_save)) {
            val success = ImageUtils.saveToGallery(this@CircleToSearchAccessibilityService, bitmap)
            android.widget.Toast.makeText(this@CircleToSearchAccessibilityService, if (success) getString(R.string.toast_saved_to_gallery) else getString(R.string.toast_save_failed), android.widget.Toast.LENGTH_SHORT).show()
            try { windowManager?.removeView(menuLayout) } catch (e: Exception) {}
        })

        // --- Action: Delete (destructive, last) ---
        menuLayout.addView(createTextActionButton(getString(R.string.pinned_btn_delete)) {
            try { windowManager?.removeView(menuLayout) } catch (e: Exception) {}
            // The touch window goes first so the disappearing sticker can't be grabbed
            try { windowManager?.removeView(touchView) } catch (e: Exception) {}
            // Playful shrink-spin-out before the visual window is removed
            springAnim(1f, 0f, 1f, 900f,
                onEnd = {
                    try { windowManager?.removeView(anchorView) } catch (e: Exception) {}
                }
            ) { p ->
                anchorView.scaleX = 0.15f + 0.85f * p
                anchorView.scaleY = 0.15f + 0.85f * p
                anchorView.rotation = 35f * (1f - p)
                anchorView.alpha = (p * 1.5f).coerceAtMost(1f)
            }
        })

        // Measure properly and clamp to screen bounds to avoid cutoff
        menuLayout.measure(View.MeasureSpec.makeMeasureSpec(displayMetrics.widthPixels, View.MeasureSpec.AT_MOST), View.MeasureSpec.UNSPECIFIED)
        val measuredMenuWidth = menuLayout.measuredWidth
        val measuredMenuHeight = menuLayout.measuredHeight
        android.util.Log.d("PinnedMenu", "pill=$measuredMenuWidth buttons=" + (0 until menuLayout.childCount).joinToString { i ->
            val c = menuLayout.getChildAt(i)
            "${c}:w=${c.measuredWidth},minW=${(c as? android.widget.Button)?.minimumWidth},pad=${c.paddingLeft}/${c.paddingRight}"
        })

        val menuParams = WindowManager.LayoutParams(
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        )
        menuParams.gravity = Gravity.TOP or Gravity.START
        
        // Perfectly center the menu above the sticker
        // anchorParams.x is start of sticker, add stickerWidth/2 to get center, then subtract menuWidth/2
        var targetX = anchorParams.x + (anchorParams.width / 2) - (measuredMenuWidth / 2)
        
        // Clamp to screen edges to prevent cutoff on left/right
        if (targetX < menuPadding) targetX = (menuPadding).toInt()
        if (targetX + measuredMenuWidth > displayMetrics.widthPixels - menuPadding) {
            targetX = (displayMetrics.widthPixels - measuredMenuWidth - menuPadding).toInt()
        }
        menuParams.x = targetX.toInt()
        
        val yPadding = (12 * displayMetrics.density).toInt()
        menuParams.y = if (contentTop > measuredMenuHeight + yPadding) {
            contentTop - measuredMenuHeight - yPadding
        } else {
            contentTop + contentHeight + yPadding
        }

        try {
            windowManager?.addView(menuLayout, menuParams)
            // Pop in with a playful overshoot
            menuLayout.alpha = 0f
            menuLayout.scaleX = 0.85f
            menuLayout.scaleY = 0.85f
            menuLayout.animate()
                .alpha(1f)
                .scaleX(1f)
                .scaleY(1f)
                .setDuration(190)
                .setInterpolator(android.view.animation.OvershootInterpolator(1.4f))
                .start()
            onMenuCreated(menuLayout)
        } catch (e: Exception) {
            android.util.Log.e("CircleToSearch", "Failed to add menu view", e)
        }
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        translatorBridge?.onHostAccessibilityEvent(event)
    }

    override fun onInterrupt() {}

    // --- Hardware button triggers ---
    // With FLAG_REQUEST_FILTER_KEY_EVENTS every volume/power press flows through
    // onKeyEvent. Volume/power are NEVER consumed (always return false), so volume
    // still adjusts and power still sleeps — presses are only observed to launch the
    // overlay. HOME is the one exception: a long-press is claimed by us (the release
    // is consumed) because this ROM routes long-press-home into the launcher's
    // contextual search, which fails silently, instead of dispatching assist — so
    // we detect the hold ourselves and a consumed UP prevents the stray "go home".
    private var lastVolUpDown = 0L
    private var lastVolDownDown = 0L
    private var lastPowerDown = 0L
    private var lastTriggerFiredAt = 0L
    private val keysCurrentlyDown = mutableSetOf<Int>()
    private val homeKeyHandler = android.os.Handler(android.os.Looper.getMainLooper())
    private var homeDownEventTime = 0L
    private var homeLongPressTriggered = false
    private val homeLongPressRunnable = Runnable {
        if (homeDownEventTime == 0L) return@Runnable
        homeLongPressTriggered = true
        android.util.Log.d("CircleToSearch", "Home long-press fired (threshold while held)")
        fireHardwareTrigger()
    }

    override fun onKeyEvent(event: android.view.KeyEvent): Boolean {
        val code = event.keyCode
        when (event.action) {
            android.view.KeyEvent.ACTION_DOWN -> {
                // Long-press repeats must not count as new presses
                if (event.repeatCount != 0) return false
                if (code == android.view.KeyEvent.KEYCODE_HOME) {
                    homeDownEventTime = event.eventTime
                    homeLongPressTriggered = false
                    homeKeyHandler.removeCallbacks(homeLongPressRunnable)
                    homeKeyHandler.postDelayed(
                        homeLongPressRunnable,
                        android.view.ViewConfiguration.getLongPressTimeout().toLong(),
                    )
                    android.util.Log.d("CircleToSearch", "HOME down observed by a11y filter")
                    return false // short tap must still behave like the Home button
                }
                if (code != android.view.KeyEvent.KEYCODE_VOLUME_UP &&
                    code != android.view.KeyEvent.KEYCODE_VOLUME_DOWN &&
                    code != android.view.KeyEvent.KEYCODE_POWER
                ) return false

                val now = android.os.SystemClock.uptimeMillis()
                keysCurrentlyDown.add(code)
                if (now - lastTriggerFiredAt < KEY_TRIGGER_COOLDOWN_MS) return false

                var fire = false
                when (code) {
                    android.view.KeyEvent.KEYCODE_VOLUME_UP -> {
                        if (prefs.getBoolean(KEY_TRIGGER_DOUBLE_VOL_UP, false) &&
                            now - lastVolUpDown <= KEY_DOUBLE_TAP_MS
                        ) fire = true
                        lastVolUpDown = now
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_POWER_VOL_UP, false) &&
                            android.view.KeyEvent.KEYCODE_POWER in keysCurrentlyDown
                        ) fire = true
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_VOL_UP_DOWN, false) &&
                            android.view.KeyEvent.KEYCODE_VOLUME_DOWN in keysCurrentlyDown
                        ) fire = true
                    }
                    android.view.KeyEvent.KEYCODE_VOLUME_DOWN -> {
                        if (prefs.getBoolean(KEY_TRIGGER_DOUBLE_VOL_DOWN, false) &&
                            now - lastVolDownDown <= KEY_DOUBLE_TAP_MS
                        ) fire = true
                        lastVolDownDown = now
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_POWER_VOL_DOWN, false) &&
                            android.view.KeyEvent.KEYCODE_POWER in keysCurrentlyDown
                        ) fire = true
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_VOL_UP_DOWN, false) &&
                            android.view.KeyEvent.KEYCODE_VOLUME_UP in keysCurrentlyDown
                        ) fire = true
                    }
                    android.view.KeyEvent.KEYCODE_POWER -> {
                        if (prefs.getBoolean(KEY_TRIGGER_DOUBLE_POWER, false) &&
                            now - lastPowerDown <= KEY_DOUBLE_TAP_MS
                        ) fire = true
                        lastPowerDown = now
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_POWER_VOL_UP, false) &&
                            android.view.KeyEvent.KEYCODE_VOLUME_UP in keysCurrentlyDown
                        ) fire = true
                        if (!fire && prefs.getBoolean(KEY_TRIGGER_POWER_VOL_DOWN, false) &&
                            android.view.KeyEvent.KEYCODE_VOLUME_DOWN in keysCurrentlyDown
                        ) fire = true
                    }
                }
                if (fire) fireHardwareTrigger()
            }
            android.view.KeyEvent.ACTION_UP -> {
                if (code == android.view.KeyEvent.KEYCODE_HOME) {
                    homeKeyHandler.removeCallbacks(homeLongPressRunnable)
                    val sawDown = homeDownEventTime != 0L
                    val held = if (sawDown) event.eventTime - homeDownEventTime else 0L
                    val wasLongPress = sawDown &&
                        (homeLongPressTriggered ||
                            held >= android.view.ViewConfiguration.getLongPressTimeout())
                    homeDownEventTime = 0L
                    if (wasLongPress) {
                        if (!homeLongPressTriggered) fireHardwareTrigger()
                        // Consume the release: we handled this press, so the system
                        // must not also run its "go home" on key-up.
                        android.util.Log.d("CircleToSearch", "HOME long-press release consumed")
                        return true
                    }
                    return false // normal short tap → Home behaves as usual
                }
                keysCurrentlyDown.remove(code)
            }
        }
        // Never consume: system keeps full normal button behavior
        return false
    }

    private fun fireHardwareTrigger() {
        lastTriggerFiredAt = android.os.SystemClock.uptimeMillis()
        // Reset double-tap state so this gesture can't fire a second time
        lastVolUpDown = 0L
        lastVolDownDown = 0L
        lastPowerDown = 0L
        android.util.Log.d("CircleToSearch", "Hardware button trigger fired")
        performCapture(null)
    }

    /**
     * Translates the screenshot captured when Circle to Search launched using the
     * translator module's overlay (same path as its floating bubble). OCR and
     * translation start immediately; the overlay chrome is drawn after [uiDelayMs],
     * once the caller's UI has been hidden. [onClosed] is invoked when the overlay is
     * dismissed (toolbar close / back press) so the caller can restore its UI. Returns
     * false if the overlay bridge is not running. [tapToCopy] — false when the live
     * screen stays interactive underneath (long-press translate); the result overlay
     * then passes taps through instead of consuming them for word selection.
     */
    fun translateCapturedScreen(
        screenshot: Bitmap,
        uiDelayMs: Long = 0L,
        onClosed: (() -> Unit)? = null,
        tapToCopy: Boolean = true,
    ): Boolean {
        val bridge = translatorBridge ?: return false
        bridge.startCapturedTranslation(screenshot, uiDelayMs, onClosed, tapToCopy)
        return true
    }

    /** Dismisses the translator overlay if active, notifying its return callback. */
    fun closeTranslateOverlay() {
        translatorBridge?.deactivate()
    }

    /** Fades translator overlay windows during the system back gesture so the
     *  predictive back arrow (drawn below accessibility overlays) stays visible. */
    fun setTranslatorGestureAlpha(alpha: Float) {
        translatorBridge?.setGestureAlpha(alpha)
    }

    /** The default target language for OCR-only plans (no translation rendered). */
    fun defaultTargetLanguage(): dev.davidv.translator.Language? =
        translatorBridge?.defaultTargetLanguage()

    companion object {
        var instance: CircleToSearchAccessibilityService? = null
            private set
            
        private var isFlashlightOn = false // Simple static state tracking
        
        fun triggerCapture() {
            android.util.Log.d("CircleToSearch", "triggerCapture static called. instance=${instance != null}")
            instance?.performCapture(null)
        }

        fun pinArea(bitmap: Bitmap, rect: android.graphics.Rect): Boolean {
            android.util.Log.d("CircleToSearch", "pinArea static called. instance=${instance != null}")
            if (instance == null) return false
            instance?.showPinnedArea(bitmap, rect)
            return true
        }
    }

    override fun onCreate() {
        super.onCreate()
        instance = this
        // configManager init moved to onServiceConnected or safe lazy? 
        // WindowManager is needed for views which happens in onServiceConnected mostly.
    }

    override fun onDestroy() {
        super.onDestroy()
        instance = null
        runCatching { translatorBridge?.destroy() }
        translatorBridge = null
        prefs.unregisterOnSharedPreferenceChangeListener(prefsListener)
        overlayPrefs.unregisterOnSharedPreferenceChangeListener(overlayPrefsListener)
        
        overlayViews.forEach { view ->
             try {
                windowManager?.removeView(view)
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }
        bottomEdgeViews.forEach { view ->
            try { windowManager?.removeView(view) } catch (_: Exception) {}
        }
        bottomEdgeViews.clear()
        hideBubble()
    }

    /**
     * Forwards a synthetic gesture (tap or swipe) to whatever is underneath an overlay window.
     * The overlay is temporarily made FLAG_NOT_TOUCHABLE so the dispatched stroke reaches the app
     * below instead of being swallowed by our own window.
     */
    private fun forwardPassthroughGesture(
        view: View,
        path: android.graphics.Path,
        durationMs: Long,
    ) {
        val params = view.layoutParams as WindowManager.LayoutParams
        val originalFlags = params.flags
        params.flags = params.flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
        try { windowManager?.updateViewLayout(view, params) } catch (_: Exception) {}

        val stroke = android.accessibilityservice.GestureDescription.StrokeDescription(path, 0, durationMs)
        val gesture = android.accessibilityservice.GestureDescription.Builder().addStroke(stroke).build()
        val handler = android.os.Handler(android.os.Looper.getMainLooper())

        // The window MUST become touchable again no matter what: if it stayed
        // FLAG_NOT_TOUCHABLE the overlay would silently stop receiving touches and
        // long-press would appear dead until the service rebuilt the views.
        var restored = false
        fun restore() {
            if (restored) return
            restored = true
            handler.post {
                params.flags = originalFlags
                try { windowManager?.updateViewLayout(view, params) } catch (_: Exception) {}
            }
        }

        handler.postDelayed({
            val dispatched = try {
                dispatchGesture(
                    gesture,
                    object : android.accessibilityservice.AccessibilityService.GestureResultCallback() {
                        override fun onCompleted(g: android.accessibilityservice.GestureDescription?) { restore() }
                        override fun onCancelled(g: android.accessibilityservice.GestureDescription?) { restore() }
                    },
                    null,
                )
            } catch (e: Exception) {
                false
            }
            // dispatchGesture returns false when a gesture is already running (or the
            // service can't dispatch): restore immediately instead of waiting forever.
            if (!dispatched) restore()
            // Safety net: never leave the window untouchable for more than a moment.
            handler.postDelayed({ restore() }, durationMs + 1000L)
        }, 100)
    }

    /**
     * A transparent, click-through overlay that sits at a bottom corner (arc) or bottom-centre
     * (semicircle). It only reacts to a LONG-PRESS (which triggers Circle to Search); taps and
     * swipes are forwarded to the app underneath so it never blocks normal interaction.
     * When [isPreview] is true it draws a translucent filled shape so the user can see/size it.
     */
    private inner class BottomEdgeOverlayView(
        context: android.content.Context,
        private val shape: BottomEdgeShape,
        private var isPreview: Boolean,
        private val accent: Int,
    ) : android.view.View(context) {

        private val fillPaint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            style = android.graphics.Paint.Style.FILL
        }
        private val strokePaint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            style = android.graphics.Paint.Style.STROKE
            strokeWidth = 3f * resources.displayMetrics.density
        }

        private val handler = android.os.Handler(android.os.Looper.getMainLooper())
        private val touchSlop = android.view.ViewConfiguration.get(context).scaledTouchSlop
        // Long-press stays alive while the finger drifts a little (more tolerant than
        // the system tap slop) so slight jitter doesn't kill the gesture.
        private val longPressSlop = touchSlop * 1.5f
        private val longPressTimeoutMs = 400L
        // Minimum gap between forwarded scroll segments (each one briefly makes the
        // window untouchable and dispatches a synthetic gesture).
        private val scrollForwardIntervalMs = 180L

        private var downX = 0f
        private var downY = 0f
        private var lastScrollRawX = 0f
        private var lastScrollRawY = 0f
        private var lastScrollForwardAt = 0L
        private var longPressConsumed = false
        private var scrollForwarded = false

        private val longPressRunnable = Runnable {
            if (longPressConsumed || scrollForwarded) return@Runnable
            longPressConsumed = true
            hapticClick()
            performCapture(null)
        }

        private fun hapticClick() {
            val vibrator = getSystemService(Context.VIBRATOR_SERVICE) as android.os.Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                vibrator.vibrate(android.os.VibrationEffect.createPredefined(android.os.VibrationEffect.EFFECT_CLICK))
            } else {
                @Suppress("DEPRECATION") vibrator.vibrate(20)
            }
        }

        private fun forwardPath(path: android.graphics.Path, durationMs: Long) {
            forwardPassthroughGesture(this@BottomEdgeOverlayView, path, durationMs)
        }

        override fun onDetachedFromWindow() {
            handler.removeCallbacksAndMessages(null)
            super.onDetachedFromWindow()
        }

        /** Hit test: only the shaped region consumes touches; the bounding-box corners pass through. */
        private fun hitTest(x: Float, y: Float): Boolean {
            val w = width.toFloat()
            val h = height.toFloat()
            return when (shape) {
                BottomEdgeShape.ARC_LEFT -> {
                    // Quarter disc centred at bottom-left corner (0, h), radius = w.
                    val dx = x; val dy = h - y
                    dx * dx + dy * dy <= w * w
                }
                BottomEdgeShape.ARC_RIGHT -> {
                    val dx = w - x; val dy = h - y
                    dx * dx + dy * dy <= w * w
                }
                BottomEdgeShape.SEMICIRCLE -> {
                    // Half disc centred at (w/2, h), radius = h, upper half only.
                    val dx = x - w / 2f; val dy = h - y
                    dy >= 0f && dx * dx + dy * dy <= h * h
                }
            }
        }

        override fun onDraw(canvas: android.graphics.Canvas) {
            super.onDraw(canvas)
            if (!isPreview) return
            val w = width.toFloat()
            val h = height.toFloat()

            fun buildPath(inset: Float): android.graphics.Path {
                val path = android.graphics.Path()
                when (shape) {
                    BottomEdgeShape.ARC_LEFT -> {
                        // Quarter disc centred on the bottom-left corner of the view.
                        val r = minOf(w, h) - inset
                        path.moveTo(inset, h - inset)
                        path.lineTo(inset, h - inset - r)
                        path.arcTo(
                            android.graphics.RectF(inset - r, h - inset - r, inset + r, h - inset + r),
                            270f, 90f,
                        )
                        path.close()
                    }
                    BottomEdgeShape.ARC_RIGHT -> {
                        // Quarter disc centred on the bottom-right corner of the view.
                        val r = minOf(w, h) - inset
                        path.moveTo(w - inset, h - inset)
                        path.lineTo(w - inset, h - inset - r)
                        path.arcTo(
                            android.graphics.RectF(w - inset - r, h - inset - r, w - inset + r, h - inset + r),
                            270f, -90f,
                        )
                        path.close()
                    }
                    BottomEdgeShape.SEMICIRCLE -> {
                        // Upper half disc centred on the bottom-centre of the view.
                        val r = minOf(w / 2f, h) - inset
                        path.moveTo(w / 2f - r, h - inset)
                        path.arcTo(
                            android.graphics.RectF(w / 2f - r, h - inset - r, w / 2f + r, h - inset + r),
                            180f, 180f,
                        )
                        path.close()
                    }
                }
                return path
            }

            val a = (0.28f * 255).toInt()
            fillPaint.color = android.graphics.Color.argb(
                a, android.graphics.Color.red(accent), android.graphics.Color.green(accent), android.graphics.Color.blue(accent),
            )
            strokePaint.color = android.graphics.Color.argb(
                (0.6f * 255).toInt(), android.graphics.Color.red(accent), android.graphics.Color.green(accent), android.graphics.Color.blue(accent),
            )
            // Even border, guaranteed: the fill sits half a stroke width from the view
            // (screen) edge, and the stroke is placed so its OUTER edge lands exactly on
            // the fill boundary. The stroke therefore lies entirely on top of the fill
            // (one uniform tone along the whole border), is never clipped by the view
            // bounds, and leaves no fill rim outside the border.
            val sw = strokePaint.strokeWidth
            canvas.drawPath(buildPath(sw / 2f), fillPaint)
            canvas.drawPath(buildPath(sw), strokePaint)
        }

        @SuppressLint("ClickableViewAccessibility")
        override fun onTouchEvent(event: MotionEvent): Boolean {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    // Only the shaped region consumes touches; the bounding-box
                    // corners pass straight through to the app underneath.
                    if (!hitTest(event.x, event.y)) return false
                    downX = event.x
                    downY = event.y
                    lastScrollRawX = event.rawX
                    lastScrollRawY = event.rawY
                    lastScrollForwardAt = 0L
                    longPressConsumed = false
                    scrollForwarded = false
                    handler.removeCallbacks(longPressRunnable)
                    handler.postDelayed(longPressRunnable, longPressTimeoutMs)
                    return true
                }
                MotionEvent.ACTION_MOVE -> {
                    if (longPressConsumed) return true
                    val dx = event.x - downX
                    val dy = event.y - downY
                    val dist = Math.sqrt((dx * dx + dy * dy).toDouble()).toFloat()
                    if (dist > longPressSlop) {
                        // Moved like a swipe: cancel the pending long-press and keep
                        // forwarding the drag so the app underneath still scrolls.
                        if (!scrollForwarded) {
                            scrollForwarded = true
                            handler.removeCallbacks(longPressRunnable)
                        }
                        val now = android.os.SystemClock.uptimeMillis()
                        if (now - lastScrollForwardAt >= scrollForwardIntervalMs) {
                            lastScrollForwardAt = now
                            val path = android.graphics.Path().apply {
                                moveTo(lastScrollRawX, lastScrollRawY)
                                lineTo(event.rawX, event.rawY)
                            }
                            lastScrollRawX = event.rawX
                            lastScrollRawY = event.rawY
                            forwardPath(path, 120)
                        }
                    }
                    // Small drift keeps the long-press timer running (jitter tolerant).
                    return true
                }
                MotionEvent.ACTION_UP -> {
                    handler.removeCallbacks(longPressRunnable)
                    if (!longPressConsumed && !scrollForwarded) {
                        // Stationary release = tap: forward immediately so taps behave
                        // exactly like they do without the overlay.
                        forwardPath(android.graphics.Path().apply { moveTo(event.rawX, event.rawY) }, 50)
                    }
                    return true
                }
                MotionEvent.ACTION_CANCEL -> {
                    handler.removeCallbacks(longPressRunnable)
                    return true
                }
                else -> return true
            }
        }

        fun setPreview(value: Boolean) {
            if (isPreview != value) {
                isPreview = value
                invalidate()
            }
        }
    }
}

