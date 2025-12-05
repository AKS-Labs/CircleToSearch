package com.akslabs.circletosearch.data

import android.graphics.Bitmap

object BitmapRepository {
    private var screenshot: Bitmap? = null

    fun setScreenshot(bitmap: Bitmap?) {
        screenshot = bitmap
    }

    fun getScreenshot(): Bitmap? {
        return screenshot
    }

    fun clear() {
        screenshot = null
    }
}
