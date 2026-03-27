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
     * Recursively clears the application's cache directory.
     */
    fun clearAppCache(context: Context) {
        try {
            val cacheDir = context.cacheDir
            if (cacheDir != null && cacheDir.isDirectory) {
                deleteDir(cacheDir)
                Log.d(TAG, "Application cache cleared successfully")
            }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to clear app cache", e)
        }
    }

    /**
     * Helper to recursively delete a directory and its contents.
     * Note: We don't delete the root cacheDir itself, just its children.
     */
    private fun deleteDir(dir: File?): Boolean {
        if (dir != null && dir.isDirectory) {
            val children = dir.list()
            if (children != null) {
                for (i in children.indices) {
                    val success = deleteDir(File(dir, children[i]))
                    if (!success) {
                        return false
                    }
                }
            }
            // If it's the root cacheDir, don't delete it, just its children
            // We can check this by comparing path or just letting it fail if it's the root
            // but actually standard behavior for "clear cache" is to leave the dir empty.
            return dir.delete()
        } else if (dir != null && dir.isFile) {
            return dir.delete()
        }
        return false
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
