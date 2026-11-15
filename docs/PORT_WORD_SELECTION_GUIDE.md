# Porting the "Lens-style" OCR Text-Selection UX into your app

**Audience:** an AI agent implementing this in a *different* Android app that currently uses ML Kit
text recognition with a custom dim-and-highlight selection UI.

**Goal:** replace that custom UI with the system proven in the CircleToSearch app:

- While OCR runs: an animated scan overlay (sweeping wave, then a breathing pulse over each
  detected line).
- When done: no dimming. Each recognized **line** gets a rounded pill highlight. Tap a word to
  select it, **long-press a word and drag** to extend the selection word-by-word with a magnifier
  loupe, or grab the **drag handles** at either end. A floating action bar (Copy / Select all /
  Share / Web search) tracks the selection.
- The copy text joins lines intelligently (CJK-tight, block-gap newlines) — identical to what's
  highlighted.

Everything below is **self-contained**: no dependency on the CircleToSearch repo at build time,
no Rust toolchain. The only asset to copy is one vector drawable.

---

## 0. Architecture — what you're copying and why

The UX has four layers. Two are pure Compose/Kotlin you copy almost verbatim; one is geometry
logic that CircleToSearch runs in **Rust** (via uniFFI) — this guide ports it to **pure Kotlin**
so you don't need a native toolchain; one is a thin adapter from your OCR output.

| Layer | Source in CircleToSearch | What you take |
|---|---|---|
| Selection geometry (hit-testing, drag ranges, merged pills, handles, copy text) | `translator-rs/crates/translator-core/src/selection.rs` (Rust, 481 lines) | **Kotlin port provided below** (`WordSelectionGeometry.kt`) |
| The interactive overlay (pill drawing, handles, ActionMode bar, all gestures) | `OCR-translation/src/main/java/dev/davidv/translator/ui/components/WordSelectionOverlay.kt` | **Adapted listing below** (`WordSelectionOverlay.kt`) — uniFFI calls swapped for the Kotlin geometry |
| Scan progress animation | `.../ui/components/ScanAnimationOverlay.kt` + `OverlayGeometry.kt` | **Adapted listings below** |
| Words/regions data | Rust records `PositionedWord`, `OrientedRect` | **Kotlin data classes below** |
| OCR itself | PaddleOCR (Rust) | **Keep your ML Kit** — you map its output with the adapter in §7 |

Key design rules baked into this system (do not change them while adapting):

1. **All geometry lives in image-pixel space** (the OCR input bitmap's pixels). The overlay maps
   image→view with a `ContentScale.Fit` transform, so it stays aligned inside any zoom/pan
   container. Your overlay must be given the *bitmap's* dimensions, not the view's.
2. **The geometry brain is one pure function**: `resolveSelection(words, startIndex, endIndex)` →
   everything needed to paint (per-line pills, handle anchor points, union bounds, copy text).
   The UI holds only *which word indices are selected* (`selStart`/`selEnd`) and calls it.
3. **Gesture contract**: the overlay consumes taps, long-presses and handle drags; a plain drag is
   *observed but not consumed* so the parent container keeps its own gesture (pan/zoom in the
   translator, circle-drawing in CircleToSearch, whatever your app does). This parent/child split
   is what lets a text-selection child coexist with a full-screen gesture.

---

## 1. Step 1 — Data types (`OcrSelectionTypes.kt`)

Create in your app (adjust the package everywhere below):

```kotlin
package com.yourapp.ocrselection

import androidx.compose.ui.geometry.Offset
import kotlin.math.cos
import kotlin.math.sin

/** One recognized word, in image-pixel space. ML Kit's `Text.Element` maps to this. */
data class OcrWord(
  val text: String,
  val bounds: OrientedRect,
  /** Index of the visual line the word belongs to. Words of one line must be adjacent in the
   *  list and lines must be in reading order (top-to-bottom) — the geometry groups runs of
   *  equal lineIndex, so this is a contract, not a lookup. */
  val lineIndex: Int,
)

/**
 * Center + size + rotation rectangle in image-pixel space. ML Kit boxes are axis-aligned, so
 * `angleRadians` is 0 there — the oriented math is kept so rotated text keeps working.
 */
data class OrientedRect(
  val cx: Float,
  val cy: Float,
  val width: Float,
  val height: Float,
  val angleRadians: Float = 0f,
) {
  /** The 4 corners: reading-direction start/end on the top edge, then on the bottom edge. */
  fun corners(): Array<Pair<Float, Float>> {
    val tx = cos(angleRadians); val ty = sin(angleRadians)
    val px = -sin(angleRadians); val py = cos(angleRadians)
    val hw = width * 0.5f; val hh = height * 0.5f
    return arrayOf(
      Pair(cx - hw * tx - hh * px, cy - hw * ty - hh * py),
      Pair(cx + hw * tx - hh * px, cy + hw * ty - hh * py),
      Pair(cx + hw * tx + hh * px, cy + hw * ty + hh * py),
      Pair(cx - hw * tx + hh * px, cy - hw * ty + hh * py),
    )
  }
}

/** Everything needed to paint and act on one selection, produced in a single call. */
data class SelectionView(
  /** One merged rounded-rect per selected line, already clamped so tightly-spaced lines meet
   *  halfway instead of overlapping. Draw these as the blue highlight. */
  val pills: List<OrientedRect>,
  /** Bottom corner at the reading-start of the first word (leading handle anchors here). */
  val startHandle: Offset,
  /** Bottom corner at the reading-end of the last word. */
  val endHandle: Offset,
  /** Axis-aligned bounds of the whole selection — anchor the action bar with this. Pure Kotlin
   *  so the geometry stays unit-testable on the JVM; convert to android.graphics.Rect at the
   *  ActionMode boundary. */
  val bounds: SelectionRect,
  /** The text to copy: per-block joins, blocks separated by newlines. */
  val text: String,
)

/** Bounds in image-pixel space, integers. */
data class SelectionRect(val left: Int, val top: Int, val right: Int, val bottom: Int) {
  fun toAndroidRect() = android.graphics.Rect(left, top, right, bottom)
}

/** Detected text regions of one image (pre-recognition, for the scan animation). */
data class DetectedRegions(
  val imageWidth: Int,
  val imageHeight: Int,
  val boxes: List<OrientedRect>,
)
```

---

## 2. Step 2 — Selection geometry (`WordSelectionGeometry.kt`)

This is a faithful Kotlin port of CircleToSearch's Rust `selection.rs` (same constants, same
order of operations, same edge cases — its unit tests are listed in §9 and should pass verbatim).

