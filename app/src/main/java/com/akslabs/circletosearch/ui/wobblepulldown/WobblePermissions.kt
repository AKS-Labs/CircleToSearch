package com.akslabs.circletosearch.ui.wobblepulldown

import android.accessibilityservice.AccessibilityServiceInfo
import android.app.role.RoleManager
import android.content.Context
import android.os.Build
import android.provider.Settings
import android.view.accessibility.AccessibilityManager
import com.akslabs.circletosearch.CircleToSearchAccessibilityService
import com.akslabs.circletosearch.isAccessibilityServiceEnabled
import com.akslabs.circletosearch.isDefaultAssistant

internal fun isA11yGranted(context: Context): Boolean {
    if (isAccessibilityServiceEnabled(context)) return true
    if (CircleToSearchAccessibilityService.instance != null) return true
    return runCatching {
        val manager = context.getSystemService(Context.ACCESSIBILITY_SERVICE)
            as? AccessibilityManager ?: return false
        manager.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
            .any { it.resolveInfo?.serviceInfo?.packageName == context.packageName }
    }.getOrDefault(false)
}

internal fun isAssistantGranted(context: Context): Boolean {
    if (isDefaultAssistant(context)) return true
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
        runCatching {
            val roleManager = context.getSystemService(RoleManager::class.java)
            if (roleManager != null && roleManager.isRoleHeld(RoleManager.ROLE_ASSISTANT)) {
                return true
            }
        }
    }
    return runCatching {
        val voiceInteraction = Settings.Secure.getString(
            context.contentResolver,
            "voice_interaction_service",
        )
        voiceInteraction != null && voiceInteraction.substringBefore('/') == context.packageName
    }.getOrDefault(false)
}
