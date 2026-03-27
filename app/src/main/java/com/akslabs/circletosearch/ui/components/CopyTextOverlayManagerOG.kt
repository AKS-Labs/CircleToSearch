///*
// * Copyright (C) 2025 AKS-Labs
// * SPDX-License-Identifier: GPL-3.0-or-later
// *
// * CopyTextOverlayManager — fully offline, zero network, zero logging.
// * Uses AccessibilityNodeInfo tree traversal + PorterDuff.Mode.CLEAR canvas
// * punch-out to highlight selectable text regions on screen.
// */
//
//package com.akslabs.circletosearch.ui.components
//
//import android.annotation.SuppressLint
//import android.content.ClipData
//import android.content.ClipboardManager
//import android.content.Context
//import android.graphics.Canvas
//import android.graphics.Color
//import android.graphics.Paint
//import android.graphics.PixelFormat
//import android.graphics.PorterDuff
//import android.graphics.PorterDuffXfermode
//import android.graphics.Rect
//import android.graphics.RectF
//import android.os.Build
//import android.os.Bundle
//import android.provider.Settings
//import android.view.GestureDetector
//import android.view.Gravity
//import android.view.MotionEvent
//import android.view.View
//import android.view.WindowManager
//import android.view.accessibility.AccessibilityNodeInfo
//import android.widget.Toast
//import kotlinx.coroutines.CoroutineScope
//import kotlinx.coroutines.Dispatchers
//import kotlinx.coroutines.Job
//import kotlinx.coroutines.SupervisorJob
//import kotlinx.coroutines.launch
//import kotlinx.coroutines.withContext
//
///** Simple holder for a floating-toolbar button's label and screen hit-rect. */
//private class ToolbarButton(val label: String, val rect: Rect)
//
///**
// * Manages the dim+punch-out Copy Text overlay.
// *
// * Usage:
// *   val mgr = CopyTextOverlayManager(context, windowManager, { getRootInActiveWindow() })
// *   mgr.show(onDismiss = { ... })
// *   mgr.rescanNodes()  // call on scroll events
// *   mgr.dismiss()
// */
//class CopyTextOverlayManager(
//    private val context: Context,
//    /** Lambda that returns the current AccessibilityNodeInfo root. */
//    private val getRoot: () -> AccessibilityNodeInfo?
//) {
//    private var dimView: DimPunchOutView? = null
//    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
//    private var scanJob: Job? = null
//    private var onDismissCallback: (() -> Unit)? = null
//
//    // Selection state
//    private var detectedNodes = mutableListOf<TextNode>()
//    private var activeNode: TextNode? = null
//    private var selectionStart: Int = -1 // word index
//    private var selectionEnd: Int = -1   // word index
//
//    /** Whether screen transition animations are effectively disabled by the user. */
//    private val reduceMotion: Boolean
//        get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.JELLY_BEAN_MR1 &&
//                Settings.Global.getFloat(
//                    context.contentResolver,
//                    Settings.Global.TRANSITION_ANIMATION_SCALE,
//                    1f
//                ) == 0f
//
//    // ── Public API ───────────────────────────────────────────────────────────
//
//    fun getOverlayView(onDismiss: () -> Unit): View {
//        if (dimView != null) return dimView!!
//        android.util.Log.d("CopyText", "getOverlayView() called")
//        onDismissCallback = onDismiss
//
//        val view = DimPunchOutView(context) { dismiss() }
//        dimView = view
//
//        // Kick off node scan
//        android.util.Log.d("CopyText", "Starting accessibility node scan...")
//        scanNodes(view)
//        return view
//    }
//
//    fun dismiss() {
//        scanJob?.cancel()
//        dimView = null
//        onDismissCallback?.invoke()
//        onDismissCallback = null
//    }
//
//    /** Call on TYPE_VIEW_SCROLLED events to refresh punched-out regions live. */
//    fun rescanNodes() {
//        dimView?.let { scanNodes(it) }
//    }
//
//    // ── Node scanning ────────────────────────────────────────────────────────
//
//    private fun scanNodes(view: DimPunchOutView) {
//        scanJob?.cancel()
//        scanJob = scope.launch {
//            android.util.Log.d("CopyText", "scanNodes triggered")
//            // Check for snapshot nodes first
//            val snapshot = com.akslabs.circletosearch.CircleToSearchAccessibilityService.lastCapturedNodes
//            if (snapshot != null) {
//                android.util.Log.d("CopyText", "Using snapshot with ${snapshot.size} nodes")
//                detectedNodes.clear()
//                detectedNodes.addAll(snapshot)
//                com.akslabs.circletosearch.CircleToSearchAccessibilityService.lastCapturedNodes = null
//                view.invalidate()
//                return@launch
//            }
//
//            val root = getRoot()
//            if (root == null) {
//                android.util.Log.e("CopyText", "scanNodes: getRoot() is NULL. Check accessibility permissions and window state.")
//                detectedNodes.clear()
//                view.invalidate()
//                return@launch
//            }
//            android.util.Log.d("CopyText", "scanNodes: Dynamic scan on root package=${root.packageName}")
//
//            val nodes = withContext(Dispatchers.Default) {
//                collectTextNodes(root)
//            }
//            android.util.Log.d("CopyText", "Dynamic scan complete: ${nodes.size} nodes found")
//            detectedNodes.clear()
//            detectedNodes.addAll(nodes)
//            view.invalidate()
//        }
//    }
//
//    internal fun collectTextNodes(root: AccessibilityNodeInfo?): List<TextNode> {
//        if (root == null) return emptyList()
//
//        val allNodes = mutableListOf<AccessibilityNodeInfo>()
//        val queue = ArrayDeque<AccessibilityNodeInfo>()
//        queue.add(root)
//
//        // 1. Breadth-first collect all nodes that potentially contain text
//        while (queue.isNotEmpty()) {
//            val node = queue.removeFirst()
//            try {
//                if (isPotentialTextNode(node)) {
//                    allNodes.add(AccessibilityNodeInfo.obtain(node))
//                }
//                for (i in 0 until node.childCount) {
//                    node.getChild(i)?.let { queue.add(it) }
//                }
//            } catch (e: Exception) {
//                android.util.Log.e("CopyText", "Error during traversal: ${e.message}")
//            }
//        }
//
//        // 2. Leaf-first filter: Remove a node if any of its descendants are already in the list
//        val leafNodes = allNodes.filter { parent ->
//            allNodes.none { child -> child != parent && isDescendantOf(child, parent) }
//        }
//
//        // 3. Convert leaf nodes to TextNodes
//        val result = mutableListOf<TextNode>()
//        leafNodes.forEach { node ->
//            val text = getBestText(node)
//            if (!text.isNullOrBlank() && text.length > 1) {
//                val rect = Rect()
//                node.getBoundsInScreen(rect)
//
//                // Final validation: check if it's visible and has dimensions
//                if (!rect.isEmpty && rect.width() > 5 && rect.height() > 5) {
//                    val words = splitIntoWords(text.toString(), rect)
//                    if (words.isNotEmpty()) {
//                        result.add(TextNode(
//                            id = java.util.UUID.randomUUID().toString(),
//                            fullText = text.toString(),
//                            bounds = rect,
//                            words = words
//                        ))
//                    }
//                }
//            }
//        }
//
//        // Optimization: Recycle collected nodes
//        allNodes.forEach { try { it.recycle() } catch (ignore: Exception) {} }
//
//        android.util.Log.d("CopyText", "LEAF-FIRST: Collected ${result.size} final text nodes from ${allNodes.size} potential nodes")
//        return result
//    }
//
//    private fun isPotentialTextNode(node: AccessibilityNodeInfo): Boolean {
//        val className = node.className?.toString() ?: ""
//        val text = node.text
//        val contentDesc = node.contentDescription
//
//        // Basic heuristics:
//        // 1. Has direct text? Yes.
//        // 2. Is a WebView node? Yes (let it pass to check children).
//        // 3. Is a clickable widget with content description? Yes.
//
//        if (!text.isNullOrEmpty()) return true
//
//        // For buttons/icons, contentDescription is text
//        if (!contentDesc.isNullOrEmpty() && (node.isClickable || className.contains("Button") || className.contains("Image"))) {
//            return true
//        }
//
//        // WebView nodes might not have text themselves but we need to traverse them
//        if (className.contains("WebView") || className.contains("webkit")) return true
//
//        return false
//    }
//
//    private fun getBestText(node: AccessibilityNodeInfo): CharSequence? {
//        val text = node.text
//        if (!text.isNullOrEmpty()) return text
//
//        // Fallback to contentDescription only for meaningful components
//        val className = node.className?.toString() ?: ""
//        if (node.isClickable || className.contains("Button") || className.contains("Image")) {
//            return node.contentDescription
//        }
//
//        return null
//    }
//
//    private fun isDescendantOf(child: AccessibilityNodeInfo, parent: AccessibilityNodeInfo): Boolean {
//        var current: AccessibilityNodeInfo? = child.parent
//        while (current != null) {
//            if (current == parent) {
//                try { current.recycle() } catch (_: Exception) {}
//                return true
//            }
//            val nextParent = current.parent
//            try { current.recycle() } catch (_: Exception) {}
//            current = nextParent
//        }
//        return false
//    }
//
//    internal fun splitIntoWords(fullText: String, nodeBounds: Rect): List<Word> {
//        val words = mutableListOf<Word>()
//        val rawWords = fullText.split(Regex("\\s+"))
//        var currentIdx = 0
//        rawWords.forEachIndexed { wordIdx, wordText ->
//            if (wordText.isEmpty()) return@forEachIndexed
//            val start = fullText.indexOf(wordText, currentIdx)
//            val end = start + wordText.length
//            val bounds = estimateWordBounds(start, end, fullText, nodeBounds)
//            words.add(Word(wordText, wordIdx, start, end, bounds))
//            currentIdx = end
//        }
//        return words
//    }
//
//    private fun estimateWordBounds(start: Int, end: Int, fullText: String, nodeBounds: Rect): RectF {
//        val totalChars = fullText.length.toFloat()
//        if (totalChars == 0f) return RectF(nodeBounds)
//
//        val leftFraction = start / totalChars
//        val rightFraction = end / totalChars
//
//        return RectF(
//            nodeBounds.left + (nodeBounds.width() * leftFraction),
//            nodeBounds.top.toFloat(),
//            nodeBounds.left + (nodeBounds.width() * rightFraction),
//            nodeBounds.bottom.toFloat()
//        )
//    }
//
//    // ── Dim + punch-out View ─────────────────────────────────────────────────
//
//    @SuppressLint("ClickableViewAccessibility")
//    inner class DimPunchOutView(
//        context: Context,
//        private val onBackPress: () -> Unit
//    ) : View(context) {
//
//        init {
//            this.background = null
//            setWillNotDraw(false)
//        }
//
//        // Coordinate conversion: Screen to Local
//        private val viewLocation = IntArray(2)
//
//        private fun toLocal(screenRect: Rect): RectF {
//            getLocationOnScreen(viewLocation)
//            return RectF(
//                (screenRect.left - viewLocation[0]).toFloat(),
//                (screenRect.top - viewLocation[1]).toFloat(),
//                (screenRect.right - viewLocation[0]).toFloat(),
//                (screenRect.bottom - viewLocation[1]).toFloat()
//            )
//        }
//
//        private fun toLocal(screenRectF: RectF): RectF {
//            getLocationOnScreen(viewLocation)
//            return RectF(
//                screenRectF.left - viewLocation[0],
//                screenRectF.top - viewLocation[1],
//                screenRectF.right - viewLocation[0],
//                screenRectF.bottom - viewLocation[1]
//            )
//        }
//
//        private fun screenToLocalX(screenX: Float): Float {
//            getLocationOnScreen(viewLocation)
//            return screenX - viewLocation[0]
//        }
//
//        private fun screenToLocalY(screenY: Float): Float {
//            getLocationOnScreen(viewLocation)
//            return screenY - viewLocation[1]
//        }
//
//        // 15% black dim over the full screen (alpha = 0.15 * 255 ≈ 38)
//        private val dimPaint = Paint().apply {
//            color = Color.BLACK
//            alpha = 38
//            isAntiAlias = false
//        }
//
//        // Paint for pulsing highlights
//        private val highlightPaint = Paint().apply {
//            color = Color.parseColor("#4FC3F7") // Light blue
//            style = Paint.Style.STROKE
//            strokeWidth = 4f
//            isAntiAlias = true
//        }
//
//        private val highlightFillPaint = Paint().apply {
//            color = Color.WHITE
//            alpha = 20 // Subtly white
//            style = Paint.Style.FILL
//            isAntiAlias = true
//        }
//
//        private val selectedWordPaint = Paint().apply {
//            color = Color.parseColor("#1565C0")
//            alpha = 90
//            isAntiAlias = true
//        }
//
//        private val wordTextPaint = Paint().apply {
//            color = Color.WHITE
//            textSize = 38f
//            isAntiAlias = true
//        }
//
//        private val handlePaint = Paint().apply {
//            color = Color.parseColor("#1565C0")
//            isAntiAlias = true
//        }
//
//        private val cardPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
//            color = Color.WHITE
//        }
//        private val copyBtnPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
//            color = Color.parseColor("#FF6750A4")
//        }
//        private val btnTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
//            color = Color.parseColor("#FF1C1B1F")
//            textSize = 42f
//            textAlign = Paint.Align.CENTER
//        }
//        private val copyBtnTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
//            color = Color.WHITE
//            textSize = 42f
//            textAlign = Paint.Align.CENTER
//        }
//
//        private var toolbarButtons: List<ToolbarButton> = emptyList()
//
//        private var pulseAlpha = 1.0f
//        private var pulseIncreasing = false
//        private val handler = android.os.Handler(android.os.Looper.getMainLooper())
//        private val pulseRunnable = object : Runnable {
//            override fun run() {
//                if (pulseIncreasing) {
//                    pulseAlpha += 0.02f
//                    if (pulseAlpha >= 1.0f) {
//                        pulseAlpha = 1.0f
//                        pulseIncreasing = false
//                    }
//                } else {
//                    pulseAlpha -= 0.02f
//                    if (pulseAlpha <= 0.4f) {
//                        pulseAlpha = 0.4f
//                        pulseIncreasing = true
//                    }
//                }
//                invalidate()
//                handler.postDelayed(this, 30)
//            }
//        }
//
//        private val gestureDetector = GestureDetector(context, object : GestureDetector.SimpleOnGestureListener() {
//            override fun onLongPress(e: MotionEvent) {
//                val x = e.x.toInt()
//                val y = e.y.toInt()
//
//                if (activeNode == null) {
//                    // Node discovery from tap (already in local coords from e.x/e.y)
//                    detectedNodes.find { toLocal(it.bounds).contains(x.toFloat(), y.toFloat()) }?.let { enterNode(it) }
//                } else {
//                     val wordIdx = findNearestWord(x.toFloat(), y.toFloat(), activeNode!!.words)
//                     selectionStart = wordIdx
//                     selectionEnd = wordIdx
//                     invalidate()
//                }
//            }
//
//            override fun onSingleTapUp(e: MotionEvent): Boolean {
//                val x = e.x.toInt()
//                val y = e.y.toInt()
//
//                // 1. Check toolbar
//                if (activeNode != null && selectionStart != -1) {
//                    for (btn in toolbarButtons) {
//                        if (btn.rect.contains(x, y)) {
//                            handleToolbarAction(btn.label)
//                            return true
//                        }
//                    }
//                }
//
//                // 2. Click in active node -> select word
//                activeNode?.let { node ->
//                    val localBounds = toLocal(node.bounds)
//                    if (localBounds.contains(x.toFloat(), y.toFloat())) {
//                        val wordIdx = findNearestWord(x.toFloat(), y.toFloat(), node.words)
//                        if (selectionStart == -1 || (selectionStart == selectionEnd && wordIdx != selectionStart)) {
//                             selectionStart = wordIdx
//                             selectionEnd = wordIdx
//                        } else {
//                            if (wordIdx < selectionStart) selectionStart = wordIdx
//                            else if (wordIdx > selectionEnd) selectionEnd = wordIdx
//                        }
//                        invalidate()
//                        return true
//                    } else {
//                        exitNode()
//                        return true
//                    }
//                }
//
//                // 3. Click highlighting node -> enter it
//                detectedNodes.find { toLocal(it.bounds).contains(x.toFloat(), y.toFloat()) }?.let {
//                    enterNode(it)
//                    return true
//                }
//
//                // 4. Click empty space -> exit overlay
//                onBackPress()
//                return true
//            }
//        })
//
//        init {
//            setLayerType(LAYER_TYPE_SOFTWARE, null)
//            handler.post(pulseRunnable)
//        }
//
//        private var dragHandleType = 0 // 0: none, 1: start, 2: end
//
//        private fun enterNode(node: TextNode) {
//            activeNode = node
//            selectionStart = 0
//            selectionEnd = node.words.lastIndex
//            invalidate()
//        }
//
//        private fun exitNode() {
//            activeNode = null
//            selectionStart = -1
//            selectionEnd = -1
//            invalidate()
//        }
//
//        fun setTextRects(rects: List<Rect>) {
//            // Deprecated, we use detectedNodes
//            invalidate()
//        }
//
//        override fun onDraw(canvas: Canvas) {
//            // 1. Dim background
//            canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)
//
//            // 2. Draw node highlights
//            detectedNodes.forEach { node ->
//                if (activeNode?.id == node.id) return@forEach // Don't draw highlight for active node
//
//                val localBounds = toLocal(node.bounds)
//                highlightPaint.alpha = (pulseAlpha * 255).toInt()
//                canvas.drawRoundRect(localBounds, 12f, 12f, highlightFillPaint)
//                canvas.drawRoundRect(localBounds, 12f, 12f, highlightPaint)
//            }
//
//            // 3. Draw active node words and selection
//            activeNode?.let { node ->
//                node.words.forEach { word ->
//                    if (word.index in selectionStart..selectionEnd) {
//                        val localWordBounds = toLocal(word.bounds)
//                        canvas.drawRoundRect(localWordBounds, 8f, 8f, selectedWordPaint)
//                    }
//                }
//
//                // Draw handles
//                if (selectionStart != -1 && selectionEnd != -1) {
//                    val startWord = node.words[selectionStart]
//                    val endWord = node.words[selectionEnd]
//
//                    val startLocal = toLocal(startWord.bounds)
//                    val endLocal = toLocal(endWord.bounds)
//
//                    drawHandle(canvas, startLocal.left, startLocal.top, true)
//                    drawHandle(canvas, endLocal.right, endLocal.bottom, false)
//
//                    // Draw toolbar
//                    drawFloatingToolbar(canvas, toLocal(node.bounds))
//                }
//            }
//        }
//
//        private fun drawHandle(canvas: Canvas, x: Float, y: Float, isStart: Boolean) {
//            canvas.drawCircle(x, y, 18f, handlePaint)
//            if (isStart) {
//                canvas.drawRect(x - 2f, y, x + 2f, y + 40f, handlePaint)
//            } else {
//                canvas.drawRect(x - 2f, y - 40f, x + 2f, y, handlePaint)
//            }
//        }
//
//        private fun drawFloatingToolbar(canvas: Canvas, anchor: RectF) {
//            val btnW = 210
//            val btnH = 88
//            val padding = 14
//            val gap = 8
//            val labels = listOf("Copy", "Select All", "Cancel")
//
//            val totalW = labels.size * btnW + (labels.size - 1) * gap + padding * 2
//            val totalH = btnH + padding * 2
//
//            var left = anchor.centerX().toInt() - totalW / 2
//            var top = anchor.top.toInt() - totalH - 18
//            if (top < 0) top = anchor.bottom.toInt() + 18
//            if (left < 0) left = 8
//            if (left + totalW > width) left = width - totalW - 8
//
//            // Card background with shadow via cardPaint
//            cardPaint.setShadowLayer(16f, 0f, 4f, Color.parseColor("#44000000"))
//            canvas.drawRoundRect(
//                RectF(left.toFloat(), top.toFloat(), (left + totalW).toFloat(), (top + totalH).toFloat()),
//                24f, 24f, cardPaint
//            )
//            cardPaint.clearShadowLayer()
//
//            val buttons = mutableListOf<ToolbarButton>()
//            labels.forEachIndexed { idx, label ->
//                val bLeft = left + padding + idx * (btnW + gap)
//                val bTop = top + padding
//                val bRect = Rect(bLeft, bTop, bLeft + btnW, bTop + btnH)
//                buttons.add(ToolbarButton(label, bRect))
//
//                val bRectF = RectF(bRect)
//                if (label == "Copy") {
//                    canvas.drawRoundRect(bRectF, 12f, 12f, copyBtnPaint)
//                    canvas.drawText(
//                        label,
//                        bRectF.centerX(), bRectF.centerY() + copyBtnTextPaint.textSize / 3f,
//                        copyBtnTextPaint
//                    )
//                } else {
//                    canvas.drawText(
//                        label,
//                        bRectF.centerX(), bRectF.centerY() + btnTextPaint.textSize / 3f,
//                        btnTextPaint
//                    )
//                }
//            }
//            toolbarButtons = buttons
//        }
//
//        override fun onTouchEvent(event: MotionEvent): Boolean {
//            val x = event.x
//            val y = event.y
//
//            when (event.action) {
//                MotionEvent.ACTION_DOWN -> {
//                    // Check handles
//                    activeNode?.let { node ->
//                        if (selectionStart != -1) {
//                            val startWord = node.words[selectionStart]
//                            val endWord = node.words[selectionEnd]
//
//                            val startLocal = toLocal(startWord.bounds)
//                            val endLocal = toLocal(endWord.bounds)
//
//                            if (isPointNear(x, y, startLocal.left, startLocal.top)) {
//                                dragHandleType = 1
//                                return true
//                            }
//                            if (isPointNear(x, y, endLocal.right, endLocal.bottom)) {
//                                dragHandleType = 2
//                                return true
//                            }
//                        }
//                    }
//                }
//                MotionEvent.ACTION_MOVE -> {
//                    if (dragHandleType != 0) {
//                        updateSelectionFromDrag(x, y)
//                        invalidate()
//                        return true
//                    }
//                }
//                MotionEvent.ACTION_UP -> {
//                    if (dragHandleType != 0) {
//                        dragHandleType = 0
//                        return true
//                    }
//
//                    // Gesture Detector handles taps
//                    gestureDetector.onTouchEvent(event)
//                }
//            }
//            return gestureDetector.onTouchEvent(event)
//        }
//
//        private fun isPointNear(px: Float, py: Float, x: Float, y: Float): Boolean {
//            val dist = Math.sqrt(Math.pow((px - x).toDouble(), 2.0) + Math.pow((py - y).toDouble(), 2.0))
//            return dist < 80 // Large touch target for handles
//        }
//
//        private fun updateSelectionFromDrag(x: Float, y: Float) {
//            val node = activeNode ?: return
//            val nearestWordIdx = findNearestWord(x, y, node.words)
//            val oldStart = selectionStart
//            val oldEnd = selectionEnd
//
//            if (dragHandleType == 1) {
//                selectionStart = Math.min(nearestWordIdx, selectionEnd)
//            } else if (dragHandleType == 2) {
//                selectionEnd = Math.max(nearestWordIdx, selectionStart)
//            }
//
//            if (oldStart != selectionStart || oldEnd != selectionEnd) {
//                performHapticFeedback(android.view.HapticFeedbackConstants.TEXT_HANDLE_MOVE)
//            }
//        }
//
//        private fun findNearestWord(x: Float, y: Float, words: List<Word>): Int {
//            var minDist = Float.MAX_VALUE
//            var nearestIdx = 0
//            words.forEachIndexed { idx, word ->
//                val dx = x - word.bounds.centerX()
//                val dy = y - word.bounds.centerY()
//                val dist = dx * dx + dy * dy
//                if (dist < minDist) {
//                    minDist = dist
//                    nearestIdx = idx
//                }
//            }
//            return nearestIdx
//        }
//
//        private fun handleToolbarAction(label: String) {
//            val node = activeNode ?: return
//            when (label) {
//                "Copy" -> {
//                    if (selectionStart != -1 && selectionEnd != -1) {
//                        val selectedWords = node.words.filter { it.index in selectionStart..selectionEnd }
//                        val selectedText = selectedWords.joinToString(" ") { it.text }
//
//                        val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
//                        clipboard.setPrimaryClip(ClipData.newPlainText("Copied Text", selectedText))
//                        Toast.makeText(context, "Text copied ✓", Toast.LENGTH_SHORT).show()
//                        dismiss()
//                    }
//                }
//                "Select All" -> {
//                    selectionStart = 0
//                    selectionEnd = node.words.lastIndex
//                    invalidate()
//                }
//                "Cancel" -> {
//                    exitNode()
//                }
//            }
//        }
//
//        override fun isInEditMode(): Boolean = false
//
//        override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent): Boolean {
//            if (keyCode == android.view.KeyEvent.KEYCODE_BACK) {
//                onBackPress()
//                return true
//            }
//            return super.onKeyDown(keyCode, event)
//        }
//    }
//}
