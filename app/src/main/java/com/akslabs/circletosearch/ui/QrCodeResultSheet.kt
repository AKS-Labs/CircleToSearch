package com.akslabs.circletosearch.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.ContactsContract
import android.widget.Toast
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ChevronLeft
import androidx.compose.material.icons.filled.ChevronRight
import androidx.compose.material.icons.filled.Phone
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.ShoppingBag
import androidx.compose.material.icons.filled.Wifi
import androidx.compose.material.icons.filled.LocationOn
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.utils.QrResult
import com.akslabs.circletosearch.utils.QrResultWithBounds
import com.akslabs.circletosearch.utils.QrScanner
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private fun copyToClipboard(context: Context, label: String, text: String) {
    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    clipboard.setPrimaryClip(ClipData.newPlainText(label, text))
    Toast.makeText(context, "Copied", Toast.LENGTH_SHORT).show()
}

private fun openUrl(context: Context, url: String) {
    var finalUrl = url
    if (!finalUrl.startsWith("http://") && !finalUrl.startsWith("https://")) {
        finalUrl = "https://" + finalUrl
    }
    try {
        context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(finalUrl)).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        })
    } catch (e: Exception) {
        Toast.makeText(context, "Cannot open link", Toast.LENGTH_SHORT).show()
    }
}

fun qrResultShortLabel(result: QrResult): String = when (result) {
    is QrResult.Url       -> result.displayUrl.take(25)
    is QrResult.WiFi      -> result.ssid
    is QrResult.Phone     -> result.number
    is QrResult.Product   -> result.barcode
    is QrResult.VCard     -> result.name ?: "Contact"
    is QrResult.GeoPoint  -> "%.2f, %.2f".format(result.lat, result.lng)
    is QrResult.PlainText -> result.text.take(25)
}

@Composable
fun QrCodeResultSheet(
    context: Context,
    bitmap: android.graphics.Bitmap?,
    onDismiss: () -> Unit,
    initialResults: List<QrResultWithBounds> = emptyList(),
    initialPage: Int = 0
) {
    val scope = rememberCoroutineScope()
    var results by remember { mutableStateOf<List<QrResultWithBounds>>(initialResults) }
    var isScanning by remember { mutableStateOf(initialResults.isEmpty()) }
    var notFound by remember { mutableStateOf(false) }
    
    val pagerState = rememberPagerState(
        initialPage = initialPage.coerceIn(0, (results.size - 1).coerceAtLeast(0)),
        pageCount = { results.size }
    )
    
    // Ensure pager stays in sync if results change (though usually they don't after opening)
    LaunchedEffect(initialPage, results.size) {
        if (results.isNotEmpty()) {
            pagerState.scrollToPage(initialPage.coerceIn(0, results.size - 1))
        }
    }
    
    val infiniteTransition = rememberInfiniteTransition(label = "scanline")
    val scanlineY by infiniteTransition.animateFloat(
        initialValue = 0f, targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1500, easing = LinearEasing), RepeatMode.Reverse),
        label = "scanlineY"
    )

    LaunchedEffect(bitmap) {
        if (initialResults.isNotEmpty()) {
            results = initialResults
            isScanning = false
            return@LaunchedEffect
        }
        if (bitmap == null) { isScanning = false; notFound = true; return@LaunchedEffect }
        isScanning = true; notFound = false
        val found = withContext(Dispatchers.Default) { QrScanner.scanBitmapAll(bitmap) }
        isScanning = false
        if (found.isEmpty()) notFound = true else { results = found }
    }

    Surface(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
        shape = RoundedCornerShape(28.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
        tonalElevation = 8.dp, shadowElevation = 12.dp
    ) {
        Column(modifier = Modifier.padding(vertical = 20.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            // Header Row
            Row(modifier = Modifier.padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Default.QrCode, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(22.dp))
                Spacer(Modifier.width(10.dp))
                val titleText = when {
                    isScanning -> "Scanning…"
                    notFound -> "No QR Found"
                    results.size > 1 -> "Result ${pagerState.currentPage + 1} of ${results.size}"
                    else -> "QR / Barcode Detected"
                }
                Text(titleText, style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold), modifier = Modifier.weight(1f))
                
                if (results.size > 1) {
                    IconButton(onClick = { scope.launch { pagerState.animateScrollToPage((pagerState.currentPage - 1 + results.size) % results.size) } }, modifier = Modifier.size(36.dp)) {
                        Icon(Icons.Default.ChevronLeft, null)
                    }
                    IconButton(onClick = { scope.launch { pagerState.animateScrollToPage((pagerState.currentPage + 1) % results.size) } }, modifier = Modifier.size(36.dp)) {
                        Icon(Icons.Default.ChevronRight, null)
                    }
                }
            }

            Spacer(Modifier.height(16.dp))

            if (isScanning) {
                ScanningIndicator(scanlineY)
            } else if (notFound) {
                NotFoundContent()
            } else if (results.isNotEmpty()) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    HorizontalPager(
                        state = pagerState,
                        modifier = Modifier.fillMaxWidth(),
                        contentPadding = PaddingValues(horizontal = 20.dp),
                        pageSpacing = 16.dp
                    ) { page ->
                        QrResultContent(context, results[page].result)
                    }
                    
                    if (results.size > 1) {
                        Spacer(Modifier.height(16.dp))
                        Row(
                            Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.Center,
                            verticalAlignment = Alignment.CenterVertically
                        ) {
                            repeat(results.size) { i ->
                                val active = pagerState.currentPage == i
                                Box(
                                    modifier = Modifier
                                        .padding(horizontal = 4.dp)
                                        .size(if (active) 8.dp else 6.dp)
                                        .clip(CircleShape)
                                        .background(if (active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.3f))
                                )
                            }
                        }
                        Text(
                            "Swipe for more results",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.6f),
                            modifier = Modifier.padding(top = 8.dp)
                        )
                    }
                }
            }
        }
    }
}

