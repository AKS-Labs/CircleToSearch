/*
 * Copyright (C) 2025 AKS-Labs
 * SPDX-License-Identifier: GPL-3.0-or-later
 *
 * CopyTextOverlayManager — fully offline, zero network, zero logging.
 * Uses AccessibilityNodeInfo tree traversal + PorterDuff.Mode.CLEAR canvas
 * punch-out to highlight selectable text regions on screen.
 */

package com.akslabs.circletosearch.ui.components

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.PixelFormat
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.Rect
import android.graphics.RectF
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.WindowManager
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Toast
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Simple holder for a floating-toolbar button's label and screen hit-rect. */
private class ToolbarButton(val label: String, val rect: Rect)

/**
 * Manages the dim+punch-out Copy Text overlay.
 *
 * Usage:
 *   val mgr = CopyTextOverlayManager(context, windowManager, { getRootInActiveWindow() })
 *   mgr.show(onDismiss = { ... })
 *   mgr.rescanNodes()  // call on scroll events
 *   mgr.dismiss()
 */
class CopyTextOverlayManager(
    private val context: Context,
    private val windowManager: WindowManager,
    /** Lambda that returns the current AccessibilityNodeInfo root. */
    private val getRoot: () -> AccessibilityNodeInfo?
) {
    private var dimView: DimPunchOutView? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var scanJob: Job? = null
    private var onDismissCallback: (() -> Unit)? = null

    /** Whether screen transition animations are effectively disabled by the user. */
    private val reduceMotion: Boolean
        get() = Build.VERSION.SDK_INT >= Build.VERSION_CODES.JELLY_BEAN_MR1 &&
                Settings.Global.getFloat(
                    context.contentResolver,
                    Settings.Global.TRANSITION_ANIMATION_SCALE,
                    1f
                ) == 0f

    // ── Public API ───────────────────────────────────────────────────────────

    fun show(onDismiss: () -> Unit) {
        if (dimView != null) return // Already visible
        onDismissCallback = onDismiss

        val view = DimPunchOutView(context) { dismiss() }
        dimView = view

        val params = WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.MATCH_PARENT,
            WindowManager.LayoutParams.TYPE_ACCESSIBILITY_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or
                    WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN or
                    WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS,
            PixelFormat.TRANSLUCENT
        ).apply {
            gravity = Gravity.TOP or Gravity.START
        }

        try {
            windowManager.addView(view, params)
        } catch (e: Exception) {
            e.printStackTrace()
            return
        }

        // Kick off node scan
        scanNodes(view)
    }

    fun dismiss() {
        scanJob?.cancel()
        dimView?.let {
            try { windowManager.removeView(it) } catch (_: Exception) {}
        }
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
            val rects = withContext(Dispatchers.Default) {
                collectTextRects(getRoot())
            }
            view.setTextRects(rects)
        }
    }

    /**
     * BFS traversal collecting bounding rects of all nodes that carry readable text.
     * Null-safe; never throws.
     */
    private fun collectTextRects(root: AccessibilityNodeInfo?): List<Rect> {
        if (root == null) return emptyList()
        val result = mutableListOf<Rect>()
        val queue = ArrayDeque<AccessibilityNodeInfo>()
        queue.add(root)

        while (queue.isNotEmpty()) {
            val node = queue.removeFirst()
            try {
                val hasText = !node.text.isNullOrEmpty() ||
                        !node.contentDescription.isNullOrEmpty()
                val isTextClass = node.className?.let { cls ->
                    cls.contains("TextView") || cls.contains("EditText") ||
                            cls.contains("Button") || cls.contains("Label")
                } == true
                val isSelectable = node.isTextSelectable

                if (((hasText || isSelectable) && isTextClass) || (hasText && node.isClickable)) {
                    val rect = Rect()
                    node.getBoundsInScreen(rect)
                    if (!rect.isEmpty) {
                        result.add(rect)
                    }
                }

                for (i in 0 until node.childCount) {
                    node.getChild(i)?.let { queue.add(it) }
                }
            } catch (_: Exception) {
                /* Skip broken/recycled nodes */
            }
        }
        return result
    }

    // ── Dim + punch-out View ─────────────────────────────────────────────────

    @SuppressLint("ClickableViewAccessibility")
    inner class DimPunchOutView(
        context: Context,
        private val onBackPress: () -> Unit
    ) : View(context) {

        private val textRects = mutableListOf<Rect>()

        // 15% black dim over the full screen (alpha = 0.15 * 255 ≈ 38)
        private val dimPaint = Paint().apply {
            color = Color.BLACK
            alpha = 38
            isAntiAlias = false
        }

        // PorterDuff CLEAR — erases dim pixels, revealing real content at full brightness
        private val clearPaint = Paint().apply {
            xfermode = PorterDuffXfermode(PorterDuff.Mode.CLEAR)
            isAntiAlias = true
        }

        // Toolbar state
        private var toolbarAnchor: Rect? = null
        private var tappedNodeText: CharSequence? = null
        private var tappedNodeInfo: AccessibilityNodeInfo? = null
        private var toolbarButtons: List<ToolbarButton> = emptyList()

        // Paint objects for the floating toolbar — created once, not per-frame
        private val cardPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.WHITE
            // shadow is set in onDraw via setLayerType(SOFTWARE)
        }
        private val copyBtnPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#FF6750A4") // M3 primary purple
        }
        private val btnTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.parseColor("#FF1C1B1F")
            textSize = 42f
            textAlign = Paint.Align.CENTER
        }
        private val copyBtnTextPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.WHITE
            textSize = 42f
            textAlign = Paint.Align.CENTER
        }

        init {
            // Software layer needed for shadow in card paint
            setLayerType(LAYER_TYPE_SOFTWARE, null)
        }

        fun setTextRects(rects: List<Rect>) {
            textRects.clear()
            textRects.addAll(rects)
            invalidate()
        }

        override fun onDraw(canvas: Canvas) {
            // saveLayer is required for PorterDuff.CLEAR to work on this layer
            val saveCount = canvas.saveLayer(
                0f, 0f, width.toFloat(), height.toFloat(), null
            )

            // 1. Dim the entire screen at 15% opacity
            canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), dimPaint)

            // 2. Punch out each text region — dim is erased, showing full-brightness content
            for (rect in textRects) {
                canvas.drawRoundRect(
                    rect.left.toFloat() - 2f,
                    rect.top.toFloat() - 2f,
                    rect.right.toFloat() + 2f,
                    rect.bottom.toFloat() + 2f,
                    8f, 8f, clearPaint
                )
            }

            canvas.restoreToCount(saveCount)

            // 3. Draw floating action toolbar above the tapped rect (outside saveLayer)
            toolbarAnchor?.let { drawFloatingToolbar(canvas, it) }
        }

        private fun drawFloatingToolbar(canvas: Canvas, anchor: Rect) {
            val btnW = 210
            val btnH = 88
            val padding = 14
            val gap = 8
            val labels = listOf("Copy", "Select All", "Cancel")

            val totalW = labels.size * btnW + (labels.size - 1) * gap + padding * 2
            val totalH = btnH + padding * 2

            var left = anchor.centerX() - totalW / 2
            var top = anchor.top - totalH - 18
            if (top < 0) top = anchor.bottom + 18
            if (left < 0) left = 8
            if (left + totalW > width) left = width - totalW - 8

            // Card background with shadow via cardPaint
            cardPaint.setShadowLayer(16f, 0f, 4f, Color.parseColor("#44000000"))
            canvas.drawRoundRect(
                RectF(left.toFloat(), top.toFloat(), (left + totalW).toFloat(), (top + totalH).toFloat()),
                24f, 24f, cardPaint
            )
            cardPaint.clearShadowLayer()

            val buttons = mutableListOf<ToolbarButton>()
            labels.forEachIndexed { idx, label ->
                val bLeft = left + padding + idx * (btnW + gap)
                val bTop = top + padding
                val bRect = Rect(bLeft, bTop, bLeft + btnW, bTop + btnH)
                buttons.add(ToolbarButton(label, bRect))

                val bRectF = RectF(bRect)
                if (label == "Copy") {
                    canvas.drawRoundRect(bRectF, 12f, 12f, copyBtnPaint)
                    canvas.drawText(
                        label,
                        bRectF.centerX(), bRectF.centerY() + copyBtnTextPaint.textSize / 3f,
                        copyBtnTextPaint
                    )
                } else {
                    canvas.drawText(
                        label,
                        bRectF.centerX(), bRectF.centerY() + btnTextPaint.textSize / 3f,
                        btnTextPaint
                    )
                }
            }
            toolbarButtons = buttons
        }

        override fun onTouchEvent(event: MotionEvent): Boolean {
            if (event.action != MotionEvent.ACTION_UP) return true

            val x = event.x.toInt()
            val y = event.y.toInt()

            // Check toolbar button tap first
            if (toolbarAnchor != null) {
                for (btn in toolbarButtons) {
                    if (btn.rect.contains(x, y)) {
                        handleToolbarAction(btn.label)
                        return true
                    }
                }
                // Tap outside toolbar — close just the toolbar
                toolbarAnchor = null
                tappedNodeInfo = null
                tappedNodeText = null
                toolbarButtons = emptyList()
                invalidate()
                return true
            }

            // Check if user tapped inside a bright (punched-out) text rect
            for (i in textRects.indices) {
                val rect = textRects[i]
                if (rect.contains(x, y)) {
                    showToolbarFor(rect, i)
                    return true
                }
            }

            // Tap outside all text regions — dismiss the whole overlay
            onBackPress()
            return true
        }

        private fun showToolbarFor(rect: Rect, nodeIndex: Int) {
            val root = getRoot()
            if (root != null) {
                val nodes = collectFlatTextNodes(root)
                if (nodeIndex < nodes.size) {
                    tappedNodeInfo = nodes[nodeIndex]
                    tappedNodeText = tappedNodeInfo?.text
                        ?: tappedNodeInfo?.contentDescription
                }
            }
            toolbarAnchor = rect
            invalidate()
        }

        /** Same BFS as outer [collectTextRects] but returns the node objects themselves. */
        private fun collectFlatTextNodes(root: AccessibilityNodeInfo): List<AccessibilityNodeInfo> {
            val result = mutableListOf<AccessibilityNodeInfo>()
            val queue = ArrayDeque<AccessibilityNodeInfo>()
            queue.add(root)
            while (queue.isNotEmpty()) {
                val node = queue.removeFirst()
                try {
                    val hasText = !node.text.isNullOrEmpty() || !node.contentDescription.isNullOrEmpty()
                    val isTextClass = node.className?.let {
                        it.contains("TextView") || it.contains("EditText") ||
                                it.contains("Button") || it.contains("Label")
                    } == true
                    if (((hasText || node.isTextSelectable) && isTextClass) || (hasText && node.isClickable)) {
                        val r = Rect()
                        node.getBoundsInScreen(r)
                        if (!r.isEmpty) result.add(node)
                    }
                    for (i in 0 until node.childCount) {
                        node.getChild(i)?.let { queue.add(it) }
                    }
                } catch (_: Exception) {}
            }
            return result
        }

        private fun handleToolbarAction(label: String) {
            when (label) {
                "Copy" -> {
                    val text = tappedNodeText
                    if (!text.isNullOrEmpty()) {
                        val clipboard = context
                            .getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                        clipboard.setPrimaryClip(ClipData.newPlainText("Copied Text", text))
                        Toast.makeText(context, "Text copied ✓", Toast.LENGTH_SHORT).show()
                    } else {
                        Toast.makeText(context, "No text found in this region", Toast.LENGTH_SHORT).show()
                    }
                }
                "Select All" -> {
                    tappedNodeInfo?.let { node ->
                        val text = node.text
                        if (!text.isNullOrEmpty()) {
                            val args = Bundle().apply {
                                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_START_INT, 0)
                                putInt(AccessibilityNodeInfo.ACTION_ARGUMENT_SELECTION_END_INT, text.length)
                            }
                            node.performAction(AccessibilityNodeInfo.ACTION_SET_SELECTION, args)
                            val clipboard = context
                                .getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                            clipboard.setPrimaryClip(ClipData.newPlainText("Copied Text", text))
                            Toast.makeText(context, "All text copied ✓", Toast.LENGTH_SHORT).show()
                        }
                    }
                }
                "Cancel" -> { /* fall through to cleanup below */ }
            }
            // Reset toolbar state regardless of action
            toolbarAnchor = null
            tappedNodeInfo = null
            tappedNodeText = null
            toolbarButtons = emptyList()
            invalidate()
        }

        override fun isInEditMode(): Boolean = false

        override fun onKeyDown(keyCode: Int, event: android.view.KeyEvent): Boolean {
            if (keyCode == android.view.KeyEvent.KEYCODE_BACK) {
                onBackPress()
                return true
            }
            return super.onKeyDown(keyCode, event)
        }
    }
}
