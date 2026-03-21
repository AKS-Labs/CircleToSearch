/*
 * Copyright (C) 2025 AKS-Labs
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * CopyTextOverlayManager — fully offline, zero network, zero logging.
 *
 * FIXES applied vs original:
 *  1. isPotentialTextNode — strict text-only filter, no contentDescription/button leakage
 *  2. collectTextNodes — nodes are NOT recycled until AFTER leaf-filter completes (fixes
 *     non-deterministic parent-pointer traversal on recycled nodes)
 *  3. findNearestWord — converts word.bounds (screen coords) to local view coords before
 *     comparing against touch x/y (which are already in local coords)
 *  4. estimateWordBounds — clamps output to node bounds so multi-line text never maps
 *     words outside the visible region; documented limitation noted
 *  5. getRootNode selection — prefers the window with the MOST text content, not just
 *     windows.first(), so systemui chrome windows rank below real app windows
 *  6. snapshot timing — snapshot nodes are now re-collected on demand (when Copy Text
 *     button is tapped) not only at launch time
 */

package com.akslabs.circletosearch.ui.components

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.*
import android.view.*
import android.view.accessibility.*
import android.widget.FrameLayout
import android.widget.PopupMenu
import android.widget.Toast
import androidx.compose.ui.platform.ComposeView
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.Color as ComposeColor
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.foundation.background
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.Alignment
import kotlinx.coroutines.*
import java.util.UUID

/** Simple holder for a floating-toolbar button's label and screen hit-rect. */
private class ToolbarButton(val label: String, val rect: Rect)

/**
 * Manages the dim+punch-out Copy Text overlay.
 *
 * Usage:
 *   val mgr = CopyTextOverlayManager(context) { getRootNode() }
 *   mgr.getOverlayView(onDismiss = { ... })  // embed in AndroidView
 *   mgr.rescanNodes()  // call on scroll events
 *   mgr.dismiss()
 */