```kotlin
package com.yourapp.ocrselection

import androidx.compose.ui.geometry.Offset
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/** Padding around a word box when hit-testing, as a fraction of its height: touch points land
 *  between lines often enough that the bare box feels unresponsive. */
private const val HIT_PADDING = 0.25f

/** Lines further apart than this multiple of their height are separate blocks, joined with a
 *  newline rather than a space. */
private const val BLOCK_GAP = 1.5f

enum class WritingAxis { Horizontal, Vertical }

private fun readingPos(r: OrientedRect) = r.cx * cos(r.angleRadians) + r.cy * sin(r.angleRadians)

private fun crossPos(r: OrientedRect) = -r.cx * sin(r.angleRadians) + r.cy * cos(r.angleRadians)

private fun axisOf(r: OrientedRect): WritingAxis {
  val a = ((r.angleRadians % PI) + PI) % PI
  return if (a < PI / 4 || a > 3 * PI / 4) WritingAxis.Horizontal else WritingAxis.Vertical
}

/** The writing axis of one word, so a handle drag keeps the selection on its starting axis. */
fun wordAxis(words: List<OcrWord>, index: Int): WritingAxis? =
  words.getOrNull(index)?.let { axisOf(it.bounds) }

/** The word under an image-space point, or null for a tap on bare image. Boxes are padded
 *  vertically; the first match in reading order wins where padded boxes overlap. */
fun hitTestWord(words: List<OcrWord>, x: Float, y: Float): Int? =
  words.indexOfFirst { w ->
    val b = w.bounds
    val (c, s) = cos(b.angleRadians) to sin(b.angleRadians)
    val dx = x - b.cx
    val dy = y - b.cy
    val along = dx * c + dy * s
    val across = -dx * s + dy * c
    val pad = b.height * HIT_PADDING
    abs(along) <= b.width * 0.5f + pad && abs(across) <= b.height * 0.5f + pad
  }.takeIf { it >= 0 }

/** The word whose centre is nearest an image-space point, optionally restricted to one writing
 *  axis. Used while dragging a handle or a long-press anchor, where the finger is rarely inside
 *  a box. */
fun nearestWord(words: List<OcrWord>, x: Float, y: Float, axis: WritingAxis?): Int? =
  words
    .withIndex()
    .filter { axis == null || axisOf(it.value.bounds) == axis }
    .minByOrNull { (_, w) ->
      val dx = w.bounds.cx - x
      val dy = w.bounds.cy - y
      dx * dx + dy * dy
    }
    ?.index

/** Indices covered by a drag between two words. `start`/`end` may arrive in either order; words
 *  off the anchor's writing axis are dropped (a horizontal drag cannot sweep into a vertical
 *  column that crosses it). */
private fun selectionIndices(words: List<OcrWord>, start: Int, end: Int): List<Int> {
  val lo = min(start, end)
  val hi = max(start, end)
  if (lo >= words.size) return emptyList()
  val hiC = hi.coerceAtMost(words.size - 1)
  val axis = axisOf(words[lo].bounds)
  return (lo..hiC).filter { axisOf(words[it].bounds) == axis }
}

private fun isCjk(c: Char): Boolean {
  val code = c.code
  return code in 0x4E00..0x9FFF   // CJK unified ideographs
    || code in 0x3000..0x303F     // CJK symbols and punctuation
    || code in 0x3400..0x4DBF     // extension A
    || code in 0x3040..0x30FF     // hiragana + katakana
    || code in 0xAC00..0xD7AF     // hangul syllables
    || code in 0xF900..0xFAFF     // compatibility ideographs
    || code in 0xFF00..0xFFEF     // halfwidth and fullwidth forms
}

/** Join one block's words. A gap closes up only when the characters on both sides of it are CJK;
 *  every other gap takes a space, so an embedded Latin run keeps the spacing it was rendered
 *  with. The decision is per gap, not per block: a block-wide ratio miscounts ("Microsoft" is
 *  nine characters and outvotes ten ideographs). */
private fun joinBlock(texts: List<String>): String {
  val joined = StringBuilder()
  for (text in texts) {
    val prev = joined.lastOrNull()
    val next = text.firstOrNull()
    val tight = if (prev != null && next != null) isCjk(prev) && isCjk(next) else true
    if (joined.isNotEmpty() && !tight) joined.append(' ')
    joined.append(text)
  }
  return joined.toString().trim()
}

/** Split words into runs sharing a lineIndex. Words arrive in reading order, so a line's words
 *  are already adjacent. */
private fun groupLines(words: List<OcrWord>): List<List<OcrWord>> {
  val lines = mutableListOf<MutableList<OcrWord>>()
  for (word in words) {
    val last = lines.lastOrNull()
    if (last != null && last[0].lineIndex == word.lineIndex) last.add(word) else lines.add(mutableListOf(word))
  }
  return lines
}

private fun isBlockBreak(prev: List<OcrWord>, cur: List<OcrWord>): Boolean {
  val height = (prev + cur).maxOf { it.bounds.height }
  return abs(crossPos(cur[0].bounds) - crossPos(prev[0].bounds)) > BLOCK_GAP * height
}

/** Merge one line's words into a single oriented rect spanning them. */
private fun mergeAlongLine(words: List<OcrWord>): OrientedRect {
  val reference = words[0].bounds
  val c = cos(reference.angleRadians)
  val s = sin(reference.angleRadians)
  var minAlong = Float.MAX_VALUE
  var maxAlong = -Float.MAX_VALUE
  var height = 0f
  for (word in words) {
    val along = word.bounds.cx * c + word.bounds.cy * s
    minAlong = min(minAlong, along - word.bounds.width * 0.5f)
    maxAlong = max(maxAlong, along + word.bounds.width * 0.5f)
    height = max(height, word.bounds.height)
  }
  val shift = (minAlong + maxAlong) * 0.5f - (reference.cx * c + reference.cy * s)
  return OrientedRect(
    cx = reference.cx + shift * c,
    cy = reference.cy + shift * s,
    width = maxAlong - minAlong,
    height = height,
    angleRadians = reference.angleRadians,
  )
}

/** Shrink each pill's height to the cross-axis gap to its nearest neighbour, counting only
 *  neighbours that overlap it along the reading axis — a parallel column at the same height
 *  must not shrink it. */
private fun clampPillHeights(pills: MutableList<OrientedRect>) {
  val spans = pills.map { p ->
    Triple(crossPos(p), readingPos(p) - p.width * 0.5f, readingPos(p) + p.width * 0.5f)
  }
  pills.indices.forEach { i ->
    val (cross, start, end) = spans[i]
    val gap = spans
      .withIndex()
      .filter { (j, other) -> j != i && start < other.third && other.second < end }
      .minOfOrNull { (_, other) -> abs(cross - other.first) } ?: Float.MAX_VALUE
    pills[i] = pills[i].copy(height = pills[i].height.coerceAtMost(gap))
  }
}

/** Bottom corner at one end of a word, where a drag handle hangs. Image y points down, so the
 *  across axis `(-sin, cos)` points below the line. */
private fun handlePoint(rect: OrientedRect, leading: Boolean): Offset {
  val c = cos(rect.angleRadians)
  val s = sin(rect.angleRadians)
  val along = (if (leading) -0.5f else 0.5f) * rect.width
  val across = rect.height * 0.5f
  return Offset(rect.cx + along * c - across * s, rect.cy + along * s + across * c)
}

private fun boundsOf(pills: List<OrientedRect>): SelectionRect {
  var left = Float.MAX_VALUE
  var top = Float.MAX_VALUE
  var right = -Float.MAX_VALUE
  var bottom = -Float.MAX_VALUE
  pills.forEach { pill ->
    pill.corners().forEach { (x, y) ->
      left = min(left, x); top = min(top, y)
      right = max(right, x); bottom = max(bottom, y)
    }
  }
  return SelectionRect(
    left.coerceAtLeast(0f).toInt(),
    top.coerceAtLeast(0f).toInt(),
    right.toInt(),
    bottom.toInt(),
  )
}

/** Resolve a drag between two word indices into everything a UI needs to paint and act on it.
 *  Returns null when the range covers no words. */
fun resolveSelection(words: List<OcrWord>, start: Int, end: Int): SelectionView? {
  val indices = selectionIndices(words, start, end)
  if (indices.isEmpty()) return null
  val selected = indices.map { words[it] }
  val lines = groupLines(selected)

  val blocks = mutableListOf<List<OcrWord>>()
  lines.forEachIndexed { i, line ->
    if (i == 0 || isBlockBreak(lines[i - 1], line)) {
      blocks.add(line)
    } else {
      // Merge into the previous block when the vertical gap is within BLOCK_GAP.
      blocks[blocks.lastIndex] = blocks.last() + line
    }
  }
  val text = blocks
    .map { block -> joinBlock(block.map { it.text }) }
    .joinToString("\n")
    .trim()

  val pills = lines.map { mergeAlongLine(it) }.toMutableList()
  clampPillHeights(pills)

  return SelectionView(
    bounds = boundsOf(pills),
    pills = pills,
    startHandle = handlePoint(selected.first().bounds, leading = true),
    endHandle = handlePoint(selected.last().bounds, leading = false),
    text = text,
  )
}
```

