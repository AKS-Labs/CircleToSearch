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

sealed class SearchEngine(@StringRes val displayNameRes: Int, @DrawableRes val iconRes: Int) {
    object Google : SearchEngine(R.string.engine_google, R.drawable.ic_engine_google)
    object Bing : SearchEngine(R.string.engine_bing, R.drawable.ic_engine_bing)
    object Yandex : SearchEngine(R.string.engine_yandex, R.drawable.ic_engine_yandex)
    object TinEye : SearchEngine(R.string.engine_tineye, R.drawable.ic_engine_tineye)

    companion object {
        fun values(): List<SearchEngine> = listOf(Google, Bing, Yandex, TinEye)
    }

    fun getDisplayName(context: Context): String = context.getString(displayNameRes)

    val name: String get() = when (this) {
        is Google -> "Google"
        is Bing -> "Bing"
        is Yandex -> "Yandex"
        is TinEye -> "TinEye"
    }
}

val SearchEngine.isDirectUpload: Boolean
    get() = false
