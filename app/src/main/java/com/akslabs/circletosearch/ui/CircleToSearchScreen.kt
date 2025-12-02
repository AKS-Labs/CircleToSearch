package com.akslabs.circletosearch.ui

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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.MusicNote
import androidx.compose.material.icons.filled.Translate
import androidx.compose.material3.BottomSheetDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.ui.theme.OverlayGradientColors
import com.akslabs.circletosearch.utils.ImageSearchUploader
import com.akslabs.circletosearch.utils.ImageUtils
import kotlinx.coroutines.launch
import java.io.ByteArrayOutputStream
import androidx.activity.compose.BackHandler
import androidx.swiperefreshlayout.widget.SwipeRefreshLayout
import androidx.webkit.WebSettingsCompat
import androidx.webkit.WebViewFeature
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
    
    // Search State
    var selectedEngine by remember { mutableStateOf<SearchEngine>(SearchEngine.Google) }
    var searchUrl by remember { mutableStateOf<String?>(null) }
    var hostedImageUrl by remember { mutableStateOf<String?>(null) }
    var isLoading by remember { mutableStateOf(false) }
    var isDesktopMode by remember { mutableStateOf(false) }
    var isDarkMode by remember { mutableStateOf(false) }
    
    // Cache for preloaded URLs to avoid re-uploading/re-generating
    val preloadedUrls = remember { mutableMapOf<SearchEngine, String>() }
    
    // WebView Cache
    val webViews = remember { mutableMapOf<SearchEngine, WebView>() }
    
    // Bottom Sheet State
    val sheetState = rememberModalBottomSheetState(
        skipPartiallyExpanded = false // Allow half-expanded state
    )
    var showBottomSheet by remember { mutableStateOf(false) }

    // Drawing State
    val currentPathPoints = remember { mutableStateListOf<Offset>() }
    
    // Selection State
    var selectedBitmap by remember { mutableStateOf<Bitmap?>(null) }
    var isSearching by remember { mutableStateOf(false) }
    var selectionRect by remember { mutableStateOf<Rect?>(null) }
    val selectionAnim = remember { androidx.compose.animation.core.Animatable(0f) }

    val searchEngines = SearchEngine.values()

    // Gradient Animation
    val alphaAnim by animateFloatAsState(
        targetValue = if (screenshot != null) 1f else 0f,
        animationSpec = tween(1000), label = "alpha"
    )

    // Root Box
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
    ) {
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
                                    showBottomSheet = true // Show the bottom sheet
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
                .padding(top = 48.dp, start = 16.dp, end = 16.dp)
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
            
            // Menu
            Box {
                var showMenu by remember { mutableStateOf(false) }
                IconButton(
                    onClick = { showMenu = true },
                    modifier = Modifier
                        .background(Color.Gray.copy(alpha = 0.5f), CircleShape)
                        .size(40.dp)
                ) {
                    Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = Color.White)
                }
                
                androidx.compose.material3.DropdownMenu(
                    expanded = showMenu,
                    onDismissRequest = { showMenu = false }
                ) {
                    androidx.compose.material3.DropdownMenuItem(
                        text = { Text(if (isDesktopMode) "Mobile Mode" else "Desktop Mode") },
                        onClick = { 
                            isDesktopMode = !isDesktopMode 
                            showMenu = false
                        }
                    )
                    androidx.compose.material3.DropdownMenuItem(
                        text = { Text(if (isDarkMode) "Light Mode" else "Dark Mode") },
                        onClick = { 
                            isDarkMode = !isDarkMode 
                            showMenu = false
                        }
                    )
                    androidx.compose.material3.DropdownMenuItem(
                        text = { Text("Copy URL") },
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
                        onClick = {
                            if (searchUrl != null) {
                                try {
                                    val intent = android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(searchUrl))
                                    context.startActivity(intent)
                                } catch (e: Exception) {
                                    android.util.Log.e("CircleToSearch", "Failed to open browser", e)
                                }
                            }
                            showMenu = false
                        }
                    )
                }
            }
        }

        // 5. Search Bar / Pill (Bottom Fixed)
        Box(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 24.dp)
                .shadow(8.dp, CircleShape)
                .background(Color(0xFF1F1F1F), CircleShape)
                .height(64.dp)
                .padding(horizontal = 20.dp)
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
                Icon(Icons.Default.Mic, "Mic", tint = Color.White)
                Spacer(modifier = Modifier.width(24.dp))
                Icon(Icons.Default.MusicNote, "Music", tint = Color.White)
                Spacer(modifier = Modifier.width(24.dp))
                Icon(Icons.Default.Translate, "Translate", tint = Color.White)
            }
        }

        // 6. Modal Bottom Sheet
        if (showBottomSheet) {
            ModalBottomSheet(
                onDismissRequest = { showBottomSheet = false },
                sheetState = sheetState,
                dragHandle = { BottomSheetDefaults.DragHandle() },
                containerColor = Color.Transparent,
                scrimColor = Color.Black.copy(alpha = 0.3f)
            ) {
                // Bottom Sheet Content (Results)
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(800.dp)
                        .background(MaterialTheme.colorScheme.surface, RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                ) {
                    Spacer(modifier = Modifier.height(16.dp))
                    
                    // Tabs
                    ScrollableTabRow(
                        selectedTabIndex = searchEngines.indexOf(selectedEngine),
                        edgePadding = 16.dp,
                        containerColor = Color.Transparent,
                        contentColor = MaterialTheme.colorScheme.onSurface,
                        indicator = {} 
                    ) {
                        searchEngines.forEach { engine ->
                            val selected = selectedEngine == engine
                            Tab(
                                selected = selected,
                                onClick = { selectedEngine = engine },
                                text = {
                                    Text(
                                        engine.name,
                                        style = MaterialTheme.typography.labelLarge,
                                        modifier = Modifier
                                            .background(
                                                if (selected) MaterialTheme.colorScheme.secondaryContainer else MaterialTheme.colorScheme.surface,
                                                RoundedCornerShape(16.dp)
                                            )
                                            .padding(horizontal = 16.dp, vertical = 8.dp),
                                        color = if (selected) MaterialTheme.colorScheme.onSecondaryContainer else MaterialTheme.colorScheme.onSurface
                                    )
                                }
                            )
                        }
                    }

                    // Reset hosted URL when bitmap changes
                    LaunchedEffect(selectedBitmap) {
                        hostedImageUrl = null
                        searchUrl = null
                    }

                    // Preload Logic
                    LaunchedEffect(selectedBitmap, hostedImageUrl) {
                        if (selectedBitmap != null) {
                            isLoading = true
                            
                            // 1. Upload to host if needed (for URL-based engines)
                            if (hostedImageUrl == null) {
                                val url = ImageSearchUploader.uploadToImageHost(selectedBitmap!!)
                                if (url != null) {
                                    hostedImageUrl = url
                                } else {
                                    isLoading = false
                                    return@LaunchedEffect
                                }
                            }

                            // 2. Preload ALL engines
                            searchEngines.forEach { engine ->
                                if (!preloadedUrls.containsKey(engine)) {
                                    val url = if (engine.isDirectUpload) {
                                        when (engine) {
                                            SearchEngine.Baidu -> ImageSearchUploader.getBaiduUrl(hostedImageUrl!!)
                                            SearchEngine.Lenso -> ImageSearchUploader.getLensoUrl(hostedImageUrl!!)
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
                                    
                                    if (url != null) {
                                        preloadedUrls[engine] = url
                                    }
                                }
                            }
                            
                            // Set current searchUrl
                            searchUrl = preloadedUrls[selectedEngine]
                            isLoading = false
                        }
                    }
                    
                    // Update searchUrl when engine changes
                    LaunchedEffect(selectedEngine, preloadedUrls) {
                        if (preloadedUrls.containsKey(selectedEngine)) {
                            searchUrl = preloadedUrls[selectedEngine]
                        }
                    }

                    // Helper to create and configure WebView
                    fun createWebView(ctx: android.content.Context): WebView {
                        return WebView(ctx).apply {
                            layoutParams = ViewGroup.LayoutParams(
                                ViewGroup.LayoutParams.MATCH_PARENT,
                                ViewGroup.LayoutParams.MATCH_PARENT
                            )
                            settings.apply {
                                javaScriptEnabled = true
                                domStorageEnabled = true
                                databaseEnabled = true
                                useWideViewPort = true
                                loadWithOverviewMode = true
                                setSupportZoom(true)
                                builtInZoomControls = true
                                displayZoomControls = false
                                
                                // Enhanced settings for "Normal" browser behavior
                                allowFileAccess = true
                                allowContentAccess = true
                                allowFileAccessFromFileURLs = true
                                allowUniversalAccessFromFileURLs = true
                                mixedContentMode = WebSettings.MIXED_CONTENT_ALWAYS_ALLOW
                                
                                userAgentString = if (isDesktopMode) {
                                    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                                } else {
                                    "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
                                }
                                
                                // Dark Mode
                                if (WebViewFeature.isFeatureSupported(WebViewFeature.FORCE_DARK)) {
                                    WebSettingsCompat.setForceDark(this, if (isDarkMode) WebSettingsCompat.FORCE_DARK_ON else WebSettingsCompat.FORCE_DARK_OFF)
                                }
                            }

                            // Enable Third-Party Cookies
                            android.webkit.CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)

                            webViewClient = object : WebViewClient() {
                                override fun shouldOverrideUrlLoading(view: WebView?, request: android.webkit.WebResourceRequest?): Boolean {
                                    return false // Load in WebView
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

                    Box(modifier = Modifier.fillMaxSize()) {
                        // Show loading only if the SELECTED engine isn't ready
                        if (isLoading || preloadedUrls[selectedEngine] == null) {
                            Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                                androidx.compose.material3.CircularProgressIndicator()
                            }
                        }

                        // Back Handler
                        BackHandler(enabled = true) {
                            val currentWebView = webViews[selectedEngine]
                            if (currentWebView != null && currentWebView.canGoBack()) {
                                currentWebView.goBack()
                            } else {
                                // Dismiss sheet
                                showBottomSheet = false
                            }
                        }
                        
                        // Dynamic Settings Update
                        LaunchedEffect(isDesktopMode, isDarkMode) {
                            val newUserAgent = if (isDesktopMode) {
                                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
                            } else {
                                "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36"
                            }
                            webViews.values.forEach { wv ->
                                if (wv.settings.userAgentString != newUserAgent) {
                                    wv.settings.userAgentString = newUserAgent
                                }
                                if (WebViewFeature.isFeatureSupported(WebViewFeature.FORCE_DARK)) {
                                    WebSettingsCompat.setForceDark(wv.settings, if (isDarkMode) WebSettingsCompat.FORCE_DARK_ON else WebSettingsCompat.FORCE_DARK_OFF)
                                }
                            }
                        }
                        
                        // Cleanup
                        DisposableEffect(Unit) {
                            onDispose {
                                webViews.values.forEach { it.destroy() }
                                webViews.clear()
                            }
                        }

                        // Render ALL WebViews that have URLs
                        searchEngines.forEach { engine ->
                            val url = preloadedUrls[engine]
                            if (url != null) {
                                val isSelected = (engine == selectedEngine)
                                
                                androidx.compose.runtime.key(engine) {
                                    AndroidView(
                                        factory = { ctx ->
                                            val swipeRefresh = SwipeRefreshLayout(ctx).apply {
                                                layoutParams = ViewGroup.LayoutParams(
                                                    ViewGroup.LayoutParams.MATCH_PARENT,
                                                    ViewGroup.LayoutParams.MATCH_PARENT
                                                )
                                            }
                                            
                                            val webView = createWebView(ctx)
                                            webViews[engine] = webView
                                            webView.loadUrl(url)
                                            
                                            swipeRefresh.addView(webView)
                                            
                                            swipeRefresh.setOnRefreshListener {
                                                webView.reload()
                                                swipeRefresh.isRefreshing = false
                                            }
                                            
                                            swipeRefresh
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
                                                // Update URL if changed (e.g. new search)
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
        }
    }
}
