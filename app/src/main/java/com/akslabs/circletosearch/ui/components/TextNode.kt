package com.akslabs.circletosearch.ui.components

import android.graphics.Rect
import android.graphics.RectF

/**
 * Represents a node with text detected on the screen.
 */
data class TextNode(
    val id: String,           // UUID or unique identifier
    val fullText: String,
    val bounds: Rect,         // screen coordinates
    val words: List<Word>
)

/**
 * Represents an individual word within a TextNode.
 */
data class Word(
    val text: String,
    val index: Int,           // position in words list
    val startIndex: Int,      // start index in fullText
    val endIndex: Int,        // end index in fullText
    val bounds: RectF         // screen coordinates, estimated
)