> **Note on one subtlety above:** the `blocks.last() + line` line inside `forEachIndexed` must
> append `line` to the last block (list concatenation). If your Kotlin style prefers clarity:
> `blocks[blocks.lastIndex] = blocks.last() + line`. Do **not** drop this — blocks decide where
> the copy text gets newlines.

---

## 3. Step 3 — Image→view geometry helpers (`OverlayGeometry.kt`)

```kotlin
package com.yourapp.ocrselection

import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.rotate
import kotlin.math.min

/** Image→view transform for a bitmap drawn with ContentScale.Fit inside `viewSize`. */
data class FitTransform(val scale: Float, val offsetX: Float, val offsetY: Float) {
  fun mapX(x: Float) = offsetX + x * scale
  fun mapY(y: Float) = offsetY + y * scale
  fun unmapX(x: Float) = (x - offsetX) / scale
  fun unmapY(y: Float) = (y - offsetY) / scale
}

fun fitTransform(viewW: Float, viewH: Float, imageW: Int, imageH: Int): FitTransform {
  val scale = min(viewW / imageW, viewH / imageH)
  return FitTransform(scale, (viewW - imageW * scale) / 2f, (viewH - imageH * scale) / 2f)
}

/** Draw an oriented rect as a rounded pill, mapped through `t` and rotated to its angle. */
fun DrawScope.drawOrientedPill(rect: OrientedRect, t: FitTransform, color: Color) {
  val cx = t.mapX(rect.cx)
  val cy = t.mapY(rect.cy)
  val w = rect.width * t.scale
  val h = rect.height * t.scale
  rotate(degrees = Math.toDegrees(rect.angleRadians.toDouble()).toFloat(), pivot = Offset(cx, cy)) {
    drawRoundRect(
      color = color,
      topLeft = Offset(cx - w / 2f, cy - h / 2f),
      size = Size(w, h),
      cornerRadius = CornerRadius(h / 2f, h / 2f),
    )
  }
}
```

