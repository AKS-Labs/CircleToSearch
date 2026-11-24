package com.akslabs.circletosearch.utils

import android.content.Context

/**
 * Persistent whitelist of OCR languages behind Circle to Search's language
 * button. An EMPTY set is a valid saved state meaning "Auto-detect (all
 * languages)" — unchecking everything is stored like any other choice, so the
 * picker never resets behind the user's back.
 *
 * One checked code  → forced single-source OCR (PULC + CLD skipped entirely).
 * Two or more      → detection restricted to the checked languages; strips the
 *                    classifier can't place are only re-read with their
 *                    recognizers.
 */
object OcrLanguagePreferences {
    private const val FILE_NAME = "ocr_language_prefs"
    private const val KEY_CHECKED = "checked_language_codes"

    fun getChecked(context: Context): Set<String> =
        context.applicationContext
            .getSharedPreferences(FILE_NAME, Context.MODE_PRIVATE)
            .getStringSet(KEY_CHECKED, emptySet())
            .orEmpty()
            .toSet()

    fun setChecked(context: Context, codes: Set<String>) {
        context.applicationContext
            .getSharedPreferences(FILE_NAME, Context.MODE_PRIVATE)
            .edit()
            .putStringSet(KEY_CHECKED, codes)
            .apply()
    }
}
