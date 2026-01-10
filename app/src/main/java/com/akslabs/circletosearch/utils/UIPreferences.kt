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

import android.content.Context
import android.content.SharedPreferences

class UIPreferences(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("ui_prefs", Context.MODE_PRIVATE)
    
    companion object {
        private const val KEY_DESKTOP_MODE = "is_desktop_mode"
        private const val KEY_DARK_MODE = "is_dark_mode"
        private const val KEY_SHOW_GRADIENT_BORDER = "show_gradient_border"
        private const val KEY_SHOW_FRIENDLY_MESSAGES = "show_friendly_messages"
        private const val KEY_SEARCH_ENGINE_ORDER = "search_engine_order"
        private const val KEY_USE_GOOGLE_LENS_ONLY = "use_google_lens_only"
    }

    fun isUseGoogleLensOnly(): Boolean {
        return prefs.getBoolean(KEY_USE_GOOGLE_LENS_ONLY, false)
    }

    fun setUseGoogleLensOnly(isEnabled: Boolean) {
        prefs.edit().putBoolean(KEY_USE_GOOGLE_LENS_ONLY, isEnabled).apply()
    }
    
    fun isDesktopMode(): Boolean {
        return prefs.getBoolean(KEY_DESKTOP_MODE, false)
    }
    
    fun setDesktopMode(isEnabled: Boolean) {
        prefs.edit().putBoolean(KEY_DESKTOP_MODE, isEnabled).apply()
    }
    
    fun isDarkMode(): Boolean {
        return prefs.getBoolean(KEY_DARK_MODE, false)
    }
    
    fun setDarkMode(isEnabled: Boolean) {
        prefs.edit().putBoolean(KEY_DARK_MODE, isEnabled).apply()
    }
    
    fun isShowGradientBorder(): Boolean {
        return prefs.getBoolean(KEY_SHOW_GRADIENT_BORDER, true)
    }
    
    fun setShowGradientBorder(isEnabled: Boolean) {
        prefs.edit().putBoolean(KEY_SHOW_GRADIENT_BORDER, isEnabled).apply()
    }

    fun isShowFriendlyMessages(): Boolean {
        return prefs.getBoolean(KEY_SHOW_FRIENDLY_MESSAGES, true)
    }

    fun setShowFriendlyMessages(isEnabled: Boolean) {
        prefs.edit().putBoolean(KEY_SHOW_FRIENDLY_MESSAGES, isEnabled).apply()
    }

    fun getSearchEngineOrder(): String? {
        return prefs.getString(KEY_SEARCH_ENGINE_ORDER, null)
    }

    fun setSearchEngineOrder(order: String) {
        prefs.edit().putString(KEY_SEARCH_ENGINE_ORDER, order).apply()
    }
}
