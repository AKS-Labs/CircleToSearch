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

import android.accessibilityservice.AccessibilityService
import android.graphics.Rect
import android.graphics.RectF
import android.view.accessibility.AccessibilityNodeInfo
import android.view.accessibility.AccessibilityWindowInfo
import com.akslabs.circletosearch.ui.components.TextNode
import com.akslabs.circletosearch.ui.components.Word
import java.util.UUID

object AccessibilityNodeHarvester {

    fun harvest(service: AccessibilityService): List<TextNode> {
        val density = service.resources.displayMetrics.density
        val smallSizeThreshold = 100 * density
        val allNodes = mutableListOf<TextNode>()
        val coveredRects = mutableListOf<Rect>()

        val windows = service.windows
        if (!windows.isNullOrEmpty()) {
            // Process windows from TOP to BOTTOM (Reverse order)
            // This allows us to track occlusion from overlays like BottomSheets.
            for (i in windows.indices.reversed()) {
                val window = windows[i]
                if (window.type == AccessibilityWindowInfo.TYPE_ACCESSIBILITY_OVERLAY) continue

                val root = window.root ?: continue
                try {
                    collectTextNodes(root, allNodes, coveredRects, smallSizeThreshold)
                    val bounds = Rect()
                    window.getBoundsInScreen(bounds)
                    if (bounds.width() > 0 && bounds.height() > 0) {
                        coveredRects.add(Rect(bounds))
                    }
                } finally {
                    root.recycle()
                }
            }
        } else {
            val root = service.rootInActiveWindow
            if (root != null) {
                try {
                    collectTextNodes(root, allNodes, coveredRects, smallSizeThreshold)
                } finally {
                    root.recycle()
                }
            }
        }

        return allNodes
    }

    private fun collectTextNodes(
        node: AccessibilityNodeInfo,
        list: MutableList<TextNode>,
        coveredRects: List<Rect>,
        smallSizeThreshold: Float
    ) {
        val nodeRect = Rect()
        node.getBoundsInScreen(nodeRect)

        // 1. OCCLUSION CHECK: If this node is completely covered by a higher-level window, skip it.
        // This prevents highlighting text that is "behind" a bottom sheet or dialog.
        val isOccluded = coveredRects.any { it.contains(nodeRect) }
        if (isOccluded) {
            recycleChildren(node)
            return
        }

        // 2. PRECISION FILTERING: Avoid whole-screen highlights from root layouts.
        // We only fallback to contentDescription/hint for small elements (icons/buttons).
        val text = node.text?.toString()?.takeIf { it.isNotBlank() } ?: run {
            // For contentDescription/hint, only capture if the element is small (likely an icon or input field)
            if (nodeRect.width() < smallSizeThreshold && nodeRect.height() < smallSizeThreshold) {
                node.contentDescription?.toString()?.takeIf { it.isNotBlank() }
                    ?: node.hintText?.toString()?.takeIf { it.isNotBlank() }
            } else {
                null
            }
        }

        if (!text.isNullOrBlank() &&
            node.isVisibleToUser &&
            nodeRect.width() > 0 &&
            nodeRect.height() > 10
        ) {
            // Filter out words that are clearly blank
            val wordStrings = text.split(Regex("\\s+")).filter { it.isNotBlank() }
            if (wordStrings.isNotEmpty()) {
                var currentStartIndex = 0
                val words = mutableListOf<Word>()
                wordStrings.forEachIndexed { wordIndex, wordText ->
                    val startIndex = text.indexOf(wordText, currentStartIndex)
                    if (startIndex != -1) {
                        val endIndex = startIndex + wordText.length
                        currentStartIndex = endIndex
                        words.add(
                            Word(
                                text = wordText,
                                index = wordIndex,
                                startIndex = startIndex,
                                endIndex = endIndex,
                                bounds = RectF(nodeRect)
                            )
                        )
                    }
                }
                list.add(
                    TextNode(
                        id = UUID.randomUUID().toString(),
                        fullText = text,
                        bounds = Rect(nodeRect),
                        words = words
                    )
                )
            }
        }

        for (i in 0 until node.childCount) {
            val child = node.getChild(i) ?: continue
            try {
                collectTextNodes(child, list, coveredRects, smallSizeThreshold)
            } finally {
                child.recycle()
            }
        }
    }

    private fun recycleChildren(node: AccessibilityNodeInfo) {
        for (i in 0 until node.childCount) {
            node.getChild(i)?.recycle()
        }
    }
}