---

## 4. Step 4 — Scan animation (`ScanAnimationOverlay.kt`)

Verbatim port (only the package changes). A wave sweeps top-to-bottom once, then the pills pulse
— a clear "still working" signal for slow OCR.

```kotlin
package com.yourapp.ocrselection

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import kotlin.math.abs

private const val BASE_ALPHA = 0.14f

@Composable
fun ScanAnimationOverlay(
  regions: DetectedRegions,
  modifier: Modifier = Modifier,
) {
  val sweep = remember { Animatable(0f) }
  // Driven from 0 only after the sweep finishes, so the pulse begins at exactly the sweep's
  // resting opacity — no jump into a random phase.
  val breathe = remember { Animatable(0f) }
  var sweepDone by remember { mutableStateOf(false) }
  LaunchedEffect(Unit) {
    sweep.animateTo(1f, tween(durationMillis = 1300, easing = LinearEasing))
    sweepDone = true
    breathe.animateTo(
      1f,
      infiniteRepeatable(
        animation = tween(durationMillis = 1100, easing = FastOutSlowInEasing),
        repeatMode = RepeatMode.Reverse,
      ),
    )
  }

  Canvas(modifier = modifier) {
    if (regions.boxes.isEmpty()) return@Canvas
    val t = fitTransform(size.width, size.height, regions.imageWidth, regions.imageHeight)
    if (!sweepDone) {
      val waveY = sweep.value * size.height
      val halfWidth = size.height * 0.06f
      for (box in regions.boxes) {
        val falloff = (1f - abs(t.mapY(box.cy) - waveY) / halfWidth).coerceAtLeast(0f)
        drawOrientedPill(box, t, Color(1f, 1f, 1f, BASE_ALPHA + 0.30f * falloff))
      }
    } else {
      val alpha = BASE_ALPHA + 0.30f * breathe.value
      for (box in regions.boxes) {
        drawOrientedPill(box, t, Color(1f, 1f, 1f, alpha))
      }
    }
  }
}
```

---

## 5. Step 5 — The selection overlay (`WordSelectionOverlay.kt`)

This is the big one: the full interactive layer — pill highlights, pin-style drag handles, the
floating Copy / **Select all** / Share / Web-search bar, and every gesture (tap-to-select,
**long-press-and-drag to extend with a loupe**, handle drags, host-driven region selection).

**Asset you must copy first**: `selection_handle.xml` (the pin-style handle) — from
`OCR-translation/src/main/res/drawable/selection_handle.xml` in the CircleToSearch repo — into
your `res/drawable/`. Also add to your `strings.xml`: `<string name="select_all">Select all</string>`.

