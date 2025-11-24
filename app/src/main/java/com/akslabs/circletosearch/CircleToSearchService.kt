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

    override fun onServiceConnected() {
        super.onServiceConnected()
        setupTriggerOverlay()
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
            false // Pass touch through so we don't block status bar interactions completely? 
                  // Actually, if we want to intercept double tap, we might consume it.
                  // But for status bar, we usually want clicks to pass through if it's not a double tap.
                  // However, returning false here means the gesture detector might not get the full stream.
                  // Let's try returning true if we want to consume, but for now we want to be "smart".
                  // A simple overlay that consumes touches might block the status bar.
                  // To avoid blocking, we might need a more complex setup or just accept that we block that thin strip.
                  // For this demo, blocking the top 100px for double tap is acceptable as per requirements "smart invisible overlay".
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
            takeScreenshot(
                Display.DEFAULT_DISPLAY,
                executor,
                object : TakeScreenshotCallback {
                    override fun onSuccess(screenshot: ScreenshotResult) {
                        val bitmap = Bitmap.wrapHardwareBuffer(
                            screenshot.hardwareBuffer,
                            screenshot.colorSpace
                        )
                        bitmap?.let {
                            // Hardware bitmaps are immutable and can't be easily saved sometimes without copy
                            // But ImageUtils.saveBitmap compresses it, which should work.
                            // We might need to copy if it fails.
                            val copy = it.copy(Bitmap.Config.ARGB_8888, false)
                            val path = ImageUtils.saveBitmap(this@CircleToSearchService, copy)
                            launchOverlay(path)
                            screenshot.hardwareBuffer.close()
                        }
                    }

                    override fun onFailure(errorCode: Int) {
                        // Handle error
                    }
                }
            )
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
        if (triggerView != null && windowManager != null) {
            windowManager?.removeView(triggerView)
        }
    }
}
