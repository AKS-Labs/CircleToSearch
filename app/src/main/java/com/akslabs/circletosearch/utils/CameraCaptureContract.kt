/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.utils

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.MediaStore
import androidx.activity.result.contract.ActivityResultContract

/**
 * What a camera capture produced.
 *
 * [ok] is the raw result code; [data] is the intent the camera returned, which some
 * OEM cameras use to deliver the photo themselves (as a thumbnail in `"data"` or a
 * `data` URI) because they ignored `EXTRA_OUTPUT`.
 *
 * Both fields are derived purely from (resultCode, intent), so this survives the
 * registry replaying the result after a process restart.
 */
data class CaptureResult(val ok: Boolean, val data: Intent?)

/**
 * Every package that can handle `ACTION_IMAGE_CAPTURE`.
 *
 * Needed because the URI grant cannot rely on the intent flags alone: on some
 * devices the camera is refused with
 *
 *   `Permission Denial: writing ...fileprovider... requires the provider be
 *    exported, or grantUriPermission()`
 *
 * and crashes mid-save, leaving a 0-byte file. Granting to these exact packages is
 * what actually lets the camera write.
 *
 * The whole list is returned, not just the resolved default, because a camera app is
 * free to hand the capture off to another component in its own package (the
 * LineageOS camera does exactly this: `Aperture`'s launcher alias targets
 * `CaptureActivity`), and because the user can change the default camera between our
 * grant and the actual write.
 */
fun resolveCameraPackages(context: Context): List<String> {
    val intent = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
    @Suppress("DEPRECATION")
    val resolved = context.packageManager
        .queryIntentActivities(intent, PackageManager.MATCH_DEFAULT_ONLY)
        .mapNotNull { it.activityInfo?.packageName }
        .distinct()
        .toMutableList()
    
    // Add common OEM/custom camera packages as fallback guarantees in case
    // package visibility filtering or aliases omit the target writing component.
    val commonCameraPackages = listOf(
        "org.lineageos.aperture",
        "com.google.android.GoogleCamera",
        "com.sec.android.app.camera",
        "com.oneplus.camera",
        "com.oppo.camera",
        "com.xiaomi.cameratest",
        "com.android.camera",
        "com.android.camera2",
    )
    for (pkg in commonCameraPackages) {
        if (!resolved.contains(pkg)) {
            resolved.add(pkg)
        }
    }
    return resolved
}

/** The single package that will handle `ACTION_IMAGE_CAPTURE`, or null if none does. */
fun resolveCameraPackage(context: Context): String? =
    resolveCameraPackages(context).firstOrNull()

/**
 * [ActivityResultContracts.TakePicture] with explicit URI grants.
 *
 * Differs from the platform contract in two ways, both needed here:
 *
 *  1. It hands the URI grant over explicitly (flags AND clip data). The platform
 *     contract only sets `EXTRA_OUTPUT`; several OEM camera apps (Samsung, Xiaomi,
 *     some AOSP forks) then fail to save into a FileProvider `content://` target.
 *     [PendingCameraCapture.grantToCamera] adds the per-package grant on top, since
 *     flags alone are not sufficient on the LineageOS camera used in testing.
 *  2. It returns the result intent too, so the caller can recover a photo that a
 *     camera returned inside the intent instead of at `EXTRA_OUTPUT`.
 */
object CameraCaptureContract : ActivityResultContract<Uri, CaptureResult>() {

    override fun createIntent(context: Context, input: Uri): Intent =
        Intent(MediaStore.ACTION_IMAGE_CAPTURE).apply {
            putExtra(MediaStore.EXTRA_OUTPUT, input)
            // Both directions: the camera writes the photo, and some camera apps read
            // the target back to show their own "review" thumbnail.
            addFlags(
                Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
            )
            clipData = ClipData.newRawUri("output", input)
        }

    override fun parseResult(resultCode: Int, intent: Intent?): CaptureResult =
        CaptureResult(ok = resultCode == Activity.RESULT_OK, data = intent)
}
