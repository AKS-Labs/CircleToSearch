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
import android.content.Intent
import android.content.SharedPreferences
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.PixelFormat
import android.hardware.camera2.CameraManager
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.view.Display
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import com.akslabs.circletosearch.data.ActionType
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.data.GestureType
import com.akslabs.circletosearch.data.OverlayConfigurationManager
import com.akslabs.circletosearch.data.OverlaySegment
import java.util.concurrent.Executor
import java.util.concurrent.Executors

class CircleToSearchAccessibilityService : AccessibilityService() {

    private var windowManager: WindowManager? = null
    private val overlayViews = mutableListOf<View>() // Track all added segment views
    private val executor: Executor = Executors.newSingleThreadExecutor()
    private lateinit var configManager: OverlayConfigurationManager
    
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
    }

    override fun onServiceConnected() {
        super.onServiceConnected()
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        configManager = OverlayConfigurationManager(this)
        
        prefs.registerOnSharedPreferenceChangeListener(prefsListener)
        overlayPrefs.registerOnSharedPreferenceChangeListener(overlayPrefsListener)
        
        updateBubbleState()
        updateOverlay()
    }
    
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        updateOverlay()
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
        params.x = 0
        params.y = 200

        bubbleView = View(this).apply {
            setBackgroundResource(R.mipmap.ic_launcher)
            elevation = 10f
            
            var initialX = 0
            var initialY = 0
            var initialTouchX = 0f
            var initialTouchY = 0f
            
            @SuppressLint("ClickableViewAccessibility")
            setOnTouchListener { _, event ->
                when (event.action) {
                    MotionEvent.ACTION_DOWN -> {
                        initialX = params.x
                        initialY = params.y
                        initialTouchX = event.rawX
                        initialTouchY = event.rawY
                        true
                    }
                    MotionEvent.ACTION_MOVE -> {
                        params.x = initialX + (event.rawX - initialTouchX).toInt()
                        params.y = initialY + (event.rawY - initialTouchY).toInt()
                        windowManager?.updateViewLayout(this, params)
                        true
                    }
                    MotionEvent.ACTION_UP -> {
                        if (Math.abs(event.rawX - initialTouchX) < 10 && Math.abs(event.rawY - initialTouchY) < 10) {
                            performCapture()
                        }
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
        
        if (!config.isEnabled) {
            overlayViews.forEach { 
                try { windowManager?.removeView(it) } catch(e: Exception) {} 
            }
            overlayViews.clear()
            return
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
                if (params.y != segment.yOffset) { params.y = segment.yOffset; changed = true }
                
                if (changed) {
                    try {
                        windowManager?.updateViewLayout(view, params)
                    } catch (e: Exception) {
                        // Fallback implies view might be detached, shouldn't happen commonly
                    }
                }
                
                // Update Color (Debug)
                if (config.isVisible) {
                    val colors = listOf(Color.parseColor("#80FF0000"), Color.parseColor("#8000FF00"), Color.parseColor("#800000FF"), Color.parseColor("#80FFFF00"), Color.parseColor("#80FF00FF")) // Red, Green, Blue, Yellow, Magenta
                    view.setBackgroundColor(colors[index % colors.size])
                } else {
                    view.setBackgroundColor(Color.TRANSPARENT)
                }
                
                // Update gesture listener
                // Since we created the detector in the loop, we can't easily "update" its inner logic if it closes over the *old* segment.
                // WE MUST re-attach the listener or make the listener dynamic.
                // The cleanest way is to just attach a NEW listener wrapper that reads the LATEST segment config.
                // But `segment` here is from the new config.
                // Creating a new detector is cheap.
                attachTouchListener(view, segment)
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
                params.y = segment.yOffset
                
                 if (config.isVisible) {
                    val colors = listOf(Color.parseColor("#80FF0000"), Color.parseColor("#8000FF00"), Color.parseColor("#800000FF"), Color.parseColor("#80FFFF00"), Color.parseColor("#80FF00FF"))
                    view.setBackgroundColor(colors[index % colors.size])
                } else {
                    view.setBackgroundColor(Color.TRANSPARENT)
                }

                attachTouchListener(view, segment)
                
                try {
                    windowManager?.addView(view, params)
                    overlayViews.add(view)
                } catch (e: Exception) {
                    e.printStackTrace()
                }
            }
        }
    }
    
    @SuppressLint("ClickableViewAccessibility")
    private fun attachTouchListener(view: View, segment: OverlaySegment) {
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

            override fun onSingleTapConfirmed(e: MotionEvent): Boolean {
                 // User wants "buttons behind to be clickable" 
                 // We temporarily disable touch on our window and dispatch the click through.
                 propagateSingleTap(view, e.rawX, e.rawY)
                 return false
            }
            
            override fun onFling(e1: MotionEvent?, e2: MotionEvent, velocityX: Float, velocityY: Float): Boolean {
                if (e1 == null) return false
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
                    if (Math.abs(diffY) > 100 && Math.abs(velocityY) > 100) {
                        if (diffY > 0) {
                             // Swipe Down
                             val action = segment.gestures[GestureType.SWIPE_DOWN] ?: ActionType.NONE
                             if (action != ActionType.NONE) { 
                                 performAction(action, segment) 
                             } else {
                                 // Smart Scroll Down Logic (Default)
                                 // If overlay is on LEFT side (< screenWidth/2), open Notifications.
                                 // If overlay is on RIGHT side (> screenWidth/2), open Quick Settings.
                                 
                                 // We need to calculate the CENTER of this segment.
                                 // segment.xOffset is the left edge.
                                 // segment.width is width.
                                 val segmentCenterX = segment.xOffset + (segment.width / 2)
                                 val screenWidth = resources.displayMetrics.widthPixels
                                 
                                 if (segmentCenterX < (screenWidth / 2)) {
                                     performGlobalAction(GLOBAL_ACTION_NOTIFICATIONS)
                                 } else {
                                     performGlobalAction(GLOBAL_ACTION_QUICK_SETTINGS)
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
                override fun onSingleTapConfirmed(e: MotionEvent): Boolean = false
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
            ActionType.CTS_LENS -> {
                 // Force Lens Mode
                 val uiPrefs = com.akslabs.circletosearch.utils.UIPreferences(this)
                 uiPrefs.setUseGoogleLensOnly(true)
                 performCapture()
            }
            ActionType.CTS_MULTI -> {
                 // Force Multi Mode
                 val uiPrefs = com.akslabs.circletosearch.utils.UIPreferences(this)
                 uiPrefs.setUseGoogleLensOnly(false)
                 performCapture()
            }
            ActionType.SPLIT_SCREEN -> {
                 val success = performGlobalAction(GLOBAL_ACTION_TOGGLE_SPLIT_SCREEN)
                 if (!success) {
                     android.widget.Toast.makeText(this, "Split Screen not supported or failed", android.widget.Toast.LENGTH_SHORT).show()
                 }
            }
            else -> {}
        }
    }
    
    private fun findGestureForAction(segment: OverlaySegment, action: ActionType): GestureType {
        return segment.gestures.entries.firstOrNull { it.value == action }?.key ?: GestureType.DOUBLE_TAP
    }
    
    // Pass-through Logic for Single Tap
    // We must temporarily make the window UNTOUCHABLE so the injected gesture falls through to the app below.
    // Otherwise, the injected tap hits our own overlay (loop/blocked).
    private fun propagateSingleTap(view: View, x: Float, y: Float) {
        val params = view.layoutParams as WindowManager.LayoutParams
        val originalFlags = params.flags
        
        // Make untouchable
        params.flags = params.flags or WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE
        windowManager?.updateViewLayout(view, params)
        
        val path = android.graphics.Path().apply { moveTo(x, y) }
        val stroke = android.accessibilityservice.GestureDescription.StrokeDescription(path, 0, 50) // 50ms tap duration (more standard)
        val gesture = android.accessibilityservice.GestureDescription.Builder().addStroke(stroke).build()
        
        // Wait for WindowManager to update input focus before dispatching
        val handler = android.os.Handler(android.os.Looper.getMainLooper())
        handler.postDelayed({
            dispatchGesture(gesture, object : android.accessibilityservice.AccessibilityService.GestureResultCallback() {
                override fun onCompleted(gestureDescription: android.accessibilityservice.GestureDescription?) {
                    super.onCompleted(gestureDescription)
                    restoreFlags()
                }
    
                override fun onCancelled(gestureDescription: android.accessibilityservice.GestureDescription?) {
                    super.onCancelled(gestureDescription)
                    restoreFlags()
                }
                
                fun restoreFlags() {
                    // Restore original flags (Touchable) using main thread to be safe with UI
                    handler.post {
                        params.flags = originalFlags
                        try {
                            windowManager?.updateViewLayout(view, params)
                        } catch (e: Exception) {
                            // View might be removed
                        }
                    }
                }
            }, null)
        }, 100) // 100ms Delay to ensure 'untouchable' takes effect solidly
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

    private fun performCapture() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // Haptic Feedback (Crisp Click) - Moved to performAction, but keeping here specifically for direct calls if any
             // (performAction handles its own vibration)
            
            // Execute immediately for instant trigger
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
                            launchOverlay()
                            
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

    private fun launchOverlay() {
        android.util.Log.d("CircleToSearchAccess", "AccessibilityService launching OverlayActivity")
        val intent = Intent(this, OverlayActivity::class.java).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION) // Disable animation for faster feel
        }
        startActivity(intent)
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    override fun onInterrupt() {}

    companion object {
        private var instance: CircleToSearchAccessibilityService? = null
        private var isFlashlightOn = false // Simple static state tracking

        fun triggerCapture() {
            instance?.performCapture()
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
        prefs.unregisterOnSharedPreferenceChangeListener(prefsListener)
        overlayPrefs.unregisterOnSharedPreferenceChangeListener(overlayPrefsListener)
        
        overlayViews.forEach { view ->
             try {
                windowManager?.removeView(view)
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }
        hideBubble()
    }
}

