package com.akslabs.circletosearch.ui.components

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import android.util.Log
import android.widget.Toast
import androidx.core.content.FileProvider
import java.io.File



/**
 * Helper class for Google Lens integration
 */
private const val TAG = "GoogleLensHelper"

/** Result of attempting to launch Google Lens */
enum class LensLaunchResult {
    /** Launched directly into Google app / Gallery — overlay can be safely dismissed */
    LAUNCHED_DIRECTLY,
    /** Showed system app chooser — overlay must stay open so user can pick */
    LAUNCHED_VIA_CHOOSER,
    /** All approaches failed */
    FAILED
}

/**
 * Launch Google Lens with the given image URI
 *
 * @param uri The URI of the image to search with Google Lens
 * @param context The context to use for launching the intent
 * @return [LensLaunchResult] indicating how Lens was launched
 */
fun searchWithGoogleLens(uri: Uri, context: Context): LensLaunchResult {
    Log.d(TAG, "Launching Google Lens with URI: $uri")

    try {
        // Get the content URI using FileProvider if needed
        val contentUri = if (uri.scheme == "content") {
            uri
        } else {
            try {
                val file = File(uri.path ?: return LensLaunchResult.FAILED)
                if (!file.exists()) {
                    Log.e(TAG, "Image file does not exist: ${file.absolutePath}")
                    Toast.makeText(context, "Image file not found", Toast.LENGTH_SHORT).show()
                    return LensLaunchResult.FAILED
                }

                FileProvider.getUriForFile(
                    context,
                    "com.akslabs.circletosearch.fileprovider",
                    file
                )
            } catch (e: Exception) {
                Log.e(TAG, "Error creating content URI: ${e.message}")
                Toast.makeText(context, "Error preparing image for Google Lens", Toast.LENGTH_SHORT).show()
                return LensLaunchResult.FAILED
            }
        }

        Log.d(TAG, "Content URI: $contentUri")

        // Helper to apply robust permissions
        fun Intent.applyRobustPermissions(uri: Uri): Intent {
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            // ClipData is critical for permission propagation on Android 10+
            clipData = android.content.ClipData.newRawUri("Lens Image", uri)
            return this
        }

        // Approach 1: Standalone Google Lens App (often more reliable)
        try {
            val lensIntent = Intent(Intent.ACTION_SEND).apply {
                type = "image/*"
                putExtra(Intent.EXTRA_STREAM, contentUri)
                setPackage("com.google.ar.lens")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                applyRobustPermissions(contentUri)
            }
            context.startActivity(lensIntent)
            vibrateDevice(context)
            Log.d(TAG, "Standalone Google Lens launched")
            return LensLaunchResult.LAUNCHED_DIRECTLY
        } catch (e: Exception) {
            Log.d(TAG, "Standalone Lens app not available")
        }

        // Approach 2: Google Search App (Integrated Lens)
        try {
            val gSearchIntent = Intent(Intent.ACTION_SEND).apply {
                type = "image/*"
                putExtra(Intent.EXTRA_STREAM, contentUri)
                setPackage("com.google.android.googlequicksearchbox")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                applyRobustPermissions(contentUri)
            }
            context.startActivity(gSearchIntent)
            vibrateDevice(context)
            Log.d(TAG, "Google Search App (Lens) launched")
            return LensLaunchResult.LAUNCHED_DIRECTLY
        } catch (e: Exception) {
            Log.e(TAG, "Failed to launch Google App: ${e.message}")
        }

        // Approach 3: Google Gallery (Lens Integration)
        try {
            val galleryIntent = Intent(Intent.ACTION_SEND).apply {
                type = "image/*"
                putExtra(Intent.EXTRA_STREAM, contentUri)
                setPackage("com.google.android.apps.Gallery")
                putExtra("lens", true)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                applyRobustPermissions(contentUri)
            }
            context.startActivity(galleryIntent)
            vibrateDevice(context)
            Log.d(TAG, "Google Gallery launched")
            return LensLaunchResult.LAUNCHED_DIRECTLY
        } catch (e: Exception) {
            Log.e(TAG, "Failed to launch Google Gallery: ${e.message}")
        }

        // Approach 4: Google app with ACTION_VIEW (Legacy/Alternative)
        try {
            val googleIntent = Intent(Intent.ACTION_VIEW).apply {
                setPackage("com.google.android.googlequicksearchbox")
                data = contentUri
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                applyRobustPermissions(contentUri)
            }
            context.startActivity(googleIntent)
            vibrateDevice(context)
            Log.d(TAG, "Google app launched with ACTION_VIEW")
            return LensLaunchResult.LAUNCHED_DIRECTLY
        } catch (e: Exception) {
            Log.e(TAG, "Failed to launch Google app with ACTION_VIEW: ${e.message}")
        }

        // Approach 5: System chooser fallback
        try {
            val sendIntent = Intent(Intent.ACTION_SEND).apply {
                type = "image/*"
                putExtra(Intent.EXTRA_STREAM, contentUri)
                applyRobustPermissions(contentUri)
            }
            val chooser = Intent.createChooser(sendIntent, "Search with Google Lens")
            chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(chooser)
            vibrateDevice(context)
            Log.d(TAG, "Chooser launched")
            return LensLaunchResult.LAUNCHED_VIA_CHOOSER
        } catch (e: Exception) {
            Log.e(TAG, "Failed to launch chooser: ${e.message}")
        }

        Toast.makeText(context, "Google Lens is not available on this device", Toast.LENGTH_SHORT).show()
        return LensLaunchResult.FAILED
    } catch (e: Exception) {
        Log.e(TAG, "Error launching Google Lens", e)
        Toast.makeText(context, "Error launching Google Lens", Toast.LENGTH_SHORT).show()
        return LensLaunchResult.FAILED
    }
}

/**
 * Vibrates the device to provide haptic feedback
 */
private fun vibrateDevice(context: Context) {
    try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) { // Android 12+
            val vibratorManager = context.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager
            val vibrator = vibratorManager.defaultVibrator
            vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
        } else {
            @Suppress("DEPRECATION")
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) { // Android 8.0+
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }
        }
    } catch (e: Exception) {
        Log.e(TAG, "Error vibrating device", e)
    }
}


