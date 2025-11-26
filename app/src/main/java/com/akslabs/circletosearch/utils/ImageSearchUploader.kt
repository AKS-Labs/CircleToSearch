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
     * Uploads the bitmap to Catbox.moe and returns the public URL
     */
    suspend fun uploadToImageHost(bitmap: Bitmap): String? = withContext(Dispatchers.IO) {
        try {
            val boundary = "----WebKitFormBoundary" + UUID.randomUUID().toString().replace("-", "")
            val url = URL("https://catbox.moe/user/api.php")
            
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
            
            // Resize and compress image
            val resized = ImageUtils.resizeBitmap(bitmap, 1280) // Good quality for search
            val outputStream = ByteArrayOutputStream()
            resized.compress(Bitmap.CompressFormat.JPEG, 90, outputStream)
            val imageBytes = outputStream.toByteArray()
            
            Log.d(TAG, "Uploading to Catbox: ${imageBytes.size} bytes")
            
            DataOutputStream(connection.outputStream).use { dos ->
                // reqtype=fileupload
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"reqtype\"\r\n\r\n")
                dos.writeBytes("fileupload\r\n")
                
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
                Log.d(TAG, "Catbox URL: $imageUrl")
                imageUrl
            } else {
                Log.e(TAG, "Catbox upload failed: $responseCode")
                null
            }
        } catch (e: Exception) {
            Log.e(TAG, "Host upload failed", e)
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

    fun getYahooUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        // Yahoo uses Bing's index, but we can try this format
        return "https://images.search.yahoo.com/search/images?p=imgurl:$encodedUrl"
    }

    fun getTinEyeUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://tineye.com/search?url=$encodedUrl"
    }

    fun getBaiduUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://graph.baidu.com/details?isfromtoybox=1&searchtype=searchSimple&objurl=$encodedUrl"
    }

    fun getSauceNAOUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://saucenao.com/search.php?db=999&url=$encodedUrl"
    }

    fun getIQDBUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://iqdb.org/?url=$encodedUrl"
    }

    fun getASCII2DUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        return "https://ascii2d.net/search/url/$encodedUrl"
    }
    
    fun getLensoUrl(imageUrl: String): String {
        val encodedUrl = URLEncoder.encode(imageUrl, "UTF-8")
        // Try passing URL as query param, though Lenso usually requires upload
        return "https://lenso.ai/en/results?url=$encodedUrl"
    }
}