```kotlin
package com.yourapp.ocrselection

import android.app.SearchManager
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.view.View
import android.widget.Magnifier
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import kotlin.math.max
import kotlin.math.min

private const val MENU_COPY = 1
private const val MENU_SELECT_ALL = 2
private const val MENU_SHARE = 3
private const val MENU_SEARCH = 4

private fun copyToClipboard(context: Context, text: String) {
  val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
  clipboard.setPrimaryClip(ClipData.newPlainText("Selection", text))
}

/**
 * A floating text-selection action bar (Copy / Select all / Share / Web search) over the image,
 * mirroring the system text toolbar. Providers are read live so the bar follows the selection;
 * Select all hides itself once everything is selected (via onPrepareActionMode, which runs on
 * invalidate() — NOT on invalidateContentRect(), which only re-queries the position).
 */
private class SelectionActionModeCallback(
  private val context: Context,
  private val textProvider: () -> String,
  private val rectProvider: () -> android.graphics.Rect,
  private val onSelectAll: () -> Unit,
  private val canSelectAll: () -> Boolean,
  private val onFinished: () -> Unit,
) : ActionMode.Callback2() {
  override fun onCreateActionMode(mode: ActionMode, menu: Menu): Boolean {
    menu.add(0, MENU_COPY, 0, android.R.string.copy)
    menu.add(0, MENU_SELECT_ALL, 1, R.string.select_all)
    menu.add(0, MENU_SHARE, 2, "Share")
    menu.add(0, MENU_SEARCH, 3, "Web search")
    return true
  }

  override fun onPrepareActionMode(mode: ActionMode, menu: Menu): Boolean {
    menu.findItem(MENU_SELECT_ALL)?.isVisible = canSelectAll()
    return true
  }

  override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
    val text = textProvider()
    when (item.itemId) {
      MENU_COPY -> copyToClipboard(context, text)
      MENU_SELECT_ALL -> {
        onSelectAll()
        // Keep the mode: the selection just grew; the invalidation from the state change
        // hides the item and re-anchors the bar over the full selection.
        return true
      }
      MENU_SHARE -> {
        val intent = Intent(Intent.ACTION_SEND).apply {
          type = "text/plain"
          putExtra(Intent.EXTRA_TEXT, text)
        }
        context.startActivity(Intent.createChooser(intent, null).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
      }
      MENU_SEARCH -> {
        val intent = Intent(Intent.ACTION_WEB_SEARCH).apply {
          putExtra(SearchManager.QUERY, text)
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        runCatching { context.startActivity(intent) }
      }
      else -> return false
    }
    mode.finish()
    return true
  }

  override fun onDestroyActionMode(mode: ActionMode) { onFinished() }

  override fun onGetContentRect(mode: ActionMode, view: View, outRect: android.graphics.Rect) {
    outRect.set(rectProvider())
  }
}

private enum class DragMode { START, END }

/** Local-space distance from a point to an image-space position, for handle grabbing. */
private fun distanceTo(point: Offset, target: Offset, t: FitTransform): Float =
  (point - Offset(t.mapX(target.x), t.mapY(target.y))).getDistance()

/**
 * Persistent Lens-style word selection over the displayed image. Tapping a word selects it;
 * tapping another word moves the selection; tapping empty space clears it. The selection stays
 * with draggable end handles, and the floating action bar tracks it. A plain drag is left for
 * the container (pan/zoom/whatever the host does); only handle drags edit the selection — unless
 * [longPressDrag] is on, in which case a long-press anchors and a drag extends word-by-word.
 * Nothing is auto-copied.
 *
 * `words` are in image-pixel space, mapped with the image's ContentScale.Fit transform so this
 * stays aligned inside the image's transformed (zoom/pan) container. `scale` is the container's
 * current zoom — the magnifier is suppressed when zoomed.
 */
@Composable
fun WordSelectionOverlay(
  words: List<OcrWord>,
  imageWidth: Int,
  imageHeight: Int,
  scale: Float = 1f,
  modifier: Modifier = Modifier,
  onSelectionChanged: (Boolean) -> Unit = {},
  clearSelectionSignal: Int = 0,
  longPressDrag: Boolean = false,
  pendingSelectRegion: android.graphics.Rect? = null,
  onSelectRegionHandled: () -> Unit = {},
) {
  val context = LocalContext.current
  val view = LocalView.current
  val haptic = LocalHapticFeedback.current
  val handlePainter = painterResource(R.drawable.selection_handle)
  // Loupe under the dragging finger. Native Magnifier is API 28+; older devices get none.
  val magnifier =
    remember(view) { if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) Magnifier(view) else null }
  DisposableEffect(magnifier) {
    onDispose { if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) magnifier?.dismiss() }
  }

  var selStart by remember(words) { mutableStateOf<Int?>(null) }
  var selEnd by remember(words) { mutableStateOf<Int?>(null) }
  var dragging by remember { mutableStateOf(false) }
  var contentRect by remember { mutableStateOf(android.graphics.Rect()) }
  var layoutCoords by remember { mutableStateOf<LayoutCoordinates?>(null) }
  val scaleState = rememberUpdatedState(scale)

  val hasSelection = selStart != null && selEnd != null
  LaunchedEffect(hasSelection, words) { onSelectionChanged(hasSelection) }
  // Host force-clear: each new signal value wipes an active selection.
  LaunchedEffect(clearSelectionSignal) {
    if (clearSelectionSignal != 0) { selStart = null; selEnd = null }
  }

  val selection = remember(words, selStart, selEnd) {
    val s = selStart
    val e = selEnd
    if (s != null && e != null) resolveSelection(words, s, e) else null
  }
  val selectedText = rememberUpdatedState(selection?.text ?: "")
  val rectState = rememberUpdatedState(contentRect)

  // Content rect for the action bar — at composable scope so host-driven selections can
  // position it too; layoutCoords.size is the same node as the gesture's `size`.
  fun recomputeRect() {
    val s = selStart ?: return
    val e = selEnd ?: return
    val coords = layoutCoords ?: return
    val size = coords.size
    val t = fitTransform(size.width.toFloat(), size.height.toFloat(), imageWidth, imageHeight)
    val box = resolveSelection(words, s, e)?.bounds ?: return
    val l = t.mapX(box.left.toFloat())
    val top = t.mapY(box.top.toFloat())
    val r = t.mapX(box.right.toFloat())
    val b = t.mapY(box.bottom.toFloat())
    val topLeft = coords.localToRoot(Offset(l, top))
    val bottomRight = coords.localToRoot(Offset(r, b))
    contentRect = android.graphics.Rect(
      topLeft.x.toInt(), topLeft.y.toInt(), bottomRight.x.toInt(), bottomRight.y.toInt(),
    )
  }

  // Drive the floating action bar: present while there's a selection and no active drag.
  var actionMode by remember { mutableStateOf<ActionMode?>(null) }
  LaunchedEffect(hasSelection, dragging, selStart, selEnd) {
    if (hasSelection && !dragging) {
      if (actionMode == null) {
        actionMode = view.startActionMode(
          SelectionActionModeCallback(
            context = context,
            textProvider = { selectedText.value },
            rectProvider = { rectState.value },
            onSelectAll = {
              if (words.isNotEmpty()) {
                selStart = 0
                selEnd = words.size - 1
                recomputeRect()
              }
            },
            canSelectAll = { words.isNotEmpty() && !(selStart == 0 && selEnd == words.size - 1) },
            onFinished = { actionMode = null },
          ),
          ActionMode.TYPE_FLOATING,
        )
      } else {
        // invalidate() (not invalidateContentRect(), which only re-queries the position) so
        // onPrepareActionMode re-runs too — Select all hides once everything is selected.
        actionMode?.invalidate()
      }
    } else {
      actionMode?.finish()
      actionMode = null
    }
  }
  DisposableEffect(Unit) { onDispose { actionMode?.finish() } }

  // Host-driven region selection: the words whose boxes intersect the image-space region
  // become the selection (empty region → clears).
  LaunchedEffect(pendingSelectRegion) {
    val region = pendingSelectRegion ?: return@LaunchedEffect
    var first: Int? = null
    var last: Int? = null
    words.forEachIndexed { i, w ->
      val b = w.bounds
      val l = b.cx - b.width / 2f
      val r = b.cx + b.width / 2f
      val tp = b.cy - b.height / 2f
      val btm = b.cy + b.height / 2f
      if (r >= region.left && l <= region.right && btm >= region.top && tp <= region.bottom) {
        if (first == null) first = i
        last = i
      }
    }
    selStart = first
    selEnd = last
    if (first != null) recomputeRect()
    onSelectRegionHandled()
  }

  Canvas(
    modifier =
      modifier
        .onGloballyPositioned { layoutCoords = it }
        .pointerInput(words, imageWidth, imageHeight, longPressDrag) {
          val slop = viewConfiguration.touchSlop

          fun transform() =
            fitTransform(size.width.toFloat(), size.height.toFloat(), imageWidth, imageHeight)

          // Word whose oriented box (slightly padded) contains the touched point, or null.
          fun wordAt(pos: Offset): Int? {
            val t = transform()
            return hitTestWord(words, t.unmapX(pos.x), t.unmapY(pos.y))
          }

          // Loupe raised a full bubble-height above the finger so it isn't occluded.
          fun showMagnifier(pos: Offset) {
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) return
            val m = magnifier ?: return
            val root = (layoutCoords ?: return).localToRoot(pos)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
              m.show(root.x, root.y, root.x, root.y - m.height.toFloat())
            } else {
              m.show(root.x, root.y)
            }
          }

          awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            val t = transform()
            // Grab radius in local coords; divide by zoom so it stays constant on-screen.
            val handleRadius = 28.dp.toPx() / scaleState.value
            val s0 = selStart
            val e0 = selEnd
            val selectionView =
              if (s0 != null && e0 != null) resolveSelection(words, s0, e0) else null
            val handleMode =
              if (selectionView != null) {
                when {
                  distanceTo(down.position, selectionView.startHandle, t) < handleRadius -> DragMode.START
                  distanceTo(down.position, selectionView.endHandle, t) < handleRadius -> DragMode.END
                  else -> null
                }
              } else null

            if (handleMode != null) {
              // Grab a handle: consume so the container doesn't pan, resize the selection.
              down.consume()
              while (true) {
                val event = awaitPointerEvent()
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                if (!change.pressed) break
                change.consume()
                dragging = true
                val fixed = if (handleMode == DragMode.START) e0!! else s0!!
                val tt = transform()
                val axis = wordAxis(words, fixed)
                nearestWord(
                  words,
                  tt.unmapX(change.position.x),
                  tt.unmapY(change.position.y),
                  axis,
                )?.let { nw ->
                  selStart = min(fixed, nw)
                  selEnd = max(fixed, nw)
                }
                recomputeRect()
                showMagnifier(change.position)
              }
              dragging = false
              if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) magnifier?.dismiss()
            } else {
              // Not a handle: observe without consuming so the container keeps its gesture; a
              // tap selects the word under the finger. With [longPressDrag], holding still on
              // a word past the long-press timeout anchors a selection and dragging extends it
              // word-by-word — events are consumed from the anchor on, so the container's own
              // drag gesture never fires, exactly like the handle drags above.
              var moved = false
              var anchored = false
              val downWord = wordAt(down.position)
              while (true) {
                val event = awaitPointerEvent()
                val change = event.changes.firstOrNull { it.id == down.id } ?: break
                if (!change.pressed) break
                if (!moved && (change.position - down.position).getDistance() > slop) moved = true
                if (!anchored && longPressDrag && !moved &&
                  downWord != null &&
                  change.uptimeMillis - down.uptimeMillis >=
                    android.view.ViewConfiguration.getLongPressTimeout()
                ) {
                  anchored = true
                  dragging = true
                  haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                  selStart = downWord
                  selEnd = downWord
                  recomputeRect()
                }
                if (!anchored) continue
                change.consume()
                val anchor = selStart
                if (anchor != null) {
                  val tt = transform()
                  val axis = wordAxis(words, anchor)
                  nearestWord(
                    words,
                    tt.unmapX(change.position.x),
                    tt.unmapY(change.position.y),
                    axis,
                  )?.let { nw ->
                    selStart = min(anchor, nw)
                    selEnd = max(anchor, nw)
                    recomputeRect()
                  }
                }
                showMagnifier(change.position)
              }
              if (anchored) {
                dragging = false
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) magnifier?.dismiss()
              } else if (!moved) {
                val w = wordAt(down.position)
                selStart = w
                selEnd = w
                if (w != null) recomputeRect()
              }
            }
          }
        },
  ) {
    val selectionView = selection ?: return@Canvas
    val t = fitTransform(size.width, size.height, imageWidth, imageHeight)
    // Pills arrive merged per line and already clamped against neighbouring lines.
    selectionView.pills.forEach { pill -> drawOrientedPill(pill, t, Color(0x553B82F6)) }
    // Pin-style handles hanging below the line ends. Divide by zoom so they stay a constant
    // on-screen size instead of growing with the image.
    val handleSize = 26.dp.toPx() / scale

    fun drawHandle(p: Offset, degrees: Float) {
      withTransform({
        translate(t.mapX(p.x) - handleSize / 2f, t.mapY(p.y) - handleSize)
        rotate(degrees, pivot = Offset(handleSize / 2f, handleSize))
      }) {
        with(handlePainter) { draw(Size(handleSize, handleSize)) }
      }
    }
    drawHandle(selectionView.startHandle, -135f)
    drawHandle(selectionView.endHandle, 135f)
  }
}
```

