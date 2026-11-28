/*
 * Copyright (C) 2025 AKS-Labs
 */

package com.akslabs.circletosearch.utils

import android.content.Context
import android.content.Intent
import android.net.Uri
import java.io.File

/**
 * The in-flight `ACTION_IMAGE_CAPTURE` output slot.
 *
 * Kept OUTSIDE the composition, and persisted, on purpose. Launching a camera app
 * puts this overlay — which holds a fullscreen bitmap — in the background, and the
 * OS routinely recreates the activity while the user is composing the shot. A
 * `remember`-ed URI dies with that recreation, so the result callback found nothing
 * to read and the photo the user had just taken was silently dropped. The system
 * photo picker never hit this because its URI arrives inside the result intent.
 *
 * Note that the result *itself* is not a usable channel here: the overlay is a
 * `singleInstance` activity, so the platform starts the camera in its own task and
 * answers the request with `RESULT_CANCELED` before any photo exists. The file the
 * camera writes is the only delivery path, which is why this slot exists at all.
 *
 * ### Why the grant is re-applied, not just set once
 *
 * The LineageOS camera (`org.lineageos.aperture`) buffers a capture in memory and
 * only writes to our URI when the user taps the *check* button — that is, seconds
 * or minutes after the camera was launched, and frequently after this overlay has
 * been paused, recreated, or destroyed and rebuilt. Intent-carried grants
 * (`FLAG_GRANT_WRITE_URI_PERMISSION` + clip data) are tied to the sending activity
 * and do not survive that churn, which surfaced as the camera crashing on confirm:
 *
 *   `SecurityException: Permission Denial: writing ...fileprovider... requires the
 *    provider be exported, or grantUriPermission()`
 *
 * The user then saw a 0-byte file and was bounced back to the image-source sheet
 * with the photo lost. So the grant is stored alongside the URI and re-applied
 * ([regrant]) on every resume and on every poll tick while the capture is in
 * flight. Re-granting is idempotent and scoped to one URI and a handful of camera
 * packages.
 */
internal object PendingCameraCapture {

    private const val PREFS = "pending_camera_capture"
    private const val TAG = "CircleToSearchCapture"
    private const val KEY_URI = "uri"
    private const val KEY_PATH = "path"
    private const val KEY_CAMERA_PACKAGES = "camera_packages"
    private const val KEY_STARTED_AT = "started_at"

    /**
     * How long an *empty* slot is considered live.
     *
     * Without this, a capture the user simply backed out of would leave an empty slot
     * behind forever, and every future resume would open a fresh 2-minute wait on it.
     * The window is well past the point where a real capture has either written its
     * bytes or been abandoned by the user.
     */
    private const val SLOT_TTL_MS = 10 * 60 * 1000L

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    private fun readPackages(context: Context): List<String> =
        prefs(context)
            .getString(KEY_CAMERA_PACKAGES, null)
            ?.split('\u0000')
            ?.filter { it.isNotBlank() }
            ?: emptyList()

    /**
     * Records where the camera app will write, and which apps must be allowed to.
     *
     * [cameraPackages] is every package that can answer `ACTION_IMAGE_CAPTURE` (see
     * [resolveCameraPackages]); granting to all of them means a hand-off inside the
     * camera package, or the user switching the default camera, cannot lock us out.
     */
    fun set(context: Context, uri: Uri, file: File, cameraPackages: List<String>) {
        prefs(context).edit()
            .putString(KEY_URI, uri.toString())
            .putString(KEY_PATH, file.absolutePath)
            .putString(KEY_CAMERA_PACKAGES, cameraPackages.joinToString("\u0000"))
            .putLong(KEY_STARTED_AT, System.currentTimeMillis())
            .commit()
    }

    /**
     * Hands the camera apps explicit read/write access to the output URI.
     *
     * `FLAG_GRANT_WRITE_URI_PERMISSION` plus clip data is supposed to be enough, but
     * on this device it is not: the camera is refused with
     *
     *   `Permission Denial: writing ...fileprovider... requires the provider be
     *    exported, or grantUriPermission()`
     *
     * and crashes while saving, leaving a 0-byte file. The flags are still sent (they
     * cost nothing and are the documented path); this call is what actually opens the
     * door, and it is scoped to one URI and the camera packages.
     */
    private fun grantToPackages(context: Context, uri: Uri, packages: List<String>) {
        val appContext = context.applicationContext
        val flags = Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
            Intent.FLAG_GRANT_READ_URI_PERMISSION or
            Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
            Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
        packages.forEach { pkg ->
            val ok = runCatching { appContext.grantUriPermission(pkg, uri, flags) }.isSuccess
            if (!ok) {
                android.util.Log.w(TAG, "grantUriPermission FAILED for $pkg on $uri")
            }
        }
    }

