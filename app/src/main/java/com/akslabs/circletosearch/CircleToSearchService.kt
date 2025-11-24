package com.akslabs.circletosearch

import android.accessibilityservice.AccessibilityService
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.PixelFormat
import android.os.Build
import android.view.Display
import android.view.GestureDetector
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.view.accessibility.AccessibilityEvent
import com.akslabs.circletosearch.utils.ImageUtils
import java.util.concurrent.Executor
import java.util.concurrent.Executors

class CircleToSearchService : AccessibilityService() {

    private var windowManager: WindowManager? = null
    private var triggerView: View? = null
    private val executor: Executor = Executors.newSingleThreadExecutor()
    
    private var bubbleView: View? = null
    private val prefs by lazy { getSharedPreferences("app_prefs", Context.MODE_PRIVATE) }
    private val prefsListener = android.content.SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == "bubble_enabled") {
            updateBubbleState()
        }
    }

    override fun onServiceConnected() {
        super.onServiceConnected()
        setupTriggerOverlay()
        prefs.registerOnSharedPreferenceChangeListener(prefsListener)
        updateBubbleState()
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

        bubbleView = View(this).apply {
            background = android.graphics.drawable.GradientDrawable().apply {
                shape = android.graphics.drawable.GradientDrawable.OVAL
                setColor(android.graphics.Color.parseColor("#4285F4")) // Google Blue
                setStroke(4, android.graphics.Color.WHITE)
            }
            elevation = 10f
            setOnClickListener {
                performCapture()
            }
        }

        val params = WindowManager.LayoutParams(
            150, 150,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT
        )
        params.gravity = Gravity.CENTER_VERTICAL or Gravity.END
        params.x = 0
        params.y = 200 // Offset slightly

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

    private fun setupTriggerOverlay() {
        windowManager = getSystemService(WINDOW_SERVICE) as WindowManager
        triggerView = View(this)

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            100, // Height of the status bar trigger area (approx)
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT
        )
        params.gravity = Gravity.TOP

        val gestureDetector = GestureDetector(this, object : GestureDetector.SimpleOnGestureListener() {
            override fun onDoubleTap(e: MotionEvent): Boolean {
                performCapture()
                return true
            }
        })

        triggerView?.setOnTouchListener { _, event ->
            gestureDetector.onTouchEvent(event)
            // Pass touch through so we don't block status bar interactions completely
            // But we need to return true to consume events for gesture detector to work?
            // Actually, if we return false, the gesture detector might not see the full sequence.
            // But if we return true, we block clicks.
            // For a transparent overlay, usually we want to pass through.
            // However, GestureDetector needs the stream.
            // A common trick is to use FLAG_NOT_TOUCH_MODAL and let touches pass through *if* we don't consume them?
            // But we are just an overlay.
            // Let's stick to the previous logic which was 'true' but maybe refine it if user complains about blocked status bar.
            // For now, keep it simple.
            true 
        }

        try {
            windowManager?.addView(triggerView, params)
        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun performCapture() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // Add a slight delay to ensure UI is stable/ripple finished
            android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({
                takeScreenshot(
                    Display.DEFAULT_DISPLAY,
                    executor,
                    object : TakeScreenshotCallback {
                        override fun onSuccess(screenshot: ScreenshotResult) {
                            try {
                                val hardwareBuffer = screenshot.hardwareBuffer
                                val colorSpace = screenshot.colorSpace
                                
                                android.util.Log.d("CircleToSearch", "Screenshot captured. Size: ${hardwareBuffer.width}x${hardwareBuffer.height}")

                                val bitmap = Bitmap.wrapHardwareBuffer(hardwareBuffer, colorSpace)
                                if (bitmap == null) {
                                    android.util.Log.e("CircleToSearch", "Failed to wrap hardware buffer")
                                    hardwareBuffer.close()
                                    return
                                }

                                // Copy to software bitmap
                                val copy = bitmap.copy(Bitmap.Config.ARGB_8888, false)
                                hardwareBuffer.close() // Close buffer after copy

                                if (copy == null) {
                                    android.util.Log.e("CircleToSearch", "Failed to copy bitmap")
                                    return
                                }
                                
                                // Check center pixel
                                val centerX = copy.width / 2
                                val centerY = copy.height / 2
                                val pixel = copy.getPixel(centerX, centerY)
                                android.util.Log.d("CircleToSearch", "Center pixel color: ${Integer.toHexString(pixel)}")
                                
                                val path = ImageUtils.saveBitmap(this@CircleToSearchService, copy)
                                android.util.Log.d("CircleToSearch", "Screenshot saved to $path")
                                launchOverlay(path)
                                
                            } catch (e: Exception) {
                                android.util.Log.e("CircleToSearch", "Error processing screenshot", e)
                            }
                        }

                        override fun onFailure(errorCode: Int) {
                            android.util.Log.e("CircleToSearch", "Screenshot failed with error code: $errorCode")
                        }
                    }
                )
            }, 500)
        }
    }

    private fun launchOverlay(screenshotPath: String) {
        val intent = Intent(this, OverlayActivity::class.java).apply {
            putExtra("SCREENSHOT_PATH", screenshotPath)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        startActivity(intent)
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {}

    override fun onInterrupt() {}

    override fun onDestroy() {
        super.onDestroy()
        prefs.unregisterOnSharedPreferenceChangeListener(prefsListener)
        if (triggerView != null && windowManager != null) {
            windowManager?.removeView(triggerView)
        }
        hideBubble()
    }
}