---

## 6. Step 6 — ML Kit adapter (`MlKitOcrAdapter.kt`)

Your existing ML Kit call stays; only the **mapping** is new. Two rules that must hold, or the
geometry produces wrong highlights:

1. **Words are in reading order, lines adjacent**: iterate blocks → lines → elements, assigning a
   monotonically increasing `lineIndex` per line.
2. **`imageWidth`/`imageHeight` are the dimensions of the bitmap you passed to ML Kit** — the same
   bitmap displayed behind the overlay.

```kotlin
package com.yourapp.ocrselection

import com.google.mlkit.vision.text.Text

/**
 * Maps ML Kit output to the selection layer. `imageWidth`/`imageHeight` MUST be the dimensions
 * of the bitmap given to the ML Kit recognizer — the same bitmap displayed behind the overlay.
 */
fun mlKitTextToWords(text: Text): List<OcrWord> {
  val words = mutableListOf<OcrWord>()
  var lineIndex = 0
  for (block in text.textBlocks) {
    // Most scripts expose lines; CJK blocks may only expose elements — fall back to treating
    // the block itself as one line.
    val lines = block.lines
    if (lines.isNullOrEmpty()) {
      for (element in block.elements) {
        val box = element.boundingBox ?: continue
        words += OcrWord(
          text = element.text,
          bounds = OrientedRect(
            cx = box.exactCenterX(),
            cy = box.exactCenterY(),
            width = box.width().toFloat(),
            height = box.height().toFloat(),
          ),
          lineIndex = lineIndex,
        )
      }
      lineIndex++
      continue
    }
    for (line in lines) {
      for (element in line.elements) {
        val box = element.boundingBox ?: continue
        words += OcrWord(
          text = element.text,
          bounds = OrientedRect(
            cx = box.exactCenterX(),
            cy = box.exactCenterY(),
            width = box.width().toFloat(),
            height = box.height().toFloat(),
          ),
          lineIndex = lineIndex,
        )
      }
      lineIndex++
    }
  }
  return words
}

/** Line-level boxes for the scan animation. */
fun mlKitTextToRegions(text: Text, imageWidth: Int, imageHeight: Int): DetectedRegions {
  val boxes = mutableListOf<OrientedRect>()
  for (block in text.textBlocks) {
    val lines = block.lines
    if (lines.isNullOrEmpty()) {
      block.boundingBox?.let {
        boxes += OrientedRect(it.exactCenterX(), it.exactCenterY(), it.width().toFloat(), it.height().toFloat())
      }
      continue
    }
    for (line in lines) {
      line.boundingBox?.let {
        boxes += OrientedRect(it.exactCenterX(), it.exactCenterY(), it.width().toFloat(), it.height().toFloat())
      }
    }
  }
  return DetectedRegions(imageWidth, imageHeight, boxes)
}
```

