/*
 * Copyright (C) 2025 AKS-Labs
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

package com.akslabs.circletosearch.ui.components

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.*
import android.util.Log
import android.view.*
import android.widget.FrameLayout
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
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.Alignment
import com.akslabs.circletosearch.data.BitmapRepository
import com.akslabs.circletosearch.ocr.TesseractEngine
import com.akslabs.circletosearch.utils.ImageUtils
import kotlinx.coroutines.*
import java.util.UUID

/** Simple holder for a floating-toolbar button's label and screen hit-rect. */
private class ToolbarButton(val label: String, val rect: Rect)

/**
 * Manages the dim+punch-out Copy Text overlay with OCR capabilities.
 */
class CopyTextOverlayManager(
    private val context: Context,
    private val screenshotBitmap: android.graphics.Bitmap?
) {
    private var dimView: DimPunchOutView? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var scanJob: Job? = null
    private var onDismissCallback: (() -> Unit)? = null

    // State for Compose and Drawing
    private val isScanning = mutableStateOf(false)
    private val textNodes = mutableStateListOf<TextNode>()
    private var allWords: List<Word> = emptyList()
    
    // Status message for the user
    private val statusMessage = mutableStateOf<String?>(null)
    
    // Assistant Data support
    var isAssistMode: Boolean = false
        private set
    
    private var nativeNodes: List<TextNode> = emptyList()

    fun setHybridMode(nodes: List<TextNode>) {
        isAssistMode = true
        nativeNodes = nodes
        textNodes.clear()
        textNodes.addAll(nodes)
        updateAllWords()
    }
    
    fun setOcrOnlyMode() {
        isAssistMode = false
        nativeNodes = emptyList()
        textNodes.clear()
        allWords = emptyList()
        statusMessage.value = null
    }
    
    private fun updateAllWords() {
        allWords = textNodes.flatMap { node -> 
            node.words.map { word -> 
                word
            }
        }
    }
    
    // Selection state
    private var globalSelectionStart: Int = -1
    private var globalSelectionEnd: Int = -1

    fun getOverlayView(onDismiss: () -> Unit): View {
        onDismissCallback = onDismiss
        
        // Reset interactive state
        globalSelectionStart = -1
        globalSelectionEnd = -1
        
        val container = FrameLayout(context)
        
        val view = DimPunchOutView(context)
        dimView = view
        container.addView(view)
        
        val topBar = ComposeView(context).apply {
            setContent {
                MaterialTheme {
                    Box(modifier = Modifier.fillMaxSize()) {
                        TopBarUI(onClose = { dismiss() })
                        
                        if (isScanning.value) {
                            Column(
                                modifier = Modifier.align(Alignment.Center),
                                horizontalAlignment = Alignment.CenterHorizontally
                            ) {
                                CircularProgressIndicator(
                                    modifier = Modifier.size(48.dp),
                                    color = ComposeColor.White,
                                    strokeWidth = 4.dp
                                )
                                Spacer(Modifier.height(16.dp))
                                Text(
                                    if (isAssistMode) "Hybrid Deep Scan..." else "Scanning text...", 
                                    style = MaterialTheme.typography.titleMedium,
                                    color = ComposeColor.White,
                                    modifier = Modifier
                                        .background(ComposeColor.Black.copy(alpha = 0.4f), RoundedCornerShape(8.dp))
                                        .padding(horizontal = 12.dp, vertical = 6.dp)
                                )
                            }
                        }
                        
                        statusMessage.value?.let { msg ->
                            Box(
                                modifier = Modifier
                                    .align(Alignment.BottomCenter)
                                    .padding(bottom = 100.dp)
                                    .background(ComposeColor.Black.copy(alpha = 0.7f), RoundedCornerShape(16.dp))
                                    .padding(horizontal = 20.dp, vertical = 10.dp)
                            ) {
                                Text(
                                    msg,
                                    color = ComposeColor.White,
                                    style = MaterialTheme.typography.bodyMedium
                                )
                            }
                        }
                    }
                }
            }
        }
        container.addView(topBar)

        if (isAssistMode) {
            // Trigger parallel OCR scan even if we have native nodes
            scanNodes(view, isHybrid = true)
        } else {
            scanNodes(view, isHybrid = false)
        }
        return container
    }

    @Composable
    private fun TopBarUI(onClose: () -> Unit) {
        val filePickerLauncher = androidx.activity.compose.rememberLauncherForActivityResult(
            contract = androidx.activity.result.contract.ActivityResultContracts.GetContent()
        ) { uri: android.net.Uri? ->
            if (uri != null) {
                TesseractEngine.importModel(context, uri) { success, msg ->
                    android.widget.Toast.makeText(context, msg, android.widget.Toast.LENGTH_SHORT).show()
                }
            }
        }

        Row(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            IconButton(
                onClick = onClose,
                modifier = Modifier
                    .background(android.graphics.Color.GRAY.toComposeColor().copy(alpha = 0.5f), CircleShape)
                    .size(40.dp)
            ) {
                Icon(Icons.Default.Close, contentDescription = "Exit Copy Mode", tint = ComposeColor.White)
            }
            
            Spacer(modifier = Modifier.weight(1f))

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
                            filePickerLauncher.launch("*/*")
                        }
                    )
                }
            }
        }
    }

    private fun Int.toComposeColor(): ComposeColor = ComposeColor(this)

    private fun showLanguageModelSelector() {
        val models = TesseractEngine.getAvailableModels(context)
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

    fun dismiss() {
        scanJob?.cancel()
        dimView = null
        onDismissCallback?.invoke()
        onDismissCallback = null
    }

    fun rescanNodes() {
        dimView?.let { scanNodes(it) }
    }

    private fun scanNodes(view: View, isHybrid: Boolean = false) {
        scanJob?.cancel()
        statusMessage.value = null
        scanJob = scope.launch(Dispatchers.Main) {
            isScanning.value = true
            val bitmap = screenshotBitmap ?: BitmapRepository.getScreenshot()
            
            if (bitmap == null) {
                if (isAssistMode && textNodes.isEmpty()) {
                    statusMessage.value = "This app doesn't allow reading screen content."
                }
                isScanning.value = false
                view.invalidate()
                return@launch
            }

            try {
                // OCR scan runs on background thread
                val ocrNodes = TesseractEngine.extractText(context, bitmap)
                
                if (isHybrid) {
                    mergeHybridNodes(ocrNodes)
                } else {
                    val sortedNodes = ocrNodes.sortedWith(compareBy({ it.bounds.top }, { it.bounds.left }))
                    textNodes.clear()
                    textNodes.addAll(sortedNodes)
                    updateAllWords()
                }
                
                if (textNodes.isEmpty()) {
                    statusMessage.value = "No text found on screen."
                }
                
                Log.d("CopyTextOverlay", "Capture complete: ${textNodes.size} total nodes")
            } catch (e: Exception) {
                Log.e("CopyTextOverlay", "Extraction failed: ${e.message}")
            } finally {
                isScanning.value = false
                view.invalidate()
            }
        }
    }

    private fun mergeHybridNodes(ocrNodes: List<TextNode>) {
        val newNodes = mutableListOf<TextNode>()
        // Start with existing native nodes
        newNodes.addAll(nativeNodes)
        
        for (ocr in ocrNodes) {
            if (!isDuplicate(ocr, nativeNodes)) {
                // Only add if OCR node is within screen bounds (roughly)
                if (ocr.bounds.left >= -50 && ocr.bounds.top >= -50) {
                    newNodes.add(ocr)
                }
            }
        }
        
        val sortedNodes = newNodes.sortedWith(compareBy({ it.bounds.top }, { it.bounds.left }))
        textNodes.clear()
        textNodes.addAll(sortedNodes)
        updateAllWords()
    }

    private fun isDuplicate(ocr: TextNode, natives: List<TextNode>): Boolean {
        for (native in natives) {
            // 1. Coordinate Overlap Check (> 50% overlap)
            val ocrRect = ocr.bounds
            val nativeRect = native.bounds
            
            val intersect = Rect(ocrRect)
            if (intersect.intersect(nativeRect)) {
                val intersectArea = intersect.width() * intersect.height()
                val ocrArea = ocrRect.width() * ocrRect.height()
                if (intersectArea > ocrArea * 0.5) return true
            }
            
            // 2. Text Similarity Check
            val ocrT = ocr.fullText.lowercase().trim()
            val nativeT = native.fullText.lowercase().trim()
            if (nativeT.contains(ocrT) || ocrT.contains(nativeT)) return true
        }
        return false
    }

    @SuppressLint("ClickableViewAccessibility")
    inner class DimPunchOutView(context: Context) : View(context) {
        private val density = resources.displayMetrics.density
        private val viewLocation = IntArray(2)

        private val dimPaint = Paint().apply { color = Color.BLACK; alpha = 38; isAntiAlias = false }
        private val selectedWordPaint = Paint().apply {
            color = try { context.getColor(android.R.color.system_accent1_200) } catch(e: Exception) { Color.parseColor("#D0BCFF") }
            alpha = 150; isAntiAlias = true
        }
        private val handlePaint = Paint().apply { color = Color.parseColor("#6750A4"); isAntiAlias = true }
        private val toolbarBgPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#F3EDF7") }
        private val toolbarActionPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.parseColor("#6750A4") }
        private val clearPaint = Paint().apply {
            xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR)
            isAntiAlias = true
        }

        private var dragHandleType = 0
        private var toolbarButtons: List<ToolbarButton> = emptyList()
        private var toolbarRect = RectF()
        private var dragHandleRect = RectF()
        private var toolbarOffsetX = 0f
        private var toolbarOffsetY = 0f
        private var isDraggingToolbar = false
        private var toolbarInitialized = false
        private var lastTouchX = 0f
        private var lastTouchY = 0f

        init {
            setLayerType(LAYER_TYPE_SOFTWARE, null)
        }

        private fun toLocal(screenRect: Rect): RectF {
            getLocationOnScreen(viewLocation)
            return RectF(
                (screenRect.left - viewLocation[0]).toFloat(),
                (screenRect.top - viewLocation[1]).toFloat(),
                (screenRect.right - viewLocation[0]).toFloat(),
                (screenRect.bottom - viewLocation[1]).toFloat()
            )
        }

        private fun toLocal(screenRectF: RectF): RectF {
            getLocationOnScreen(viewLocation)
            return RectF(
                screenRectF.left - viewLocation[0],
                screenRectF.top - viewLocation[1],
                screenRectF.right - viewLocation[0],
                screenRectF.bottom - viewLocation[1]
            )
        }

        private fun toScreenX(localX: Float): Float { getLocationOnScreen(viewLocation); return localX + viewLocation[0] }
        private fun toScreenY(localY: Float): Float { getLocationOnScreen(viewLocation); return localY + viewLocation[1] }

        override fun onDraw(canvas: Canvas) {
            val saveCount = canvas.saveLayer(0f, 0f, width.toFloat(), height.toFloat(), null)
            canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)

            textNodes.forEach { node ->
                val localBounds = toLocal(node.bounds)
                localBounds.inset(-12f, -8f)
                canvas.drawRoundRect(localBounds, 12f, 12f, clearPaint)
            }

            if (globalSelectionStart != -1 && globalSelectionEnd != -1) {
                val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
                val end = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
                val selectedWords = (start..end).mapNotNull { allWords.getOrNull(it) }
                
                val highlightPath = Path()
                selectedWords.groupBy { (it.bounds.centerY() / 20).toInt() }.forEach { (_, wordsInLine) ->
                    if (wordsInLine.isEmpty()) return@forEach
                    val first = wordsInLine.minBy { it.bounds.left }
                    val last = wordsInLine.maxBy { it.bounds.right }
                    val lineRect = RectF(first.bounds.left, wordsInLine.minOf { it.bounds.top }, last.bounds.right, wordsInLine.maxOf { it.bounds.bottom })
                    val localHighlight = toLocal(lineRect)
                    localHighlight.inset(-12f, -8f)
                    highlightPath.addRoundRect(localHighlight, 8f, 8f, Path.Direction.CW)
                }
                canvas.drawPath(highlightPath, selectedWordPaint)

                val startLocal = toLocal(allWords[start].bounds)
                val endLocal = toLocal(allWords[end].bounds)
                drawHandle(canvas, startLocal.left, startLocal.top, true)
                drawHandle(canvas, endLocal.right, endLocal.bottom, false)

                val encompassing = RectF(allWords[start].bounds)
                selectedWords.forEach { encompassing.union(it.bounds) }
                drawFloatingToolbar(canvas, toLocal(encompassing))
            }
            canvas.restoreToCount(saveCount)
        }

        private fun drawHandle(canvas: Canvas, x: Float, y: Float, isStart: Boolean) {
            canvas.drawCircle(x, y, 18f, handlePaint)
            if (isStart) canvas.drawRect(x - 2f, y, x + 2f, y + 40f, handlePaint)
            else canvas.drawRect(x - 2f, y - 40f, x + 2f, y, handlePaint)
        }

        private fun drawFloatingToolbar(canvas: Canvas, anchor: RectF) {
            val buttonLabels = listOf("Copy", "Share", "All", "Cancel")
            val btnPadding = 16f * density
            val btnHeight = 36f * density
            val btnSpacing = 6f * density
            val m = 10f * density
            
            toolbarActionPaint.textSize = 30f
            val labelWidths = buttonLabels.map { toolbarActionPaint.measureText(it) + btnPadding * 2 }
            val dragHandleWidth = 24f * density
            val totalWidth = labelWidths.sum() + (buttonLabels.size - 1) * btnSpacing + m * 2 + dragHandleWidth + btnSpacing
            
            val tx = ((width - totalWidth) / 2) + toolbarOffsetX
            if (!toolbarInitialized) {
                toolbarOffsetY = anchor.top - (btnHeight + m * 2) - 32f
                if (toolbarOffsetY < 150f) toolbarOffsetY = anchor.bottom + 32f
                toolbarInitialized = true
            }
            val ty = toolbarOffsetY
            toolbarRect.set(tx, ty, tx + totalWidth, ty + btnHeight + m * 2)
            
            val dynamicSurface = try { context.getColor(android.R.color.system_surface_container_light) } catch(e: Exception) { Color.parseColor("#F3EDF7") }
            val shadowPaint = Paint(toolbarBgPaint).apply { setShadowLayer(12f * density, 0f, 4f * density, Color.BLACK and 0x2F000000) }
            canvas.drawRoundRect(toolbarRect, 22f * density, 22f * density, shadowPaint)
            canvas.drawRoundRect(toolbarRect, 22f * density, 22f * density, toolbarBgPaint.apply { color = dynamicSurface })

            val dx = tx + m
            dragHandleRect.set(dx, ty + m, dx + dragHandleWidth, ty + m + btnHeight)
            val hPaint = Paint(toolbarActionPaint).apply { color = Color.LTGRAY; style = Paint.Style.FILL }
            canvas.drawRoundRect(dx + 8f * density, ty + m + 8f * density, dx + 16f * density, ty + m + btnHeight - 8f * density, 4f * density, 4f * density, hPaint)

            var currentX = tx + m + dragHandleWidth + btnSpacing
            val newButtons = mutableListOf<ToolbarButton>()
            buttonLabels.forEachIndexed { i, label ->
                val bWidth = labelWidths[i]
                val bRect = RectF(currentX, ty + m, currentX + bWidth, ty + m + btnHeight)
                val dynamicPrimary = try { context.getColor(android.R.color.system_accent1_600) } catch(e: Exception) { Color.parseColor("#6750A4") }
                canvas.drawRoundRect(bRect, btnHeight / 2, btnHeight / 2, handlePaint.apply { color = dynamicPrimary })
                
                val fontMetrics = toolbarActionPaint.fontMetrics
                val textOffset = ((fontMetrics.descent - fontMetrics.ascent) / 2) - fontMetrics.descent
                canvas.drawText(label, bRect.centerX(), bRect.centerY() + textOffset, toolbarActionPaint.apply { 
                    color = Color.WHITE; style = Paint.Style.FILL; textSize = 30f; typeface = Typeface.create(Typeface.DEFAULT, Typeface.BOLD); textAlign = Paint.Align.CENTER
                })
                newButtons.add(ToolbarButton(label, Rect(bRect.left.toInt(), bRect.top.toInt(), bRect.right.toInt(), bRect.bottom.toInt())))
                currentX += bWidth + btnSpacing
            }
            toolbarButtons = newButtons
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            val lx = event.x; val ly = event.y
            when (event.action) {
                MotionEvent.ACTION_DOWN -> {
                    lastTouchX = lx; lastTouchY = ly
                    for (btn in toolbarButtons) {
                        if (Rect(btn.rect).apply { inset(-24, -24) }.contains(lx.toInt(), ly.toInt())) {
                            handleToolbarAction(btn.label); return true
                        }
                    }
                    if (dragHandleRect.contains(lx, ly)) { isDraggingToolbar = true; return true }
                    if (globalSelectionStart != -1) {
                        val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
                        val end = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
                        val startLocal = toLocal(allWords[start].bounds)
                        val endLocal = toLocal(allWords[end].bounds)
                        if (isPointNear(lx, ly, startLocal.left, startLocal.top)) { dragHandleType = 1; return true }
                        if (isPointNear(lx, ly, endLocal.right, endLocal.bottom)) { dragHandleType = 2; return true }
                    }
                    val sx = toScreenX(lx); val sy = toScreenY(ly)
                    val nearest = findNearestWordGlobal(sx, sy)
                    if (nearest != -1) {
                        globalSelectionStart = nearest; globalSelectionEnd = nearest
                        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS); invalidate(); return true
                    } else { globalSelectionStart = -1; globalSelectionEnd = -1; invalidate() }
                }
                MotionEvent.ACTION_MOVE -> {
                    val dx = lx - lastTouchX; val dy = ly - lastTouchY
                    lastTouchX = lx; lastTouchY = ly
                    if (isDraggingToolbar) { toolbarOffsetX += dx; toolbarOffsetY += dy; invalidate(); return true }
                    val sx = toScreenX(lx); val sy = toScreenY(ly)
                    if (dragHandleType != 0) {
                        val nearest = findNearestWordGlobal(sx, sy)
                        if (nearest != -1) {
                            if (dragHandleType == 1) globalSelectionStart = nearest else globalSelectionEnd = nearest
                            invalidate()
                        }
                        return true
                    } else if (globalSelectionStart != -1) {
                        val nearest = findNearestWordGlobal(sx, sy)
                        if (nearest != -1) { globalSelectionEnd = nearest; invalidate(); return true }
                    }
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    isDraggingToolbar = false; dragHandleType = 0; invalidate()
                }
            }
            return true
        }

        private fun findNearestWordGlobal(sx: Float, sy: Float): Int {
            var minDist = Float.MAX_VALUE; var nearest = -1
            allWords.forEachIndexed { idx, word ->
                val dx = sx - word.bounds.centerX(); val dy = sy - word.bounds.centerY()
                val d = dx * dx + dy * dy
                if (d < minDist) { minDist = d; nearest = idx }
            }
            return if (minDist < 600 * 600) nearest else -1
        }

        private fun isPointNear(px: Float, py: Float, x: Float, y: Float): Boolean {
            val dx = px - x; val dy = py - y
            return dx * dx + dy * dy < 80 * 80
        }

        private fun handleToolbarAction(label: String) {
            val start = globalSelectionStart.coerceAtMost(globalSelectionEnd)
            val end = globalSelectionStart.coerceAtLeast(globalSelectionEnd)
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
                        type = "text/plain"; putExtra(android.content.Intent.EXTRA_TEXT, selectedText); addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    context.startActivity(android.content.Intent.createChooser(intent, "Share text via").apply { addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK) })
                    dismiss()
                }
                "All" -> { globalSelectionStart = 0; globalSelectionEnd = allWords.lastIndex; invalidate() }
                "Cancel" -> { globalSelectionStart = -1; globalSelectionEnd = -1; invalidate() }
            }
        }
    }
}
