package com.akslabs.circletosearch.ocr

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Rect
import android.util.Log
import com.akslabs.circletosearch.ui.components.TextNode
import com.akslabs.circletosearch.ui.components.Word
import com.googlecode.tesseract.android.TessBaseAPI
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import java.util.UUID

object TesseractEngine {
    private const val TAG = "TesseractEngine"
    private var isPrepared = false

    /**
     * Ensures the tessdata folder and eng.traineddata exist in the app's internal files directory.
     * Tesseract requires this specific directory structure (`tessdata/`).
     */
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

    suspend fun extractText(context: Context, bitmap: Bitmap): List<TextNode> = withContext(Dispatchers.Default) {
        val result = mutableListOf<TextNode>()
        try {
            val dataPath = prepareTessData(context)
            Log.d(TAG, "Initializing Tesseract with dataPath=$dataPath")

            val prefs = context.getSharedPreferences("OcrSettings", Context.MODE_PRIVATE)
            val lang = prefs.getString("selected_lang", "eng") ?: "eng"

            val tess = TessBaseAPI()
            if (!tess.init(dataPath, lang)) {
                Log.e(TAG, "Failed to initialize Tesseract API with language '$lang'!")
                return@withContext emptyList()
            }

            tess.setImage(bitmap)
            
            // CRITICAL: We MUST call getUTF8Text() (or other recognition trigger) before accessing the iterator.
            // Otherwise, resultIterator will be null or empty.
            val fullAppText = tess.getUTF8Text()
            Log.d(TAG, "Tesseract recognition complete. Full text length: ${fullAppText?.length ?: 0}")

            val iterator = tess.resultIterator ?: run {
                Log.e(TAG, "ResultIterator is null after recognition!")
                tess.recycle()
                return@withContext emptyList()
            }
            val allDetectedWords = mutableListOf<Word>()
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

                val wordBounds = android.graphics.RectF(wRect)
                val wordObj = Word(
                    text = wordText,
                    index = allDetectedWords.size,
                    startIndex = 0, // Not strictly needed for global selection
                    endIndex = wordText.length,
                    bounds = wordBounds
                )

                // Add to a generic list first, then group by Y-coordinate similarity to form lines
                allDetectedWords.add(wordObj)

            } while (iterator.next(TessBaseAPI.PageIteratorLevel.RIL_WORD))

            // Group words into lines based on Y-overlap for better UI grouping
            val sortedWords = allDetectedWords.sortedBy { it.bounds.top }
            val lines = mutableListOf<MutableList<Word>>()
            
            if (sortedWords.isNotEmpty()) {
                var currentLine = mutableListOf<Word>()
                currentLine.add(sortedWords[0])
                lines.add(currentLine)
                
                for (i in 1 until sortedWords.size) {
                    val prev = currentLine.last()
                    val curr = sortedWords[i]
                    // If the vertical centers are close enough, they are likely on the same line
                    // Increased tolerance (0.7f) to better capture slightly misaligned text
                    val verticalOverlap = Math.abs(curr.bounds.centerY() - prev.bounds.centerY()) < (prev.bounds.height() * 0.7f)
                    
                    if (verticalOverlap) {
                        currentLine.add(curr)
                    } else {
                        currentLine = mutableListOf(curr)
                        lines.add(currentLine)
                    }
                }
            }

            lines.forEach { lineWords ->
                // Sort words in line by X-coordinate
                val finalLineWords = lineWords.sortedBy { it.bounds.left }
                val fullText = finalLineWords.joinToString(" ") { it.text }
                
                val lineBounds = Rect()
                finalLineWords.forEach { w ->
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
            
            iterator.delete()
            tess.recycle()

            Log.d(TAG, "Tesseract successfully extracted ${result.size} text nodes.")
            
        } catch (e: Exception) {
            Log.e(TAG, "Tesseract processing error: ${e.message}")
        }
        
        return@withContext result
    }
}
