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
    // Use Desktop UA to force classic redirect behavior
    private const val USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"

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
        // Use the classic endpoint which is more likely to redirect
        val urlString = "https://www.google.com/searchbyimage/upload"
        return performUpload(urlString, bitmap, "encoded_image", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "Google Upload Response Code: $responseCode")
            
            if (responseCode == HttpURLConnection.HTTP_OK) {
                // Sometimes it returns 200 with a JS redirect
                val reader = BufferedReader(InputStreamReader(connection.inputStream))
                val response = reader.readText()
                reader.close()
                
                // Regex for window.location.replace or href
                val replaceMatch = """window\.location\.replace\(['"](.*?)['"]\)""".toRegex().find(response)
                if (replaceMatch != null) return@performUpload replaceMatch.groupValues[1]
                
                val hrefMatch = """href="(/search\?tbs=sbi:[^"]+)""".toRegex().find(response)
                if (hrefMatch != null) return@performUpload "https://www.google.com" + hrefMatch.groupValues[1]

                Log.e(TAG, "Google: 200 OK but no redirect found.")
                null
            } else if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                val redirectUrl = connection.getHeaderField("Location")
                Log.d(TAG, "Google Redirect URL: $redirectUrl")
                redirectUrl
            } else {
                null
            }
        })
    }

    suspend fun uploadToBing(bitmap: Bitmap): String? {
        val uploadUrl = "https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP"
        return performUpload(uploadUrl, bitmap, "imageBin", fileName = "image.jpg", parseResponse = { connection ->
            val responseCode = connection.responseCode
            Log.d(TAG, "Bing Upload Response Code: $responseCode")
            
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                val loc = connection.getHeaderField("Location")
                if (loc != null && !loc.startsWith("http")) "https://www.bing.com$loc" else loc
            } else if (responseCode == HttpURLConnection.HTTP_OK) {
                val reader = BufferedReader(InputStreamReader(connection.inputStream))
                val response = reader.readText()
                reader.close()
                
                Log.d(TAG, "Bing 200 OK Body (First 1000 chars): ${response.take(1000)}")

                // 1. Try og:url
                // <meta property="og:url" content="https://..." />
                val ogUrlRegex = """<meta\s+property="og:url"\s+content="([^"]+)"""".toRegex(RegexOption.IGNORE_CASE)
                val ogUrlMatch = ogUrlRegex.find(response)
                if (ogUrlMatch != null) {
                    val url = ogUrlMatch.groupValues[1].replace("&amp;", "&")
                    Log.d(TAG, "Bing: Found URL in og:url: $url")
                    return@performUpload url
                }

                // 2. Try refresh meta tag
                // <meta http-equiv="refresh" content="0;url=..." />
                val refreshRegex = """<meta\s+http-equiv="refresh"\s+content="[^;]+;url=([^"]+)"""".toRegex(RegexOption.IGNORE_CASE)
                val refreshMatch = refreshRegex.find(response)
                if (refreshMatch != null) {
                    val url = refreshMatch.groupValues[1].replace("&amp;", "&")
                    Log.d(TAG, "Bing: Found URL in refresh meta: $url")
                    return@performUpload url
                }
                
                // 3. Try window.location.replace
                val jsRegex = """window\.location\.replace\(['"](.*?)['"]\)""".toRegex()
                val jsMatch = jsRegex.find(response)
                if (jsMatch != null) {
                     val url = jsMatch.groupValues[1].replace("\\x3a", ":").replace("\\x2f", "/")
                     Log.d(TAG, "Bing: Found URL in JS replace: $url")
                     return@performUpload url
                }

                Log.w(TAG, "Bing returned 200. Cannot display result without a GET URL.")
                null
            } else {
                null
            }
        })
    }

    suspend fun uploadToYandex(bitmap: Bitmap): String? {
        val urlString = "https://yandex.com/images/search?rpt=imageview"
        return performUpload(urlString, bitmap, "upfile", parseResponse = { connection ->
            val responseCode = connection.responseCode
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM) {
                val loc = connection.getHeaderField("Location")
                if (loc != null && !loc.startsWith("http")) "https://yandex.com$loc" else loc
            } else if (responseCode == HttpURLConnection.HTTP_OK) {
                // Yandex sometimes returns JSON
                val reader = BufferedReader(InputStreamReader(connection.inputStream))
                val response = reader.readText()
                reader.close()
                try {
                    val json = JSONObject(response)
                    // Check for blocks -> content -> ... -> url? 
                    // Or sometimes it's just a redirect in HTML.
                    null
                } catch (e: Exception) { null }
            } else {
                null
            }
        })
    }

    suspend fun uploadToTinEye(bitmap: Bitmap): String? {
        val urlString = "https://tineye.com/search"
        return performUpload(urlString, bitmap, "image", fileName = "image.jpg", parseResponse = { connection ->
            val responseCode = connection.responseCode
            if (responseCode == HttpURLConnection.HTTP_MOVED_TEMP || responseCode == HttpURLConnection.HTTP_MOVED_PERM || responseCode == 303) {
                val loc = connection.getHeaderField("Location")
                if (loc != null && !loc.startsWith("http")) "https://tineye.com$loc" else loc
            } else {
                null
            }
        })
    }
    
    suspend fun uploadToBaidu(bitmap: Bitmap): String? {
        val urlString = "https://graph.baidu.com/upload"
        return performUpload(urlString, bitmap, "image", fileName = "image.jpg", onConnection = { connection ->
             connection.setRequestProperty("Referer", "https://graph.baidu.com/pcpage/index?tpl_from=pc")
             connection.setRequestProperty("Origin", "https://graph.baidu.com")
        }, parseResponse = { connection ->
             val responseCode = connection.responseCode
             if (responseCode == HttpURLConnection.HTTP_OK) {
                 val reader = BufferedReader(InputStreamReader(connection.inputStream))
                 val response = reader.readText()
                 reader.close()
                 try {
                     val json = JSONObject(response)
                     val data = json.optJSONObject("data")
                     data?.optString("url")
                 } catch (e: Exception) { null }
             } else {
                 null
             }
        })
    }
}
