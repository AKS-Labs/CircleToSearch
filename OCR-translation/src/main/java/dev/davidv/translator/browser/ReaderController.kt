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

import android.webkit.WebView
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject
import org.json.JSONTokener

sealed class ReaderState {
  data object Idle : ReaderState()

  data object Available : ReaderState()

  data class Active(val originalUrl: String) : ReaderState()
}

data class ReaderPageState(
  val reader: ReaderState = ReaderState.Idle,
  val pendingActivation: Boolean = false,
)

class ReaderController(
  private val readabilityScript: String,
  private val readerModeScript: String,
) {
  private val _state = MutableStateFlow(ReaderPageState())
  val state: StateFlow<ReaderPageState> = _state.asStateFlow()

  fun onPageStarted() {
    val current = _state.value
    if (current.pendingActivation) {
      _state.value = current.copy(pendingActivation = false)
      return
    }
    if (current != ReaderPageState()) _state.value = ReaderPageState()
  }

  fun probeOnPageFinished(webView: WebView) {
    if (_state.value.reader is ReaderState.Active) return
    webView.evaluateJavascript(buildReaderModeJs(probeOnly = true)) { result ->
      val cur = _state.value
      if (cur.reader is ReaderState.Active) return@evaluateJavascript
      val available = result == "true"
      _state.value =
        cur.copy(
          reader = if (available) ReaderState.Available else ReaderState.Idle,
        )
    }
  }

  fun toggle(
    webView: WebView,
    onUnavailable: () -> Unit,
  ) {
    when (val r = _state.value.reader) {
      ReaderState.Idle -> {}
      ReaderState.Available -> activate(webView, onUnavailable)
      is ReaderState.Active -> deactivate(webView, r.originalUrl)
    }
  }

  private fun activate(
    webView: WebView,
    onUnavailable: () -> Unit,
  ) {
    val originalUrl = webView.url ?: return
    webView.evaluateJavascript(buildReaderModeJs(probeOnly = false)) { result ->
      val html = decodeJavascriptString(result)
      if (html == null) {
        _state.value = _state.value.copy(reader = ReaderState.Idle)
        onUnavailable()
        return@evaluateJavascript
      }
      _state.value =
        ReaderPageState(
          reader = ReaderState.Active(originalUrl),
          pendingActivation = true,
        )
      webView.loadDataWithBaseURL(originalUrl, html, "text/html", "UTF-8", originalUrl)
    }
  }

  private fun deactivate(
    webView: WebView,
    originalUrl: String,
  ) {
    _state.value = ReaderPageState()
    if (webView.canGoBack()) webView.goBack() else webView.loadUrl(originalUrl)
  }

  private fun buildReaderModeJs(probeOnly: Boolean): String {
    val readabilityLiteral = JSONObject.quote(readabilityScript)
    val readerModeLiteral = JSONObject.quote(readerModeScript)
    return "window.__translatorReadabilityScript = $readabilityLiteral;\n" +
      "window.__translatorReaderModeProbe = $probeOnly;\n" +
      "(0, eval)($readerModeLiteral);"
  }

  private fun decodeJavascriptString(value: String): String? {
    if (value == "null") return null
    return runCatching { JSONTokener(value).nextValue() as? String }.getOrNull()
  }
}
