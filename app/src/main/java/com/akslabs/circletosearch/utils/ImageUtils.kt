package com.akslabs.circletosearch.utils

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Rect
import java.io.File
import java.io.FileOutputStream

object ImageUtils {
    private const val SCREENSHOT_FILENAME = "screenshot.png"

    fun saveBitmap(context: Context, bitmap: Bitmap): String {
        val file = File(context.cacheDir, SCREENSHOT_FILENAME)
        FileOutputStream(file).use { out ->
            bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
        }
        return file.absolutePath
    }

    fun loadBitmap(path: String): Bitmap? {
        return BitmapFactory.decodeFile(path)
    }

    fun cropBitmap(source: Bitmap, rect: Rect): Bitmap {
        // Ensure rect is within bounds
        val left = rect.left.coerceIn(0, source.width)
        val top = rect.top.coerceIn(0, source.height)
        val width = rect.width().coerceAtMost(source.width - left)
        val height = rect.height().coerceAtMost(source.height - top)
        
        return if (width > 0 && height > 0) {
            Bitmap.createBitmap(source, left, top, width, height)
        } else {
            source // Fallback or handle error
        }
    }

    fun resizeBitmap(source: Bitmap, maxLength: Int): Bitmap {
        try {
            if (source.width <= maxLength && source.height <= maxLength) return source
            val aspectRatio = source.width.toDouble() / source.height.toDouble()
            val targetWidth = if (aspectRatio >= 1) maxLength else (maxLength * aspectRatio).toInt()
            val targetHeight = if (aspectRatio < 1) maxLength else (maxLength / aspectRatio).toInt()
            return Bitmap.createScaledBitmap(source, targetWidth, targetHeight, true)
        } catch (e: Exception) {
            return source
        }
    }
}