@Composable
fun ScanningIndicator(scanlineY: Float) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(
            modifier = Modifier.size(90.dp).background(MaterialTheme.colorScheme.surfaceContainerHighest, RoundedCornerShape(20.dp)),
            contentAlignment = Alignment.Center
        ) {
            CircularProgressIndicator(modifier = Modifier.size(44.dp), strokeWidth = 3.dp, color = MaterialTheme.colorScheme.primary)
            Box(modifier = Modifier.fillMaxWidth().height(3.dp)
                .offset(y = (90.dp * scanlineY) - 45.dp)
                .background(Brush.horizontalGradient(listOf(Color.Transparent, MaterialTheme.colorScheme.primary, Color.Transparent)))
            )
        }
        Spacer(Modifier.height(16.dp))
        Text("Scanning screenshot…", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
fun NotFoundContent() {
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(vertical = 10.dp)) {
        Text("🔎", fontSize = 48.sp)
        Spacer(Modifier.height(12.dp))
        Text("No QR code detected", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        Text("Ensure the code is clearly visible", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
fun QrResultContent(context: Context, result: QrResult) {
    Box(modifier = Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        when (result) {
            is QrResult.Url       -> UrlResult(context, result)
            is QrResult.WiFi      -> WifiResult(context, result)
            is QrResult.Phone     -> PhoneResult(context, result)
            is QrResult.Product   -> ProductResult(context, result)
            is QrResult.VCard     -> VCardResult(context, result)
            is QrResult.GeoPoint  -> GeoResult(context, result)
            is QrResult.PlainText -> PlainTextResult(context, result)
        }
    }
}

@Composable
private fun UrlResult(context: Context, result: QrResult.Url) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text("🔗", fontSize = 42.sp)
        Spacer(Modifier.height(8.dp))
        Text(result.displayUrl.substringBefore("/"), style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.ExtraBold))
        Text(
            text = result.displayUrl,
            style = MaterialTheme.typography.bodyLarge.copy(
                color = Color(0xFF1A73E8), 
                textDecoration = TextDecoration.Underline,
                fontWeight = FontWeight.Medium
            ),
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.clickable { openUrl(context, result.url) }.padding(4.dp)
        )
        Spacer(Modifier.height(20.dp))
        ActionRow { PrimaryAction("Open Browser") { openUrl(context, result.url) }; SecondaryAction("Copy Link") { copyToClipboard(context, "URL", result.url) } }
    }
}

@Composable
private fun WifiResult(context: Context, result: QrResult.WiFi) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(Icons.Default.Wifi, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(42.dp))
        Spacer(Modifier.height(10.dp))
        Text(result.ssid, style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold))
        Text("${result.security}${if (result.password != null) " · Password: ${result.password}" else ""}", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(20.dp))
        ActionRow { if (result.password != null) PrimaryAction("Copy Password") { copyToClipboard(context, "WiFi Password", result.password) } }
    }
}

