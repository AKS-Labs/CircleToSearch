/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *  *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *  *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *  *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 */

package com.akslabs.circletosearch.utils

import android.content.Context
import android.graphics.Bitmap
import com.akslabs.circletosearch.CircleToSearchAccessibilityService
import com.akslabs.circletosearch.R
import dev.davidv.translator.TranslatorApplication
import dev.davidv.translator.ui.components.DetectedRegions
import dev.davidv.translator.ui.components.ImageWordSelection

/** A completed instant-OCR scan over a screenshot. [detection] carries the
 *  full-resolution detector boxes (screenshot coordinates) so a later
 *  translation of the same capture can skip its own detector run. */
internal data class InstantOcrScan(
    val words: ImageWordSelection,
    val regions: DetectedRegions?,
    val detection: List<uniffi.translator_core.DetectedTextBox>,
)

internal object InstantOcrScanner {

    /**
     * Runs the translator module's OCR over [bmp] and returns its recognized
     * source words — no translation, no overlay bitmap. [onRegions] fires as
     * soon as detection boxes arrive (for the scan animation). Returns null on
     * failure: missing OCR models and no-text both show a toast.
     */
    suspend fun scan(
        context: Context,
        bmp: Bitmap,
        onRegions: (DetectedRegions) -> Unit = {},
    ): InstantOcrScan? {
        val app = context.applicationContext as? TranslatorApplication ?: return null
        val coordinator = runCatching { app.translationCoordinator }.getOrNull() ?: return null
        // The accessibility service exposes the configured target language when it is
        // running, but the assistant launch path must also work with accessibility
        // disabled — resolve the same setting from the translator application directly.
        val target = CircleToSearchAccessibilityService.instance?.defaultTargetLanguage()
            ?: runCatching {
                app.languageCatalog?.languageByCode(
                    app.settingsManager.settings.value.defaultTargetLanguageCode
                )
            }.getOrNull()
            ?: return null
        var ocrMissing = false
        var sourceDetectFailed = false
        // User's OCR language whitelist: empty = auto, one = forced source (PULC and
        // CLD skipped), many = detection narrowed to the checked languages.
        val checkedLanguages = OcrLanguagePreferences.getChecked(context)
        val sourceSelection = when (checkedLanguages.size) {
            0 -> uniffi.translator_core.OcrSourceSelection.Auto
            1 -> uniffi.translator_core.OcrSourceSelection.Specific(
                uniffi.translator_core.LanguageCode(checkedLanguages.first()),
            )
            else -> uniffi.translator_core.OcrSourceSelection.Restricted(
                checkedLanguages.map { uniffi.translator_core.LanguageCode(it) },
            )
        }
        var latestRegions: DetectedRegions? = null
        val result = coordinator.ocrWordsOnly(
            bitmap = bmp,
            to = target,
            sourceSelection = sourceSelection,
            onDetectedRegions = { boxes, w, h ->
                val regions = DetectedRegions(w, h, boxes)
                latestRegions = regions
                onRegions(regions)
            },
            onOcrUnavailable = { ocrMissing = true },
            onAutoSourceFailed = { sourceDetectFailed = true },
        )
        if (result == null) {
            val messageId =
                when {
                    ocrMissing -> dev.davidv.translator.R.string.ocr_models_missing
                    sourceDetectFailed -> R.string.label_language_detect_failed
                    else -> R.string.label_no_text_found
                }
            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                android.widget.Toast.makeText(
                    context,
                    context.getString(messageId),
                    android.widget.Toast.LENGTH_SHORT,
                ).show()
            }
            return null
        }
        return InstantOcrScan(
            words = ImageWordSelection(
                imageWidth = result.imageWidth,
                imageHeight = result.imageHeight,
                sourceWords = result.sourceWords,
                translatedWords = result.sourceWords,
            ),
            regions = latestRegions,
            detection = result.detection,
        )
    }
}

/**
 * Hands the capture-time instant-OCR scan (started by the accessibility service
 * the moment the screenshot lands) to the overlay screen, so words are ready
 * when (or shortly after) the overlay composes instead of the scan starting
 * from zero there. State is keyed by the exact [Bitmap] instance of the capture:
 * a different image (gallery pick, a refresh's fresh re-capture) never matches
 * and scans fresh, as before.
 */
internal object InstantOcrStore {

    /** Live state of one capture's scan; [regions] lands at detection time (the
     *  screen adopts it for the scan animation), [words] and [detection] at
     *  completion. */
    class ScanState internal constructor(val bitmap: Bitmap) {
        @Volatile var regions: DetectedRegions? = null
        @Volatile var words: ImageWordSelection? = null
        @Volatile var detection: List<uniffi.translator_core.DetectedTextBox> = emptyList()

        /** True once the scan finished — [words] null means it failed (the
         *  scanner already showed the toast). */
        @Volatile var done: Boolean = false
    }

    @Volatile private var current: ScanState? = null

    /** Marks [bitmap] as the capture being scanned; the previous state is dropped. */
    fun beginScan(bitmap: Bitmap): ScanState {
        val state = ScanState(bitmap)
        current = state
        return state
    }

    /** The scan started for this exact bitmap instance, or null when the capture
     *  did not come from the pre-scan path. */
    fun stateFor(bitmap: Bitmap): ScanState? = current?.takeIf { it.bitmap === bitmap }

    fun clear() {
        current = null
    }

    /**
     * Detection boxes for [bitmap], shifted by ([offsetX], [offsetY]) into a
     * crop's coordinate space. The scan runs over the full screenshot while the
     * translate path crops to the content bounds — boxes cut by the crop edge
     * are dropped (partial lines dewarp badly). Returns null when there is no
     * matching scan for this bitmap instance (a select-mode sub-crop, or a
     * refresh's fresh re-capture — those detect fresh, as before).
     */
    fun detectionForCrop(
        bitmap: Bitmap,
        offsetX: Int,
        offsetY: Int,
        cropWidth: Int,
        cropHeight: Int,
    ): List<uniffi.translator_core.DetectedTextBox>? {
        val state = stateFor(bitmap) ?: return null
        if (state.words == null) return null
        if (offsetX == 0 && offsetY == 0 &&
            cropWidth == bitmap.width && cropHeight == bitmap.height
        ) {
            return state.detection
        }
        val remapped = mutableListOf<uniffi.translator_core.DetectedTextBox>()
        for (box in state.detection) {
            val r = box.rect
            val left = r.left.toInt() - offsetX
            val top = r.top.toInt() - offsetY
            val right = r.right.toInt() - offsetX
            val bottom = r.bottom.toInt() - offsetY
            if (left < 0 || top < 0 || right > cropWidth || bottom > cropHeight) continue
            remapped.add(
                box.copy(
                    rect = uniffi.translator_core.Rect(
                        left.toUInt(),
                        top.toUInt(),
                        right.toUInt(),
                        bottom.toUInt(),
                    ),
                    orientedBox = box.orientedBox.copy(
                        cx = box.orientedBox.cx - offsetX,
                        cy = box.orientedBox.cy - offsetY,
                    ),
                    tightBox = box.tightBox.copy(
                        cx = box.tightBox.cx - offsetX,
                        cy = box.tightBox.cy - offsetY,
                    ),
                    // Flat [x0, y0, x1, y1, ...] point list: even = x, odd = y.
                    contour = box.contour.mapIndexed { i, v ->
                        if (i % 2 == 0) v - offsetX else v - offsetY
                    },
                ),
            )
        }
        return remapped
    }
}
