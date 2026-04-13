package com.akslabs.circletosearch.ocr

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import android.graphics.RectF
import android.util.Log
import com.akslabs.circletosearch.ui.components.TextNode
import com.akslabs.circletosearch.ui.components.Word
import com.googlecode.tesseract.android.TessBaseAPI
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.util.UUID

object TesseractEngine {
    private const val TAG = "TesseractEngine"
    private var isPrepared = false

    fun prepareTessData(context: Context): String {
        val filesDir = context.filesDir.absolutePath
        val tessDir = File(filesDir, "tessdata")
        if (!tessDir.exists()) {
            tessDir.mkdirs()
        }

        val engFile = File(tessDir, "eng.traineddata")
        if (!engFile.exists()) {
            Log.d(TAG, "Copying eng.traineddata from assets...")
            try {
                context.assets.open("tessdata/eng.traineddata").use { input ->
                    FileOutputStream(engFile).use { output ->
                        input.copyTo(output)
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "Failed to copy eng.traineddata: ${e.message}")
            }
        }
        isPrepared = true
        return filesDir
    }

    fun getAvailableModels(context: Context): List<String> {
        val dir = File(context.filesDir, "tessdata")
        if (!dir.exists()) {
            prepareTessData(context)
        }
        
        val files = dir.listFiles() ?: return listOf("eng")
        return files.filter { it.name.endsWith(".traineddata") }
            .map { it.name.removeSuffix(".traineddata") }
            .sorted()
    }

    fun importModel(context: Context, uri: android.net.Uri, callback: (Boolean, String) -> Unit) {
        try {
            val cursor = context.contentResolver.query(uri, null, null, null, null)
            var fileName = "unknown.traineddata"
            if (cursor != null && cursor.moveToFirst()) {
                val nameIndex = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
                if (nameIndex != -1) {
                    fileName = cursor.getString(nameIndex)
                }
                cursor.close()
            }

            if (!fileName.endsWith(".traineddata")) {
                callback(false, "File must be a .traineddata Tesseract model.")
                return
            }

            val tessDir = File(context.filesDir, "tessdata")
            if (!tessDir.exists()) tessDir.mkdirs()

            val destFile = File(tessDir, fileName)
            
            context.contentResolver.openInputStream(uri)?.use { input ->
                FileOutputStream(destFile).use { output ->
                    input.copyTo(output)
                }
            }

            Log.d(TAG, "Imported model to ${destFile.absolutePath}")
            callback(true, "Successfully imported ${fileName.removeSuffix(".traineddata").uppercase()} model!")

        } catch (e: Exception) {
            Log.e(TAG, "Error importing model: ${e.message}")
            callback(false, "Failed to import model")
        }
    }

    /**
     * Tiled extraction: Runs multiple OCR passes in parallel and merges results.
     * This mirrors the QR scanner's multi-resolution logic to maximize accuracy.
     */
    suspend fun extractText(context: Context, bitmap: Bitmap): List<TextNode> = coroutineScope {
        val dataPath = withContext(Dispatchers.IO) { prepareTessData(context) }
        val prefs = context.getSharedPreferences("OcrSettings", Context.MODE_PRIVATE)
        val lang = prefs.getString("selected_lang", "eng") ?: "eng"

        val w = bitmap.width
        val h = bitmap.height

        // Define Tiles (Full + 2x2 Grid with 20% overlap)
        val tiles = mutableListOf<Rect>()
        // 1. Full
        tiles.add(Rect(0, 0, w, h))
        
        // 2. 2x2 Grid (Each tile is ~60% size to provide nice overlap)
        val tw = (w * 0.6f).toInt()
        val th = (h * 0.6f).toInt()
        tiles.add(Rect(0, 0, tw, th)) // Top Left
        tiles.add(Rect(w - tw, 0, w, th)) // Top Right
        tiles.add(Rect(0, h - th, tw, h)) // Bottom Left
        tiles.add(Rect(w - tw, h - th, w, h)) // Bottom Right

        val allPasses = tiles.mapIndexed { index, rect ->
            async(Dispatchers.Default) {
                index to internalExtractWords(dataPath, lang, bitmap, rect)
            }
        }

        val allWordsWithSource = allPasses.awaitAll().flatMap { (index, words) ->
            words.map { index to it }
        }
        
        // Final Merge & Line Grouping
        groupWordsIntoNodes(allWordsWithSource)
    }

    private fun internalExtractWords(dataPath: String, lang: String, fullBitmap: Bitmap, crop: Rect): List<Word> {
        val words = mutableListOf<Word>()
        val tess = TessBaseAPI()
        try {
            if (!tess.init(dataPath, lang)) return emptyList()
            
            // If the crop is a sub-region, create a subset bitmap
            val tileBitmap = if (crop.left == 0 && crop.top == 0 && crop.width() == fullBitmap.width && crop.height() == fullBitmap.height) {
                fullBitmap
            } else {
                Bitmap.createBitmap(fullBitmap, crop.left, crop.top, crop.width(), crop.height())
            }

            tess.setImage(tileBitmap)
            tess.getUTF8Text() // Trigger recognition

            val iterator = tess.resultIterator ?: run {
                tess.recycle()
                return emptyList()
            }

            iterator.begin()
            do {
                val wordText = iterator.getUTF8Text(TessBaseAPI.PageIteratorLevel.RIL_WORD)
                if (wordText.isNullOrBlank()) continue

                val wordRectParams = iterator.getBoundingRect(TessBaseAPI.PageIteratorLevel.RIL_WORD)
                    ?: iterator.getBoundingBox(TessBaseAPI.PageIteratorLevel.RIL_WORD)

                val wRect = if (wordRectParams is Rect) {
                    wordRectParams
                } else if (wordRectParams is IntArray) {
                    Rect(wordRectParams[0], wordRectParams[1], wordRectParams[2], wordRectParams[3])
                } else {
                    continue
                }

                if (wRect.isEmpty || wRect.width() < 2) continue

                // Adjust coordinates to global screen space
                val globalBounds = RectF(
                    (wRect.left + crop.left).toFloat(),
                    (wRect.top + crop.top).toFloat(),
                    (wRect.right + crop.left).toFloat(),
                    (wRect.bottom + crop.top).toFloat()
                )

                words.add(
                    Word(
                        text = wordText,
                        index = 0, // Assigned later
                        startIndex = 0,
                        endIndex = wordText.length,
                        bounds = globalBounds
                    )
                )

            } while (iterator.next(TessBaseAPI.PageIteratorLevel.RIL_WORD))

            iterator.delete()
        } catch (e: Exception) {
            Log.e(TAG, "Tile process error: ${e.message}")
        } finally {
            tess.recycle()
        }
        return words
    }

    private fun groupWordsIntoNodes(allWordsWithSource: List<Pair<Int, Word>>): List<TextNode> {
        if (allWordsWithSource.isEmpty()) return emptyList()

        // 1. Spatial Deduplication
        // We prefer results from quadrants (indices 1-4) over the full pass (index 0) 
        // because zoomed-in crops generally yield higher accuracy for small text.
        val uniqueWords = mutableListOf<Word>()
        val sortedByPreference = allWordsWithSource.sortedWith(compareByDescending<Pair<Int, Word>> { it.first }.thenByDescending { it.second.bounds.width() * it.second.bounds.height() })
        
        for (pair in sortedByPreference) {
            val w = pair.second
            val isDuplicate = uniqueWords.any { existing ->
                val overlap = RectF(w.bounds)
                if (overlap.intersect(existing.bounds)) {
                    val overlapArea = overlap.width() * overlap.height()
                    val wArea = w.bounds.width() * w.bounds.height()
                    val textMatch = w.text.equals(existing.text, ignoreCase = true)
                    // If text matches and there is significant overlap, it's a duplicate
                    overlapArea > wArea * 0.7 && textMatch
                } else false
            }
            if (!isDuplicate) uniqueWords.add(w)
        }

        // 2. Line Clustering (Vertical Overlap)
        val sortedWords = uniqueWords.sortedBy { it.bounds.top }
        val lines = mutableListOf<MutableList<Word>>()
        
        if (sortedWords.isNotEmpty()) {
            var currentLine = mutableListOf<Word>()
            currentLine.add(sortedWords[0])
            lines.add(currentLine)
            
            for (i in 1 until sortedWords.size) {
                val prev = currentLine.last()
                val curr = sortedWords[i]
                
                // If vertical centers are close enough, they are on the same line
                val verticalOverlap = Math.abs(curr.bounds.centerY() - prev.bounds.centerY()) < (prev.bounds.height() * 0.6f)
                
                if (verticalOverlap) {
                    currentLine.add(curr)
                } else {
                    currentLine = mutableListOf(curr)
                    lines.add(currentLine)
                }
            }
        }

        // 3. Horizontal Sorting & Node Construction
        val result = mutableListOf<TextNode>()
        lines.forEach { lineWords ->
            val finalLineWords = lineWords.sortedBy { it.bounds.left }
            val fullText = finalLineWords.joinToString(" ") { it.text }
            
            val lineBounds = Rect()
            finalLineWords.forEachIndexed { idx, w ->
                // Re-index words for the line
                finalLineWords[idx].copy(index = idx) 
                
                val r = Rect()
                w.bounds.roundOut(r)
                if (lineBounds.isEmpty()) lineBounds.set(r) else lineBounds.union(r)
            }

            result.add(
                TextNode(
                    id = UUID.randomUUID().toString(),
                    fullText = fullText,
                    bounds = lineBounds,
                    words = finalLineWords
                )
            )
        }
        
        return result
    }
}
