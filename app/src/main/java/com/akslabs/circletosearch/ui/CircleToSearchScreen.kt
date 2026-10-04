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

package com.akslabs.circletosearch.ui

import android.graphics.Bitmap
import android.graphics.Rect
import com.akslabs.circletosearch.CircleToSearchAccessibilityService
import android.util.Base64
import android.view.Display
import android.view.ViewGroup
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.activity.compose.BackHandler
import androidx.activity.compose.PredictiveBackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.BorderOuter
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.DesktopWindows
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Fullscreen
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.TextFields
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material.icons.filled.Upload
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Link
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material.icons.filled.Email
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateList
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import com.akslabs.circletosearch.R
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.zIndex
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.webkit.WebSettingsCompat
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.ui.components.FriendlyMessageBubble
import com.akslabs.circletosearch.ui.components.searchWithGoogleLens
import com.akslabs.circletosearch.ui.theme.OverlayGradientColors
import com.akslabs.circletosearch.utils.FriendlyMessageManager
import com.akslabs.circletosearch.utils.ImageSearchUploader
import com.akslabs.circletosearch.utils.ImageUtils
import com.akslabs.circletosearch.utils.QrResult
import com.akslabs.circletosearch.utils.QrResultWithBounds
import com.akslabs.circletosearch.utils.QrScanner
import com.akslabs.circletosearch.ui.qrResultShortLabel
import com.akslabs.circletosearch.utils.UIPreferences
import com.akslabs.circletosearch.ui.components.MoreAppsBottomSheet
import com.akslabs.circletosearch.ui.components.DonateBottomSheet
import dev.davidv.translator.TranslatorApplication
import dev.davidv.translator.ui.components.DetectedRegions
import dev.davidv.translator.ui.components.ImageWordSelection
import dev.davidv.translator.ui.components.ScanAnimationOverlay
import dev.davidv.translator.ui.components.WordSelectionOverlay
import android.net.Uri
import kotlinx.coroutines.delay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import androidx.webkit.WebViewFeature
import androidx.webkit.UserAgentMetadata
import com.akslabs.circletosearch.data.isDirectUpload
import kotlin.math.max
import kotlin.math.min
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import android.os.Build
import androidx.compose.material.icons.filled.Apps
import androidx.compose.material.icons.filled.PushPin
import androidx.compose.material3.Surface

/** Lets the Circle to Search UI finish closing before the translator overlay draws. */
private const val TRANSLATE_TRIGGER_DELAY_MS = 400L

/** Lets the Circle to Search chrome hide before the translator overlay draws. */
private const val TRANSLATE_HIDE_DELAY_MS = 150L

/**
 * Runs the translator module's OCR over [bmp] and returns its recognized source words —
 * no translation, no overlay bitmap. [onRegions] fires as soon as detection boxes arrive
 * (for the scan animation). Returns null on failure: missing OCR models and no-text both
 * show a toast.
 */
private suspend fun runInstantScreenshotOcr(
    context: android.content.Context,
    bmp: Bitmap,
    onRegions: (DetectedRegions) -> Unit,
): ImageWordSelection? {
    val app = context.applicationContext as? TranslatorApplication ?: return null
    val coordinator = runCatching { app.translationCoordinator }.getOrNull() ?: return null
    val target = CircleToSearchAccessibilityService.instance?.defaultTargetLanguage() ?: return null
    var ocrMissing = false
    var sourceDetectFailed = false
    val result = coordinator.ocrWordsOnly(
        bitmap = bmp,
        to = target,
        onDetectedRegions = { boxes, w, h -> onRegions(DetectedRegions(w, h, boxes)) },
        onOcrUnavailable = { ocrMissing = true },
        onAutoSourceFailed = { sourceDetectFailed = true },
    )
    if (result == null) {
        val messageId =
            when {
                ocrMissing -> dev.davidv.translator.R.string.ocr_models_missing
                sourceDetectFailed -> R.string.label_language_detect_failed
                else -> R.string.label_no_text_found
            }
        withContext(Dispatchers.Main) {
            android.widget.Toast.makeText(
                context,
                context.getString(messageId),
                android.widget.Toast.LENGTH_SHORT,
            ).show()
        }
        return null
    }
    return ImageWordSelection(
        imageWidth = result.imageWidth,
        imageHeight = result.imageHeight,
        sourceWords = result.sourceWords,
        translatedWords = result.sourceWords,
    )
}

/**
 * Fit (contain) mapping for a bitmap centered in a view — the app-side mirror of the
 * module's internal fitTransform, which the word/scan overlays use. Unit-agnostic: pass
 * view/image sizes in px (crop path) or dp (chip path) consistently for both.
 */
private class FitMapping(val scale: Float, val offsetX: Float, val offsetY: Float) {
    fun unmapX(x: Float) = (x - offsetX) / scale
    fun unmapY(y: Float) = (y - offsetY) / scale
}

private fun fitMapping(viewW: Float, viewH: Float, imageW: Float, imageH: Float): FitMapping {
    val scale = min(viewW / imageW, viewH / imageH)
    return FitMapping(scale, (viewW - imageW * scale) / 2f, (viewH - imageH * scale) / 2f)
}

/**
 * Maps a screen-space rect into bitmap space for the ContentScale.Fit display, clamping to
 * the bitmap. For screen captures (bitmap size == view size) this is the identity.
 */
