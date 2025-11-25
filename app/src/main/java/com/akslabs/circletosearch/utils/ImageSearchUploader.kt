package com.akslabs.circletosearch.utils

import android.graphics.Bitmap
import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.UUID
import java.io.BufferedReader
import java.io.InputStreamReader

object ImageSearchUploader {
    private const val TAG = "ImageSearchUploader"
    private const val TIMEOUT = 15000
    private const val USER_AGENT = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"

    private suspend fun performUpload(
        urlString: String,
        bitmap: Bitmap,
        imageFieldName: String,
        fileName: String = "image.jpg",
        onConnection: (HttpURLConnection) -> Unit = {},
        parseResponse: (HttpURLConnection) -> String?
    ): String? = withContext(Dispatchers.IO) {
        val boundary = "Boundary-${UUID.randomUUID()}"
        val url = URL(urlString)

        try {
            val connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "POST"
                doOutput = true
                doInput = true
                useCaches = false
                connectTimeout = TIMEOUT
                readTimeout = TIMEOUT
                setRequestProperty("User-Agent", USER_AGENT)
                setRequestProperty("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
                setRequestProperty("Accept-Language", "en-US,en;q=0.9")
                setRequestProperty("Content-Type", "multipart/form-data; boundary=$boundary")
                instanceFollowRedirects = false // Important for parsing redirects
                onConnection(this)
            }

            val resized = ImageUtils.resizeBitmap(bitmap, 1024)
            val outputStream = ByteArrayOutputStream()
            resized.compress(Bitmap.CompressFormat.JPEG, 85, outputStream)
            val imageBytes = outputStream.toByteArray()

            DataOutputStream(connection.outputStream).use { dos ->
                dos.writeBytes("--$boundary\r\n")
                dos.writeBytes("Content-Disposition: form-data; name=\"$imageFieldName\"; filename=\"$fileName\"\r\n")
                dos.writeBytes("Content-Type: image/jpeg\r\n\r\n")
                dos.write(imageBytes)
                dos.writeBytes("\r\n--$boundary--\r\n")
                dos.flush()
            }

            return@withContext parseResponse(connection)

        } catch (e: Exception) {
            Log.e(TAG, "Upload failed for $urlString", e)
        }
        return@withContext null
    }

    suspend fun uploadToGoogle(bitmap: Bitmap): String? {
        val urlString = "https://lens.google.com/upload?ep=ccm&s=android&st=${System.currentTimeMillis()}"
        return performUpload(urlString, bitmap, "encoded_image", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "Google Upload Response Code: $responseCode")
            if (responseCode == HttpURLConnection.HTTP_OK) {
                val reader = BufferedReader(InputStreamReader(connection.inputStream))
                val response = reader.readText()
                reader.close()

                // 1) Try explicit window.location.replace('URL')
                val replaceRegex = """location\.replace\(['\"](.*?)['\"]\)""".toRegex()
                val replaceMatch = replaceRegex.find(response)
                val replaceUrl = replaceMatch?.groups?.get(1)?.value?.replace("&amp;", "&")
                if (!replaceUrl.isNullOrBlank()) {
                    Log.d(TAG, "Google Redirect URL via replace(): $replaceUrl")
                    return@performUpload replaceUrl
                }

                // 2) Try noscript fallback link to enable JS or direct search
                val hrefRegex = """<a\s+href=\"(/[^\"]+)\"\>here\<""".toRegex(RegexOption.IGNORE_CASE)
                val hrefMatch = hrefRegex.find(response)
                val hrefUrl = hrefMatch?.groups?.get(1)?.value?.replace("&amp;", "&")
                if (!hrefUrl.isNullOrBlank()) {
                    val absolute = if (hrefUrl.startsWith("http")) hrefUrl else "https://lens.google.com$hrefUrl"
                    Log.d(TAG, "Google Fallback URL via noscript: $absolute")
                    return@performUpload absolute
                }

                // 3) Try to find any URL-like string pointing to lens/search
                val genericRegex = """https?://[^"]*lens\.google\.com[^"]*""".toRegex()
                val genericMatch = genericRegex.find(response)
                val genericUrl = genericMatch?.value
                if (!genericUrl.isNullOrBlank()) {
                    Log.d(TAG, "Google Generic URL: $genericUrl")
                    return@performUpload genericUrl
                }

                Log.e(TAG, "Google: Could not parse redirect URL from response.")
                null
            } else if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                // Some regions may redirect directly
                val redirectUrl = connection.getHeaderField("Location")
                if (!redirectUrl.isNullOrBlank()) {
                    Log.d(TAG, "Google Redirect via header: $redirectUrl")
                    redirectUrl
                } else {
                    Log.e(TAG, "Google: Redirect without Location header")
                    null
                }
            } else {
                Log.e(TAG, "Google: Invalid response code: $responseCode")
                null
            }
        })
    }

    suspend fun uploadToBing(bitmap: Bitmap): String? {
        val uploadUrl = "https://www.bing.com/images/visualsearch/upload"
        return performUpload(uploadUrl, bitmap, "image", fileName = "image.jpg", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "Bing Upload Response Code: $responseCode")
            // Bing redirects to the results page. The URL is in the 'Location' header.
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                val redirectUrl = connection.getHeaderField("Location")
                if (redirectUrl != null) {
                    val finalUrl = if (redirectUrl.startsWith("http")) redirectUrl else "https://www.bing.com$redirectUrl"
                    Log.d(TAG, "Bing Redirect URL: $finalUrl")
                    finalUrl
                } else {
                    Log.e(TAG, "Bing: Could not get redirect URL from header.")
                    null
                }
            } else {
                Log.e(TAG, "Bing: Invalid response code: $responseCode")
                null
            }
        })
    }

    suspend fun uploadToYandex(bitmap: Bitmap): String? {
        val urlString = "https://yandex.com/images-search"
        return performUpload(urlString, bitmap, "upfile", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "Yandex Upload Response Code: $responseCode")
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                val redirectUrl = connection.getHeaderField("Location")
                if (redirectUrl != null) {
                    // Yandex provides a full URL in the location header
                    Log.d(TAG, "Yandex Redirect URL: $redirectUrl")
                    redirectUrl
                } else {
                    Log.e(TAG, "Yandex: Could not get redirect URL from header.")
                    null
                }
            } else {
                Log.e(TAG, "Yandex: Invalid response code: $responseCode")
                null
            }
        })
    }

    suspend fun uploadToTinEye(bitmap: Bitmap): String? {
        val urlString = "https://tineye.com/search"
        return performUpload(urlString, bitmap, "image", fileName = "image.jpg", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "TinEye Upload Response Code: $responseCode")
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM || responseCode == 303) {
                val redirectUrl = connection.getHeaderField("Location")
                if (!redirectUrl.isNullOrBlank()) {
                    val finalUrl = if (redirectUrl.startsWith("http")) redirectUrl else "https://tineye.com$redirectUrl"
                    Log.d(TAG, "TinEye Redirect URL: $finalUrl")
                    finalUrl
                } else {
                    Log.e(TAG, "TinEye: Could not get redirect URL from header.")
                    null
                }
            } else {
                Log.e(TAG, "TinEye: Invalid response code: $responseCode")
                null
            }
        })
    }
}
