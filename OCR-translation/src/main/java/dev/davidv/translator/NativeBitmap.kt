package dev.davidv.translator

import android.graphics.Bitmap

// Pins a software ARGB_8888 bitmap and returns its native pixel address so the OCR pipeline
// can read/write the pixels in place — no byte[] copy across the uniffi boundary. Returns 0
// when the bitmap can't be locked tightly-packed (caller falls back to the copying path).
internal object NativeBitmap {
  init {
    System.loadLibrary("bindings")
  }

  external fun lockPixels(bitmap: Bitmap): Long

  external fun unlockPixels(bitmap: Bitmap)
}
