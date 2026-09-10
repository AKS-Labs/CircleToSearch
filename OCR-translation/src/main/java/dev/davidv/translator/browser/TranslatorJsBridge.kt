/*
 * Copyright (C) 2024 David V
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

package dev.davidv.translator.browser

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import android.util.Log
import android.webkit.JavascriptInterface
import android.webkit.WebView
import dev.davidv.translator.Language
import dev.davidv.translator.TranslationCoordinator
import dev.davidv.translator.TranslationService
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.ByteArrayOutputStream

class TranslatorJsBridge(
  private val scope: CoroutineScope,
  private val webView: WebView,
  private val translationService: TranslationService,
  private val translationCoordinator: TranslationCoordinator,
  private val bridgeToken: String,
  @Volatile var sourceLanguage: Language,
  @Volatile var targetLanguage: Language,
) {
  companion object {
    private const val MAX_BATCH_ITEMS = 50
    private const val MAX_TOTAL_CHARS = 100_000
    private const val MAX_ITEM_CHARS = 20_000
  }

  @JavascriptInterface
  fun translateHtmlFragments(
    requestId: Int,
    fragmentsPayload: String,
    token: String,
    nonce: String,
  ) {
    if (token != bridgeToken) return
    scope.launch {
      val fragments = decodeWire(fragmentsPayload)
      if (fragments == null) {
        resolveOnJs(requestId, emptyList(), nonce)
        return@launch
      }
      val translated =
        try {
          translationService.translateHtmlFragments(sourceLanguage, targetLanguage, fragments)
        } catch (t: Throwable) {
          Log.w("TranslatorJsBridge", "translateHtmlFragments failed", t)
          fragments
        }
      resolveOnJs(requestId, translated, nonce)
    }
  }

  @JavascriptInterface
  fun translateTexts(
    requestId: Int,
    textsPayload: String,
    token: String,
    nonce: String,
  ) {
    if (token != bridgeToken) return
    scope.launch {
      val texts = decodeWire(textsPayload)
      if (texts == null) {
        resolveOnJs(requestId, emptyList(), nonce)
        return@launch
      }
      val translated =
        try {
          val result =
            translationService.translateMixedTexts(
              texts,
              sourceLanguage,
              targetLanguage,
              listOf(sourceLanguage, targetLanguage),
            )
          mapMixedTextsToInputOrder(texts, result)
        } catch (t: Throwable) {
          Log.w("TranslatorJsBridge", "translateTexts failed", t)
          texts
        }
      resolveOnJs(requestId, translated, nonce)
    }
  }

  @JavascriptInterface
  fun translateImage(
    requestId: Int,
    dataUri: String,
    token: String,
    nonce: String,
  ) {
    if (token != bridgeToken) return
    Log.d("TranslatorJsBridge", "translateImage requestId=$requestId payload=${dataUri.length}B")
    scope.launch {
      val output =
        if (dataUri.isEmpty()) {
          ""
        } else {
          try {
            translateImageDataUri(dataUri)
          } catch (t: Throwable) {
            Log.w("TranslatorJsBridge", "translateImage failed", t)
            ""
          }
        }
      Log.d("TranslatorJsBridge", "translateImage requestId=$requestId result=${output.length}B")
      resolveRawOnJs(requestId, output, nonce)
    }
  }

  private suspend fun translateImageDataUri(dataUri: String): String {
    if (!dataUri.startsWith("data:")) return ""
    val comma = dataUri.indexOf(',')
    if (comma < 0) return ""
    val header = dataUri.substring(5, comma)
    if (!header.contains("base64")) return ""
    val bytes =
      withContext(Dispatchers.Default) {
        Base64.decode(dataUri.substring(comma + 1), Base64.DEFAULT)
      }
    val source = BitmapFactory.decodeByteArray(bytes, 0, bytes.size) ?: return ""
    val result =
      translationCoordinator.translateImageWithOverlay(
        from = sourceLanguage,
        to = targetLanguage,
        finalBitmap = source,
        onMessage = {},
      )
    if (!source.isRecycled) source.recycle()
    if (result == null) return ""
    return withContext(Dispatchers.Default) { encodeBitmapAsDataUri(result.correctedBitmap) }
  }

  private fun encodeBitmapAsDataUri(bitmap: Bitmap): String {
    val baos = ByteArrayOutputStream()
    bitmap.compress(Bitmap.CompressFormat.JPEG, 90, baos)
    val b64 = Base64.encodeToString(baos.toByteArray(), Base64.NO_WRAP)
    return "data:image/jpeg;base64,$b64"
  }

  private fun mapMixedTextsToInputOrder(
    inputs: List<String>,
    result: dev.davidv.translator.BatchTextTranslationOutput,
  ): List<String> =
    when (result) {
      is dev.davidv.translator.BatchTextTranslationOutput.Translated ->
        inputs.map { result.results[it] ?: it }
      is dev.davidv.translator.BatchTextTranslationOutput.NothingToTranslate -> inputs
    }

  private suspend fun resolveOnJs(
    requestId: Int,
    translated: List<String>,
    nonce: String,
  ) {
    val payload = JSONObject.quote(encodeWire(translated))
    val jsNonce = JSONObject.quote(nonce)
    withContext(Dispatchers.Main) {
      webView.evaluateJavascript(
        "window.__translator && window.__translator.resolve($requestId, $payload, $jsNonce);",
        null,
      )
    }
  }

  private suspend fun resolveRawOnJs(
    requestId: Int,
    payload: String,
    nonce: String,
  ) {
    val jsPayload = JSONObject.quote(payload)
    val jsNonce = JSONObject.quote(nonce)
    withContext(Dispatchers.Main) {
      webView.evaluateJavascript(
        "window.__translator && window.__translator.resolveRaw($requestId, $jsPayload, $jsNonce);",
        null,
      )
    }
  }

  // Wire format: length-prefixed records `<len>:<text><len>:<text>...`
  // where `len` is the UTF-16 code-unit count (matches String.length on
  // both sides). Robust to any content; cheaper than JSON because no
  // per-char escape work.
  private fun decodeWire(payload: String): List<String>? {
    if (payload.isEmpty()) return emptyList()
    if (payload.length > MAX_TOTAL_CHARS + MAX_BATCH_ITEMS * 8) return null
    val items = ArrayList<String>()
    var i = 0
    val n = payload.length
    while (i < n) {
      val colon = payload.indexOf(':', i)
      if (colon < 0) return null
      val len =
        try {
          payload.substring(i, colon).toInt()
        } catch (_: NumberFormatException) {
          return null
        }
      if (len < 0 || len > MAX_ITEM_CHARS) return null
      val start = colon + 1
      val end = start + len
      if (end > n) return null
      items.add(payload.substring(start, end))
      if (items.size > MAX_BATCH_ITEMS) return null
      i = end
    }
    return items
  }

  private fun encodeWire(values: List<String>): String {
    val sb = StringBuilder()
    for (v in values) {
      sb.append(v.length).append(':').append(v)
    }
    return sb.toString()
  }
}