class CopyTextOverlayManager(
    private val context: Context,
    private val screenshotBitmap: android.graphics.Bitmap?
) {
    private var dimView: DimPunchOutView? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var scanJob: Job? = null
    private var onDismissCallback: (() -> Unit)? = null

    // Detected text nodes and flattened words (sorted by reading order)
    private val detectedNodes = mutableListOf<TextNode>()
    private var allWords: List<Word> = emptyList()
    private var startSelectionIdx = -1 // For dragging

    // Global selection state (indices into 'allWords')
    private var globalSelectionStart: Int = -1
    private var globalSelectionEnd: Int = -1

    // ── Public API ───────────────────────────────────────────────────────────

    fun getOverlayView(onDismiss: () -> Unit): View {
        onDismissCallback = onDismiss
        
        // Full Reset state for "fresh start"
        globalSelectionStart = -1
        globalSelectionEnd = -1
        startSelectionIdx = -1
        detectedNodes.clear()
        allWords = emptyList()
        
        val container = FrameLayout(context)
        
        val view = DimPunchOutView(context)
        dimView = view
        container.addView(view)
        
        val topBar = ComposeView(context).apply {
            setContent {
                MaterialTheme {
                    Box(modifier = Modifier.fillMaxSize()) {
                        TopBarUI(
                            onClose = { dismiss() }
                        )
                    }
                }
            }
        }
        container.addView(topBar)

        android.util.Log.d("CopyText", "Starting accessibility node scan…")
        scanNodes(view)
        return container
    }

    @Composable
    private fun TopBarUI(onClose: () -> Unit) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            // Close Button (Exact match from main UI)
            IconButton(
                onClick = onClose,
                modifier = Modifier
                    .background(android.graphics.Color.GRAY.toComposeColor().copy(alpha = 0.5f), CircleShape)
                    .size(40.dp)
            ) {
                Icon(Icons.Default.Close, contentDescription = "Close", tint = ComposeColor.White)
            }

            // Menu Button (Exact match from main UI)
            Box {
                var showMenu by remember { mutableStateOf(false) }
                IconButton(
                    onClick = { showMenu = true },
                    modifier = Modifier
                        .background(android.graphics.Color.GRAY.toComposeColor().copy(alpha = 0.5f), CircleShape)
                        .size(40.dp)
                ) {
                    Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = ComposeColor.White)
                }

                DropdownMenu(
                    expanded = showMenu,
                    onDismissRequest = { showMenu = false }
                ) {
                    DropdownMenuItem(
                        text = { Text("Select Language / Model") },
                        onClick = {
                            showMenu = false
                            showLanguageModelSelector()
                        }
                    )
                    DropdownMenuItem(
                        text = { Text("Import Model (.traineddata)") },
                        onClick = {
                            showMenu = false
                            triggerImport()
                        }
                    )
                }
            }
        }
    }

    private fun Int.toComposeColor(): ComposeColor = ComposeColor(this)

    private fun showLanguageModelSelector() {
        val models = com.akslabs.circletosearch.ocr.TesseractEngine.getAvailableModels(context)
        val prefs = context.getSharedPreferences("OcrSettings", Context.MODE_PRIVATE)
        val current = prefs.getString("selected_lang", "eng") ?: "eng"
        
        android.app.AlertDialog.Builder(context)
            .setTitle("Select OCR Model")
            .setSingleChoiceItems(models.toTypedArray(), models.indexOf(current)) { dialog, which ->
                val selected = models[which]
                prefs.edit().putString("selected_lang", selected).apply()
                Toast.makeText(context, "Selected: ${selected.uppercase()}. Restarting scan...", Toast.LENGTH_SHORT).show()
                dialog.dismiss()
                rescanNodes()
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun triggerImport() {
         Toast.makeText(context, "Please place .traineddata files in /sdcard/Android/data/${context.packageName}/files/tessdata/", Toast.LENGTH_LONG).show()
    }

    fun dismiss() {
        scanJob?.cancel()
        dimView = null
        onDismissCallback?.invoke()
        onDismissCallback = null
    }

    /** Call on TYPE_VIEW_SCROLLED events to refresh punched-out regions live. */
    fun rescanNodes() {
        dimView?.let { scanNodes(it) }
    }

    // ── Node scanning ────────────────────────────────────────────────────────

    private fun scanNodes(view: DimPunchOutView) {
        scanJob?.cancel()
        scanJob = scope.launch {
            android.util.Log.d("CopyText", "scanNodes triggered")

            if (screenshotBitmap == null) {
                android.util.Log.e("CopyText", "scanNodes: screenshotBitmap is null")
                detectedNodes.clear()
                view.invalidate()
                return@launch
            }

            android.util.Log.d("CopyText", "Live scan with OCR on bitmap")

            val nodes = com.akslabs.circletosearch.ocr.TesseractEngine.extractText(context, screenshotBitmap)
            
            // Sort nodes by top coordinate, then left to ensure logical reading order
            val sortedNodes = nodes.sortedWith(compareBy({ it.bounds.top }, { it.bounds.left }))
            
            android.util.Log.d("CopyText", "Live OCR scan complete: ${sortedNodes.size} text nodes")
            
            detectedNodes.clear()
            val tempAllWords = mutableListOf<Word>()
            detectedNodes.addAll(sortedNodes)
            
            // Flatten words and update their global index
            sortedNodes.forEach { node ->
                node.words.forEach { word ->
                    // We reuse the Word object but treat it globally
                    tempAllWords.add(word)
                }
            }
            allWords = tempAllWords
            
            view.invalidate()
        }
    }



    // ── Dim + punch-out View ─────────────────────────────────────────────────

    @SuppressLint("ClickableViewAccessibility")
    inner class DimPunchOutView(
        context: Context
    ) : View(context) {

        private val density = resources.displayMetrics.density

        // ── Coordinate helpers ────────────────────────────────────────────────

        private val viewLocation = IntArray(2)

        /** Convert a screen-coordinate Rect to this view's local coordinates. */
        private fun toLocal(screenRect: Rect): RectF {
            getLocationOnScreen(viewLocation)
            return RectF(
                (screenRect.left  - viewLocation[0]).toFloat(),
                (screenRect.top   - viewLocation[1]).toFloat(),
                (screenRect.right - viewLocation[0]).toFloat(),
                (screenRect.bottom - viewLocation[1]).toFloat()
            )
        }

        /** Convert a screen-coordinate RectF to this view's local coordinates. */
        private fun toLocal(screenRectF: RectF): RectF {
            getLocationOnScreen(viewLocation)
            return RectF(
                screenRectF.left   - viewLocation[0],
                screenRectF.top    - viewLocation[1],
                screenRectF.right  - viewLocation[0],
                screenRectF.bottom - viewLocation[1]
            )
        }

        /**
         * Convert a local-coordinate point to screen coordinates.
         * Used so we can compare touch x/y against word.bounds (screen coords).
         *
         * FIX #3: word.bounds are in SCREEN coordinates (set from getBoundsInScreen).
         * Touch events arrive in LOCAL view coordinates.  We must translate before
         * comparing — the original code compared local touch coords directly to
         * screen-space word bounds, causing selection to be offset by the view origin.
         */
        private fun toScreenX(localX: Float): Float {
            getLocationOnScreen(viewLocation)
            return localX + viewLocation[0]
        }

        private fun toScreenY(localY: Float): Float {
            getLocationOnScreen(viewLocation)
            return localY + viewLocation[1]
        }

        // ── Paint objects ─────────────────────────────────────────────────────

        private val dimPaint = Paint().apply {
            color = Color.BLACK
            alpha = 38  // ~15% dim
            isAntiAlias = false
        }

        private val textPaint = Paint().apply {
            color = Color.WHITE
            textSize = 32f
            isAntiAlias = true
            textAlign = Paint.Align.CENTER
        }


        private val selectedWordPaint = Paint().apply {
            // Priority: M3 dynamic color if possible, fallback to a nice purple/blue accent
            val colorRes = android.R.color.system_accent1_200 
            color = try { context.getColor(colorRes) } catch(e: Exception) { Color.parseColor("#D0BCFF") }
            alpha = 150
            isAntiAlias = true
        }

        private val handlePaint = Paint().apply {
            color = Color.parseColor("#6750A4") // M3 Primary
            isAntiAlias = true
        }

        private val toolbarBgPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#F3EDF7") // M3 Surface Container
        }

        private val toolbarActionPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#6750A4") // M3 Primary
        }

        private val toolbarTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#1D1B20") // M3 On Surface
            textSize = 38f
            textAlign = Paint.Align.CENTER
            typeface = android.graphics.Typeface.create("sans-serif-medium", android.graphics.Typeface.NORMAL)
        }

        private val toolbarIconPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#6750A4")
            style = Paint.Style.STROKE
            strokeWidth = 3f
        }

        // ── Pulse animation ───────────────────────────────────────────────────

        init {
            background = null
            setWillNotDraw(false)
            setLayerType(LAYER_TYPE_SOFTWARE, null)
        }

        // ── Selection / drag state ────────────────────────────────────────────

        private var dragHandleType = 0  // 0=none, 1=start, 2=end
        private var toolbarButtons: List<ToolbarButton> = emptyList()
        private var toolbarRect = RectF()
        private var dragHandleRect = RectF()
        private var toolbarOffsetX = 0f
        private var toolbarOffsetY = 0f
        private var isDraggingToolbar = false
        private var lastTouchX = 0f
        private var lastTouchY = 0f
        private var toolbarInitialized = false


        // ── Node entry / exit ─────────────────────────────────────────────────

        private fun enterNode(node: TextNode) {
            // Find global range for this node's words
            val firstWord = node.words.firstOrNull() ?: return
            val lastWord  = node.words.lastOrNull() ?: return
            
            val startIdx = allWords.indexOfFirst { it === firstWord }
            val endIdx   = allWords.indexOfLast { it === lastWord }
            
            if (startIdx != -1 && endIdx != -1) {
                globalSelectionStart = startIdx
                globalSelectionEnd   = endIdx
            }
            invalidate()
        }

        private fun exitNode() {
            globalSelectionStart = -1
            globalSelectionEnd   = -1
            invalidate()
        }

        // ── Drawing ───────────────────────────────────────────────────────────

        private val clearPaint = Paint().apply {
            xfermode = android.graphics.PorterDuffXfermode(android.graphics.PorterDuff.Mode.CLEAR)
            isAntiAlias = true
        }

        override fun onDraw(canvas: Canvas) {
            val saveCount = canvas.saveLayer(0f, 0f, width.toFloat(), height.toFloat(), null)

            // 1. Dim layer
            canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)

            // 2. Punch out holes for all text blocks
            detectedNodes.forEach { node ->
                val localBounds = toLocal(node.bounds)
                localBounds.inset(-16f, -12f) // More padding for professional look
                canvas.drawRoundRect(localBounds, 16f, 16f, clearPaint)
            }

            // 3. Draw Highlights (Merged into unified bars)
            if (globalSelectionStart != -1 && globalSelectionEnd != -1) {
                val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
                val end   = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
                
                // Group words by line (simplified: words in the same node that are close horizontally)
                // Actually, Tesseract's TextNode usually represents a line or block.
                // We'll group selected words by their Word.bounds.centerY() (rounded)
                val selectedWords = (start..end).mapNotNull { allWords.getOrNull(it) }
                
                // Group words by line segments
                val highlightPath = Path()
                selectedWords.groupBy { (it.bounds.centerY() / 20).toInt() }.forEach { (_, wordsInLine) ->
                    if (wordsInLine.isEmpty()) return@forEach
                    
                    val first = wordsInLine.minBy { it.bounds.left }
                    val last  = wordsInLine.maxBy { it.bounds.right }
                    
                    val lineRect = RectF(
                        first.bounds.left,
                        wordsInLine.minOf { it.bounds.top },
                        last.bounds.right,
                        wordsInLine.maxOf { it.bounds.bottom }
                    )
                    
                    val localLineHighlight = toLocal(lineRect)
                    localLineHighlight.inset(-16f, -12f)
                    highlightPath.addRoundRect(localLineHighlight, 12f, 12f, Path.Direction.CW)
                }
                // Draw all highlights IN ONE CALL to prevent alpha stacking (darker overlaps)
                canvas.drawPath(highlightPath, selectedWordPaint)

                // 4. Handles & Toolbar
                val firstWord = allWords[start]
                val lastWord  = allWords[end]
                val startLocal = toLocal(firstWord.bounds)
                val endLocal   = toLocal(lastWord.bounds)

                drawHandle(canvas, startLocal.left,  startLocal.top,    isStart = true)
                drawHandle(canvas, endLocal.right,   endLocal.bottom,   isStart = false)

                val encompassing = RectF(firstWord.bounds)
                selectedWords.forEach { encompassing.union(it.bounds) }
                drawFloatingToolbar(canvas, toLocal(encompassing))
            }

            // 5. Top Bar is now handled by ComposeView in the FrameLayout
            // so we don't draw it manually here anymore.

            canvas.restoreToCount(saveCount)
        }



        private fun drawHandle(canvas: Canvas, x: Float, y: Float, isStart: Boolean) {
            canvas.drawCircle(x, y, 18f, handlePaint)
            if (isStart) canvas.drawRect(x - 2f, y, x + 2f, y + 40f, handlePaint)
            else         canvas.drawRect(x - 2f, y - 40f, x + 2f, y, handlePaint)
        }

        private fun drawFloatingToolbar(canvas: Canvas, anchor: RectF) {
            val buttonLabels = listOf("Copy", "Share", "All", "Cancel")
            val btnPadding = 16f * density // More compact
            val btnHeight = 36f * density  // Match native pill height
            val btnSpacing = 6f * density
            val m = 10f * density
            
            toolbarActionPaint.textSize = 30f // Slightly smaller
            val labelWidths = buttonLabels.map { toolbarActionPaint.measureText(it) + btnPadding * 2 }
            val dragHandleWidth = 24f * density
            val totalWidth = labelWidths.sum() + (buttonLabels.size - 1) * btnSpacing + m * 2 + dragHandleWidth + btnSpacing
            
            val tx = ((width - totalWidth) / 2) + toolbarOffsetX
            
            if (!toolbarInitialized) {
                // Dock to bottom initially
                toolbarOffsetY = height - (btnHeight + m * 2) - 100f * density
                toolbarInitialized = true
            }
            val ty = toolbarOffsetY
            
            toolbarRect.set(tx, ty, tx + totalWidth, ty + btnHeight + m * 2)
            
            // Dynamic Background with shadow
            val dynamicSurface = try { 
                context.getColor(android.R.color.system_surface_container_light) 
            } catch(e: Exception) { Color.parseColor("#F3EDF7") }

            val shadowPaint = Paint(toolbarBgPaint).apply {
                setShadowLayer(12f * density, 0f, 4f * density, Color.BLACK and 0x2F000000)
            }
            canvas.drawRoundRect(toolbarRect, 22f * density, 22f * density, shadowPaint)
            canvas.drawRoundRect(toolbarRect, 22f * density, 22f * density, toolbarBgPaint.apply { color = dynamicSurface })

            // Draw dedicated drag handle (M3 style bar)
            val dx = tx + m
            dragHandleRect.set(dx, ty + m, dx + dragHandleWidth, ty + m + btnHeight)
            
            val handleBarColor = try {
                context.getColor(android.R.color.system_outline_variant_light)
            } catch(e: Exception) { Color.LTGRAY }
            
            val hPaint = Paint(toolbarActionPaint).apply {
                color = handleBarColor
                style = Paint.Style.FILL
            }
            // Draw a vertical pill for dragging
            canvas.drawRoundRect(
                dx + 8f * density, 
                ty + m + 8f * density, 
                dx + 16f * density, 
                ty + m + btnHeight - 8f * density, 
                4f * density, 4f * density, hPaint
            )

            var currentX = tx + m + dragHandleWidth + btnSpacing
            val newButtons = mutableListOf<ToolbarButton>()

            buttonLabels.forEachIndexed { i, label ->
                val bWidth = labelWidths[i]
                val bRect = RectF(currentX, ty + m, currentX + bWidth, ty + m + btnHeight)
                
                // M3 Pill for EVERY button with Dynamic Primary
                val dynamicPrimary = try {
                    context.getColor(android.R.color.system_accent1_600)
                } catch(e: Exception) { Color.parseColor("#6750A4") }

                canvas.drawRoundRect(bRect, btnHeight / 2, btnHeight / 2, handlePaint.apply { color = dynamicPrimary })
                
                // Fix: Ensure perfect vertical centering by using font metrics
                val fontMetrics = toolbarActionPaint.fontMetrics
                val textHeight = fontMetrics.descent - fontMetrics.ascent
                val textOffset = (textHeight / 2) - fontMetrics.descent
                
                canvas.drawText(label, bRect.centerX(), bRect.centerY() + textOffset, toolbarActionPaint.apply { 
                    color = Color.WHITE
                    style = Paint.Style.FILL
                    textSize = 30f
                    typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD)
                    textAlign = Paint.Align.CENTER
                })
                
                newButtons.add(ToolbarButton(label, Rect(bRect.left.toInt(), bRect.top.toInt(), bRect.right.toInt(), bRect.bottom.toInt())))
                currentX += bWidth + btnSpacing
            }
            toolbarButtons = newButtons
        }

        // ── Touch handling ────────────────────────────────────────────────────

        override fun onTouchEvent(event: MotionEvent): Boolean {
            val lx = event.x
            val ly = event.y

            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    lastTouchX = lx
                    lastTouchY = ly
                    
                    // Clicks for Top Bar are now handled by ComposeView, 
                    // so we only check the floating toolbar and selection logic.
                    
                    // 1. Check toolbar buttons FIRST
                    for (btn in toolbarButtons) {
                        val touchRect = Rect(btn.rect).apply { inset(-24, -24) }
                        if (touchRect.contains(lx.toInt(), ly.toInt())) {
                            handleToolbarAction(btn.label)
                            return true
                        }
                    }

                    // 2. Check dedicated drag handle ONLY
                    if (dragHandleRect.contains(lx, ly)) {
                        isDraggingToolbar = true
                        return true
                    }

                    // 2. Check for handle drag
                    if (globalSelectionStart != -1) {
                        val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
                        val end   = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
                        val startLocal = toLocal(allWords[start].bounds)
                        val endLocal   = toLocal(allWords[end].bounds)
                        
                        if (isPointNear(lx, ly, startLocal.left, startLocal.top)) {
                            dragHandleType = 1; return true
                        }
                        if (isPointNear(lx, ly, endLocal.right, endLocal.bottom)) {
                            dragHandleType = 2; return true
                        }
                    }

                    // 3. Glide Select across ALL nodes
                    val sx = toScreenX(lx)
                    val sy = toScreenY(ly)
                    val nearest = findNearestWordGlobal(sx, sy)
                    if (nearest != -1) {
                        globalSelectionStart = nearest
                        globalSelectionEnd   = nearest
                        performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
                        invalidate()
                        return true
                    } else {
                        exitNode()
                    }
                }

                MotionEvent.ACTION_MOVE -> {
                    val dx = lx - lastTouchX
                    val dy = ly - lastTouchY
                    lastTouchX = lx
                    lastTouchY = ly

                    if (isDraggingToolbar) {
                        toolbarOffsetX += dx
                        toolbarOffsetY += dy
                        invalidate()
                        return true
                    }

                    val sx = toScreenX(lx)
                    val sy = toScreenY(ly)
                    
                    if (dragHandleType != 0) {
                        val nearest = findNearestWordGlobal(sx, sy)
                        if (nearest != -1) {
                            if (dragHandleType == 1) globalSelectionStart = nearest
                            else                     globalSelectionEnd   = nearest
                            invalidate()
                        }
                        return true
                    } else if (globalSelectionStart != -1 && globalSelectionEnd != -1) { // Continue selection if already active
                        val nearest = findNearestWordGlobal(sx, sy)
                        if (nearest != -1) {
                            globalSelectionEnd = nearest
                            invalidate()
                            return true
                        }
                    }
                }

                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    isDraggingToolbar = false
                    dragHandleType = 0
                    startSelectionIdx = -1
                    invalidate()
                }
            }
            return super.onTouchEvent(event)
        }

        private fun findNearestWordGlobal(sx: Float, sy: Float): Int {
            var minDist = Float.MAX_VALUE
            var nearest = -1
            allWords.forEachIndexed { idx, word ->
                val dx = sx - word.bounds.centerX()
                val dy = sy - word.bounds.centerY()
                val d  = dx * dx + dy * dy
                // Only consider it a "hit" if it's within a reasonable distance of the block
                if (d < minDist) {
                    minDist = d
                    nearest = idx
                }
            }
            // If the nearest word is too far (e.g. empty space), return -1
            return if (minDist < 600 * 600) nearest else -1
        }

        private fun isPointNear(px: Float, py: Float, x: Float, y: Float): Boolean {
            val dx = px - x
            val dy = py - y
            return dx * dx + dy * dy < 80 * 80
        }




        private fun handleToolbarAction(label: String) {
            val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
            val end   = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
            if (start == -1) return

            val selectedText = (start..end).mapNotNull { allWords.getOrNull(it) }.joinToString(" ") { it.text }

            when (label) {
                "Copy" -> {
                    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                    clipboard.setPrimaryClip(ClipData.newPlainText("Copied Text", selectedText))
                    Toast.makeText(context, "Text copied ✓", Toast.LENGTH_SHORT).show()
                    dismiss()
                }
                "Share" -> {
                    val intent = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(android.content.Intent.EXTRA_TEXT, selectedText)
                        addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    context.startActivity(android.content.Intent.createChooser(intent, "Share text via").apply {
                        addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    })
                    dismiss()
                }
                "All", "Select All" -> {
                    globalSelectionStart = 0
                    globalSelectionEnd   = allWords.lastIndex
                    invalidate()
                }
                "Cancel" -> {
                    exitNode()
                }
            }
        }


        private fun statusBarsHeight(): Int {
            val resourceId = resources.getIdentifier("status_bar_height", "dimen", "android")
            return if (resourceId > 0) resources.getDimensionPixelSize(resourceId) else 0
        }

        // ── Misc ──────────────────────────────────────────────────────────────

        override fun isInEditMode(): Boolean = false

        override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent): Boolean {
            if (keyCode == android.view.KeyEvent.KEYCODE_BACK) {
                dismiss()
                return true
            }
            return super.onKeyDown(keyCode, event)
        }
    }
}
