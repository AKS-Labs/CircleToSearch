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

package com.akslabs.circletosearch

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.service.voice.VoiceInteractionSession
import android.service.voice.VoiceInteractionSessionService
import android.app.assist.AssistContent
import android.app.assist.AssistStructure
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.Build
import com.akslabs.circletosearch.data.BitmapRepository

class AssistSessionService : VoiceInteractionSessionService() {

    override fun onCreate() {
        super.onCreate()
        android.util.Log.d("AssistSessionService", "Service onCreate")
    }

    override fun onNewSession(args: Bundle?): VoiceInteractionSession {
        android.util.Log.d("AssistSessionService", "onNewSession created")
        return CircleToSearchSession(this)
    }

    inner class CircleToSearchSession(context: Context) : VoiceInteractionSession(context) {

        override fun onShow(args: Bundle?, showFlags: Int) {
            super.onShow(args, showFlags)
            android.util.Log.d("AssistSessionService", "onShow called with flags: $showFlags")

            // Clear data at the absolute start of the session to prevent race conditions 
            // with onHandleAssist/onHandleScreenshot delivery order.
            com.akslabs.circletosearch.data.AssistDataRepository.clear()

            // Ensure our session window doesn't intercept target app touches
            window?.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE)
            window?.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE)
            
            // Haptic feedback to acknowledge the trigger
            val vibrator = context.getSystemService(Context.VIBRATOR_SERVICE) as Vibrator
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                vibrator.vibrate(VibrationEffect.createOneShot(50, VibrationEffect.DEFAULT_AMPLITUDE))
            } else {
                @Suppress("DEPRECATION")
                vibrator.vibrate(50)
            }
        }

        override fun onHandleAssist(data: Bundle?, structure: AssistStructure?, content: AssistContent?) {
            super.onHandleAssist(data, structure, content)
            android.util.Log.d("AssistSessionService", "onHandleAssist called")
            
            if (structure == null) {
                android.util.Log.w("AssistSessionService", "AssistStructure is null")
                return
            }

            val allNodes = mutableListOf<com.akslabs.circletosearch.ui.components.TextNode>()
            val coveredRects = mutableListOf<android.graphics.Rect>()
            
            android.util.Log.d("AssistSessionService", "Capturing AssistStructure - Window count: ${structure.windowNodeCount}")
            
            // Process windows from TOP to BOTTOM (Reverse order)
            // This allows us to track occlusion from overlays like BottomSheets.
            for (i in (structure.windowNodeCount - 1) downTo 0) {
                val windowNode = structure.getWindowNodeAt(i)
                val windowTitle = windowNode.title?.toString() ?: "No Title"
                
                android.util.Log.d("AssistSessionService", "Processing Window [$i]: \"$windowTitle\"")
                
                val windowOffsetX = windowNode.left
                val windowOffsetY = windowNode.top
                
                // Collect all text from this window, passing the occlusion mask
                collectTextNodes(windowNode.rootViewNode, windowOffsetX, windowOffsetY, allNodes, coveredRects)
                
                // After processing a window, we treat its main bounds as a "covered" area for lower windows.
                // We only do this for windows that are likely to be opaque overlays (Dialogs, BottomSheets, etc.)
                // System windows like StatusBar are handled separately or excluded if needed.
                if (windowNode.width > 0 && windowNode.height > 0) {
                    coveredRects.add(android.graphics.Rect(windowOffsetX, windowOffsetY, windowOffsetX + windowNode.width, windowOffsetY + windowNode.height))
                }
            }

            if (allNodes.isEmpty()) {
                android.util.Log.w("AssistSessionService", "No text nodes found in assist data")
            } else {
                android.util.Log.d("AssistSessionService", "Extracted total of ${allNodes.size} text nodes from all windows")
            }
            
            com.akslabs.circletosearch.data.AssistDataRepository.setNodes(allNodes)
        }

        private fun collectTextNodes(
            node: AssistStructure.ViewNode,
            parentX: Int,
            parentY: Int,
            list: MutableList<com.akslabs.circletosearch.ui.components.TextNode>,
            coveredRects: List<android.graphics.Rect>
        ) {
            val nodeX = parentX + node.left
            val nodeY = parentY + node.top
            val nodeRect = android.graphics.Rect(nodeX, nodeY, nodeX + node.width, nodeY + node.height)

            // 1. OCCLUSION CHECK: If this node is completely covered by a higher-level window, skip it.
            // This prevents highlighting text that is "behind" a bottom sheet or dialog.
            val isOccluded = coveredRects.any { it.contains(nodeRect) }
            if (isOccluded) return

            // 2. PRECISION FILTERING: Avoid whole-screen highlights from root layouts.
            // We only fallback to contentDescription/hint for small elements (icons/buttons).
            val density = context.resources.displayMetrics.density
            val smallSizeThreshold = 100 * density // Approx 100dp
            
            val text = node.text?.toString() ?: run {
                // For contentDescription/hint, only capture if the element is small (likely an icon or input field)
                if (node.width < smallSizeThreshold && node.height < smallSizeThreshold) {
                    node.contentDescription?.toString() ?: node.hint?.toString()
                } else {
                    null
                }
            }

            if (!text.isNullOrBlank() && 
                node.visibility == android.view.View.VISIBLE &&
                node.width > 0 && node.height > 10) {
                
                // Only add if it's within reasonable screen bounds (optional but safer)
                // Filter out words that are clearly blank
                val wordStrings = text.split(Regex("\\s+")).filter { it.isNotBlank() }
                if (wordStrings.isNotEmpty()) {
                    var currentStartIndex = 0
                    val words = mutableListOf<com.akslabs.circletosearch.ui.components.Word>()
                    
                    wordStrings.forEachIndexed { wordIndex, wordText ->
                        val startIndex = text.indexOf(wordText, currentStartIndex)
                        val endIndex = startIndex + wordText.length
                        if (startIndex != -1) {
                            currentStartIndex = endIndex
                            val wordBounds = android.graphics.RectF(nodeRect)
                            words.add(
                                com.akslabs.circletosearch.ui.components.Word(
                                    text = wordText,
                                    index = wordIndex,
                                    startIndex = startIndex,
                                    endIndex = endIndex,
                                    bounds = wordBounds
                                )
                            )
                        }
                    }

                    list.add(
                        com.akslabs.circletosearch.ui.components.TextNode(
                            id = java.util.UUID.randomUUID().toString(),
                            fullText = text,
                            bounds = nodeRect,
                            words = words
                        )
                    )
                }
            }

            // Important: For children, subtract current node's scroll position from translated coordinates
            val nextParentX = nodeX - node.scrollX
            val nextParentY = nodeY - node.scrollY

            for (i in 0 until node.childCount) {
                collectTextNodes(node.getChildAt(i), nextParentX, nextParentY, list, coveredRects)
            }
        }

        override fun onHandleScreenshot(screenshot: android.graphics.Bitmap?) {
            super.onHandleScreenshot(screenshot)
            android.util.Log.d("AssistSessionService", "onHandleScreenshot received, bitmap null? ${screenshot == null}")
            
            // 1. Save screenshot to repository
            BitmapRepository.setScreenshot(screenshot)
            
            // 2. Launch the search overlay
            launchOverlayDirectly()
            
            // 3. Keep session alive for a moment to receive onHandleAssist data
        }

        private fun launchOverlayDirectly() {
            android.util.Log.d("AssistSessionService", "Launching OverlayActivity")
            val intent = Intent(context, OverlayActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION)
                // Signal that this was triggered by the assistant session
                putExtra("triggered_by", "assistant")
            }
            
            try {
                // Using startActivity as an assistant session is allowed to start activities
                context.startActivity(intent)
            } catch (e: Exception) {
                android.util.Log.e("AssistSessionService", "Failed to launch OverlayActivity", e)
            }
        }
    }
}
