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

package com.akslabs.circletosearch.utils

import android.graphics.Bitmap
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.UUID

object ImageSearchUploader {
    private const val TAG = "ImageSearchUploader"
    private const val USER_AGENT = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
    private const val TIMEOUT = 30000

    /**
     * Uploads the bitmap to imgdb.io (1-hour TTL) with Litterbox (1h) as
     * fallback. No permanent host is used: every upload must expire within
     * 1 hour. Returns a valid https URL, or null if both hosts fail.
     */
    suspend fun uploadToImageHost(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        // Try imgdb.io first (explicit 1h TTL, verified below)
        val imgdbUrl = uploadToImgdb(bitmap)
        if (imgdbUrl != null) {
            Log.d(TAG, "Successfully uploaded to imgdb.io (1h expiration)")
            return@withContext imgdbUrl
        }

        // Fallback to Litterbox (1h expiration)
        Log.w(TAG, "imgdb.io failed, falling back to Litterbox")
        val litterboxUrl = uploadToLitterbox(bitmap)
        if (litterboxUrl != null) {
            Log.d(TAG, "Successfully uploaded to Litterbox (1h expiration)")
            return@withContext litterboxUrl
        }

        Log.e(TAG, "Both imgdb.io and Litterbox uploads failed")
        null
    }

    private fun compressForUpload(bitmap: Bitmap): ByteArray {
        val resized = ImageUtils.resizeBitmap(bitmap, 1280)
        val outputStream = ByteArrayOutputStream()
        resized.compress(Bitmap.CompressFormat.JPEG, 90, outputStream)
        return outputStream.toByteArray()
    }

    private fun validHttpsUrl(candidate: String?): String? {
        val url = candidate?.trim()
        return if (url != null && url.startsWith("https://") && !url.contains(Regex("\\s"))) {
            url
        } else {
            Log.e(TAG, "Host returned invalid URL: $candidate")
            null
        }
    }

    /**
     * Uploads to imgdb.io with an explicit 1-hour TTL (ttl=3600). The
     * response's `expires` field is verified to be at most ~1 hour ahead —
     * if the server ever ignored the TTL, the link is rejected rather than
     * used beyond the promised lifetime.
     */
    private suspend fun uploadToImgdb(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        try {
            val boundary = "----WebKitFormBoundary" + UUID.randomUUID().toString().replace("-", "")
            val url = URL("https://imgdb.io/api/v1/upload?ttl=3600")

            val connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                doOutput = true
                doInput = true
                useCaches = false
                connectTimeout = TIMEOUT
                readTimeout = TIMEOUT
                setRequestProperty("User-Agent", USER_AGENT)
                setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
            }

            val imageBytes = compressForUpload(bitmap)
            Log.d(TAG, "Uploading to imgdb.io: ${imageBytes.size} bytes (ttl=3600s)")

            DataOutputStream(connection.outputStream).use { dos ->
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"file\"; filename=\"image.jpg\"\r\n")
                dos.writeBytes("Content-Type: image/jpeg\r\n\r\n")
                dos.write(imageBytes)
                dos.writeBytes("\r\n")
                dos.writeBytes("--$boundary--\r\n")
                dos.flush()
            }

            val responseCode = connection.responseCode
            val body = (if (responseCode in 200..299) connection.inputStream else connection.errorStream)
                ?.bufferedReader()?.use { it.readText() }
            if (responseCode !in 200..299) {
                Log.e(TAG, "imgdb.io upload failed: $responseCode $body")
                return@withContext null
            }

            val json = org.json.JSONObject(body ?: "")
            val imageUrl = validHttpsUrl(json.optString("url").ifEmpty { null })
                ?: return@withContext null

            // Enforce the 1-hour lifetime: reject anything expiring later.
            val expires = json.optLong("expires", -1L)
            val now = System.currentTimeMillis() / 1000
            if (expires <= 0 || expires - now > 3600 + 60) {
                Log.e(TAG, "imgdb.io link exceeds 1h lifetime (expires=$expires), rejecting")
                return@withContext null
            }
            Log.d(TAG, "imgdb.io URL: $imageUrl (expires in ${expires - now}s)")
            imageUrl
        } catch (e: Exception) {
            Log.e(TAG, "imgdb.io upload error", e)
            null
        }
    }

    /**
     * Uploads to Litterbox.catbox.moe with 1-hour expiration for privacy
     */
    private suspend fun uploadToLitterbox(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        try {
            val boundary = "----WebKitFormBoundary" + UUID.randomUUID().toString().replace("-", "")
            val url = URL("https://litterbox.catbox.moe/resources/internals/api.php")

            val connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                doOutput = true
                doInput = true
                useCaches = false
                connectTimeout = TIMEOUT
                readTimeout = TIMEOUT
                setRequestProperty("User-Agent", USER_AGENT)
                setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
            }

            val imageBytes = compressForUpload(bitmap)
            Log.d(TAG, "Uploading to Litterbox: ${imageBytes.size} bytes")

            DataOutputStream(connection.outputStream).use { dos ->
                // reqtype=fileupload
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"reqtype\"\r\n\r\n")
                dos.writeBytes("fileupload\r\n")

                // time=1h (1 hour expiration)
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"time\"\r\n\r\n")
                dos.writeBytes("1h\r\n")

                // fileToUpload
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"fileToUpload\"; filename=\"image.jpg\"\r\n")
                dos.writeBytes("Content-Type: image/jpeg\r\n\r\n")
                dos.write(imageBytes)
                dos.writeBytes("\r\n")

                dos.writeBytes("--$boundary--\r\n")
                dos.flush()
            }

            val responseCode = connection.responseCode
            if (responseCode in 200..299) {
                val raw = connection.inputStream.bufferedReader().use { it.readText() }
                val imageUrl = validHttpsUrl(raw)
                if (imageUrl != null) Log.d(TAG, "Litterbox URL: $imageUrl (expires in 1h)")
                imageUrl
            } else {
                Log.e(TAG, "Litterbox upload failed: $responseCode")
                null
            }
        } catch (e: Exception) {
            Log.e(TAG, "Litterbox upload error", e)
            null
        }
    }

    // --- URL Generators ---

    fun getGoogleLensUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://lens.google.com/uploadbyurl?url=$encodedUrl"
    }
    
    fun getGoogleImagesUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://www.google.com/searchbyimage?image_url=$encodedUrl"
    }

    fun getBingUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        // Simplified Bing URL
        return "https://www.bing.com/images/search?view=detailv2&iss=sbi&q=imgurl:$encodedUrl"
    }

    fun getYandexUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://yandex.com/images/search?rpt=imageview&url=$encodedUrl"
    }

    fun getTinEyeUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://tineye.com/search?url=$encodedUrl"
    }
}
