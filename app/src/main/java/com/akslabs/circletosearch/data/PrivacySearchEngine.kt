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

package com.akslabs.circletosearch.data

import android.content.Context
import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import com.akslabs.circletosearch.R
import java.net.URLEncoder

/**
 * Privacy-focused web search engines for the text-selection "Web search" action. Unlike
 * [SearchEngine] (visual/reverse image search), these run inside the app's web-search bottom
 * sheet: one tab per engine, results loaded lazily in an in-app WebView.
 */
sealed class PrivacySearchEngine(
    @StringRes val displayNameRes: Int,
    @DrawableRes val iconRes: Int,
) {
    object DuckDuckGo : PrivacySearchEngine(R.string.engine_duckduckgo, R.drawable.ic_engine_ddg)
    object Brave : PrivacySearchEngine(R.string.engine_brave, R.drawable.ic_engine_brave)
    object Startpage : PrivacySearchEngine(R.string.engine_startpage, R.drawable.ic_engine_startpage)
    object Mojeek : PrivacySearchEngine(R.string.engine_mojeek, R.drawable.ic_engine_mojeek)

    /** DuckDuckGo's AI chat. Duck.ai blocks programmatic prompting (abuse prevention), so the
     *  query is pre-filled via the ?q= deep link and the user sends it themselves. */
    object DuckAI : PrivacySearchEngine(R.string.engine_duckai, R.drawable.ic_engine_duckai)

    /** Brave's AI mode ("Ask Brave" at search.brave.com/ai). */
    object BraveAI : PrivacySearchEngine(R.string.engine_braveai, R.drawable.ic_engine_brave)

    companion object {
        fun values(): List<PrivacySearchEngine> = listOf(DuckDuckGo, Brave, Startpage, Mojeek, DuckAI, BraveAI)
    }

    val name: String
        get() = when (this) {
            is DuckDuckGo -> "DuckDuckGo"
            is Brave -> "Brave Search"
            is Startpage -> "Startpage"
            is Mojeek -> "Mojeek"
            is DuckAI -> "Duck.ai"
            is BraveAI -> "Brave AI"
        }

    fun getDisplayName(context: Context): String = context.getString(displayNameRes)

    /** The engine's web-results URL for [query] (form-encoded). Duck.ai accepts the prompt
     *  via `?q=` only when opening a FRESH chat — the loader clears duck.ai cookies before
     *  loading so prefill works every time. */
    fun searchUrl(query: String): String {
        val q = URLEncoder.encode(query, "UTF-8")
        return when (this) {
            is DuckDuckGo -> "https://duckduckgo.com/?q=$q"
            is Brave -> "https://search.brave.com/search?q=$q"
            is Startpage -> "https://www.startpage.com/sp/search?query=$q"
            is Mojeek -> "https://www.mojeek.com/search?q=$q"
            is DuckAI -> "https://duck.ai/?q=$q"
            is BraveAI -> "https://search.brave.com/ai?q=$q"
        }
    }
}
