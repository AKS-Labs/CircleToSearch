package com.akslabs.circletosearch.utils

import android.graphics.Bitmap
import android.graphics.RectF
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.NotFoundException
import com.google.zxing.RGBLuminanceSource
import com.google.zxing.Result
import com.google.zxing.ResultPoint
import com.google.zxing.common.HybridBinarizer
import com.google.zxing.multi.GenericMultipleBarcodeReader

// Sealed class representing all supported QR / barcode result types
sealed class QrResult {
    data class Url(val url: String, val displayUrl: String) : QrResult()
    data class WiFi(val ssid: String, val password: String?, val security: String) : QrResult()
    data class Phone(val number: String) : QrResult()
    data class Product(val barcode: String) : QrResult()
    data class VCard(val name: String?, val phone: String?, val email: String?, val raw: String) : QrResult()
    data class GeoPoint(val lat: Double, val lng: Double) : QrResult()
    data class PlainText(val text: String) : QrResult()
}

/** Wraps a parsed QR result with its bounding box in bitmap-pixel coordinates */
data class QrResultWithBounds(
    val result: QrResult,
    val rawText: String,
    /** Bounds in bitmap pixel coords (may be null if position unavailable) */
    val bounds: RectF?
)

object QrScanner {

    private val HINTS = mapOf(
        DecodeHintType.TRY_HARDER to true,
        DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE)
    )

    /** Scan for all barcodes / QR codes in the given bitmap. Returns empty list when none found. */
    fun scanBitmapAll(bitmap: Bitmap): List<QrResultWithBounds> {
        val allResults = mutableListOf<QrResultWithBounds>()
        val foundTexts = mutableSetOf<String>()

        fun processResults(rawResults: List<com.google.zxing.Result>, xOffset: Int, yOffset: Int) {
            rawResults.forEach { raw ->
                if (!foundTexts.contains(raw.text)) {
                    foundTexts.add(raw.text)
                    val globalBounds = computeBounds(raw.resultPoints)?.let { b ->
                        android.graphics.RectF(b.left + xOffset, b.top + yOffset, b.right + xOffset, b.bottom + yOffset)
                    }
                    allResults.add(QrResultWithBounds(parseResult(raw.text), raw.text, globalBounds))
                }
            }
        }

        try {
            val w = bitmap.width
            val h = bitmap.height

            // 1. FULL SCAN
            val fullPixels = IntArray(w * h)
            bitmap.getPixels(fullPixels, 0, w, 0, 0, w, h)
            processResults(scanLuminanceSource(RGBLuminanceSource(w, h, fullPixels)), 0, 0)
            android.util.Log.d("CircleToSearch", "QrScanner: Full scan found ${allResults.size} codes")

            // 2. TILED SCAN (2x2 Overlapping Quadrants)
            // Using ~65% size to ensure 15% middle overlap
            val tileW = (w * 0.65f).toInt()
            val tileH = (h * 0.65f).toInt()

            val tiles = listOf(
                android.graphics.Rect(0, 0, tileW, tileH),                 // Top Left
                android.graphics.Rect(w - tileW, 0, w, tileH),             // Top Right
                android.graphics.Rect(0, h - tileH, tileW, h),             // Bottom Left
                android.graphics.Rect(w - tileW, h - tileH, w, h)          // Bottom Right
            )

            tiles.forEachIndexed { index, rect ->
                try {
                    val tw = rect.width()
                    val th = rect.height()
                    val tilePixels = IntArray(tw * th)
                    bitmap.getPixels(tilePixels, 0, tw, rect.left, rect.top, tw, th)
                    
                    val results = scanLuminanceSource(RGBLuminanceSource(tw, th, tilePixels))
                    val beforeCount = allResults.size
                    processResults(results, rect.left, rect.top)
                    android.util.Log.d("CircleToSearch", "QrScanner: Tile $index found ${allResults.size - beforeCount} NEW codes")
                } catch (e: Exception) {
                    android.util.Log.e("CircleToSearch", "QrScanner: Tile $index failed", e)
                }
            }

            android.util.Log.d("CircleToSearch", "QrScanner: Tiled scan COMPLETE. Total codes: ${allResults.size}")
            return allResults
        } catch (e: Exception) {
            android.util.Log.e("CircleToSearch", "QrScanner: Fatal error in scanBitmapAll", e)
            return emptyList()
        }
    }

    /** Core scanner: tries Hybrid, Global, and Inverted versions of a source. */
    private fun scanLuminanceSource(source: com.google.zxing.LuminanceSource): List<com.google.zxing.Result> {
        val results = mutableListOf<com.google.zxing.Result>()
        val foundTexts = mutableSetOf<String>()
        val multiReader = GenericMultipleBarcodeReader(MultiFormatReader())

        fun run(binarizer: com.google.zxing.Binarizer) {
            try {
                val bitmap = BinaryBitmap(binarizer)
                val raw = multiReader.decodeMultiple(bitmap, HINTS)
                raw.forEach { r ->
                    if (!foundTexts.contains(r.text)) {
                        foundTexts.add(r.text)
                        results.add(r)
                    }
                }
            } catch (e: NotFoundException) {
                // Ignore
            } catch (e: Exception) {
                // Log minor errors if needed
            }
        }

        // Try strategies
        run(HybridBinarizer(source))
        run(com.google.zxing.common.GlobalHistogramBinarizer(source))
        
        // Try inverted
        val inverted = source.invert()
        run(HybridBinarizer(inverted))
        run(com.google.zxing.common.GlobalHistogramBinarizer(inverted))

        return results
    }

    /** Compatibility single-result scan (kept for backward compat). */
    fun scanBitmap(bitmap: Bitmap): QrResult? = scanBitmapAll(bitmap).firstOrNull()?.result

    private fun computeBounds(points: Array<ResultPoint>?): RectF? {
        if (points.isNullOrEmpty()) return null
        var minX = Float.MAX_VALUE; var minY = Float.MAX_VALUE
        var maxX = Float.MIN_VALUE; var maxY = Float.MIN_VALUE
        for (p in points) {
            if (p == null) continue
            if (p.x < minX) minX = p.x; if (p.x > maxX) maxX = p.x
            if (p.y < minY) minY = p.y; if (p.y > maxY) maxY = p.y
        }
        return if (minX == Float.MAX_VALUE) null else RectF(minX - 20f, minY - 20f, maxX + 20f, maxY + 20f)
    }

    fun parseResult(text: String): QrResult {
        return when {
            text.startsWith("http://", ignoreCase = true) || text.startsWith("https://", ignoreCase = true) -> {
                val display = text.removePrefix("http://").removePrefix("https://").trimEnd('/')
                QrResult.Url(text, display)
            }
            text.startsWith("WIFI:", ignoreCase = true) -> parseWifi(text)
            text.startsWith("tel:", ignoreCase = true) -> QrResult.Phone(text.removePrefix("tel:").trim())
            text.startsWith("BEGIN:VCARD", ignoreCase = true) -> parseVCard(text)
            text.startsWith("geo:", ignoreCase = true) -> parseGeo(text)
            text.matches(Regex("\\d{8,14}")) -> QrResult.Product(text)
            else -> QrResult.PlainText(text)
        }
    }

    private fun parseWifi(text: String): QrResult {
        val ssid = Regex("S:([^;]*)").find(text)?.groupValues?.get(1) ?: ""
        val pass = Regex("P:([^;]*)").find(text)?.groupValues?.get(1)
        val sec  = Regex("T:([^;]*)").find(text)?.groupValues?.get(1) ?: "WPA"
        return QrResult.WiFi(ssid, pass, sec)
    }

    private fun parseVCard(text: String): QrResult {
        val name  = Regex("FN:([^\r\n]+)").find(text)?.groupValues?.get(1)
        val phone = Regex("TEL[^:]*:([^\r\n]+)").find(text)?.groupValues?.get(1)
        val email = Regex("EMAIL[^:]*:([^\r\n]+)").find(text)?.groupValues?.get(1)
        return QrResult.VCard(name, phone, email, text)
    }

    private fun parseGeo(text: String): QrResult {
        return try {
            val coords = text.removePrefix("geo:").split(",")
            QrResult.GeoPoint(coords[0].toDouble(), coords[1].split("?")[0].toDouble())
        } catch (e: Exception) { QrResult.PlainText(text) }
    }
}
