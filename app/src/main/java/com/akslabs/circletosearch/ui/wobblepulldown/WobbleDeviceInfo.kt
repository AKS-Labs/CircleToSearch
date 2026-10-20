package com.akslabs.circletosearch.ui.wobblepulldown

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.os.Environment
import android.os.StatFs
import android.provider.Settings
import java.util.Locale

data class WobbleDeviceInfo(
    val deviceName: String,
    val brand: String = Build.BRAND,
    val model: String = Build.MODEL,
    val device: String = Build.DEVICE,
    val hardware: String = Build.HARDWARE,
    val product: String = Build.PRODUCT,
    val androidVersion: String = Build.VERSION.RELEASE,
    val sdkInt: Int = Build.VERSION.SDK_INT,
    val manufacturer: String = Build.MANUFACTURER,
    val board: String = Build.BOARD,
    val display: String = Build.DISPLAY,
    val fingerprint: String = Build.FINGERPRINT,
    val totalStorage: Long = 0L,
    val availableStorage: Long = 0L,
    val totalRam: Long = 0L,
    val availableRam: Long = 0L,
    val securityPatch: String = Build.VERSION.SECURITY_PATCH,
    val osCodename: String = Build.VERSION.CODENAME,
)

object WobbleDeviceUtils {
    fun getDeviceInfo(context: Context): WobbleDeviceInfo {
        val deviceName = try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N_MR1) {
                Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
                    ?: Settings.Secure.getString(context.contentResolver, "bluetooth_name")
                    ?: Build.MODEL
            } else {
                Settings.Secure.getString(context.contentResolver, "bluetooth_name") ?: Build.MODEL
            }
        } catch (_: Exception) {
            Build.MODEL
        } ?: Build.MODEL

        var totalStorage = 0L
        var availStorage = 0L
        try {
            val stat = StatFs(Environment.getDataDirectory().path)
            totalStorage = stat.blockCountLong * stat.blockSizeLong
            availStorage = stat.availableBlocksLong * stat.blockSizeLong
        } catch (_: Exception) {}

        var totalRam = 0L
        var availRam = 0L
        try {
            val actManager = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            val memInfo = ActivityManager.MemoryInfo()
            actManager.getMemoryInfo(memInfo)
            totalRam = memInfo.totalMem
            availRam = memInfo.availMem
        } catch (_: Exception) {}

        return WobbleDeviceInfo(
            deviceName = deviceName,
            totalStorage = totalStorage,
            availableStorage = availStorage,
            totalRam = totalRam,
            availableRam = availRam,
        )
    }

    fun formatHardwareSize(bytes: Long): String {
        if (bytes <= 0) return "Unknown"
        val gb = bytes / (1024.0 * 1024.0 * 1024.0)
        return when {
            gb > 512 -> "1 TB"
            gb > 256 -> "512 GB"
            gb > 128 -> "256 GB"
            gb > 64 -> "128 GB"
            gb > 32 -> "64 GB"
            gb > 16 -> "32 GB"
            else -> String.format(Locale.US, "%.1f GB", gb)
        }
    }
}
