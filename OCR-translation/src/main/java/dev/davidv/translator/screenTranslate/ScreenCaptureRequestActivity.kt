/*
 * Copyright (C) 2024 David V
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

package dev.davidv.translator.screenTranslate

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.projection.MediaProjectionManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.util.Log
import androidx.core.content.ContextCompat

/**
 *  Transient, invisible activity that turns the screen-translate button into a
 *  running [ScreenTranslateService]. A `VoiceInteractionSession` can't itself
 *  ask for the SYSTEM_ALERT_WINDOW grant or the MediaProjection consent (both
 *  need an Activity result), so the button launches this; once both are
 *  granted it hands the projection token to the service and finishes.
 */
class ScreenCaptureRequestActivity : Activity() {
  private val tag = "ScreenCaptureRequest"

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    if (!canDrawOverlays()) {
      requestOverlayPermission()
      return
    }
    requestProjection()
  }

  private fun canDrawOverlays(): Boolean = Build.VERSION.SDK_INT < Build.VERSION_CODES.M || Settings.canDrawOverlays(this)

  private fun requestOverlayPermission() {
    val intent =
      Intent(
        Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
        Uri.parse("package:$packageName"),
      )
    startActivityForResult(intent, REQ_OVERLAY)
  }

  private fun requestProjection() {
    val pm = getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
    // On Android 14+ force whole-display capture, skipping the "single app vs
    // entire screen" picker.
    val intent =
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        pm.createScreenCaptureIntent(
          android.media.projection.MediaProjectionConfig.createConfigForDefaultDisplay(),
        )
      } else {
        pm.createScreenCaptureIntent()
      }
    startActivityForResult(intent, REQ_PROJECTION)
  }

  override fun onActivityResult(
    requestCode: Int,
    resultCode: Int,
    data: Intent?,
  ) {
    super.onActivityResult(requestCode, resultCode, data)
    when (requestCode) {
      REQ_OVERLAY -> {
        if (canDrawOverlays()) {
          requestProjection()
        } else {
          Log.i(tag, "Overlay permission denied")
          finishAndRemoveTask()
        }
      }
      REQ_PROJECTION -> {
        if (resultCode == RESULT_OK && data != null) {
          ContextCompat.startForegroundService(
            this,
            ScreenTranslateService.startIntent(
              this,
              resultCode,
              data,
              sourceCode = intent.getStringExtra(ScreenTranslateService.EXTRA_SOURCE_LANG),
              targetCode = intent.getStringExtra(ScreenTranslateService.EXTRA_TARGET_LANG),
              isAutoSource = intent.getBooleanExtra(ScreenTranslateService.EXTRA_AUTO_SOURCE, true),
            ),
          )
        } else {
          Log.i(tag, "Screen capture consent denied")
        }
        finish()
      }
    }
  }

  companion object {
    private const val REQ_OVERLAY = 1
    private const val REQ_PROJECTION = 2

    fun intent(
      context: Context,
      sourceCode: String?,
      targetCode: String?,
      isAutoSource: Boolean,
    ): Intent =
      Intent(context, ScreenCaptureRequestActivity::class.java).apply {
        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        sourceCode?.let { putExtra(ScreenTranslateService.EXTRA_SOURCE_LANG, it) }
        targetCode?.let { putExtra(ScreenTranslateService.EXTRA_TARGET_LANG, it) }
        putExtra(ScreenTranslateService.EXTRA_AUTO_SOURCE, isAutoSource)
      }
  }
}
