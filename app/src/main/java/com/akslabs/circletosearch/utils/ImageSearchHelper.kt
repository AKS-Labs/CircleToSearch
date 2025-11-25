package com.akslabs.circletosearch.utils

import android.graphics.Bitmap
import android.util.Base64
import android.util.Log
import java.io.ByteArrayOutputStream

/**
 * Simplified image search helper that generates reverse image search URLs
 * using data URIs and standard search engine endpoints.
 * This approach is simpler and more reliable than uploading images.
 */
object ImageSearchHelper {
    private const val TAG = "ImageSearchHelper"
    
    /**
     * Generate a reverse image search URL for the given search engine.
     * Returns the URL immediately without any network calls.
     */
    fun getSearchUrl(bitmap: Bitmap, engine: com.akslabs.circletosearch.data.SearchEngine): String {
        return when (engine) {
            is com.akslabs.circletosearch.data.SearchEngine.Google -> getGoogleSearchUrl(bitmap)
            is com.akslabs.circletosearch.data.SearchEngine.Bing -> getBingSearchUrl(bitmap)
            is com.akslabs.circletosearch.data.SearchEngine.Yandex -> getYandexSearchUrl(bitmap)
            is com.akslabs.circletosearch.data.SearchEngine.TinEye -> getTinEyeSearchUrl(bitmap)
            is com.akslabs.circletosearch.data.SearchEngine.Baidu -> getBaiduSearchUrl(bitmap)
        }
    }
    
    /**
     * Convert bitmap to base64-encoded data URI
     */
    private fun bitmapToDataUri(bitmap: Bitmap, quality: Int = 85): String {
        val resized = ImageUtils.resizeBitmap(bitmap, 1024)
        val outputStream = ByteArrayOutputStream()
        resized.compress(Bitmap.CompressFormat.JPEG, quality, outputStream)
        val imageBytes = outputStream.toByteArray()
        val base64 = Base64.encodeToString(imageBytes, Base64.NO_WRAP)
        return "data:image/jpeg;base64,$base64"
    }
    
    /**
     * Google Lens - Uses the lens.google.com endpoint with uploadbyurl parameter
     */
    private fun getGoogleSearchUrl(bitmap: Bitmap): String {
        // Google Lens supports data URIs directly
        val dataUri = bitmapToDataUri(bitmap)
        Log.d(TAG, "Generated Google Lens URL with data URI (${dataUri.length} chars)")
        return "https://lens.google.com/uploadbyurl?url=${android.net.Uri.encode(dataUri)}"
    }
    
    /**
     * Bing Visual Search - Uses the Bing image search with data URI
     */
    private fun getBingSearchUrl(bitmap: Bitmap): String {
        // Bing also supports data URIs through their search endpoint
        val dataUri = bitmapToDataUri(bitmap)
        Log.d(TAG, "Generated Bing URL with data URI (${dataUri.length} chars)")
        // Using the regular Bing image search with imgurl parameter
        return "https://www.bing.com/images/search?view=detailv2&iss=sbi&form=SBIHMP&sbisrc=UrlPaste&q=imgurl:${android.net.Uri.encode(dataUri)}"
    }
    
    /**
     * Yandex Images - Generates a simple search URL
     * Note: Yandex may not support data URIs well, so we provide a fallback message
     */
    private fun getYandexSearchUrl(bitmap: Bitmap): String {
        // Yandex doesn't support data URIs well in their public API
        // We'll return their upload page - user will need to manually upload
        Log.d(TAG, "Yandex doesn't support direct data URIs, using upload page")
        return "https://yandex.com/images/search?rpt=imageview&url=${android.net.Uri.encode(bitmapToDataUri(bitmap))}"
    }
    
    /**
     * TinEye - Reverse image search
     * Note: TinEye requires actual upload, data URIs won't work
     */
    private fun getTinEyeSearchUrl(bitmap: Bitmap): String {
        // TinEye doesn't support data URIs, return their main page
        Log.d(TAG, "TinEye doesn't support data URIs, showing main search page")
        return "https://tineye.com/"
    }
    
    /**
     * Baidu Images - Chinese search engine
     * Note: Baidu may not support data URIs well
     */
    private fun getBaiduSearchUrl(bitmap: Bitmap): String {
        // Try Baidu with data URI, though it may not work
        val dataUri = bitmapToDataUri(bitmap)
        Log.d(TAG, "Generated Baidu URL with data URI (${dataUri.length} chars)")
        return "https://graph.baidu.com/details?isfromtusoupc=1&tn=pc&carousel=0&promotion_name=pc_image_shituindex&url=${android.net.Uri.encode(dataUri)}"
    }
}
