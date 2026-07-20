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

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.text.TextUtils
import com.akslabs.circletosearch.CircleToSearchAccessibilityService

object SearchTrigger {
    const val ACTION_TRIGGER_SEARCH = "com.akslabs.circletosearch.action.TRIGGER_SEARCH"
    const val EXTRA_SEARCH_MODE = "search_mode"
    const val EXTRA_COLLAPSE_SHADE = "collapse_shade"

    const val MODE_AUTO = "auto"
    const val MODE_LENS = "lens"
    const val MODE_MULTI = "multi"

    enum class Result {
        Triggered,
        AccessibilityDisabled,
        ServiceNotConnected
    }

    fun parseSearchModeOverride(mode: String?): Boolean? = when (mode?.lowercase()?.trim()) {
        MODE_LENS -> true
        MODE_MULTI -> false
        else -> null
    }

    fun parseSearchModeOverride(intent: Intent): Boolean? =
        parseSearchModeOverride(intent.getStringExtra(EXTRA_SEARCH_MODE))

    fun shouldCollapseShade(intent: Intent): Boolean =
        intent.getBooleanExtra(EXTRA_COLLAPSE_SHADE, false)

    fun resolveTriggerResult(
        accessibilityEnabled: Boolean,
        serviceConnected: Boolean
    ): Result = when {
        !accessibilityEnabled -> Result.AccessibilityDisabled
        !serviceConnected -> Result.ServiceNotConnected
        else -> Result.Triggered
    }

    fun isAccessibilityEnabled(context: Context): Boolean {
        val expected = ComponentName(context, CircleToSearchAccessibilityService::class.java)
        val enabledServices = Settings.Secure.getString(
            context.contentResolver,
            Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES
        ) ?: return false

        val splitter = TextUtils.SimpleStringSplitter(':')
        splitter.setString(enabledServices)
        while (splitter.hasNext()) {
            val enabled = ComponentName.unflattenFromString(splitter.next())
            if (enabled != null && enabled == expected) return true
        }
        return false
    }

    fun isServiceConnected(): Boolean =
        CircleToSearchAccessibilityService.instance != null

    fun triggerSearch(context: Context, searchModeOverride: Boolean? = null): Result {
        val result = resolveTriggerResult(
            accessibilityEnabled = isAccessibilityEnabled(context),
            serviceConnected = isServiceConnected()
        )
        if (result == Result.Triggered) {
            CircleToSearchAccessibilityService.triggerCapture(searchModeOverride)
        }
        return result
    }

    fun createTriggerIntent(context: Context, collapseShade: Boolean = false): Intent =
        Intent(context, com.akslabs.circletosearch.SearchTriggerActivity::class.java).apply {
            action = ACTION_TRIGGER_SEARCH
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            putExtra(EXTRA_COLLAPSE_SHADE, collapseShade)
        }
}
