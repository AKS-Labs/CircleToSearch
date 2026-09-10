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

package dev.davidv.translator.ui.components

import android.content.Context
import android.graphics.Bitmap
import android.graphics.RectF
import android.util.AttributeSet
import com.yalantis.ucrop.callback.BitmapCropCallback
import com.yalantis.ucrop.util.RectUtils
import com.yalantis.ucrop.view.CropImageView
import com.yalantis.ucrop.view.GestureCropImageView

class AppCropImageView
  @JvmOverloads
  constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyle: Int = 0,
  ) : GestureCropImageView(context, attrs, defStyle) {
    override fun setImageToWrapCropBounds() {}

    override fun setImageToWrapCropBounds(animate: Boolean) {}

    override fun postScale(
      deltaScale: Float,
      px: Float,
      py: Float,
    ) {
      if (deltaScale == 0f) return
      mCurrentImageMatrix.postScale(deltaScale, deltaScale, px, py)
      imageMatrix = mCurrentImageMatrix
    }

    override fun cropAndSaveImage(
      compressFormat: Bitmap.CompressFormat,
      compressQuality: Int,
      cropCallback: BitmapCropCallback?,
    ) {
      val imageBounds = RectUtils.trapToRect(mCurrentImageCorners)
      val cropRect = cropRectField.get(this) as RectF
      cropRect.intersect(
        imageBounds.left,
        imageBounds.top,
        imageBounds.right,
        imageBounds.bottom,
      )
      super.cropAndSaveImage(compressFormat, compressQuality, cropCallback)
    }

    companion object {
      private val cropRectField =
        CropImageView::class.java.getDeclaredField("mCropRect").apply { isAccessible = true }
    }
  }
