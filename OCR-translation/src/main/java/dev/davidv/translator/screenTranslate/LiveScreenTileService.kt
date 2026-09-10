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

import android.app.PendingIntent
import android.os.Build
import android.service.quicksettings.TileService
import android.widget.Toast
import androidx.annotation.RequiresApi
import dev.davidv.translator.R
import dev.davidv.translator.SettingsManager

/**
 *  Quick Settings tile that starts live screen translation with the user's
 *  default languages — needs only the overlay grant + MediaProjection consent
 *  (handled by [ScreenCaptureRequestActivity]), no accessibility or assistant.
 *  Live can't run the per-frame script classifier, so a default source language
 *  must be set.
 */
@RequiresApi(Build.VERSION_CODES.N)
class LiveScreenTileService : TileService() {
  override fun onClick() {
    super.onClick()
    val settings = SettingsManager(this).settings.value
    val sourceCode = settings.defaultSourceLanguageCode
    if (sourceCode == null) {
      Toast.makeText(this, getString(R.string.screen_translate_needs_source), Toast.LENGTH_LONG).show()
      return
    }
    val intent =
      ScreenCaptureRequestActivity.intent(
        this,
        sourceCode = sourceCode,
        targetCode = settings.defaultTargetLanguageCode,
        isAutoSource = false,
      )
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      val pi =
        PendingIntent.getActivity(
          this,
          0,
          intent,
          PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
      startActivityAndCollapse(pi)
    } else {
      @Suppress("DEPRECATION")
      startActivityAndCollapse(intent)
    }
  }
}
