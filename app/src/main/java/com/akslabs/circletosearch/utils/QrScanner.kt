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
        return try {
            val width = bitmap.width
            val height = bitmap.height
            val pixels = IntArray(width * height)
            bitmap.getPixels(pixels, 0, width, 0, 0, width, height)

            val source = RGBLuminanceSource(width, height, pixels)
            val multiReader = GenericMultipleBarcodeReader(MultiFormatReader())
            
            val allResults = mutableListOf<QrResultWithBounds>()
            val foundTexts = mutableSetOf<String>()

            fun tryStrategy(binarizerType: String, inverted: Boolean) {
                try {
                    val currentSource = if (inverted) source.invert() else source
                    val binarizer = if (binarizerType == "Hybrid") {
                        HybridBinarizer(currentSource)
                    } else {
                        com.google.zxing.common.GlobalHistogramBinarizer(currentSource)
                    }
                    
                    val binaryBitmap = BinaryBitmap(binarizer)
                    val rawResults = multiReader.decodeMultiple(binaryBitmap, HINTS)
                    
                    rawResults.forEach { raw ->
                        if (!foundTexts.contains(raw.text)) {
                            foundTexts.add(raw.text)
                            val bounds = computeBounds(raw.resultPoints)
                            allResults.add(QrResultWithBounds(parseResult(raw.text), raw.text, bounds))
                        }
                    }
                    android.util.Log.d("CircleToSearch", "QrScanner: Strategy $binarizerType (inverted=$inverted) found ${rawResults.size} new codes")
                } catch (e: NotFoundException) {
                    // Normal, keep going
                } catch (e: Exception) {
                    android.util.Log.e("CircleToSearch", "QrScanner: Strategy $binarizerType (inverted=$inverted) error", e)
                }
            }

            // 1. Standard Hybrid (Fast and good for most cases)
            tryStrategy("Hybrid", false)
            
            // 2. Global Histogram (Handles low contrast / lighting issues better)
            tryStrategy("Global", false)
            
            // 3. Inverted Hybrid (For light-on-dark QR codes)
            tryStrategy("Hybrid", true)
            
            // 4. Inverted Global (For light-on-dark with poor contrast)
            tryStrategy("Global", true)

            android.util.Log.d("CircleToSearch", "QrScanner: Total distinct codes found: ${allResults.size}")
            allResults
        } catch (e: Exception) {
            android.util.Log.e("CircleToSearch", "QrScanner: Fatal error in scanBitmapAll", e)
            emptyList()
        }
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
