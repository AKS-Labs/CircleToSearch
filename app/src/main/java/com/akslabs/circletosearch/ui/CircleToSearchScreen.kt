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
import androidx.compose.foundation.layout.offset
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
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.rememberBottomSheetScaffoldState
import androidx.compose.material3.rememberStandardBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.blur
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
            initialValue = SheetValue.Hidden,
            skipHiddenState = false
        )
    )

    // Drawing State
    val paths = remember { mutableStateListOf<Pair<Path, Color>>() }
    var currentPath by remember { mutableStateOf<Path?>(null) }
    var currentPathPoints = remember { mutableListOf<Offset>() }
    
    // Selection State
    var selectedBitmap by remember { mutableStateOf<Bitmap?>(null) }
    var isSearching by remember { mutableStateOf(false) }

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
            .background(
                brush = Brush.verticalGradient(
                    colors = OverlayGradientColors
                )
            )
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

                // 2. Drawing Canvas
                Canvas(
                    modifier = Modifier
                        .fillMaxSize()
                        .pointerInput(Unit) {
                            detectDragGestures(
                                onDragStart = { offset ->
                                    currentPath = Path().apply { moveTo(offset.x, offset.y) }
                                    currentPathPoints.clear()
                                    currentPathPoints.add(offset)
                                },
                                onDrag = { change, _ ->
                                    val offset = change.position
                                    currentPath?.lineTo(offset.x, offset.y)
                                    currentPathPoints.add(offset)
                                },
                                onDragEnd = {
                                    currentPath?.let {
                                        paths.add(it to Color.White)
                                        // Calculate bounding box
                                        if (currentPathPoints.isNotEmpty()) {
                                            isSearching = true
                                            scope.launch {
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
                    paths.forEach { (path, color) ->
                        drawPath(
                            path = path,
                            color = color,
                            style = Stroke(
                                width = 10f,
                                cap = StrokeCap.Round,
                                join = StrokeJoin.Round
                            )
                        )
                        // Glow effect
                        drawPath(
                            path = path,
                            color = Color.Blue.copy(alpha = 0.3f),
                            style = Stroke(
                                width = 20f,
                                cap = StrokeCap.Round,
                                join = StrokeJoin.Round
                            )
                        )
                    }
                    currentPath?.let {
                        drawPath(
                            path = it,
                            color = Color.White,
                            style = Stroke(
                                width = 10f,
                                cap = StrokeCap.Round,
                                join = StrokeJoin.Round
                            )
                        )
                    }
                }
            }
        }

        // 3. Header
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = 48.dp, start = 16.dp, end = 16.dp),
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

        // 4. Bottom Sheet
        BottomSheetScaffold(
            scaffoldState = scaffoldState,
            sheetPeekHeight = 0.dp,
            containerColor = Color.Transparent, // Fix: Make transparent
            sheetContainerColor = Color.White,
            sheetShape = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp),
            sheetContent = {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(600.dp) // Fixed height for demo
                ) {
                    // Search Bar
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(16.dp)
                            .background(Color(0xFFF1F3F4), RoundedCornerShape(32.dp))
                            .height(56.dp)
                            .padding(horizontal = 16.dp)
                    ) {
                        Row(
                            modifier = Modifier.align(Alignment.CenterStart),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            // G Logo Placeholder
                            Box(
                                modifier = Modifier
                                    .size(24.dp)
                                    .background(Color.Red, CircleShape) // Placeholder
                            )
                            Spacer(modifier = Modifier.width(16.dp))
                            Text("Ask about this image", color = Color.Gray)
                        }
                        Row(
                            modifier = Modifier.align(Alignment.CenterEnd),
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            Icon(Icons.Default.Mic, "Mic", tint = Color.Gray)
                            Spacer(modifier = Modifier.width(16.dp))
                            Icon(Icons.Default.MusicNote, "Music", tint = Color.Gray)
                            Spacer(modifier = Modifier.width(16.dp))
                            Icon(Icons.Default.Translate, "Translate", tint = Color.Gray)
                        }
                    }

                    // Tabs
                    ScrollableTabRow(
                        selectedTabIndex = searchEngines.indexOf(selectedEngine),
                        edgePadding = 16.dp,
                        containerColor = Color.White,
                        contentColor = Color.Black,
                        indicator = {} // Hide default indicator for custom look if needed
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
                                                if (selectedEngine == engine) Color.Black else Color.Transparent,
                                                RoundedCornerShape(16.dp)
                                            )
                                            .padding(horizontal = 12.dp, vertical = 6.dp),
                                        color = if (selectedEngine == engine) Color.White else Color.Black
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
                                loadUrl(selectedEngine.queryUrl + "test") // Placeholder query
                            }
                        },
                        update = { view ->
                            view.loadUrl(selectedEngine.queryUrl + "test")
                        },
                        modifier = Modifier.fillMaxSize()
                    )
                }
            }
        ) {
            // Main content behind sheet (transparent)
            Box(modifier = Modifier.fillMaxSize())
        }
    }
}
