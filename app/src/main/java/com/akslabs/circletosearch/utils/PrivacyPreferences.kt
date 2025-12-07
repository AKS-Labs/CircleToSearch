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