private fun fitCrop(
    rect: android.graphics.Rect,
    t: FitMapping,
    imageW: Int,
    imageH: Int,
): android.graphics.Rect =
    android.graphics.Rect(
        t.unmapX(rect.left.toFloat()).toInt().coerceIn(0, imageW),
        t.unmapY(rect.top.toFloat()).toInt().coerceIn(0, imageH),
        t.unmapX(rect.right.toFloat()).toInt().coerceIn(0, imageW),
        t.unmapY(rect.bottom.toFloat()).toInt().coerceIn(0, imageH),
    )

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CircleToSearchScreen(
    capturedScreenshot: Bitmap?,
    onClose: () -> Unit,
    searchModeOverride: Boolean? = null,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    // Upload flow: the user can swap the canvas for a gallery image and run Circle to
    // Search on it. Keyed on the capture param so a fresh capture supersedes a pick.
    var uploadedImage by remember(capturedScreenshot) { mutableStateOf<Bitmap?>(null) }
    val screenshot = uploadedImage ?: capturedScreenshot

    // Pixel size of the canvas layer — the image-fit mapping (crop-to-search, chips) needs it.
    var canvasSizePx by remember { mutableStateOf(IntSize.Zero) }

    val pickImageLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.PickVisualMedia()
    ) { uri ->
        if (uri != null) {
            // Decode at original proportions (EXIF-rotated, only sampled past a 12 MP budget).
            // The display fits the bitmap to the screen, so nothing is cropped or stretched.
            val maxPixels = 12 * 1000 * 1000
            scope.launch(kotlinx.coroutines.Dispatchers.IO) {
                val picked = ImageUtils.loadImageUri(context, uri, maxPixels)
                if (picked != null) {
                    uploadedImage = picked
                } else {
                    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                        android.widget.Toast.makeText(
                            context,
                            context.getString(R.string.toast_image_load_failed),
                            android.widget.Toast.LENGTH_SHORT,
                        ).show()
                    }
                }
            }
        }
    }
    
    // Initialize preferences
    val uiPreferences = remember { UIPreferences(context) }
    
    // Reactive search method state — observed via flow so it is always in sync
    // with whatever set it (home screen, settings sheet, or accessibility actions).
    // Reading from the flow avoids the SharedPreferences in-memory cache lag
    // that occurs when a write comes from a different activity instance.
    val isGoogleLensOnly by uiPreferences.observeUseGoogleLensOnly()
        .collectAsState(initial = uiPreferences.isUseGoogleLensOnly())
    
    // Effective state: use override if provided by trigger, else use global preference
    val effectiveLensOnly = searchModeOverride ?: isGoogleLensOnly
    
    
    // New Sheet States
    var showMoreAppsSheet by remember { mutableStateOf(false) }
    var showDonateSheet by remember { mutableStateOf(false) }

    // Material You logic for colors
    val isDark = isSystemInDarkTheme()
    val dynamicColor = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    val colorScheme = when {
        dynamicColor && isDark -> dynamicDarkColorScheme(context)
        dynamicColor && !isDark -> dynamicLightColorScheme(context)
        else -> MaterialTheme.colorScheme // Fallback standard
    }
    //Colors for dark theme
    val barBgColor = if (isDark) Color(0xFF0D0D0D) else colorScheme.surface
    val bubbleColor = if (isDark) Color(0xFF1F1F1F) else colorScheme.secondaryContainer.copy(alpha = 0.9f)
    val contentColor = if (isDark) Color.White else colorScheme.onSecondaryContainer

    //Haptic feedback
    val haptic = androidx.compose.ui.platform.LocalHapticFeedback.current

    //Slide-in animation
    var isUIVisible by remember { mutableStateOf(false) }
    LaunchedEffect(Unit) {
        isUIVisible = true // Déclenche l'animation à l'ouverture
    }

    // Search Engines Order Logic
    val preferredOrder = remember(uiPreferences.getSearchEngineOrder()) {
        val allEngines = SearchEngine.values()
        val orderString = uiPreferences.getSearchEngineOrder()
        if (orderString == null) allEngines
        else {
            val preferredNames = orderString.split(",")
            val ordered = mutableListOf<SearchEngine>()
            preferredNames.forEach { name ->
                allEngines.find { it.name == name }?.let { ordered.add(it) }
            }
            allEngines.forEach { if (!ordered.contains(it)) ordered.add(it) }
            ordered
        }
    }
    val searchEngines = preferredOrder
    
    // Support Settings Sheet
    var showSettingsScreen by remember { mutableStateOf(false) }

    // Friendly Message State
    var friendlyMessage by remember { mutableStateOf("") }
    var isMessageVisible by remember { mutableStateOf(false) }
    
    // Resizing state
    var isResizing by remember { mutableStateOf(false) }
    var activeHandle by remember { mutableStateOf<String?>(null) } // "tl", "tr", "bl", "br"
    
    // QR Scanner state
    var showQrSheet by remember { mutableStateOf(false) }
    var qrScanBitmap by remember { mutableStateOf<android.graphics.Bitmap?>(null) }
    var detectedQrCodes by remember { mutableStateOf<List<QrResultWithBounds>>(emptyList()) }
    var selectedQrResult by remember { mutableStateOf<QrResultWithBounds?>(null) }
    
    // Phase 44: Smart Entity Extractor
    var isEntityExtractMode by remember { mutableStateOf(false) }
    var detectedEntities by remember { mutableStateOf<List<SmartEntity>>(emptyList()) }
    var isExtractingEntities by remember { mutableStateOf(false) }
    // Bumped whenever the canvas image changes so an in-flight SmartScan can't deliver
    // entities extracted for the previous image.
    var entityScanGeneration by remember { mutableStateOf(0) }
    
    LaunchedEffect(Unit) {
        if (uiPreferences.isShowFriendlyMessages()) {
            val manager = FriendlyMessageManager(context)
            friendlyMessage = manager.getNextMessage()
            delay(500) // Small delay for smooth entrance
            isMessageVisible = true
            delay(4000) // Show for 4 seconds
            isMessageVisible = false
        }
    }

    // Search State
    var selectedEngine by remember(searchEngines) { mutableStateOf<SearchEngine>(searchEngines.first()) }
    var searchUrl by remember { mutableStateOf<String?>(null) }
    var hostedImageUrl by remember { mutableStateOf<String?>(null) }
    var pageBackgroundColor by remember { mutableStateOf(Color.White) }
    var refreshingEngine by remember { mutableStateOf<SearchEngine?>(null) }
    var isLoading by remember { mutableStateOf(false) }
    
    // Desktop Mode - Per Tab
    // We use a set to track which engines are in desktop mode
    val initialDesktopMode = uiPreferences.isDesktopMode() // Global default
    var desktopModeEngines by remember { mutableStateOf<Set<SearchEngine>>(if(initialDesktopMode) searchEngines.toSet() else emptySet()) }
    
    var isDarkMode by remember { mutableStateOf(uiPreferences.isDarkMode()) }
    var showGradientBorder by remember { mutableStateOf(uiPreferences.isShowGradientBorder()) }
    
    // Back gesture handler for QR sheet
    // We use a high-priority check or ensure it's at the very top of the hierarchy
    BackHandler(enabled = showQrSheet) {
        showQrSheet = false
        selectedQrResult = null
    }

    // Track initialized engines for Smart Loading
    val initializedEngines = remember { mutableStateListOf<SearchEngine>() }

    // Keep the tab strip background equal to the webpage's own background so
    // the sheet never shows a Material color that clashes with the page.
    fun samplePageBackgroundColor(view: WebView?) {
        if (view == null) return
        val js = "(function(){var b=getComputedStyle(document.body).backgroundColor;" +
            "if(b==='rgba(0, 0, 0, 0)'||b==='transparent'){b=getComputedStyle(document.documentElement).backgroundColor;}" +
            "return b;})()"
        val parse: (String) -> Unit = { raw ->
            val m = Regex("""rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*([\d.]+))?\s*\)""").find(raw.trim('"'))
            if (m != null) {
                val a = m.groupValues[4].toFloatOrNull() ?: 1f
                if (a > 0f) {
                    pageBackgroundColor = Color(
                        red = m.groupValues[1].toInt() / 255f,
                        green = m.groupValues[2].toInt() / 255f,
                        blue = m.groupValues[3].toInt() / 255f
                    )
                }
            }
        }
        try {
            view.evaluateJavascript(js) { raw -> parse(raw ?: "") }
            view.postDelayed({
                try { view.evaluateJavascript(js) { raw -> parse(raw ?: "") } } catch (_: Exception) {}
            }, 800L)
        } catch (_: Exception) {}
    }
    
    // Save global preference if user toggles it for the MAIN engine (Google) - Optional choice, 
    // or we just keep it per session. Let's keep it simple: no auto-save of per-tab state to verify complex persistence yet.
    // simpler: If user toggles, we just update the state.
    
    // Helper to check desktop mode
    fun isDesktop(engine: SearchEngine) = desktopModeEngines.contains(engine)
    
    // Removed auto-save of isDesktopMode for now as it is complex with per-tab
    // We could save "If ALL are desktop" or just the active one? 
    // User requested "depending on opened tab", so per-session state is safer.
    
    LaunchedEffect(isDarkMode) {
        uiPreferences.setDarkMode(isDarkMode)
    }
    
    LaunchedEffect(showGradientBorder) {
        uiPreferences.setShowGradientBorder(showGradientBorder)
    }
    
    // Cache for preloaded URLs to avoid re-uploading/re-generating
    val preloadedUrls = remember { mutableMapOf<SearchEngine, String>() }
    
    // WebView Cache
    val webViews = remember { mutableMapOf<SearchEngine, WebView>() }

    // Clears the pull-to-refresh spinner once the refreshed tab finishes loading.
    fun clearRefreshIfDone(view: WebView?) {
        val re = refreshingEngine
        if (view != null && re != null && view === webViews[re]) refreshingEngine = null
    }
    
    // Update User Agent dynamically when desktop mode changes for a specific engine
    // This is now handled in the AndroidView update block or individual engine effects
    // BUT we need to force reload if the state changes.
    LaunchedEffect(desktopModeEngines) {
        webViews.forEach { (engine, wv) ->
             val isDesktop = desktopModeEngines.contains(engine)
             val newUserAgent = if (isDesktop) {
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
            } else {
                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
            }

            if (wv.settings.userAgentString != newUserAgent) {
                wv.settings.userAgentString = newUserAgent
                wv.reload()
                android.util.Log.d("CircleToSearch", "Desktop mode changed for $engine - Reloaded")
            }
        }
    }
    
    // Update WebViews when dark mode changes
    LaunchedEffect(isDarkMode) {
        webViews.values.forEach { wv ->
            try {
                // Update WebViewClient for dark mode
                if (isDarkMode) {
                    wv.webViewClient = object : android.webkit.WebViewClient() {
                        override fun onPageFinished(view: android.webkit.WebView?, url: String?) {
                            super.onPageFinished(view, url)
                            samplePageBackgroundColor(view)
                            clearRefreshIfDone(view)
                            val darkModeCSS = """
                                (function() {
                                    var style = document.createElement('style');
                                    style.innerHTML = `
                                        html { filter: invert(1) hue-rotate(180deg) !important; background: #000 !important; }
                                        img, video, [style*="background-image"] { filter: invert(1) hue-rotate(180deg) !important; }
                                    `;
                                    document.head.appendChild(style);
                                })()
                            """.trimIndent()
                            view?.evaluateJavascript(darkModeCSS, null)
                        }
                    }
                } else {
                    wv.webViewClient = object : android.webkit.WebViewClient() {
                        override fun onPageFinished(view: android.webkit.WebView?, url: String?) {
                            super.onPageFinished(view, url)
                            samplePageBackgroundColor(view)
                            clearRefreshIfDone(view)
                        }
                    }
                }
                wv.reload()
                android.util.Log.d("CircleToSearch", "Dark mode changed to: $isDarkMode - Reloaded WebViews")
            } catch (e: Exception) {
                android.util.Log.e("CircleToSearch", "Error updating dark mode", e)
            }
        }
    }
    
    // Bottom Sheet State
    val scaffoldState = androidx.compose.material3.rememberBottomSheetScaffoldState(
        bottomSheetState = androidx.compose.material3.rememberStandardBottomSheetState(
            initialValue = androidx.compose.material3.SheetValue.Hidden,
            skipHiddenState = false
        )
    )

    // Drawing State
    val currentPathPoints = remember { mutableStateListOf<Offset>() }
    
    // Selection State
    var selectedBitmap by remember { mutableStateOf<Bitmap?>(null) }
    var isSearching by remember { mutableStateOf(false) }
    var selectionRect by remember { mutableStateOf<Rect?>(null) }
    val selectionAnim = remember { androidx.compose.animation.core.Animatable(0f) }

    // Translator overlay mode: Circle to Search chrome hidden, the frozen screenshot
    // stays visible as the backdrop under the module's scan/result overlays.
    var isTranslatorMode by remember { mutableStateOf(false) }

    // Instant OCR over the screenshot: scan animation boxes while recognizing, then the
    // recognized source words — tap a word to select and copy (original text, no translation).
    var instantWords by remember { mutableStateOf<ImageWordSelection?>(null) }
    var instantRegions by remember { mutableStateOf<DetectedRegions?>(null) }
    // OCR finished with no words (plan failed / nothing recognized): stop the scan
    // animation instead of breathing over the image forever.
    var instantOcrDone by remember { mutableStateOf(false) }
    // A word selection in the instant-OCR overlay hides the bottom bar; tapping outside the
    // text clears the selection and the bar comes back.
    var wordSelected by remember { mutableStateOf(false) }
    // Bumped to force the word overlay to drop its selection — text selection and a drawn
    // circle are mutually exclusive, so completing a circle clears any active selection.
    var wordSelectionClearSignal by remember { mutableStateOf(0) }

    // Circle-draw gesture handlers, shared by the drag-detector wrapper above the drawing
    // layer (see "Circle-draw gestures" below). Plain drags draw; handle grabs and taps are
    // claimed by the word overlay (child, seen first) so text selection keeps priority.
    fun beginCircleDrag(offset: Offset) {
        val rect = selectionRect
        if (rect != null && selectionAnim.value == 1f) {
            val handleSize = 64f // px for hit testing
            val tl = Offset(rect.left.toFloat(), rect.top.toFloat())
            val tr = Offset(rect.right.toFloat(), rect.top.toFloat())
            val bl = Offset(rect.left.toFloat(), rect.bottom.toFloat())
            val br = Offset(rect.right.toFloat(), rect.bottom.toFloat())

            when {
                (offset - tl).getDistance() < handleSize -> { isResizing = true; activeHandle = "tl" }
                (offset - tr).getDistance() < handleSize -> { isResizing = true; activeHandle = "tr" }
                (offset - bl).getDistance() < handleSize -> { isResizing = true; activeHandle = "bl" }
                (offset - br).getDistance() < handleSize -> { isResizing = true; activeHandle = "br" }
                else -> { isResizing = false; activeHandle = null }
            }

            if (isResizing) return
        }

        // Clear previous state if starting new draw
        currentPathPoints.clear()
        currentPathPoints.add(offset)
        selectionRect = null
        scope.launch { selectionAnim.snapTo(0f) }
        // Text selection and drawing are exclusive from the first moment of the gesture:
        // drop any active selection as soon as the user starts drawing, not when the
        // circle completes.
        if (wordSelected) wordSelectionClearSignal++
    }

    fun moveCircleDrag(change: PointerInputChange) {
        if (isResizing && activeHandle != null) {
            val rect = selectionRect ?: return
            val pos = change.position
            val newRect = android.graphics.Rect(rect)
            when (activeHandle) {
                "tl" -> { newRect.left = pos.x.toInt(); newRect.top = pos.y.toInt() }
                "tr" -> { newRect.right = pos.x.toInt(); newRect.top = pos.y.toInt() }
                "bl" -> { newRect.left = pos.x.toInt(); newRect.bottom = pos.y.toInt() }
                "br" -> { newRect.right = pos.x.toInt(); newRect.bottom = pos.y.toInt() }
            }
            // Basic validation (min size)
            if (newRect.width() > 20 && newRect.height() > 20) {
                selectionRect = newRect
            }
        } else {
            currentPathPoints.add(change.position)
        }
    }

    fun endCircleDrag() {
        val bmp = screenshot
        // View size in px — with no bitmap the old behavior (degenerate rect) is kept.
        val viewW = bmp?.let { if (canvasSizePx.width > 0) canvasSizePx.width else it.width } ?: 0
        val viewH = bmp?.let { if (canvasSizePx.height > 0) canvasSizePx.height else it.height } ?: 0
        if (isResizing) {
            isResizing = false
            activeHandle = null
            // Update cropped bitmap after resize. selectionRect is screen-space; map it into
            // image space first (the Fit display may letterbox/scale the bitmap).
            if (bmp != null && selectionRect != null) {
                val t = fitMapping(viewW.toFloat(), viewH.toFloat(), bmp.width.toFloat(), bmp.height.toFloat())
                selectedBitmap = ImageUtils.cropBitmap(bmp, fitCrop(selectionRect!!, t, bmp.width, bmp.height))
            }
        } else if (currentPathPoints.isNotEmpty()) {
            var minX = Float.MAX_VALUE
            var minY = Float.MAX_VALUE
            var maxX = Float.MIN_VALUE
            var maxY = Float.MIN_VALUE
            currentPathPoints.forEach { p ->
                minX = kotlin.math.min(minX, p.x)
                minY = kotlin.math.min(minY, p.y)
                maxX = kotlin.math.max(maxX, p.x)
                maxY = kotlin.math.max(maxY, p.y)
            }

            val border = 20
            val rect = android.graphics.Rect(
                (minX - border).toInt().coerceAtLeast(0),
                (minY - border).toInt().coerceAtLeast(0),
                (maxX + border).toInt().coerceAtMost(viewW),
                (maxY + border).toInt().coerceAtMost(viewH)
            )

            if (rect.width() > 10 && rect.height() > 10) {
                selectionRect = rect
                currentPathPoints.clear() // Hide the drawn circle
                if (bmp != null) {
                    val t = fitMapping(viewW.toFloat(), viewH.toFloat(), bmp.width.toFloat(), bmp.height.toFloat())
                    selectedBitmap = ImageUtils.cropBitmap(bmp, fitCrop(rect, t, bmp.width, bmp.height))
                }
                scope.launch {
                    selectionAnim.animateTo(1f, tween(600))
                    isSearching = true
                }
            }
        }
    }

    // Lifecycle reset: When screenshot changes, reset selection and modes
    LaunchedEffect(screenshot) {
        if (screenshot != null) {
            selectionRect = null
            selectedBitmap = null
            isSearching = false
            currentPathPoints.clear()
            selectionAnim.snapTo(0f)
            // The canvas image changed: drop entities extracted for the previous image and
            // invalidate any extraction still in flight.
            detectedEntities = emptyList()
            isExtractingEntities = false
            entityScanGeneration = entityScanGeneration + 1
            if (isTranslatorMode) {
                CircleToSearchAccessibilityService.instance?.closeTranslateOverlay()
            }
        }
    }
    
    
    // searchEngines moved to top
    // val searchEngines = SearchEngine.values()

    // Gradient Animation
    val alphaAnim by animateFloatAsState(
        targetValue = if (screenshot != null) 1f else 0f,
        animationSpec = tween(1000), label = "alpha"
    )

    // Auto-scan entire screenshot for QR codes when it arrives
    LaunchedEffect(screenshot) {
        android.util.Log.d("CircleToSearch", "Auto-scan Effect: screenshot=${screenshot != null}")
        if (screenshot != null) {
            val found = withContext(Dispatchers.Default) {
                android.util.Log.d("CircleToSearch", "Auto-scan starting...")
                val res = QrScanner.scanBitmapAll(screenshot!!)
                android.util.Log.d("CircleToSearch", "Auto-scan finished: found ${res.size} codes")
                res
            }
            detectedQrCodes = found
        } else {
            detectedQrCodes = emptyList()
        }
    }

    // Instant OCR: recognize the screenshot's words as soon as it arrives (module
    // pipeline, no translation) — shown as tappable words over the frozen capture.
    LaunchedEffect(screenshot) {
        instantWords = null
        instantRegions = null
        instantOcrDone = false
        wordSelected = false
        val bmp = screenshot ?: return@LaunchedEffect
        instantWords = runInstantScreenshotOcr(context, bmp) { instantRegions = it }
        instantOcrDone = true
    }

    // The word overlay unmounts in these modes (dropping its selection) - drop ours too so
    // the bottom bar doesn't stay hidden after leaving them.
    LaunchedEffect(isEntityExtractMode, isTranslatorMode) {
        if (isEntityExtractMode || isTranslatorMode) wordSelected = false
    }

    // Helper to create and configure WebView
    fun createWebView(ctx: android.content.Context, engine: SearchEngine): WebView {
        return WebView(ctx).apply {
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            
            // Caching & Performance - must be set on WebView directly
            setLayerType(android.view.View.LAYER_TYPE_HARDWARE, null)
            
            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                databaseEnabled = true
                allowFileAccess = true
                allowContentAccess = true
                allowFileAccessFromFileURLs = true
                allowUniversalAccessFromFileURLs = true
                mixedContentMode = WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
                
                // Performance & UI
                setRenderPriority(WebSettings.RenderPriority.HIGH)
                
                // Caching for Speed
                cacheMode = WebSettings.LOAD_DEFAULT // Was LOAD_CACHE_ELSE_NETWORK - caused refresh issues
                
                // Zoom support
                setSupportZoom(true)
                builtInZoomControls = true
                displayZoomControls = false
                
                useWideViewPort = true
                loadWithOverviewMode = true
                
                userAgentString = if (isDesktop(engine)) {
                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                } else {
                    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
                }

                // Dark Mode - Use CSS injection for universal compatibility
                // Algorithmic darkening alone doesn't work on many websites
                android.util.Log.d("CircleToSearch", "Dark mode enabled: $isDarkMode")
            }

            // UI Tweaks
            isVerticalScrollBarEnabled = false
            isHorizontalScrollBarEnabled = false

            // Enable Third-Party Cookies
            android.webkit.CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)

            // Instrument fetch/XHR so the Lens results RPC (and its status) is
            // visible even when it fails silently. Injected at page start AND
            // finish (start can race the document commit); per-document guard.
            fun installNetDiag(view: WebView) {
                val netJs = """
                    (function() {
                        if (window.__netDiag) return 'already';
                        window.__netDiag = 1;
                        try {
                            var of = window.fetch;
                            window.fetch = function(u, init) {
                                var m = (init && init.method) || 'GET';
                                var url = (typeof u === 'string') ? u : ((u && u.url) || '');
                                console.warn('NET fetch ' + m + ' ' + url.slice(0, 200));
                                return of.apply(this, arguments).then(function(r) {
                                    console.warn('NET fetchdone ' + r.status + ' ' + m + ' ' + url.slice(0, 200));
                                    if (/Validate|qfmetadata|sgasync|httpservice|batchexecute|folsrch|bgasy|\/search\?/.test(url)) {
                                        try {
                                            console.warn('NETBODY_CLONE ' + url.split('?')[0].slice(-70));
                                            r.clone().text().then(function(t) {
                                                var i = t.search(/not associated with your account|No image at the URL|Image not found|Something went wrong/);
                                                if (i >= 0) {
                                                    console.warn('NETBODY_HIT @' + i + '/' + t.length + ' >> ' + t.slice(Math.max(0, i - 150), i + 250));
                                                } else {
                                                    console.warn('NETBODY ' + url.split('?')[0].slice(-70) + ' len=' + t.length + ' head=' + t.slice(0, 200));
                                                }
                                            }).catch(function(e) { console.warn('NETBODY_ERR ' + e); });
                                        } catch (e) { console.warn('NETBODY_ERR ' + e); }
                                    }
                                    return r;
                                }, function(e) {
                                    console.warn('NET fetcherr ' + m + ' ' + url.slice(0, 200) + ' ' + e);
                                    throw e;
                                });
                            };
                            var ox = XMLHttpRequest.prototype.open;
                            var os = XMLHttpRequest.prototype.send;
                            XMLHttpRequest.prototype.open = function(m, u) { this.__dm = m; this.__du = u; return ox.apply(this, arguments); };
                            XMLHttpRequest.prototype.send = function() {
                                var s = this, m = this.__dm, u = this.__du;
                                console.warn('NET xhr ' + m + ' ' + String(u).slice(0, 200));
                                s.addEventListener('loadend', function() {
                                    console.warn('NET xhrdone ' + s.status + ' ' + m + ' ' + String(u).slice(0, 200));
                                    try {
                                        if (/Validate|qfmetadata|sgasync|httpservice|batchexecute|folsrch|bgasy/.test(String(u)) && s.responseText) {
                                            console.warn('NETBODY_XHR ' + String(u).split('?')[0].slice(-70) + ' >> ' + String(s.responseText).slice(0, 400));
                                        }
                                    } catch (e) {}
                                });
                                return os.apply(this, arguments);
                            };
                            return 'installed';
                        } catch (e) { return 'NETDIAG_ERR ' + e; }
                    })()
                """.trimIndent()
                view.evaluateJavascript(netJs) { r ->
                    if (r != null && r != "\"already\"") {
                        android.util.Log.d("LensDiag", "NETDIAG $r")
                    }
                }
            }
            
            webViewClient = object : WebViewClient() {
                
                override fun onPageStarted(view: WebView?, url: String?, favicon: android.graphics.Bitmap?) {
                    super.onPageStarted(view, url, favicon)
                    if (url != null && url.contains("google.") && view != null) {
                        android.util.Log.d("LensDiag", "NAV $url")
                        installNetDiag(view)
                    }
                }
                
                override fun onReceivedHttpError(
                    view: WebView?,
                    request: android.webkit.WebResourceRequest?,
                    response: android.webkit.WebResourceResponse?
                ) {
                    val host = request?.url?.host ?: ""
                    if (response != null && response.statusCode in 400..599 &&
                        (host.contains("google") || host.contains("imgdb") || host.contains("catbox"))
                    ) {
                        android.util.Log.e("LensDiag", "HTTP ${response.statusCode} ${response.reasonPhrase} ${request?.url}")
                    }
                }
                
                override fun onPageFinished(view: WebView?, url: String?) {
                    super.onPageFinished(view, url)
                    samplePageBackgroundColor(view)
                    clearRefreshIfDone(view)
                    // Lens loads its result tabs via async RPCs after page finish.
                    // Snapshot the network + DOM state so silent failures are visible.
                    if (url != null && url.contains("google.") && view != null) {
                        val v = view
                        installNetDiag(v)
                        v.postDelayed({
                            try {
                                val resJs = """
                                    (function() {
                                        try {
                                            var noise = ['xjs', '/log', 'gen_204', 'client_204', 'folsrch', 'sprites', '.css', '.woff', '.ttf'];
                                            var r = performance.getEntriesByType('resource')
                                                .filter(function(e) {
                                                    var n = e.name;
                                                    var host = n.indexOf('google') >= 0 || n.indexOf('gstatic') >= 0;
                                                    var bad = noise.some(function(s) { return n.indexOf(s) >= 0; });
                                                    return host && !bad;
                                                })
                                                .slice(-40)
                                                .map(function(e) { return (e.responseStatus || '?') + ' ' + e.name.split('?')[0].slice(0, 110); })
                                                .join(String.fromCharCode(10));
                                            return r || 'NO_GOOGLE_RESOURCES';
                                        } catch (err) { return 'SNAP_ERR ' + err; }
                                    })()
                                """.trimIndent()
                                v.evaluateJavascript(resJs) { r ->
                                    android.util.Log.d("LensDiag", "GOOGLE_RES $r")
                                }
                                val envJs = """
                                    (function() {
                                        try {
                                            var d = {
                                                ua: (navigator.userAgent || '').slice(0, 110),
                                                brands: navigator.userAgentData ? navigator.userAgentData.brands : 'NO_UADATA',
                                                mobile: navigator.userAgentData ? navigator.userAgentData.mobile : '?',
                                                platform: navigator.userAgentData ? navigator.userAgentData.platform : '?',
                                                chrome: typeof window.chrome === 'undefined' ? 'UNDEFINED' : Object.keys(window.chrome).join(','),
                                                webdriver: navigator.webdriver,
                                                cookies: (document.cookie || '').slice(0, 300)
                                            };
                                            return JSON.stringify(d);
                                        } catch (err) { return 'ENV_ERR ' + err; }
                                    })()
                                """.trimIndent()
                                v.evaluateJavascript(envJs) { r ->
                                    android.util.Log.d("LensDiag", "GOOGLE_ENV $r")
                                }
                                val bodyJs = """
                                    (function() {
                                        try {
                                            var txt = (document.body.innerText || '').replace(/\s+/g, ' ').slice(0, 450);
                                            return location.href.slice(0, 250) + ' >> ' + document.title + ' >> ' + txt;
                                        } catch (err) { return 'SNAP_ERR ' + err; }
                                    })()
                                """.trimIndent()
                                v.postDelayed({
                                    try {
                                        v.evaluateJavascript(bodyJs) { r ->
                                            android.util.Log.d("LensDiag", "GOOGLE_BODY $r")
                                        }
                                        android.util.Log.d("LensDiag", "GOOGLE_COOKIES ${android.webkit.CookieManager.getInstance().getCookie("https://lens.google.com")}")
                                    } catch (e: Exception) { android.util.Log.e("LensDiag", "snap failed", e) }
                                }, 3000)
                            } catch (e: Exception) {
                                android.util.Log.e("LensDiag", "snapshot failed", e)
                            }
                        }, 4000)
                    }
                    // Inject dark mode CSS if enabled
                    if (isDarkMode) {
                        val darkModeCSS = """
                            (function() {
                                var style = document.createElement('style');
                                style.innerHTML = `
                                    html { filter: invert(1) hue-rotate(180deg) !important; background: #000 !important; }
                                    img, video, [style*="background-image"] { filter: invert(1) hue-rotate(180deg) !important; }
                                `;
                                document.head.appendChild(style);
                            })()
                        """.trimIndent()
                        view?.evaluateJavascript(darkModeCSS, null)
                        android.util.Log.d("CircleToSearch", "Dark mode CSS injected for: $url")
                    }
                }
            }
            webChromeClient = object : android.webkit.WebChromeClient() {
                override fun onConsoleMessage(consoleMessage: android.webkit.ConsoleMessage?): Boolean {
                    val msg = consoleMessage?.message() ?: return super.onConsoleMessage(consoleMessage)
                    val isNet = msg.contains("NET ") || msg.contains("NETBODY")
                    val isGoogleSrc = consoleMessage.sourceId()?.contains("google") == true
                    when {
                        consoleMessage.messageLevel() == android.webkit.ConsoleMessage.MessageLevel.ERROR ->
                            android.util.Log.e("LensDiag", "JS $msg @${consoleMessage.sourceId()}:${consoleMessage.lineNumber()}")
                        consoleMessage.messageLevel() == android.webkit.ConsoleMessage.MessageLevel.WARNING && (isNet || isGoogleSrc) ->
                            android.util.Log.w("LensDiag", "JSW $msg")
                    }
                    return super.onConsoleMessage(consoleMessage)
                }
            }
            isNestedScrollingEnabled = true
            setOnTouchListener { v, event ->
                when (event.action) {
                    android.view.MotionEvent.ACTION_DOWN -> {
                        v.parent.requestDisallowInterceptTouchEvent(true)
                    }
                    android.view.MotionEvent.ACTION_UP, android.view.MotionEvent.ACTION_CANCEL -> {
                        v.parent.requestDisallowInterceptTouchEvent(false)
                    }
                }
                false
            }
        }
    }

    // Back Handler Logic
    BackHandler(enabled = true) {
        if (isTranslatorMode) {
            CircleToSearchAccessibilityService.instance?.closeTranslateOverlay()
            return@BackHandler
        }
        val currentWebView = webViews[selectedEngine]
        if (currentWebView != null && currentWebView.canGoBack()) {
            currentWebView.goBack()
        } else if (scaffoldState.bottomSheetState.currentValue == androidx.compose.material3.SheetValue.Expanded) {
             scope.launch { scaffoldState.bottomSheetState.partialExpand() }
        } else if (scaffoldState.bottomSheetState.currentValue == androidx.compose.material3.SheetValue.PartiallyExpanded) {
             scope.launch { scaffoldState.bottomSheetState.hide() }
        } else {
            onClose()
        }
    }

    // Predictive back arrow: the system animates the back gesture (edge arrow + progress)
    // only when a prioritized callback receives it. In translator mode back is consumed
    // with no window transition, so a predictive handler keeps the arrow visible — and
    // the overlay windows (drawn above the arrow) fade out while the gesture runs.
    PredictiveBackHandler(enabled = isTranslatorMode) { progress ->
        val service = CircleToSearchAccessibilityService.instance
        try {
            progress.collect { event ->
                val fade = (1f - event.progress * 6f).coerceIn(0f, 1f)
                service?.setTranslatorGestureAlpha(fade)
            }
            service?.closeTranslateOverlay()
        } catch (_: java.util.concurrent.CancellationException) {
            // Gesture cancelled — restore the overlay windows.
            service?.setTranslatorGestureAlpha(1f)
        }
    }

    Surface(
        color = Color.Transparent,
        tonalElevation = 0.dp
    ) {
        androidx.compose.material3.BottomSheetScaffold(
            scaffoldState = scaffoldState,
            sheetPeekHeight = (androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp.dp * 0.55f), // Dynamic 55% peek
            containerColor = Color.Transparent,
            sheetContainerColor = Color.Transparent,
            sheetContentColor = MaterialTheme.colorScheme.onSurface,
            sheetDragHandle = null,
            sheetSwipeEnabled = true,
            sheetContent = {
                // Bottom Sheet Content (Results)
                // Background tracks the loaded page's own background color so the
                // tab strip matches the webpage instead of Material surface colors.
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(800.dp)
                        .background(pageBackgroundColor, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                ) {
                Spacer(modifier = Modifier.height(16.dp))

                // Tabs - Polished UI
                ScrollableTabRow(
                    selectedTabIndex = searchEngines.indexOf(selectedEngine),
                    edgePadding = 16.dp,
                    containerColor = Color.Transparent,
                    contentColor = MaterialTheme.colorScheme.onSurface,
                    divider = {},
                    indicator = {}
                ) {
                    searchEngines.forEach { engine ->
                        val selected = selectedEngine == engine
                        val transition = androidx.compose.animation.core.updateTransition(targetState = selected, label = "TabSelect")
                        val scale by transition.animateFloat(label = "Scale") { if (it) 1.05f else 1f }
                        val alpha by transition.animateFloat(label = "Alpha") { if (it) 1f else 0.7f }

                        Tab(
                            selected = selected,
                            onClick = { selectedEngine = engine },
                            modifier = Modifier.graphicsLayer { 
                                scaleX = scale
                                scaleY = scale
                                this.alpha = alpha
                            },
                            text = {
                                Text(
                                    engine.getDisplayName(context),
                                    style = MaterialTheme.typography.labelLarge.copy(
                                        fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium
                                    ),
                                    modifier = Modifier
                                        .background(
                                            pageBackgroundColor,
                                            RoundedCornerShape(16.dp)
                                        )
                                        .border(
                                             width = 1.dp,
                                             color = if(selected) MaterialTheme.colorScheme.primary.copy(alpha = 0.5f) else Color.Transparent,
                                             shape = RoundedCornerShape(16.dp)
                                        )
                                        .padding(horizontal = 16.dp, vertical = 8.dp),
                                    color = if (selected) MaterialTheme.colorScheme.onSecondaryContainer else MaterialTheme.colorScheme.onSurfaceVariant
                                )
                            }
                        )
                    }
                }

                // Reset everything when bitmap changes (new area selected)
                LaunchedEffect(selectedBitmap) {
                    hostedImageUrl = null
                    refreshingEngine = null
                    searchUrl = null
                    preloadedUrls.clear()
                    initializedEngines.clear() // Reset smart loading
                    // Do NOT destroy webviews here to keep them cached if possible? 
                    // PROBABLY safer to destroy to avoid stale state from previous searches.
                    webViews.values.forEach { 
                        it.stopLoading()
                        it.clearHistory()
                        it.clearCache(true)
                        it.loadUrl("about:blank")
                        it.destroy() 
                    }
                    webViews.clear()
                }

                LaunchedEffect(selectedBitmap, hostedImageUrl) {
                    if (selectedBitmap != null) {
                        isLoading = true
                        
                        val effectiveLensOnly = searchModeOverride ?: isGoogleLensOnly
                        
                        // 1. Google Lens Only Mode Check
                        // Use the reactive state (not a direct pref read) so we always
                        // get the value that was last committed, even from another activity.
                        if (effectiveLensOnly) {
                            // Save to cache and launch Lens
                            val path = ImageUtils.saveBitmap(context, selectedBitmap!!)
                            val uri = android.net.Uri.fromFile(java.io.File(path))
                            
                            val result = searchWithGoogleLens(uri, context)
                            when (result) {
                                com.akslabs.circletosearch.ui.components.LensLaunchResult.LAUNCHED_DIRECTLY -> {
                                    // Lens launched directly — safe to dismiss the overlay
                                    onClose()
                                    return@LaunchedEffect
                                }
                                com.akslabs.circletosearch.ui.components.LensLaunchResult.LAUNCHED_VIA_CHOOSER -> {
                                    // Chooser shown — overlay must stay open, user picks the app
                                    isLoading = false
                                    return@LaunchedEffect
                                }
                                com.akslabs.circletosearch.ui.components.LensLaunchResult.FAILED -> {
                                    // Fallback to multi-search
                                    android.util.Log.e("CircleToSearch", "Google Lens launch failed, falling back to multi-search")
                                }
                            }
                        }

                        // 2. Multi-Search Mode (Expanded UI)
                        scope.launch { scaffoldState.bottomSheetState.expand() }

                        // 3. Upload to host if needed (Multi-Search Mode)
                        if (hostedImageUrl == null) {
                            val url = ImageSearchUploader.uploadToImageHost(selectedBitmap!!)
                            if (url != null) {
                                hostedImageUrl = url
                            } else {
                                isLoading = false
                                return@LaunchedEffect
                            }
                        }

                        // 2. Generate URLs for ALL engines (lightweight string op)
                        searchEngines.forEach { engine ->
                            if (!preloadedUrls.containsKey(engine)) {
                                val url = when (engine) {
                                    SearchEngine.Google -> ImageSearchUploader.getGoogleLensUrl(hostedImageUrl!!)
                                    SearchEngine.Bing -> ImageSearchUploader.getBingUrl(hostedImageUrl!!)
                                    SearchEngine.Yandex -> ImageSearchUploader.getYandexUrl(hostedImageUrl!!)
                                    SearchEngine.TinEye -> ImageSearchUploader.getTinEyeUrl(hostedImageUrl!!)
                                    else -> null
                                }
                                if (url != null) preloadedUrls[engine] = url
                            }
                        }

                        // 3. Set initial URL
                        if (preloadedUrls.containsKey(selectedEngine)) {
                             searchUrl = preloadedUrls[selectedEngine]
                        }
                        
                        // 4. SMART LOADING SEQUENCE
                        // First, ensure selected engine is initialized
                        if (!initializedEngines.contains(selectedEngine)) {
                            initializedEngines.add(selectedEngine)
                        }
                        
                        isLoading = false
                        
                        // Then, load others sequentially
                        scope.launch {
                            searchEngines.forEach { engine ->
                                if (engine != selectedEngine) {
                                    delay(300) // Reduced from 800ms for faster loading
                                    if (!initializedEngines.contains(engine)) {
                                        initializedEngines.add(engine)
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Update searchUrl when engine changes
                LaunchedEffect(selectedEngine, preloadedUrls) {
                    if (preloadedUrls.containsKey(selectedEngine)) {
                        searchUrl = preloadedUrls[selectedEngine]
                    }
                }

                // Memory Optimization: REMOVED Aggressive Cleanup
                // User Requirement: "dont refresh tabs when user switch tabs keep them in background"
                // We keep them alive.
                /* 
                LaunchedEffect(selectedEngine) {
                     // ... (Cleanup logic removed)
                }
                */

                PullToRefreshBox(
                    isRefreshing = refreshingEngine != null,
                    onRefresh = {
                        val wv = webViews[selectedEngine]
                        if (wv != null) {
                            refreshingEngine = selectedEngine
                            wv.reload()
                        }
                    },
                    modifier = Modifier.fillMaxSize()
                ) {
                    // Show loading only if the SELECTED engine isn't ready or just starting
                    if (isLoading || (preloadedUrls.containsKey(selectedEngine) && !webViews.containsKey(selectedEngine))) {
                         // We delay showing the loader slightly to avoid flicker if WebView attaches instantly
                        var showLoader by remember { mutableStateOf(false) }
                        LaunchedEffect(Unit) {
                            kotlinx.coroutines.delay(100)
                            showLoader = true
                        }
                        
                        if (showLoader || isLoading) {
                             Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                                ContainedLoadingIndicatorSample()
                             }
                        }
                    }

                    // Dynamic Settings Update (User Agent etc) - Now handled at top level
                    // Cleanup on Dispose
                    DisposableEffect(Unit) {
                        onDispose {
                            webViews.values.forEach { 
                                it.stopLoading()
                                it.clearHistory()
                                it.clearCache(true)
                                it.loadUrl("about:blank")
                                it.destroy() 
                            }
                            webViews.clear()
                        }
                    }

                    // Render WebViews
                    searchEngines.forEach { engine ->
                         // Logic: Render if it's in the initialized set (Smart Loading)
                         // This ensures we don't load everything at once, but once loaded, we keep it.
                         if (initializedEngines.contains(engine) && preloadedUrls.containsKey(engine)) {
                             val url = preloadedUrls[engine]!!
                             val isSelected = (engine == selectedEngine)
                             
                             androidx.compose.runtime.key(engine) {
                                AndroidView(
                                    factory = { ctx ->
                                        if (webViews.containsKey(engine)) {
                                            // Should not happen with key(), but safety check
                                            val v = webViews[engine]!!
                                            (v.parent as? ViewGroup)?.removeView(v)
                                            v
                                        } else {
                                            val webView = createWebView(ctx, engine)
                                            // Apply current settings
                                             if (isDesktop(engine)) {
                                                 webView.settings.userAgentString = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                                             }
                                            
                                            webViews[engine] = webView
                                            webView.loadUrl(url)
                                            webView
                                        }
                                    },
                                    update = { webView ->
                                        if (webView.url != url && url != webView.originalUrl) {
                                            webView.loadUrl(url)
                                        }
                                    },
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .zIndex(if (isSelected) 1f else 0f)
                                        .graphicsLayer { 
                                            alpha = if (isSelected) 1f else 0f 
                                        }
                                )
                             }
                        }
                    }
                }
            }
        }
    ) { _ ->
        // Root Box
        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(Color.Transparent) // Changed from Black to Transparent
        ) {
            // Close button for Copy Mode (Top Left)

            // Friendly Message Overlay (Top Center)
            if (!isTranslatorMode) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .offset(y = 100.dp) // Offset to not cover potential top icons
                        .zIndex(100f), // Ensure on top
                    contentAlignment = Alignment.TopCenter
                ) {
                    FriendlyMessageBubble(
                        message = friendlyMessage,
                        visible = isMessageVisible
                    )
                }
            }

            // 1. Screenshot Layer
            if (screenshot != null) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        // Letterbox bars behind the Fit image are solid black, not see-through.
                        .background(Color.Black)
                        // View size for the image-fit mapping (crop-to-search, chips).
                        .onSizeChanged { canvasSizePx = it }
                ) {
                    Image(
                        bitmap = screenshot.asImageBitmap(),
                        contentDescription = stringResource(R.string.cd_screenshot),
                        contentScale = ContentScale.Fit,
                        modifier = Modifier.fillMaxSize()
                    )


                    // Tint Overlay (Punch-out style) — dimmed during normal CTS use,
                    // hidden in translator mode so the frozen backdrop stays undimmed.
                    // The Canvas renders to its OWN offscreen buffer: BlendMode.Clear then
                    // erases only the tint (the hole), while the image underneath — drawn in
                    // the parent, a separate buffer — stays intact and shows through the hole.
                    // (Offscreen on the parent Box would share one buffer: Clear would erase
                    // the image too and the hole would punch through to the black window.)
                    Canvas(modifier = Modifier.fillMaxSize().graphicsLayer {
                        alpha = if (isTranslatorMode) 0f else 1f
                        compositingStrategy = androidx.compose.ui.graphics.CompositingStrategy.Offscreen
                    }) {
                        val strokeWidth = 0f
                        val dimAlpha = 0.15f
                        
                        // 1. Draw global dim
                        drawRect(Color.Black.copy(alpha = dimAlpha))
                        drawRect(
                            brush = Brush.verticalGradient(
                                colors = OverlayGradientColors.map { it.copy(alpha = dimAlpha) }
                            )
                        )
                        
                        // 2. Clear selection area if it exists
                        if (selectionRect != null && selectionAnim.value > 0f) {
                            val rect = selectionRect!!
                            val progress = selectionAnim.value
                            val holeRect = androidx.compose.ui.geometry.Rect(
                                rect.left.toFloat(), 
                                rect.top.toFloat(), 
                                rect.right.toFloat(), 
                                rect.bottom.toFloat()
                            )
                            
                            // Punch the hole
                            drawRoundRect(
                                color = Color.Transparent,
                                topLeft = holeRect.topLeft,
                                size = holeRect.size,
                                cornerRadius = CornerRadius(48f),
                                blendMode = androidx.compose.ui.graphics.BlendMode.Clear
                            )
                        }
                    }
                }
            }

            // 2. Gradient Border Layer (Overlaying screenshot, clipped to rounded corners)
            if (showGradientBorder && !isTranslatorMode) {
                androidx.compose.animation.AnimatedVisibility(
                    visible = isUIVisible,
                    enter = androidx.compose.animation.fadeIn(animationSpec = tween(700))
                ) {
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .border(
                                width = 8.dp,
                                brush = Brush.verticalGradient(
                                    colors = OverlayGradientColors.map { it.copy(alpha = 0.5f) }
                                ),
                                shape = RoundedCornerShape(24.dp) // Rounded corners for device
                            )
                            .clip(RoundedCornerShape(24.dp))
                    )
                }
            }

            // 3. Drawing Canvas (Interactive Layer)
            if (!isTranslatorMode) {                Canvas(
                    modifier = Modifier.fillMaxSize()
                ) {
                    // Draw current path (Real-time)
                    if (currentPathPoints.size > 1) {
                        val path = Path().apply {
                            moveTo(currentPathPoints.first().x, currentPathPoints.first().y)
                            for (i in 1 until currentPathPoints.size) {
                                lineTo(currentPathPoints[i].x, currentPathPoints[i].y)
                            }
                        }
                        
                        // Glow
                        drawPath(
                            path = path,
                            brush = Brush.linearGradient(OverlayGradientColors),
                            style = Stroke(width = 30f, cap = StrokeCap.Round, join = StrokeJoin.Round),
                            alpha = 0.6f
                        )
                        // Core
                        drawPath(
                            path = path,
                            color = Color.White,
                            style = Stroke(width = 12f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        )
                    }

                    // Draw Lens Animation (Rounded Corner Brackets)
                    if (selectionRect != null && selectionAnim.value > 0f) {
                        val rect = selectionRect!!
                        val progress = selectionAnim.value
                        val left = rect.left.toFloat()
                        val top = rect.top.toFloat()
                        val right = rect.right.toFloat()
                        val bottom = rect.bottom.toFloat()
                        
                        val width = right - left
                        val height = bottom - top
                        val cornerRadius = 48f // Slightly smaller radius
                        val armLength = min(width, height) * 0.15f // Slightly shorter arms

                        // Top Left
                        val tlPath = Path().apply {
                            moveTo(left, top + armLength)
                            lineTo(left, top + cornerRadius)
                            arcTo(
                                rect = androidx.compose.ui.geometry.Rect(left, top, left + 2 * cornerRadius, top + 2 * cornerRadius),
                                startAngleDegrees = 180f,
                                sweepAngleDegrees = 90f,
                                forceMoveTo = false
                            )
                            lineTo(left + armLength, top)
                        }
                        // Top Right
                        val trPath = Path().apply {
                            moveTo(right - armLength, top)
                            lineTo(right - cornerRadius, top)
                            arcTo(
                                rect = androidx.compose.ui.geometry.Rect(right - 2 * cornerRadius, top, right, top + 2 * cornerRadius),
                                startAngleDegrees = 270f,
                                sweepAngleDegrees = 90f,
                                forceMoveTo = false
                            )
                            lineTo(right, top + armLength)
                        }
                        // Bottom Right
                        val brPath = Path().apply {
                            moveTo(right, bottom - armLength)
                            lineTo(right, bottom - cornerRadius)
                            arcTo(
                                rect = androidx.compose.ui.geometry.Rect(right - 2 * cornerRadius, bottom - 2 * cornerRadius, right, bottom),
                                startAngleDegrees = 0f,
                                sweepAngleDegrees = 90f,
                                forceMoveTo = false
                            )
                            lineTo(right - armLength, bottom)
                        }
                        // Bottom Left
                        val blPath = Path().apply {
                            moveTo(left + armLength, bottom)
                            lineTo(left + cornerRadius, bottom)
                            arcTo(
                                rect = androidx.compose.ui.geometry.Rect(left, bottom - 2 * cornerRadius, left + 2 * cornerRadius, bottom),
                                startAngleDegrees = 90f,
                                sweepAngleDegrees = 90f,
                                forceMoveTo = false
                            )
                            lineTo(left, bottom - armLength)
                        }

                        val bracketAlpha = progress
                        val bracketStroke = Stroke(width = 12f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        
                        // Draw Brackets (Solid White)
                        listOf(tlPath, trPath, brPath, blPath).forEach { p ->
                            drawPath(p, Color.White, style = bracketStroke, alpha = bracketAlpha)
                        }
                        
                        // Optional: Flash effect inside
                         drawRoundRect(
                            color = Color.White,
                            topLeft = Offset(left, top),
                            size = Size(width, height),
                            cornerRadius = CornerRadius(48f),
                            style = Stroke(width = 4f),
                            alpha = (1f - progress) * 0.5f
                         )
                    }
                }

            }

            // Circle-draw gestures + instant OCR overlays. The drag detector is the PARENT of
            // the word overlay, not a lower sibling: an ancestor is always in the hit path, the
            // child sees events first (taps and handle-drags stay with text selection, since it
            // consumes them), and plain drags fall through to circle drawing — the same
            // container/overlay split the translator module uses for pan vs. word selection.
            if (!isTranslatorMode) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .pointerInput(Unit) {
                            detectDragGestures(
                                onDragStart = { beginCircleDrag(it) },
                                onDrag = { change, _ -> moveCircleDrag(change) },
                                onDragEnd = { endCircleDrag() },
                            )
                        }
                ) {
                    if (screenshot != null && !isEntityExtractMode) {
                        val words = instantWords
                        if (words != null) {
                            WordSelectionOverlay(
                                words = words.sourceWords,
                                imageWidth = words.imageWidth,
                                imageHeight = words.imageHeight,
                                modifier = Modifier.fillMaxSize(),
                                onSelectionChanged = { selected ->
                                    wordSelected = selected
                                    // Mutually exclusive with the drawn circle: selecting
                                    // text drops the frame (and its crop) so only one stays.
                                    if (selected && selectionRect != null) {
                                        selectionRect = null
                                        selectedBitmap = null
                                    }
                                },
                                clearSelectionSignal = wordSelectionClearSignal,
                            )
                        } else if (!instantOcrDone) {
                            instantRegions?.let { regions ->
                                ScanAnimationOverlay(
                                    regions = regions,
                                    modifier = Modifier.fillMaxSize(),
                                )
                            }
                        }
                    }
                }
            }

            // 4. Header (Top)
            androidx.compose.animation.AnimatedVisibility(
                visible = isUIVisible && !isEntityExtractMode && !isTranslatorMode,
                enter = androidx.compose.animation.slideInVertically(
                    initialOffsetY = { -it }, // Commence au-dessus de l'écran (-100%)
                    animationSpec = tween(500, easing = androidx.compose.animation.core.FastOutSlowInEasing)
                ),
                modifier = Modifier.align(Alignment.TopCenter).zIndex(2000f)
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .statusBarsPadding()
                        .padding(top = 4.dp, start = 16.dp, end = 16.dp)
                        .align(Alignment.TopCenter),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    IconButton(
                        onClick =
                            {
                                haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                onClose()
                            },
                        modifier = Modifier
                            .background(Color.Gray.copy(alpha = 0.5f), CircleShape)
                            .size(40.dp)
                    ) {
                        Icon(Icons.Default.Close, contentDescription = stringResource(R.string.cd_close), tint = Color.White)
                    }
                    Spacer(modifier = Modifier.weight(1f))
                    if (selectedEngine.name == "Google") {
                        // Si c'est Google, on affiche ta belle typo officielle
                        Image(
                            painter = painterResource(id = com.akslabs.circletosearch.R.drawable.googletypo),
                            contentDescription = stringResource(R.string.cd_google_search),
                            modifier = Modifier.height(50.dp), // Hauteur ajustable selon ton PNG
                            colorFilter = androidx.compose.ui.graphics.ColorFilter.tint(Color.White)
                        )
                    } else {
                        // Si c'est Bing, Yandex, etc., on garde le texte stylé d'origine
                        Text(
                            text = selectedEngine.getDisplayName(context),
                            style = MaterialTheme.typography.headlineMedium.copy(
                                fontWeight = FontWeight.Bold,
                                color = Color.White
                            )
                        )
                    }
                    Spacer(modifier = Modifier.weight(1f))

                    // Action Button (Menu)
                    Box(
                        modifier = Modifier
                            .background(Color.Black.copy(alpha = 0.35f), CircleShape)
                            .size(40.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        var showMenu by remember { mutableStateOf(false) }
                        IconButton(onClick = {
                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                            showMenu = true
                        }) {
                            Icon(
                                Icons.Default.MoreVert,
                                contentDescription = stringResource(R.string.cd_menu),
                                tint = Color.White
                            )
                        }

                        androidx.compose.material3.DropdownMenu(
                            expanded = showMenu,
                            onDismissRequest = { showMenu = false },
                            shape = androidx.compose.foundation.shape.RoundedCornerShape(28.dp),
                            tonalElevation = 6.dp
                        ) {
                            val isDesktop = isDesktop(selectedEngine)
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(if (isDesktop) stringResource(R.string.menu_mobile_mode) else stringResource(R.string.menu_desktop_mode)) },
                                leadingIcon = {
                                    Icon(
                                        if (isDesktop) Icons.Default.Smartphone else Icons.Default.DesktopWindows,
                                        contentDescription = null
                                    )
                                },
                                onClick = {
                                    val newSet = desktopModeEngines.toMutableSet()
                                    if (newSet.contains(selectedEngine)) {
                                        newSet.remove(selectedEngine)
                                    } else {
                                        newSet.add(selectedEngine)
                                    }
                                    desktopModeEngines = newSet
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(if (isDarkMode) stringResource(R.string.menu_light_mode) else stringResource(R.string.menu_dark_mode)) },
                                leadingIcon = {
                                    Icon(
                                        if (isDarkMode) Icons.Default.LightMode else Icons.Default.DarkMode,
                                        contentDescription = null
                                    )
                                },
                                onClick = {
                                    isDarkMode = !isDarkMode
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(if (showGradientBorder) stringResource(R.string.menu_hide_border) else stringResource(R.string.menu_show_border)) },
                                leadingIcon = {
                                    Icon(Icons.Default.BorderOuter, contentDescription = null)
                                },
                                onClick = {
                                    showGradientBorder = !showGradientBorder
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(stringResource(R.string.menu_refresh)) },
                                leadingIcon = {
                                    Icon(Icons.Default.Refresh, contentDescription = null)
                                },
                                onClick = {
                                    webViews[selectedEngine]?.reload()
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(stringResource(R.string.menu_copy_url)) },
                                leadingIcon = {
                                    Icon(Icons.Default.ContentCopy, contentDescription = null)
                                },
                                onClick = {
                                    if (searchUrl != null) {
                                        val clipboard =
                                            context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                                        val clip = android.content.ClipData.newPlainText(
                                            "Search URL",
                                            searchUrl
                                        )
                                        clipboard.setPrimaryClip(clip)
                                    }
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(stringResource(R.string.menu_open_in_browser)) },
                                leadingIcon = {
                                    Icon(Icons.Default.OpenInNew, contentDescription = null)
                                },
                                onClick = {
                                    val currentUrl = webViews[selectedEngine]?.url ?: searchUrl
                                    if (currentUrl != null) {
                                        try {
                                            val intent = android.content.Intent(
                                                android.content.Intent.ACTION_VIEW,
                                                android.net.Uri.parse(currentUrl)
                                            )
                                            context.startActivity(intent)
                                        } catch (e: Exception) {
                                            android.util.Log.e(
                                                "CircleToSearch",
                                                "Failed to open browser",
                                                e
                                            )
                                        }
                                    }
                                    showMenu = false
                                }
                            )
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(stringResource(R.string.menu_settings)) },
                                leadingIcon = {
                                    Icon(Icons.Default.Settings, contentDescription = null)
                                },
                                onClick = {
                                    showSettingsScreen = true
                                    showMenu = false
                                }
                            )
                        }
                    }
                }
            }
            // 5. Bottom Bar — Material 3 Expressive two-row card
            androidx.compose.animation.AnimatedVisibility(
                visible = isUIVisible && !isEntityExtractMode && !isTranslatorMode && !wordSelected,
                enter = slideInVertically(
                    initialOffsetY = { it }, // slides up from below
                    animationSpec = tween(300, easing = androidx.compose.animation.core.CubicBezierEasing(0f, 0f, 0.2f, 1f))
                ) + fadeIn(animationSpec = tween(300)),
                exit = slideOutVertically(
                    targetOffsetY = { it }, // slides back down
                    animationSpec = tween(200, easing = androidx.compose.animation.core.CubicBezierEasing(0.4f, 0f, 1f, 1f))
                ) + fadeOut(animationSpec = tween(200)),
                modifier = Modifier.align(Alignment.BottomCenter).zIndex(2000f)
            ) {
                // ── Outer card container ──────────────────────────────────────────
                androidx.compose.material3.Surface(
                    modifier = Modifier
                        .align(Alignment.BottomCenter)
                        .navigationBarsPadding()
                        .fillMaxWidth()
                        .padding(horizontal = 12.dp, vertical = 16.dp),
                    shape = RoundedCornerShape(28.dp),
                    color = MaterialTheme.colorScheme.surfaceContainerHigh,
                    shadowElevation = 8.dp,
                    tonalElevation = 4.dp
                ) {
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 12.dp, vertical = 10.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp)
                    ) {
                        // ── ROW 1: Search Pill + Instant Actions (Song, Translate) ──
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp)
                        ) {
                            // Main Search Pill
                            androidx.compose.material3.Surface(
                                modifier = Modifier
                                    .weight(1f)
                                    .height(60.dp),
                                shape = CircleShape,
                                color = MaterialTheme.colorScheme.surfaceContainer,
                                tonalElevation = 2.dp
                            ) {
                                Row(
                                    modifier = Modifier
                                        .fillMaxSize()
                                        .padding(horizontal = 14.dp),
                                    verticalAlignment = Alignment.CenterVertically
                                ) {
                                    // App logo
                                    Image(
                                        painter = painterResource(id = com.akslabs.circletosearch.R.drawable.circletosearch),
                                        contentDescription = stringResource(R.string.cd_logo),
                                        modifier = Modifier
                                            .size(44.dp)
                                            .clickable {
                                                haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                                scope.launch { scaffoldState.bottomSheetState.expand() }
                                            }
                                    )

                                    Spacer(modifier = Modifier.weight(1f))

                                    // Mic Button
                                    IconButton(
                                        onClick = {
                                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                            try {
                                                val intent = android.content.Intent(android.speech.RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                                                    putExtra(android.speech.RecognizerIntent.EXTRA_LANGUAGE_MODEL, android.speech.RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                                                    addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                                }
                                                context.startActivity(intent)
                                            } catch (e: Exception) {}
                                        },
                                        modifier = Modifier.size(44.dp)
                                    ) {
                                        Icon(Icons.Default.Mic, contentDescription = stringResource(R.string.cd_voice_search))
                                    }

                                    Spacer(modifier = Modifier.width(4.dp))

                                    // Lens Button
                                    IconButton(
                                        onClick = {
                                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                            if (screenshot != null) {
                                                // Trigger Lens directly as this is the explicit Lens button
                                                // but do NOT change the global preference.
                                                val path = ImageUtils.saveBitmap(context, screenshot)
                                                val uri = android.net.Uri.fromFile(java.io.File(path))
                                                searchWithGoogleLens(uri, context)
                                                onClose()
                                            }
                                        },
                                        modifier = Modifier.size(44.dp)
                                    ) {
                                        Icon(painterResource(id = com.akslabs.circletosearch.R.drawable.circletosearch), contentDescription = stringResource(R.string.cd_lens), tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(26.dp))
                                    }
                                }
                            }

                            // Circular Button: Translate
                            // Short press: freeze CTS (chrome hides, screenshot stays) and run the module's
                            //   translator overlay on the capture; close/back returns to Circle to Search.
                            // Long press: close CTS and run the same overlay on the real screen (bubble behavior).
                            Box(
                                modifier = Modifier
                                    .size(60.dp)
                                    .clip(CircleShape)
                                    .background(MaterialTheme.colorScheme.surfaceContainer)
                                    .combinedClickable(
                                        onClick = {
                                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                            val bmp = screenshot
                                            val translatorService = CircleToSearchAccessibilityService.instance
                                            if (bmp == null || translatorService == null) {
                                                android.widget.Toast.makeText(context, context.getString(R.string.toast_translate_unavailable), android.widget.Toast.LENGTH_SHORT).show()
                                            } else {
                                                selectionRect = null
                                                selectedBitmap = null
                                                isSearching = false
                                                currentPathPoints.clear()
                                                scope.launch { selectionAnim.snapTo(0f) }
                                                isTranslatorMode = true
                                                // OCR starts now; only the overlay chrome waits for the hide delay.
                                                val started = translatorService.translateCapturedScreen(bmp, TRANSLATE_HIDE_DELAY_MS) {
                                                    isTranslatorMode = false
                                                }
                                                if (!started) {
                                                    isTranslatorMode = false
                                                    android.widget.Toast.makeText(context, context.getString(R.string.toast_translate_unavailable), android.widget.Toast.LENGTH_SHORT).show()
                                                }
                                            }
                                        },
                                        onLongClick = {
                                            haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                            val bmp = screenshot
                                            val translatorService = CircleToSearchAccessibilityService.instance
                                            if (bmp == null || translatorService == null) {
                                                android.widget.Toast.makeText(context, context.getString(R.string.toast_translate_unavailable), android.widget.Toast.LENGTH_SHORT).show()
                                            } else {
                                                translatorService.translateCapturedScreen(bmp, TRANSLATE_TRIGGER_DELAY_MS)
                                                onClose()
                                            }
                                        }
                                    ),
                                contentAlignment = Alignment.Center
                            ) {
                                Icon(Icons.Default.Translate, contentDescription = stringResource(R.string.cd_translate))
                            }
                        }

                        // ── ROW 2: Action buttons ───────────────────────────────
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 0.dp), // Maximize space for labels
                            horizontalArrangement = Arrangement.Center,
                            verticalAlignment = Alignment.Bottom
                        ) {
                            @Composable
                            fun BottomBarButton(
                                label: String, 
                                icon: @Composable () -> Unit, 
                                enabled: Boolean = true,
                                onClick: () -> Unit
                            ) {
                                Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.BottomCenter) {
                                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                                        FilledTonalIconButton(
                                            onClick = { 
                                                haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                                onClick() 
                                            },
                                            enabled = enabled,
                                            modifier = Modifier.size(48.dp),
                                            colors = IconButtonDefaults.filledTonalIconButtonColors(
                                                containerColor = MaterialTheme.colorScheme.surfaceContainer,
                                                contentColor = if (enabled) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f),
                                                disabledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                                                disabledContentColor = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.38f)
                                            )
                                        ) { icon() }
                                        
                                        Spacer(modifier = Modifier.height(4.dp))
                                        
                                        Text(
                                            text = label, 
                                            style = MaterialTheme.typography.labelSmall.copy(
                                                fontSize = 10.5.sp, 
                                                lineHeight = 12.sp,
                                                letterSpacing = (-0.4).sp,
                                                fontWeight = FontWeight.Medium
                                            ), 
                                            color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.38f), 
                                            textAlign = TextAlign.Center, 
                                            maxLines = 1,
                                            overflow = TextOverflow.Visible,
                                            modifier = Modifier.padding(horizontal = 0.dp)
                                        )
                                    }
                                }
                            }

                            // More Apps
