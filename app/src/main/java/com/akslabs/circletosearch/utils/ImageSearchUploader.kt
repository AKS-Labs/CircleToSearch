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
     * Uploads the bitmap to Litterbox (1-hour temporary storage).
     * Litterbox is used for privacy as images auto-delete after 1 hour.
     * No permanent fallback — user privacy is non-negotiable.
     */
    suspend fun uploadToImageHost(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        val litterboxUrl = uploadToLitterbox(bitmap)
        if (litterboxUrl != null) {
            Log.d(TAG, "Successfully uploaded to Litterbox (1h expiration)")
            return@withContext litterboxUrl
        }
        
        Log.e(TAG, "Litterbox upload failed")
        null
    }
    
    /**
     * Uploads to Litterbox.catbox.moe with 1-hour expiration for privacy
     */
    private suspend fun uploadToLitterbox(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        var connection: HttpURLConnection? = null
        try {
            val boundary = "----WebKitFormBoundary" + UUID.randomUUID().toString().replace("-", "")
            val url = URL("https://litterbox.catbox.moe/resources/internals/api.php")
            
            connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                doOutput = true
                doInput = true
                useCaches = false
                connectTimeout = TIMEOUT
                readTimeout = TIMEOUT
                setRequestProperty("User-Agent", USER_AGENT)
                setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
            }
            
            // Resize and compress image
            val resized = ImageUtils.resizeBitmap(bitmap, 1280)
            val outputStream = ByteArrayOutputStream()
            resized.compress(Bitmap.CompressFormat.JPEG, 90, outputStream)
            val imageBytes = outputStream.toByteArray()
            
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
            if (responseCode == 200) {
                val imageUrl = connection.inputStream.bufferedReader().use { it.readText() }
                Log.d(TAG, "Litterbox URL: $imageUrl (expires in 1h)")
                imageUrl
            } else {
                Log.e(TAG, "Litterbox upload failed: $responseCode")
                null
            }
        } catch (e: Exception) {
            Log.e(TAG, "Litterbox upload error", e)
            null
        } finally {
            connection?.disconnect()
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
