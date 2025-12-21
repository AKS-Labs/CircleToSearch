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

/**
 * Launch Google Lens with the given image URI
 *
 * @param uri The URI of the image to search with Google Lens
 * @param context The context to use for launching the intent
 * @return True if Google Lens was launched successfully, false otherwise
 */
fun searchWithGoogleLens(uri: Uri, context: Context): Boolean {
    Log.d(TAG, "Launching Google Lens with URI: $uri")

    try {
        // Get the content URI using FileProvider if needed
        val contentUri = if (uri.scheme == "content") {
            uri
        } else {
            try {
                val file = File(uri.path ?: return false)
                if (!file.exists()) {
                    Log.e(TAG, "Image file does not exist: ${file.absolutePath}")
                    Toast.makeText(context, "Image file not found", Toast.LENGTH_SHORT).show()
                    return false
                }

                FileProvider.getUriForFile(
                    context,
                    "com.akslabs.circletosearch.fileprovider",
                    file
                )
            } catch (e: Exception) {
                Log.e(TAG, "Error creating content URI: ${e.message}")
                Toast.makeText(context, "Error preparing image for Google Lens", Toast.LENGTH_SHORT).show()
                return false
            }
        }

        Log.d(TAG, "Content URI: $contentUri")

        // Try different approaches to launch Google Lens
        var success = false

        // Approach 1: Use Google Lens directly with ACTION_SEND
        try {
            val lensIntent = Intent(Intent.ACTION_SEND).apply {
                type = "image/*"
                putExtra(Intent.EXTRA_STREAM, contentUri)
                setPackage("com.google.android.googlequicksearchbox")
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }

            context.startActivity(lensIntent)
            vibrateDevice(context) // Provide haptic feedback
            Log.d(TAG, "Google Lens launched with ACTION_SEND")
            success = true
        } catch (e: Exception) {
            Log.e(TAG, "Failed to launch Google Lens with ACTION_SEND: ${e.message}")
        }

        // Approach 2: Use Google Gallery with ACTION_SEND
        if (!success) {
            try {
                val GalleryIntent = Intent(Intent.ACTION_SEND).apply {
                    type = "image/*"
                    putExtra(Intent.EXTRA_STREAM, contentUri)
                    setPackage("com.google.android.apps.Gallery")
                    putExtra("lens", true) // Hint to open in Lens mode
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }

                context.startActivity(GalleryIntent)
                vibrateDevice(context) // Provide haptic feedback
                Log.d(TAG, "Google Gallery launched with ACTION_SEND")
                success = true
            } catch (e: Exception) {
                Log.e(TAG, "Failed to launch Google Gallery: ${e.message}")
            }
        }

        // Approach 3: Use a chooser with ACTION_SEND
        if (!success) {
            try {
                val sendIntent = Intent(Intent.ACTION_SEND).apply {
                    type = "image/*"
                    putExtra(Intent.EXTRA_STREAM, contentUri)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                }

                val chooser = Intent.createChooser(sendIntent, "Search with Google Lens")
                chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

                context.startActivity(chooser)
                vibrateDevice(context) // Provide haptic feedback
                Log.d(TAG, "Chooser launched with ACTION_SEND")
                success = true
            } catch (e: Exception) {
                Log.e(TAG, "Failed to launch chooser: ${e.message}")
            }
        }

        // Approach 4: Use Google app with ACTION_VIEW
        if (!success) {
            try {
                val googleIntent = Intent(Intent.ACTION_VIEW).apply {
                    setPackage("com.google.android.googlequicksearchbox")
                    data = contentUri
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }

                context.startActivity(googleIntent)
                vibrateDevice(context) // Provide haptic feedback
                Log.d(TAG, "Google app launched with ACTION_VIEW")
                success = true
            } catch (e: Exception) {
                Log.e(TAG, "Failed to launch Google app with ACTION_VIEW: ${e.message}")
            }
        }

        // Approach 5: Use web browser with lens.google.com
        if (!success) {
            try {
                val webIntent = Intent(Intent.ACTION_VIEW, Uri.parse("https://lens.google.com")).apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }

                context.startActivity(webIntent)
                vibrateDevice(context) // Provide haptic feedback

                Toast.makeText(
                    context,
                    "Opening Google Lens website as fallback",
                    Toast.LENGTH_SHORT
                ).show()

                Log.d(TAG, "Web Google Lens launched")
                success = true
            } catch (e: Exception) {
                Log.e(TAG, "Failed to launch web Google Lens: ${e.message}")
            }
        }

        // If all approaches failed
        if (!success) {
            Toast.makeText(context, "Google Lens is not available on this device", Toast.LENGTH_SHORT).show()
            Log.e(TAG, "Google Lens not available - all approaches failed")
            return false
        }

        return true
    } catch (e: Exception) {
        Log.e(TAG, "Error launching Google Lens", e)
        Toast.makeText(context, "Error launching Google Lens", Toast.LENGTH_SHORT).show()
        return false
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


