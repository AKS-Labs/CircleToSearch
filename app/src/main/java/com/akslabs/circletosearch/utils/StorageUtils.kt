/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.utils

import android.content.Context
import android.util.Log
import java.io.File

object StorageUtils {
    private const val TAG = "StorageUtils"

    /**
     * Clears the contents of the application's cache directory (preserving the directory itself).
     */
    fun clearAppCache(context: Context) {
        try {
            val cacheDir = context.cacheDir
            if (cacheDir != null && cacheDir.isDirectory) {
                deleteDirContents(cacheDir)
                Log.d(TAG, "Application cache cleared successfully")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to clear app cache", e)
        }
    }

    /**
     * Helper to recursively delete a directory's contents.
     * Deletes files and subdirectories but preserves the given root directory.
     */
    private fun deleteDirContents(dir: File) {
        val children = dir.listFiles() ?: return
        for (child in children) {
            if (child.isDirectory) {
                deleteDirContents(child)
            }
            child.delete()
        }
    }
    
    /**
     * Specifically deletes the temporary screenshot file.
     */
    fun deleteTemporaryScreenshot(context: Context) {
        try {
            val file = File(context.cacheDir, "screenshot.png")
            if (file.exists()) {
                file.delete()
                Log.d(TAG, "Temporary screenshot deleted")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Error deleting temporary screenshot", e)
        }
    }
}
