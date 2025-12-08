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

class PrivacyPreferences(context: Context) {
    private val prefs: SharedPreferences = context.getSharedPreferences("privacy_prefs", Context.MODE_PRIVATE)
    
    companion object {
        private const val KEY_PRIVACY_ACCEPTED = "privacy_accepted"
    }
    
    fun hasAcceptedPrivacyPolicy(): Boolean {
        return prefs.getBoolean(KEY_PRIVACY_ACCEPTED, false)
    }
    
    fun setPrivacyAccepted() {
        prefs.edit().putBoolean(KEY_PRIVACY_ACCEPTED, true).apply()
    }
}