//                            BottomBarButton("More Apps", { Icon(Icons.Default.Apps, null) }) {
//                                showMoreAppsSheet = true
//                            }

                            // Pin
//                            val isPinEnabled = selectedBitmap != null
//                            BottomBarButton(
//                                label = "Pin",
//                                icon = { Icon(Icons.Default.PushPin, contentDescription = null, modifier = Modifier.size(22.dp)) },
//                                enabled = isPinEnabled
//                            ) {
//                                selectedBitmap?.let { bmp ->
//                                    CircleToSearchAccessibilityService.pinArea(bmp, selectionRect ?: android.graphics.Rect())
//                                    (context as? android.app.Activity)?.finish()
//                                }
//                            }

                            // Smart Entity Extractor — instant OCR over the screenshot using
                            // the translator module's recognition (no Tesseract, no re-OCR).
                            BottomBarButton(stringResource(R.string.label_smartscan), { Icon(Icons.Default.Search, null, modifier = Modifier.size(22.dp)) }) {
                                isEntityExtractMode = true
                                if (detectedEntities.isEmpty() && !isExtractingEntities) {
                                    isExtractingEntities = true
                                    val cachedWords = instantWords?.sourceWords
                                    val scanGeneration = entityScanGeneration
                                    scope.launch(kotlinx.coroutines.Dispatchers.IO) {
                                        val bmp = screenshot ?: return@launch
                                        val sourceWords = cachedWords ?: runInstantScreenshotOcr(context, bmp) { }?.sourceWords
                                        if (sourceWords == null) {
                                            kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                                                isExtractingEntities = false
                                            }
                                            return@launch
                                        }
                                        val entities = mutableListOf<SmartEntity>()

                                        val urlRegex = android.util.Patterns.WEB_URL.toRegex()
                                        val emailRegex = android.util.Patterns.EMAIL_ADDRESS.toRegex()
                                        val phoneRegexLine = Regex("""(\+?[\d\s\-\(\).]{9,20})""")
                                        val upiRegex = Regex("""[a-zA-Z0-9.\-_]{2,256}@[a-zA-Z]{2,64}""")

                                        // Group recognized words into lines; phone matches map into
                                        // the line's union bounds by character ratio, like before.
                                        sourceWords.groupBy { it.lineIndex }.forEach { (_, lineWords) ->
                                            val lineText = lineWords.joinToString(" ") { it.text }
                                            var left = Float.MAX_VALUE
                                            var top = Float.MAX_VALUE
                                            var right = -Float.MAX_VALUE
                                            var bottom = -Float.MAX_VALUE
                                            lineWords.forEach { w ->
                                                left = min(left, w.bounds.cx - w.bounds.width / 2f)
                                                top = min(top, w.bounds.cy - w.bounds.height / 2f)
                                                right = max(right, w.bounds.cx + w.bounds.width / 2f)
                                                bottom = max(bottom, w.bounds.cy + w.bounds.height / 2f)
                                            }
                                            phoneRegexLine.findAll(lineText).forEach { match ->
                                                val phoneCandidate = match.value.trim()
                                                if (phoneCandidate.count { it.isDigit() } >= 10) {
                                                    val startIdx = match.range.first
                                                    val endIdx = match.range.last + 1
                                                    val ratioStart = startIdx.toFloat() / lineText.length.coerceAtLeast(1)
                                                    val ratioEnd = endIdx.toFloat() / lineText.length.coerceAtLeast(1)
                                                    val entityBounds = android.graphics.RectF(
                                                        left + (ratioStart * (right - left)),
                                                        top,
                                                        left + (ratioEnd * (right - left)),
                                                        bottom
                                                    )
                                                    entities.add(SmartEntity.Phone(phoneCandidate, entityBounds, context.getString(R.string.entity_type_phone)))
                                                }
                                            }
                                        }
                                        sourceWords.forEach { word ->
                                            val txt = word.text.trim()
                                            val wordBounds = android.graphics.RectF(
                                                word.bounds.cx - word.bounds.width / 2f,
                                                word.bounds.cy - word.bounds.height / 2f,
                                                word.bounds.cx + word.bounds.width / 2f,
                                                word.bounds.cy + word.bounds.height / 2f
                                            )
                                            if (emailRegex.matches(txt)) entities.add(SmartEntity.Email(txt, wordBounds, context.getString(R.string.entity_type_email)))
                                            else if (upiRegex.matches(txt)) entities.add(SmartEntity.Upi(txt, wordBounds, context.getString(R.string.entity_type_upi)))
                                            else if (urlRegex.matches(txt)) entities.add(SmartEntity.Url(txt, wordBounds, context.getString(R.string.entity_type_link)))
                                        }
                                        kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Main) {
                                            isExtractingEntities = false
                                            // Discard results if the canvas image changed mid-scan.
                                            if (scanGeneration == entityScanGeneration) {
                                                detectedEntities = entities
                                            }
                                        }
                                    }
                                }
                            }

                            // Donate