@Composable
private fun PhoneResult(context: Context, result: QrResult.Phone) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(Icons.Default.Phone, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(42.dp))
        Spacer(Modifier.height(10.dp))
        Text(result.number, style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold))
        Spacer(Modifier.height(20.dp))
        ActionRow {
            PrimaryAction("Call Now") { openUrl(context, "tel:${result.number}") }
            SecondaryAction("Send SMS") { openUrl(context, "sms:${result.number}") }
            SecondaryAction("Copy") { copyToClipboard(context, "Phone", result.number) }
        }
    }
}

@Composable
private fun ProductResult(context: Context, result: QrResult.Product) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(Icons.Default.ShoppingBag, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(42.dp))
        Spacer(Modifier.height(10.dp))
        Text(result.barcode, style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold))
        Spacer(Modifier.height(20.dp))
        ActionRow {
            PrimaryAction("Amazon Search") { openUrl(context, "https://www.amazon.com/s?k=${result.barcode}") }
            SecondaryAction("Google") { openUrl(context, "https://www.google.com/search?q=${result.barcode}") }
            SecondaryAction("Copy") { copyToClipboard(context, "Barcode", result.barcode) }
        }
    }
}

@Composable
private fun VCardResult(context: Context, result: QrResult.VCard) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text("👤", fontSize = 42.sp)
        if (result.name != null) Text(result.name, style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Bold))
        val sub = listOfNotNull(result.phone, result.email).joinToString(" · ")
        if (sub.isNotEmpty()) Text(sub, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(20.dp))
        ActionRow {
            PrimaryAction("Save Contact") {
                context.startActivity(Intent(Intent.ACTION_INSERT).apply {
                    type = ContactsContract.RawContacts.CONTENT_TYPE
                    result.name?.let { putExtra(ContactsContract.Intents.Insert.NAME, it) }
                    result.phone?.let { putExtra(ContactsContract.Intents.Insert.PHONE, it) }
                    result.email?.let { putExtra(ContactsContract.Intents.Insert.EMAIL, it) }
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                })
            }
            SecondaryAction("Copy Raw") { copyToClipboard(context, "Contact", result.raw) }
        }
    }
}

@Composable
private fun GeoResult(context: Context, result: QrResult.GeoPoint) {
    val coord = "%.4f, %.4f".format(result.lat, result.lng)
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Icon(Icons.Default.LocationOn, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(42.dp))
        Spacer(Modifier.height(10.dp))
        Text(coord, style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold))
        Spacer(Modifier.height(20.dp))
        ActionRow {
            PrimaryAction("Open in Maps") { openUrl(context, "geo:${result.lat},${result.lng}?q=${result.lat},${result.lng}") }
            SecondaryAction("Copy") { copyToClipboard(context, "Coordinates", coord) }
        }
    }
}

@Composable
private fun PlainTextResult(context: Context, result: QrResult.PlainText) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text("📝", fontSize = 42.sp)
        Spacer(Modifier.height(8.dp))
        Text(
            text = "\"${result.text}\"", 
            style = MaterialTheme.typography.bodyLarge.copy(lineHeight = 22.sp), 
            maxLines = 5, 
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = 10.dp)
        )
        Spacer(Modifier.height(20.dp))
        ActionRow {
            PrimaryAction("Search") { openUrl(context, "https://www.google.com/search?q=${Uri.encode(result.text)}") }
            SecondaryAction("Copy Text") { copyToClipboard(context, "Text", result.text) }
            SecondaryAction("Translate") { openUrl(context, "https://translate.google.com/?text=${Uri.encode(result.text)}") }
        }
    }
}

@Composable
private fun ActionRow(content: @Composable () -> Unit) {
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.CenterHorizontally), verticalAlignment = Alignment.CenterVertically) { content() }
}

@Composable
private fun PrimaryAction(label: String, onClick: () -> Unit) {
    Button(onClick = onClick, shape = RoundedCornerShape(14.dp), contentPadding = PaddingValues(horizontal = 20.dp, vertical = 10.dp)) { Text(label, style = MaterialTheme.typography.labelLarge) }
}

@Composable
private fun SecondaryAction(label: String, onClick: () -> Unit) {
    FilledTonalButton(onClick = onClick, shape = RoundedCornerShape(14.dp), contentPadding = PaddingValues(horizontal = 20.dp, vertical = 10.dp)) { Text(label, style = MaterialTheme.typography.labelLarge) }
}
