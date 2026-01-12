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

import com.akslabs.circletosearch.utils.FriendlyMessageManager
import com.akslabs.circletosearch.ui.components.FriendlyMessageBubble
import com.akslabs.circletosearch.utils.UIPreferences
import kotlinx.coroutines.delay

import android.graphics.Bitmap
import android.graphics.Rect
import android.util.Base64
import android.webkit.WebView
import android.webkit.WebViewClient
import android.webkit.WebSettings
import android.widget.FrameLayout
import android.view.ViewGroup
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Code
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Send
import androidx.compose.material.icons.filled.Settings
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material.icons.filled.DesktopWindows
import androidx.compose.material.icons.filled.LightMode
import androidx.compose.material.icons.filled.DarkMode
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.OpenInNew
import androidx.compose.material.icons.filled.BorderOuter
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
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
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.zIndex
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.ui.theme.OverlayGradientColors
import com.akslabs.circletosearch.utils.ImageSearchUploader
import com.akslabs.circletosearch.utils.ImageUtils
import com.akslabs.circletosearch.ui.components.searchWithGoogleLens
import kotlinx.coroutines.launch
import java.io.ByteArrayOutputStream
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.animateFloat
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature
import com.akslabs.circletosearch.data.isDirectUpload
import kotlin.math.max
import kotlin.math.min

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CircleToSearchScreen(
    screenshot: Bitmap?,
    onClose: () -> Unit
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    
    // Initialize preferences
    val uiPreferences = remember { UIPreferences(context) }
    
    // Support Sheet State
    var showSupportSheet by remember { mutableStateOf(false) }
    val supportSheetState = rememberModalBottomSheetState()
    

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
    var isLoading by remember { mutableStateOf(false) }
    
    // Desktop Mode - Per Tab
    // We use a set to track which engines are in desktop mode
    val initialDesktopMode = uiPreferences.isDesktopMode() // Global default
    var desktopModeEngines by remember { mutableStateOf<Set<SearchEngine>>(if(initialDesktopMode) searchEngines.toSet() else emptySet()) }
    
    var isDarkMode by remember { mutableStateOf(uiPreferences.isDarkMode()) }
    var showGradientBorder by remember { mutableStateOf(uiPreferences.isShowGradientBorder()) }
    
    // Track initialized engines for Smart Loading
    val initializedEngines = remember { mutableStateListOf<SearchEngine>() }
    
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
                            val darkModeCSS = """
                                javascript:(function() {
                                    var style = document.createElement('style');
                                    style.innerHTML = `
                                        html { filter: invert(1) hue-rotate(180deg) !important; background: #000 !important; }
                                        img, video, [style*="background-image"] { filter: invert(1) hue-rotate(180deg) !important; }
                                    `;
                                    document.head.appendChild(style);
                                })()
                            """.trimIndent()
                            view?.loadUrl(darkModeCSS)
                        }
                    }
                } else {
                    wv.webViewClient = android.webkit.WebViewClient()
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
    
    // searchEngines moved to top
    // val searchEngines = SearchEngine.values()

    // Gradient Animation
    val alphaAnim by animateFloatAsState(
        targetValue = if (screenshot != null) 1f else 0f,
        animationSpec = tween(1000), label = "alpha"
    )

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

            webViewClient = object : WebViewClient() {
                
                override fun onPageFinished(view: WebView?, url: String?) {
                    super.onPageFinished(view, url)
                    // Inject dark mode CSS if enabled
                    if (isDarkMode) {
                        val darkModeCSS = """
                            javascript:(function() {
                                var style = document.createElement('style');
                                style.innerHTML = `
                                    html { filter: invert(1) hue-rotate(180deg) !important; background: #000 !important; }
                                    img, video, [style*="background-image"] { filter: invert(1) hue-rotate(180deg) !important; }
                                `;
                                document.head.appendChild(style);
                            })()
                        """.trimIndent()
                        view?.loadUrl(darkModeCSS)
                        android.util.Log.d("CircleToSearch", "Dark mode CSS injected for: $url")
                    }
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

    androidx.compose.material3.BottomSheetScaffold(
        scaffoldState = scaffoldState,
        sheetPeekHeight = (androidx.compose.ui.platform.LocalConfiguration.current.screenHeightDp.dp * 0.55f), // Dynamic 55% peek
        sheetContainerColor = Color.Transparent,
        sheetContentColor = MaterialTheme.colorScheme.onSurface,
        sheetDragHandle = { BottomSheetDefaults.DragHandle() },
        sheetSwipeEnabled = true,
        sheetContent = {
            // Bottom Sheet Content (Results)
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(800.dp)
                    .background(MaterialTheme.colorScheme.surface, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
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
                                    engine.name,
                                    style = MaterialTheme.typography.labelLarge.copy(
                                        fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium
                                    ),
                                    modifier = Modifier
                                        .background(
                                            if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surfaceVariant.copy(alpha=0.5f),
                                            RoundedCornerShape(16.dp)
                                        )
                                        .border(
                                             width = 1.dp,
                                             color = if(selected) MaterialTheme.colorScheme.primary.copy(alpha=0.5f) else Color.Transparent,
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
                    searchUrl = null
                    preloadedUrls.clear()
                    initializedEngines.clear() // Reset smart loading
                    // Do NOT destroy webviews here to keep them cached if possible? 
                    // PROBABLY safer to destroy to avoid stale state from previous searches.
                    webViews.values.forEach { it.destroy() }
                    webViews.clear()
                }

                LaunchedEffect(selectedBitmap, hostedImageUrl) {
                    if (selectedBitmap != null) {
                        isLoading = true
                        
                        // 1. Google Lens Only Mode Check
                        if (uiPreferences.isUseGoogleLensOnly()) {
                            // Save to cache and launch Lens
                            val path = ImageUtils.saveBitmap(context, selectedBitmap!!)
                            val uri = android.net.Uri.fromFile(java.io.File(path))
                            
                            // Prepare content URI for Lens (using existing FileProvider logic in helper)
                            val success = searchWithGoogleLens(uri, context)
                            
                            if (success) {
                                // Close the overlay since Lens is taking over
                                onClose()
                                return@LaunchedEffect
                            } else {
                                // Fallback to multi-search if Lens failed
                                android.util.Log.e("CircleToSearch", "Google Lens launch failed, falling back to multi-search")
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
                                val url = if (engine.isDirectUpload) {
                                     when (engine) {
                                        SearchEngine.Perplexity -> ImageSearchUploader.getPerplexityUrl(hostedImageUrl!!)
                                        SearchEngine.ChatGPT -> ImageSearchUploader.getChatGPTUrl(hostedImageUrl!!)
                                        else -> null
                                    }
                                } else {
                                     when (engine) {
                                        SearchEngine.Google -> ImageSearchUploader.getGoogleLensUrl(hostedImageUrl!!)
                                        SearchEngine.Bing -> ImageSearchUploader.getBingUrl(hostedImageUrl!!)
                                        SearchEngine.Yandex -> ImageSearchUploader.getYandexUrl(hostedImageUrl!!)
                                        SearchEngine.TinEye -> ImageSearchUploader.getTinEyeUrl(hostedImageUrl!!)
                                        else -> null
                                    }
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

                Box(modifier = Modifier.fillMaxSize()) {
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
                            webViews.values.forEach { it.destroy() }
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
                                            
                                            val swipeRefresh = SwipeRefreshLayout(ctx).apply {
                                                layoutParams = ViewGroup.LayoutParams(
                                                    ViewGroup.LayoutParams.MATCH_PARENT,
                                                    ViewGroup.LayoutParams.MATCH_PARENT
                                                )
                                            }
                                            swipeRefresh.addView(v)
                                            swipeRefresh.setOnRefreshListener {
                                                v.reload()
                                                swipeRefresh.isRefreshing = false
                                            }
                                            swipeRefresh
                                        } else {
                                            val swipeRefresh = SwipeRefreshLayout(ctx).apply {
                                                layoutParams = ViewGroup.LayoutParams(
                                                    ViewGroup.LayoutParams.MATCH_PARENT,
                                                    ViewGroup.LayoutParams.MATCH_PARENT
                                                )
                                            }
                                            val webView = createWebView(ctx, engine)
                                            // Apply current settings
                                             if (isDesktop(engine)) {
                                                 webView.settings.userAgentString = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                                             }
                                            
                                            webViews[engine] = webView
                                            webView.loadUrl(url)
                                            
                                            swipeRefresh.addView(webView)
                                            swipeRefresh.setOnRefreshListener {
                                                webView.reload()
                                                swipeRefresh.isRefreshing = false
                                            }
                                            swipeRefresh
                                        }
                                    },
                                    update = { swipeRefresh ->
                                        var webView: WebView? = null
                                        for (i in 0 until swipeRefresh.childCount) {
                                            val child = swipeRefresh.getChildAt(i)
                                            if (child is WebView) {
                                                webView = child
                                                break
                                            }
                                        }
                                        
                                        if (webView != null) {
                                            if (webView.url != url && url != webView.originalUrl) {
                                                webView.loadUrl(url)
                                            }
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
                .background(Color.Black)
                .background(Color.Black)
        ) {
            // Friendly Message Overlay (Top Center)
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

            // 1. Screenshot Layer
            if (screenshot != null) {
                Box(
                    modifier = Modifier.fillMaxSize()
                ) {
                    Image(
                        bitmap = screenshot.asImageBitmap(),
                        contentDescription = "Screenshot",
                        contentScale = ContentScale.Crop,
                        modifier = Modifier.fillMaxSize()
                    )
                    
                    // Tint Overlay
                    Box(
                        modifier = Modifier
                            .fillMaxSize()
                            .background(
                                brush = Brush.verticalGradient(
                                    colors = OverlayGradientColors.map { it.copy(alpha = 0.3f) }
                                )
                            )
                    )
                }
            }

            // 2. Gradient Border Layer (Overlaying screenshot, clipped to rounded corners)
            if (showGradientBorder) {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .border(
                            width = 8.dp,
                            brush = Brush.verticalGradient(colors = OverlayGradientColors),
                            shape = RoundedCornerShape(24.dp) // Rounded corners for device
                        )
                        .clip(RoundedCornerShape(24.dp))
                )
            }

            // 3. Drawing Canvas (Interactive Layer)
            Canvas(
                modifier = Modifier
                    .fillMaxSize()
                    .pointerInput(Unit) {
                        detectDragGestures(
                            onDragStart = { offset ->
                                // Clear previous state
                                currentPathPoints.clear()
                                currentPathPoints.add(offset)
                                selectionRect = null
                                scope.launch { selectionAnim.snapTo(0f) }
                            },
                            onDrag = { change, _ ->
                                val offset = change.position
                                currentPathPoints.add(offset)
                            },
                            onDragEnd = {
                                if (currentPathPoints.isNotEmpty()) {
                                    var minX = Float.MAX_VALUE
                                    var minY = Float.MAX_VALUE
                                    var maxX = Float.MIN_VALUE
                                    var maxY = Float.MIN_VALUE

                                    currentPathPoints.forEach { p ->
                                        minX = min(minX, p.x)
                                        minY = min(minY, p.y)
                                        maxX = max(maxX, p.x)
                                        maxY = max(maxY, p.y)
                                    }
                                    
                                    val rect = Rect(
                                        minX.toInt(),
                                        minY.toInt(),
                                        maxX.toInt(),
                                        maxY.toInt()
                                    )
                                    
                                    selectionRect = rect
                                    // Clear points to remove the drawn line and show the lens rect
                                    currentPathPoints.clear() 
                                    
                                    scope.launch {
                                        selectionAnim.animateTo(
                                            targetValue = 1f,
                                            animationSpec = tween(600)
                                        )
                                        android.util.Log.d("CircleToSearch", "Selection rect: ${rect.left},${rect.top},${rect.right},${rect.bottom}")
                                        selectedBitmap = ImageUtils.cropBitmap(screenshot!!, rect)
                                        android.util.Log.d("CircleToSearch", "Cropped bitmap size: ${selectedBitmap!!.width}x${selectedBitmap!!.height}")
                                        isSearching = true
                                        // Sheet expands automatically in LaunchedEffect
                                    }
                                }
                            }
                        )
                    }
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
                    val cornerRadius = 64f // Increased radius for rounder look
                    val armLength = min(width, height) * 0.2f // Length of the straight part

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
                    
                    // Draw Brackets with Glow
                    listOf(tlPath, trPath, brPath, blPath).forEach { p ->
                        drawPath(p, Color.White, style = bracketStroke, alpha = bracketAlpha)
                        drawPath(p, Brush.linearGradient(OverlayGradientColors), style = Stroke(width = 20f, cap = StrokeCap.Round), alpha = bracketAlpha * 0.5f)
                    }
                    
                    // Optional: Flash effect inside
                     drawRoundRect(
                        color = Color.White,
                        topLeft = Offset(left, top),
                        size = Size(width, height),
                        cornerRadius = CornerRadius(32f),
                        style = Stroke(width = 4f),
                        alpha = (1f - progress) * 0.5f
                    )
                }
            }

            // 4. Header (Top)
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .statusBarsPadding()
                    .padding(top = 16.dp, start = 16.dp, end = 16.dp)
                    .align(Alignment.TopCenter),
                verticalAlignment = Alignment.CenterVertically
            ) {
                IconButton(
                    onClick = onClose,
                    modifier = Modifier
                        .background(Color.Gray.copy(alpha = 0.5f), CircleShape)
                        .size(40.dp)
                ) {
                    Icon(Icons.Default.Close, contentDescription = "Close", tint = Color.White)
                }
                Spacer(modifier = Modifier.weight(1f))
                Text(
                    text = selectedEngine.name,
                    style = MaterialTheme.typography.headlineMedium.copy(
                        fontWeight = FontWeight.Bold,
                        color = Color.White
                    )
                )
                Spacer(modifier = Modifier.weight(1f))
                
                // Action Button (Menu)
                Box(
                    modifier = Modifier
                        .background(Color.Gray.copy(alpha = 0.5f), CircleShape)
                        .size(40.dp),
                    contentAlignment = Alignment.Center
                ) {
                    var showMenu by remember { mutableStateOf(false) }
                    IconButton(onClick = { showMenu = true }) {
                        Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = Color.White)
                    }
                
                    androidx.compose.material3.DropdownMenu(
                        expanded = showMenu,
                        onDismissRequest = { showMenu = false }
                    ) {
                        val isDesktop = isDesktop(selectedEngine)
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text(if (isDesktop) "Mobile Mode" else "Desktop Mode") },
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
                            text = { Text(if (isDarkMode) "Light Mode" else "Dark Mode") },
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
                            text = { Text(if (showGradientBorder) "Hide Border" else "Show Border") },
                            leadingIcon = {
                                Icon(Icons.Default.BorderOuter, contentDescription = null)
                            },
                            onClick = { 
                                showGradientBorder = !showGradientBorder 
                                showMenu = false
                            }
                        )
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text("Refresh") },
                            leadingIcon = {
                                Icon(Icons.Default.Refresh, contentDescription = null)
                            },
                            onClick = { 
                                webViews[selectedEngine]?.reload()
                                showMenu = false
                            }
                        )
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text("Copy URL") },
                            leadingIcon = {
                                Icon(Icons.Default.ContentCopy, contentDescription = null)
                            },
                            onClick = {
                                if (searchUrl != null) {
                                    val clipboard = context.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager
                                    val clip = android.content.ClipData.newPlainText("Search URL", searchUrl)
                                    clipboard.setPrimaryClip(clip)
                                }
                                showMenu = false
                            }
                        )
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text("Open in Browser") },
                            leadingIcon = {
                                Icon(Icons.Default.OpenInNew, contentDescription = null)
                            },
                            onClick = {
                                val currentUrl = webViews[selectedEngine]?.url ?: searchUrl
                                if (currentUrl != null) {
                                    try {
                                        val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(currentUrl))
                                        context.startActivity(intent)
                                    } catch (e: Exception) {
                                        android.util.Log.e("CircleToSearch", "Failed to open browser", e)
                                    }
                                }
                                showMenu = false
                            }
                        )
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text("Settings") },
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
            }      // 5. Search Bar / Pill (Bottom Fixed) - Clickable to open sheet
            Box(
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .navigationBarsPadding()
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 24.dp)
                    .shadow(8.dp, CircleShape)
                    .background(Color(0xFF1F1F1F), CircleShape)
                    .height(64.dp)
                    .padding(horizontal = 20.dp)
                    .pointerInput(Unit) {
                        detectTapGestures {
                            scope.launch { 
                                scaffoldState.bottomSheetState.expand()
                            }
                        }
                    }
            ) {
                Row(
                    modifier = Modifier.align(Alignment.CenterStart),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    if (selectedBitmap != null) {
                        Image(
                            bitmap = selectedBitmap!!.asImageBitmap(),
                            contentDescription = "Selected",
                            contentScale = ContentScale.Crop,
                            modifier = Modifier
                                .size(40.dp)
                                .clip(RoundedCornerShape(12.dp))
                                .border(1.dp, Color.Gray, RoundedCornerShape(12.dp))
                        )
                    } else {
                        // G Logo
                        Row {
                            Text("G", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFF4285F4)))
                            Text("o", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFFEA4335)))
                            Text("o", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFFFBBC05)))
                            Text("g", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFF4285F4)))
                            Text("l", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFF34A853)))
                            Text("e", style = MaterialTheme.typography.headlineMedium.copy(fontWeight = FontWeight.Bold, color = Color(0xFFEA4335)))
                        }
                    }
                }
                
                Row(
                    modifier = Modifier.align(Alignment.CenterEnd),
                    verticalAlignment = Alignment.CenterVertically
                ) {

                    IconButton(onClick = {
                        showSupportSheet = true
                    }, modifier = Modifier.size(32.dp)) {
                        Icon(
                            painter = painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                            contentDescription = "Donate",
                            tint = Color.White,
                            modifier = Modifier.size(22.dp)
                        )
                    }
                    Spacer(modifier = Modifier.width(3.dp))
                    IconButton(onClick = {
                        val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("https://github.com/aks-labs"))
                        context.startActivity(intent)
                    }, modifier = Modifier.size(32.dp)) {
                        Icon(
                            painter = painterResource(id = com.akslabs.circletosearch.R.drawable.github),
                            contentDescription = "Github",
                            tint = Color.White,
                            modifier = Modifier.size(23.dp)
                        )
                    }

                    Spacer(modifier = Modifier.width(3.dp))
                    IconButton(onClick = {
                        val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse("https://t.me/akslabs"))
                        context.startActivity(intent)
                    }, modifier = Modifier.size(32.dp)) {
                        Icon(
                            painter = painterResource(id = com.akslabs.circletosearch.R.drawable.telegram),
                            contentDescription = "Telegram",
                            tint = Color.White,
                            modifier = Modifier.size(23.dp)
                        )
                    }
                }
            }

        
        if (showSupportSheet) {
            com.akslabs.circletosearch.SupportSheet(
                sheetState = supportSheetState,
                onDismissRequest = {
                    scope.launch {
                        supportSheetState.hide()
                    }.invokeOnCompletion {
                        if (!supportSheetState.isVisible) {
                            showSupportSheet = false
                        }
                    }
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

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@androidx.compose.ui.tooling.preview.Preview
@androidx.compose.runtime.Composable
fun ContainedLoadingIndicatorSample() {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        androidx.compose.material3.ContainedLoadingIndicator()
    }
}