//                            BottomBarButton("Donate", { Icon(painterResource(id = com.akslabs.circletosearch.R.drawable.donation), null, modifier = Modifier.size(22.dp)) }) {
//                                showDonateSheet = true
//                            }

                            // QR Scan
                            BottomBarButton(stringResource(R.string.label_scan_qr), { Icon(Icons.Default.QrCode, null) }) {
                                qrScanBitmap = selectedBitmap ?: screenshot
                                showQrSheet = true
                            }

                            // Fullscreen
                            BottomBarButton(stringResource(R.string.label_fullscreen), { Icon(Icons.Default.Fullscreen, null) }) {
                                if (screenshot != null) {
                                    // Full-image selection in screen space (view size, not bitmap size).
                                    val viewW = if (canvasSizePx.width > 0) canvasSizePx.width else screenshot.width
                                    val viewH = if (canvasSizePx.height > 0) canvasSizePx.height else screenshot.height
                                    selectionRect = Rect(0, 0, viewW, viewH)
                                    currentPathPoints.clear()
                                    scope.launch { 
                                        selectionAnim.snapTo(0f)
                                        selectionAnim.animateTo(1f, tween(600))
                                        selectedBitmap = screenshot
                                        isSearching = true 
                                    }
                                }
                            }

                            // Upload — pick a gallery image and run Circle to Search on it
                            BottomBarButton(stringResource(R.string.label_upload), { Icon(Icons.Default.Upload, null) }) {
                                pickImageLauncher.launch(
                                    PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)
                                )
                            }
                        }
                    }
                }
            }

            // Phase 44: Smart Entity Extractor Exit Overlay (Full Screen tap capture)
            if (isEntityExtractMode) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .zIndex(2550f)
                        .pointerInput(Unit) {
                            detectTapGestures {
                                isEntityExtractMode = false
                            }
                        }
                )
            }

            // 4. Selection Actions (Share) — Positioned at the very end for absolute top-layer rendering
            if (selectionRect != null && selectionAnim.value == 1f && !isTranslatorMode) {
                val rect = selectionRect!!
                val density = androidx.compose.ui.platform.LocalDensity.current
                val leftPx = rect.left.toFloat()
                val topPx = rect.top.toFloat()
                val rightPx = rect.right.toFloat()
                val bottomPx = rect.bottom.toFloat()
                
                val leftDp = with(density) { leftPx.toDp() }
                val topDp = with(density) { topPx.toDp() }
                val rightDp = with(density) { rightPx.toDp() }
                val bottomDp = with(density) { bottomPx.toDp() }
                val widthDp = rightDp - leftDp
                
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .zIndex(2000f) // Highest Z-index
                ) {
                    val screenWidth = androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp.dp
                    val screenHeight = androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp.dp
                    val centerX = (leftDp + rightDp) / 2
                    Box(
                        modifier = Modifier
                            .offset(
                                x = (centerX - 160.dp).coerceIn(0.dp, screenWidth - 320.dp),
                                y = if (topPx > 200f) (topDp - 72.dp).coerceAtLeast(16.dp) else (bottomDp + 16.dp).coerceAtMost(screenHeight - 80.dp)
                            )
                            .width(320.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        // Wrapped in Surface to match Bottom Bar tonal environment
                        androidx.compose.material3.Surface(
                            color = MaterialTheme.colorScheme.surfaceContainerHigh,
                            shape = CircleShape,
                            tonalElevation = 4.dp,
                            shadowElevation = 8.dp
                        ) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                // 1. SHARE BUTTON
                                androidx.compose.material3.FilledTonalButton(
                                    onClick = {
                                        if (selectedBitmap != null) {
                                            scope.launch {
                                                try {
                                                    val fileName = "selection_${java.util.UUID.randomUUID()}.png"
                                                    val path = ImageUtils.saveBitmap(context, selectedBitmap!!, fileName)
                                                    val file = java.io.File(path)
                                                    val uri = androidx.core.content.FileProvider.getUriForFile(context, "com.akslabs.circletosearch.fileprovider", file)
                                                    val shareIntent = android.content.Intent(android.content.Intent.ACTION_SEND).apply { 
                                                        type = "image/png"
                                                        putExtra(android.content.Intent.EXTRA_STREAM, uri)
                                                        clipData = android.content.ClipData.newRawUri("Selection", uri)
                                                        addFlags(android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                                        addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                                    }
                                                    context.startActivity(android.content.Intent.createChooser(shareIntent, context.getString(R.string.share_chooser_title)).apply { 
                                                        addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK) 
                                                    })
                                                } catch (e: Exception) {
                                                    android.util.Log.e("CircleToSearch", "Failed to share selection", e)
                                                }
                                            }
                                        }
                                    },
                                    modifier = Modifier.height(48.dp),
                                    shape = CircleShape,
                                    colors = androidx.compose.material3.ButtonDefaults.filledTonalButtonColors(
                                        containerColor = Color.Transparent, // Parent surface handles color
                                        contentColor = MaterialTheme.colorScheme.onSurface
                                    ),
                                    elevation = null,
                                    contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 20.dp)
                                ) {
                                    Text(
                                        stringResource(R.string.label_share), 
                                        style = MaterialTheme.typography.labelLarge.copy(
                                            fontWeight = androidx.compose.ui.text.font.FontWeight.Normal
                                        )
                                    )
                                }



                                // Separator
                                androidx.compose.material3.VerticalDivider(
                                    modifier = Modifier.height(24.dp).padding(horizontal = 2.dp),
                                    color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)
                                )

                                // 2. SAVE BUTTON
                                androidx.compose.material3.FilledTonalButton(
                                    onClick = {
                                        if (selectedBitmap != null) {
                                            val success = ImageUtils.saveToGallery(context, selectedBitmap!!)
                                            if (success) {
                                                haptic.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress)
                                                android.widget.Toast.makeText(context, context.getString(R.string.toast_saved_to_gallery), android.widget.Toast.LENGTH_SHORT).show()
                                            } else {
                                                android.widget.Toast.makeText(context, context.getString(R.string.toast_failed_to_save), android.widget.Toast.LENGTH_SHORT).show()
                                            }
                                        }
                                    },
                                    modifier = Modifier.height(48.dp),
                                    shape = CircleShape,
                                    colors = androidx.compose.material3.ButtonDefaults.filledTonalButtonColors(
                                        containerColor = Color.Transparent,
                                        contentColor = MaterialTheme.colorScheme.onSurface
                                    ),
                                    elevation = null,
                                    contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 20.dp)
                                ) {
                                    Text(
                                        stringResource(R.string.save), 
                                        style = MaterialTheme.typography.labelLarge.copy(
                                            fontWeight = androidx.compose.ui.text.font.FontWeight.Normal
                                        )
                                    )
                                }

                                // Separator
                                androidx.compose.material3.VerticalDivider(
                                    modifier = Modifier.height(24.dp).padding(horizontal = 2.dp),
                                    color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f)
                                )

                                // 3. PIN BUTTON
                                androidx.compose.material3.FilledTonalButton(
                                    onClick = {
                                        if (selectedBitmap != null && selectionRect != null) {
                                            val pinned = CircleToSearchAccessibilityService.pinArea(selectedBitmap!!, selectionRect!!)
                                            if (pinned) {
                                                (context as? android.app.Activity)?.finish()
                                            } else {
                                                android.widget.Toast.makeText(context, context.getString(R.string.toast_accessibility_permission_needed), android.widget.Toast.LENGTH_LONG).show()
                                            }
                                        }
                                    },
                                    modifier = Modifier.height(48.dp),
                                    shape = CircleShape,
                                    colors = androidx.compose.material3.ButtonDefaults.filledTonalButtonColors(
                                        containerColor = Color.Transparent,
                                        contentColor = MaterialTheme.colorScheme.onSurface
                                    ),
                                    elevation = null,
                                    contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 20.dp)
                                ) {
                                    Text(
                                        stringResource(R.string.pin),
                                        style = MaterialTheme.typography.labelLarge.copy(
                                            fontWeight = androidx.compose.ui.text.font.FontWeight.Normal
                                        )
                                    )
                                }
                            }
                        }
                    }
                }
            }

        // --- NEW: QR Overlay Chips (High Layer) ---
        if (screenshot != null && !isTranslatorMode) {
            BoxWithConstraints(modifier = Modifier.fillMaxSize().zIndex(2500f)) {
                val screenWidth = maxWidth
                val screenHeight = maxHeight
                val bitmapWidth = screenshot.width.toFloat()
                val bitmapHeight = screenshot.height.toFloat()
                // ContentScale.Fit mapping — chips sit over the letterboxed image, in dp.
                val t = fitMapping(screenWidth.value, screenHeight.value, bitmapWidth, bitmapHeight)

                android.util.Log.d("CircleToSearch", "Rendering QR Chips: count=${detectedQrCodes.size}")
                detectedQrCodes.forEach { qr ->
                    qr.bounds?.let { bounds ->
                        val chipX = t.offsetX + bounds.centerX() * t.scale
                        val chipY = t.offsetY + bounds.centerY() * t.scale
                        val isUrl = qr.result is QrResult.Url

                        // Using a Box with pointerInput to consume taps and prevent circling
                        Box(
                            modifier = Modifier
                                .offset(x = chipX.dp - 24.dp, y = chipY.dp - 24.dp)
                                .size(48.dp)
                                .shadow(6.dp, CircleShape)
                                .background(Color.White, CircleShape)
                                .border(1.5.dp, if(isUrl) Color(0xFF1A73E8) else MaterialTheme.colorScheme.primary.copy(alpha = 0.5f), CircleShape)
                                .pointerInput(qr) {
                                    detectTapGestures {
                                        selectedQrResult = qr
                                        qrScanBitmap = null 
                                        showQrSheet = true
                                    }
                                },
                            contentAlignment = Alignment.Center
                        ) {
                            Icon(
                                Icons.Default.QrCode, 
                                null, 
                                tint = if(isUrl) Color(0xFF1A73E8) else MaterialTheme.colorScheme.primary, 
                                modifier = Modifier.size(24.dp)
                            )
                        }
                        
                        val label = qrResultShortLabel(qr.result, context)
                        if (label.isNotEmpty()) {
                            Surface(
                                modifier = Modifier
                                    .offset(x = chipX.dp + 28.dp, y = chipY.dp - 12.dp)
                                    .pointerInput(qr) {
                                        detectTapGestures {
                                            selectedQrResult = qr
                                            qrScanBitmap = null 
                                            showQrSheet = true
                                        }
                                    },
                                shape = RoundedCornerShape(12.dp),
                                color = Color.White.copy(alpha = 0.95f),
                                tonalElevation = 4.dp,
                                shadowElevation = 4.dp,
                                border = if(isUrl) androidx.compose.foundation.BorderStroke(1.dp, Color(0xFF1A73E8).copy(alpha = 0.3f)) else null
                            ) {
                                Text(
                                    text = label,
                                    style = MaterialTheme.typography.labelSmall.copy(
                                        color = if(isUrl) Color(0xFF1A73E8) else MaterialTheme.colorScheme.onSurface,
                                        fontWeight = if(isUrl) FontWeight.Bold else FontWeight.Medium
                                    ),
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis
                                )
                            }
                        }
                    }
                }
            }
        }

        // --- Phase 44: Smart Entity Extractor Overlay Chips ---
        if (isEntityExtractMode && screenshot != null) {
            BoxWithConstraints(modifier = Modifier.fillMaxSize().zIndex(2600f)) {
                val screenWidth = maxWidth
                val screenHeight = maxHeight
                val bitmapWidth = screenshot.width.toFloat()
                val bitmapHeight = screenshot.height.toFloat()
                // ContentScale.Fit mapping — chips sit over the letterboxed image, in dp.
                val t = fitMapping(screenWidth.value, screenHeight.value, bitmapWidth, bitmapHeight)

                if (isExtractingEntities) {
                    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            androidx.compose.material3.CircularProgressIndicator(color = MaterialTheme.colorScheme.primary)
                            Spacer(Modifier.height(16.dp))
                            Text(stringResource(R.string.label_extracting_entities), color = Color.White)
                        }
                    }
                }

                detectedEntities.forEach { entity ->
                    val chipX = t.offsetX + entity.bounds.centerX() * t.scale
                    val chipY = t.offsetY + entity.bounds.centerY() * t.scale

                    // Main Chip Icon
                    Box(
                        modifier = Modifier
                            .offset(x = chipX.dp - 24.dp, y = chipY.dp - 24.dp)
                            .size(48.dp)
                            .shadow(6.dp, CircleShape)
                            .background(Color.White, CircleShape)
                            .border(1.5.dp, entity.sourceColor, CircleShape)
                            .clickable {
                                try {
                                    // Auto-copy clicked content to clipboard
                                    val clipboard = context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                                    val clip = android.content.ClipData.newPlainText(entity.typeName, entity.text)
                                    clipboard.setPrimaryClip(clip)

                                    val intent = when(entity) {
                                        is SmartEntity.Url -> android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(if (!entity.text.startsWith("http")) "http://${entity.text}" else entity.text))
                                        is SmartEntity.Email -> android.content.Intent(android.content.Intent.ACTION_SENDTO, android.net.Uri.parse("mailto:${entity.text}"))
                                        is SmartEntity.Phone -> android.content.Intent(android.content.Intent.ACTION_DIAL, android.net.Uri.parse("tel:${entity.text}"))
                                        is SmartEntity.Upi -> android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("upi://pay?pa=${entity.text}"))
                                    }
                                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                    context.startActivity(intent)
                                    isEntityExtractMode = false
                                } catch (e: Exception) {}
                            },
                        contentAlignment = Alignment.Center
                    ) {
                        Icon(entity.icon, null, tint = entity.sourceColor, modifier = Modifier.size(24.dp))
                    }
                    
                    // Label Chip
                    Surface(
                        modifier = Modifier
                            .offset(x = chipX.dp + 28.dp, y = chipY.dp - 12.dp)
                            .clickable {
                                try {
                                    val intent = when(entity) {
                                        is SmartEntity.Url -> android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(if (!entity.text.startsWith("http")) "http://${entity.text}" else entity.text))
                                        is SmartEntity.Email -> android.content.Intent(android.content.Intent.ACTION_SENDTO, android.net.Uri.parse("mailto:${entity.text}"))
                                        is SmartEntity.Phone -> android.content.Intent(android.content.Intent.ACTION_DIAL, android.net.Uri.parse("tel:${entity.text}"))
                                        is SmartEntity.Upi -> android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("upi://pay?pa=${entity.text}"))
                                    }
                                    intent.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)
                                    context.startActivity(intent)
                                    isEntityExtractMode = false
                                } catch (e: Exception) {}
                            },
                        shape = RoundedCornerShape(12.dp),
                        color = Color.White.copy(alpha = 0.95f),
                        tonalElevation = 4.dp,
                        shadowElevation = 4.dp,
                        border = androidx.compose.foundation.BorderStroke(1.dp, entity.sourceColor.copy(alpha = 0.3f))
                    ) {
                        Text(
                            text = entity.text,
                            style = MaterialTheme.typography.labelSmall.copy(
                                color = entity.sourceColor,
                                fontWeight = FontWeight.Bold
                            ),
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis
                        )
                    }
                }
            }
        }

        // Input shield while the translator overlay owns the screen: consumes touches so
        // the hidden chrome underneath can't be tapped (module windows sit above this).
        if (isTranslatorMode) {
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .zIndex(4000f)
                    .pointerInput(Unit) {
                        awaitEachGesture {
                            awaitFirstDown(requireUnconsumed = false)
                            while (true) {
                                val event = awaitPointerEvent()
                                event.changes.forEach { it.consume() }
                            }
                        }
                    }
            )
        }

        // QR Code Result Sheet
        if (showQrSheet) {
            // Full-screen invisible overlay to catch "click outside"
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .zIndex(2500f)
                    .background(Color.Black.copy(alpha = 0.01f)) // Almost invisible but catches taps
                    .clickable(
                        interactionSource = remember { MutableInteractionSource() },
                        indication = null
                    ) { 
                        showQrSheet = false
                        selectedQrResult = null
                    }
            )

            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .align(Alignment.BottomCenter)
                    .navigationBarsPadding()
                    .padding(bottom = 16.dp)
                    .zIndex(3000f),
                contentAlignment = Alignment.BottomCenter
            ) {
                // Ensure BackHandler is active when sheet is shown
                androidx.activity.compose.BackHandler {
                    showQrSheet = false
                    selectedQrResult = null
                }

                QrCodeResultSheet(
                    context = context,
                    bitmap = qrScanBitmap,
                    onDismiss = { 
                        showQrSheet = false
                        selectedQrResult = null
                    },
                    initialResults = detectedQrCodes,
                    initialPage = if (selectedQrResult != null) detectedQrCodes.indexOf(selectedQrResult!!) else 0
                )
            }
        }

        if (showMoreAppsSheet) {
            MoreAppsBottomSheet(
                onDismiss = { showMoreAppsSheet = false },
                onAppSelected = { url ->
                    try {
                        context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, Uri.parse(url)).apply { addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK) })
                    } catch (e: Exception) {}
                    showMoreAppsSheet = false
                }
            )
        }

        if (showDonateSheet) {
            DonateBottomSheet(
                onDismiss = { showDonateSheet = false },
                onDonateOptionSelected = { url ->
                    try {
                        context.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, Uri.parse(url)).apply { addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK) })
                    } catch (e: Exception) {}
                    showDonateSheet = false
                }
            )
        }

        if (showSettingsScreen) {
            SettingsScreen(
                uiPreferences = uiPreferences,
                onDismissRequest = { showSettingsScreen = false }
            )
        }
    }
}
}
}

// --- Phase 44: Smart Entity Extractor Models ---
sealed class SmartEntity(val text: String, val bounds: android.graphics.RectF, val typeName: String, val icon: androidx.compose.ui.graphics.vector.ImageVector, val sourceColor: Color) {
    class Url(text: String, bounds: android.graphics.RectF, typeName: String) : SmartEntity(text, bounds, typeName, Icons.Default.Link, Color(0xFF1A73E8))
    class Email(text: String, bounds: android.graphics.RectF, typeName: String) : SmartEntity(text, bounds, typeName, Icons.Default.Email, Color(0xFF1A73E8))
    class Phone(text: String, bounds: android.graphics.RectF, typeName: String) : SmartEntity(text, bounds, typeName, Icons.Default.Phone, Color(0xFF43A047))
    class Upi(text: String, bounds: android.graphics.RectF, typeName: String) : SmartEntity(text, bounds, typeName, Icons.Default.Person, Color(0xFF8E24AA))
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@androidx.compose.ui.tooling.preview.Preview
@androidx.compose.runtime.Composable
fun ContainedLoadingIndicatorSample() {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        androidx.compose.material3.ContainedLoadingIndicator()
    }
}
