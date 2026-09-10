package dev.davidv.translator

import android.content.Context
import android.provider.Settings

fun isHostAccessibilityGranted(context: Context): Boolean {
  val pkg = context.packageName
  val enabled =
    Settings.Secure.getString(
      context.contentResolver,
      Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,
    ) ?: return false
  return enabled.split(':').any { it.startsWith("$pkg/") }
}