    /** Grants now, for a capture that is about to be launched. */
    fun grantToCamera(context: Context, uri: Uri, cameraPackages: List<String>) =
        grantToPackages(context, uri, cameraPackages)

    /**
     * Re-applies the grant for the in-flight capture.
     *
     * Called on every resume and on every poll tick: the camera may write many
     * seconds after launch, long after any intent-scoped grant has expired. Safe and
     * cheap to call repeatedly.
     */
    fun regrant(context: Context) {
        val (uri, _) = peek(context) ?: return
        grantToPackages(context, uri, readPackages(context))
    }

    /** True while a capture is in flight — launched but not yet collected. */
    fun hasPending(context: Context): Boolean {
        val store = prefs(context)
        if (!store.contains(KEY_URI)) return false
        // An empty, ancient slot is a capture the user abandoned (or a camera that
        // never wrote). Drop it so it cannot trigger a pointless wait forever.
        val startedAt = store.getLong(KEY_STARTED_AT, 0L)
        val empty = store.getString(KEY_PATH, null)?.let { File(it).length() <= 0L } != false
        if (empty && startedAt > 0L && System.currentTimeMillis() - startedAt > SLOT_TTL_MS) {
            discard(context)
            return false
        }
        return true
    }

    /** True once the camera has put real bytes in the slot. */
    fun hasBytes(context: Context): Boolean =
        peek(context)?.second?.let { it.length() > 0L } == true

    /**
     * Reads the slot WITHOUT consuming it.
     *
     * Cameras save asynchronously and — for the preview-then-confirm cameras — only
     * after the user taps *check*. A reader that committed on the first look, and
     * deleted the "empty" file, pulled the target out from under the camera
     * mid-write. Peeking lets the caller wait for real bytes first.
     */
    fun peek(context: Context): Pair<Uri, File?>? {
        val store = prefs(context)
        val uriString = store.getString(KEY_URI, null) ?: return null
        val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return null
        return uri to store.getString(KEY_PATH, null)?.let(::File)
    }

    /**
     * Takes the slot only if the camera actually wrote bytes into it.
     *
     * Returns null and LEAVES THE SLOT INTACT when the file is still empty: an empty
     * file does not mean the user cancelled — it means the camera has not written
     * yet (the confirm-then-save cameras write only on *check*). Deleting here is
     * what used to throw away photos that landed moments later. Only [discard] or a
     * new capture clears an unconsumed slot.
     */
    fun takeWritten(context: Context): Pair<Uri, File>? {
        val (uri, file) = peek(context) ?: return null
        if (file != null && file.length() > 0L) {
            // Consume only now: the photo is on disk and will be decoded exactly once.
            take(context)
            return uri to file
        }
        return null
    }

    /**
     * Consumes the slot — returns it and clears it in one step, so a second read
     * can't decode the same photo twice.
     */
    fun take(context: Context): Pair<Uri, File?>? {
        val store = prefs(context)
        val uriString = store.getString(KEY_URI, null) ?: return null
        val file = store.getString(KEY_PATH, null)?.let(::File)
        val packages = readPackages(context)
        store.edit().clear().commit()
        val uri = runCatching { Uri.parse(uriString) }.getOrNull() ?: return null
        // The capture is over; the camera has no further reason to reach the file.
        revokeFromPackages(context, uri, packages)
        return uri to file
    }

    @Suppress("UNUSED_PARAMETER")
    private fun revokeFromPackages(context: Context, uri: Uri, packages: List<String>) {
        // Do NOT call context.revokeUriPermission while camera processes or background
        // writers are active. Revoking permissions mid-write triggers a SecurityException
        // in ContentProvider that crashes the camera app (e.g. LineageOS Aperture).
        // The temporary capture file in cacheDir is automatically cleaned up when decoded.
    }

    /**
     * Drops the slot and the temp file, for a capture that definitely produced
     * nothing (the user backed out of the camera, or a launch never happened).
     *
     * Never call this while a camera might still be writing: it unlinks the file the
     * camera is about to save into.
     */
    fun discard(context: Context) {
        val store = prefs(context)
        val uriString = store.getString(KEY_URI, null)
        val packages = readPackages(context)
        store.getString(KEY_PATH, null)?.let { runCatching { File(it).delete() } }
        store.edit().clear().commit()
        uriString?.let { runCatching { Uri.parse(it) }.getOrNull() }?.let { uri ->
            revokeFromPackages(context, uri, packages)
        }
    }

    /** Alias kept for call sites that just mean "abandon this launch". */
    fun clear(context: Context) = discard(context)
}
