package com.akslabs.circletosearch.ui

import android.graphics.Bitmap
import android.graphics.Rect
import android.webkit.WebView
import android.webkit.WebViewClient
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
import androidx.compose.material3.BottomSheetScaffold
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.rememberBottomSheetScaffoldState
import androidx.compose.material3.rememberStandardBottomSheetState
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.akslabs.circletosearch.data.SearchEngine
import com.akslabs.circletosearch.ui.theme.OverlayGradientColors
import com.akslabs.circletosearch.utils.ImageUtils
import kotlinx.coroutines.launch
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
    val scaffoldState = rememberBottomSheetScaffoldState(
        bottomSheetState = rememberStandardBottomSheetState(
            initialValue = SheetValue.PartiallyExpanded,
            skipHiddenState = true
        )
    )

    // Drawing State
    val paths = remember { mutableStateListOf<Pair<Path, Color>>() }
    var currentPath by remember { mutableStateOf<Path?>(null) }
    var currentPathPoints = remember { mutableListOf<Offset>() }
    
    // Selection State
    var selectedBitmap by remember { mutableStateOf<Bitmap?>(null) }
    var isSearching by remember { mutableStateOf(false) }
    var selectionRect by remember { mutableStateOf<Rect?>(null) }
    val selectionAnim = remember { androidx.compose.animation.core.Animatable(0f) }

    // Search State
    var selectedEngine by remember { mutableStateOf<SearchEngine>(SearchEngine.Google) }
    val searchEngines = SearchEngine.getAll()

    // Gradient Animation
    val alphaAnim by animateFloatAsState(
        targetValue = if (screenshot != null) 1f else 0f,
        animationSpec = tween(1000), label = "alpha"
    )

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black)
            .border(
                width = 8.dp,
                brush = Brush.verticalGradient(colors = OverlayGradientColors),
                shape = RectangleShape
            )
    ) {
        // 1. Screenshot Layer (Background)
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

        // 2. Bottom Sheet (Results)
        BottomSheetScaffold(
            scaffoldState = scaffoldState,
            sheetPeekHeight = 0.dp, // Hidden initially, triggered by selection
            containerColor = Color.Transparent,
            sheetContainerColor = Color.Transparent,
            sheetContent = {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(800.dp)
                        .background(Color(0xFF131314), RoundedCornerShape(topStart = 24.dp, topEnd = 24.dp))
                ) {
                    Spacer(modifier = Modifier.height(16.dp)) // Spacing from top of sheet
                    
                    // Tabs
                    ScrollableTabRow(
                        selectedTabIndex = searchEngines.indexOf(selectedEngine),
                        edgePadding = 16.dp,
                        containerColor = Color.Transparent,
                        contentColor = Color.White,
                        indicator = {} 
                    ) {
                        searchEngines.forEach { engine ->
                            Tab(
                                selected = selectedEngine == engine,
                                onClick = { selectedEngine = engine },
                                text = {
                                    Text(
                                        engine.name,
                                        style = MaterialTheme.typography.labelLarge,
                                        modifier = Modifier
                                            .background(
                                                if (selectedEngine == engine) Color.White else Color.Transparent,
                                                RoundedCornerShape(16.dp)
                                            )
                                            .padding(horizontal = 12.dp, vertical = 6.dp),
                                        color = if (selectedEngine == engine) Color.Black else Color.White
                                    )
                                }
                            )
                        }
                    }

                    // Web View Content
                    AndroidView(
                        factory = { ctx ->
                            WebView(ctx).apply {
                                webViewClient = WebViewClient()
                                settings.javaScriptEnabled = true
                                loadUrl(selectedEngine.queryUrl + "test") 
                            }
                        },
                        update = { view ->
                            view.loadUrl(selectedEngine.queryUrl + "test")
                        },
                        modifier = Modifier.fillMaxSize()
                    )
                }
            }
        ) { paddingValues ->
            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .padding(paddingValues)
            ) {
                // 3. Drawing Canvas (Interactive Layer)
                Canvas(
                    modifier = Modifier
                        .fillMaxSize()
                        .pointerInput(Unit) {
                            detectDragGestures(
                                onDragStart = { offset ->
                                    paths.clear()
                                    currentPath = Path().apply { moveTo(offset.x, offset.y) }
                                    currentPathPoints.clear()
                                    currentPathPoints.add(offset)
                                    selectionRect = null
                                    scope.launch { selectionAnim.snapTo(0f) }
                                },
                                onDrag = { change, _ ->
                                    val offset = change.position
                                    currentPath?.lineTo(offset.x, offset.y)
                                    currentPathPoints.add(offset)
                                },
                                onDragEnd = {
                                    currentPath?.let {
                                        paths.add(it to Color.White)
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
                                            scope.launch {
                                                selectionAnim.animateTo(
                                                    targetValue = 1f,
                                                    animationSpec = tween(600)
                                                )
                                                selectedBitmap = ImageUtils.cropBitmap(screenshot!!, rect)
                                                isSearching = true
                                                scaffoldState.bottomSheetState.expand()
                                            }
                                        }
                                    }
                                    currentPath = null
                                    currentPathPoints = mutableListOf()
                                }
                            )
                        }
                ) {
                    paths.forEach { (path, _) ->
                        drawPath(
                            path = path,
                            brush = Brush.linearGradient(OverlayGradientColors),
                            style = Stroke(width = 25f, cap = StrokeCap.Round, join = StrokeJoin.Round),
                            alpha = 0.6f
                        )
                        drawPath(
                            path = path,
                            color = Color.White,
                            style = Stroke(width = 12f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        )
                    }
                    currentPath?.let {
                        drawPath(
                            path = it,
                            brush = Brush.linearGradient(OverlayGradientColors),
                            style = Stroke(width = 25f, cap = StrokeCap.Round, join = StrokeJoin.Round),
                            alpha = 0.6f
                        )
                        drawPath(
                            path = it,
                            color = Color.White,
                            style = Stroke(width = 12f, cap = StrokeCap.Round, join = StrokeJoin.Round)
                        )
                    }
                    
                    if (selectionRect != null && selectionAnim.value > 0f) {
                        val rect = selectionRect!!
                        val progress = selectionAnim.value
                        val left = rect.left.toFloat()
                        val top = rect.top.toFloat()
                        val right = rect.right.toFloat()
                        val bottom = rect.bottom.toFloat()
                        
                        drawRoundRect(
                            brush = Brush.linearGradient(OverlayGradientColors),
                            topLeft = Offset(left, top),
                            size = androidx.compose.ui.geometry.Size(right - left, bottom - top),
                            cornerRadius = androidx.compose.ui.geometry.CornerRadius(32f * progress),
                            style = Stroke(width = 8f),
                            alpha = progress
                        )
                        
                         drawRoundRect(
                            color = Color.White,
                            topLeft = Offset(left, top),
                            size = androidx.compose.ui.geometry.Size(right - left, bottom - top),
                            cornerRadius = androidx.compose.ui.geometry.CornerRadius(32f * progress),
                            style = Stroke(width = 4f),
                            alpha = (1f - progress) * 0.8f
                        )
                    }
                }
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
                text = "Google",
                style = MaterialTheme.typography.headlineMedium.copy(
                    fontWeight = FontWeight.Bold,
                    color = Color.White
                )
            )
            Spacer(modifier = Modifier.weight(1f))
            IconButton(
                onClick = { /* Menu */ },
                modifier = Modifier
                    .background(Color.Gray.copy(alpha = 0.5f), CircleShape)
                    .size(40.dp)
            ) {
                Icon(Icons.Default.MoreVert, contentDescription = "Menu", tint = Color.White)
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
    }
}