---

## 7. Step 7 — Screen integration (replacing the dimming UI)

The target structure (from CircleToSearch's screen — this parent/child split is required):

```
Box (fills the screen, shows the frozen screenshot bitmap with ContentScale.Fit)
└── Box  ← pointerInput(YOUR gesture)          ← parent: keeps whatever gesture your app needs
    └── WordSelectionOverlay(                  ← child: SEEN FIRST, consumes taps/long-press/handles
          words = words,                       (in Compose, ancestors are always in the hit path
          imageWidth = ocrBitmap.width,         and children see events first; the overlay only
          imageHeight = ocrBitmap.height,       consumes what it claims)
          modifier = Modifier.fillMaxSize(),
          onSelectionChanged = { selected -> ... },   ← e.g. hide your bottom chrome while selecting
          longPressDrag = true,                        ← opt in to long-press + drag
          pendingSelectRegion = ...,                   ← optional (see below)
          onSelectRegionHandled = { ... },
        )
```

Rules:

1. **The screenshot displayed behind the overlay must use `ContentScale.Fit` inside the same
   box**, and `imageWidth`/`imageHeight` must be that bitmap's dimensions. The overlay assumes the
   Fit mapping — if your display crops/scales differently, highlights will be offset.
2. **`scale` parameter**: if your container can zoom/pan, pass the current zoom so handles stay
   constant-sized. Pass `1f` when there's no zoom.
3. **`onSelectionChanged(true)`** → hide your bottom chrome (in CircleToSearch the bottom bar
   hides while text is selected and reappears on clear).
4. **Mutual exclusion with your gestures**: if a user gesture must wipe an active selection, bump
   a `clearSelectionSignal` counter and pass it in — each new value clears the selection.
5. **New screenshot → new words**: `words` is a remember-key, so passing a new list resets the
   selection automatically.

### If your app has a free-hand gesture too (optional, CircleToSearch's trick)

If users can draw on the screen with their finger, a stroke that is **too small or flat** (a
long-press that drifted, a horizontal line) should become a *text selection* instead of a crop.
The pattern, from CircleToSearch's `endCircleDrag`:

1. Measure the stroke's raw bounding box; if height or width < 16dp → degenerate.
2. Map the stroke bounds into image space (via `FitTransform.unmapX/unmapY`, padding ~8dp).
3. Pass that `android.graphics.Rect` as `pendingSelectRegion` — the overlay selects the words
   intersecting it and calls `onSelectRegionHandled()`.
4. Only non-degenerate strokes go down the crop/search path.

---

## 8. Removal checklist (the old custom UI)

Delete only after the new overlay is verified on-device:

- [ ] The full-screen dim view (and any alpha animation tied to it)
- [ ] The custom text-area highlight drawing (ML Kit box painting)
- [ ] The custom selection highlight (paint/brush over selected ranges)
- [ ] The custom action bar (Copy/Share implementation) — the ActionMode bar replaces it
- [ ] Any tap/long-press handlers that fed the old selection state
- [ ] ML Kit → old-highlight data structures

Keep: the ML Kit recognizer call, the frozen-screenshot display, and any preprocessing (rotation,
scaling) that produces the bitmap handed to ML Kit.

---

## 9. Testing checklist

**Geometry unit tests** (port of CircleToSearch's Rust tests — these MUST pass; they encode the
edge cases — and they ARE plain-JUnit runnable: `SelectionView.bounds` is deliberately pure
Kotlin. This exact Kotlin port was verified in the CircleToSearch repo: 10/10 fixtures passed):

```kotlin
// Fixtures: two words per line, lines 12px apart (within the block gap).
private fun word(text: String, cx: Float, cy: Float, width: Float, lineIndex: Int) =
  OcrWord(text, OrientedRect(cx, cy, width, 10f), lineIndex)

private val paragraph = listOf(
  word("hello", 20f, 10f, 40f, 0), word("world", 70f, 10f, 40f, 0),
  word("second", 20f, 22f, 40f, 1), word("line", 70f, 22f, 40f, 1),
)
```

- [ ] `hitTestWord(paragraph, 20f, 10f) == 0`; `(20f, 17f) == 0` (25%-height pad); `(300f, 300f) == null`
- [ ] `nearestWord` ignores the other axis when restricted
- [ ] `resolveSelection(paragraph, 0, 2)` == `resolveSelection(paragraph, 2, 0)` (order-independent)
- [ ] Close lines join with a space; lines > 1.5× height apart join with `\n`
- [ ] Latin inside CJK keeps spaces; CJK+CJK punctuation stays tight
- [ ] One pill per line spanning its words; pill height clamped to the neighbouring line gap;
      a parallel column does not clamp the pill
- [ ] Handles sit at the outer bottom corners of the first/last word

**On-device tests**:

- [ ] Tap a word → pill highlight + handles + bar
- [ ] Long-press a word → haptic + anchor; drag → extends word-by-word with loupe; release → bar
- [ ] Drag handles → selection resizes, bar follows
- [ ] Tap empty space → clears; `onSelectionChanged(false)` fires
- [ ] Copy → clipboard matches the highlighted lines (check multi-line newlines and CJK spacing)
- [ ] Select all → everything highlights; the Select-all button disappears (invalidate() path)
- [ ] Rotated/zoomed container (if applicable) → highlights track the image
- [ ] Low-end device (Android 10): no jank from the overlay (it's a single Canvas, no allocations
      per frame outside `resolveSelection` on selection *changes*)

---

## 10. Gotchas

1. **`invalidateContentRect()` ≠ `invalidate()`** — the former only re-queries the bar position
   and never refreshes menu visibility. This exact bug shipped once in CircleToSearch; use
   `invalidate()` (as the listing above already does).
2. **`Magnifier` is API 28+** — guarded; below P the loupe is silently skipped. CircleToSearch's
   minSdk is 29; if yours is lower, nothing breaks.
3. **Reading-order contract** — the geometry groups *consecutive runs* of equal `lineIndex`. If
   your ML Kit iteration is out of order (or two lines interleave), pills and copy text merge
   wrongly. Sort blocks/lines by `boundingBox.top` (then left) before mapping if needed.
4. **Don't reuse the OCR bitmap's dimensions for the displayed image** — if you display a
   *different* bitmap than the one OCR ran on (scaled crop), either re-run the mapping or
   re-run OCR on the displayed bitmap.
5. **The overlay draws nothing when `words` is empty** — safe to mount before OCR completes
   (that's what the scan animation is for).
6. **`ActionMode.TYPE_FLOATING`** positions itself from `onGetContentRect` — if the bar appears
   at the screen corner, the `contentRect` mapping through `localToRoot` is failing; check that
   `layoutCoords` belongs to the same node the gesture reads `size` from.
