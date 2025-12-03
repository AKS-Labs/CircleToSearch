package com.akslabs.circletosearch

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Favorite
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.ui.text.font.FontWeight
import android.widget.Toast
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Toast.makeText(this, "Double tap status bar or use floating bubble to start.", Toast.LENGTH_LONG).show()
        setContent {
            CircleToSearchTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background
                ) {
                    SetupScreen()
                }
            }
        }
    }
}

@Composable
fun SetupScreen() {
    val context = LocalContext.current
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    val showSupportDialog = remember { mutableStateOf(!prefs.getBoolean("support_dialog_dismissed", false)) }
    val dontShowAgain = remember { mutableStateOf(false) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center
    ) {
        Text(
            text = "Circle to Search Setup",
            style = MaterialTheme.typography.headlineMedium
        )
        Spacer(modifier = Modifier.height(24.dp))
        Text(
            text = "To use this app, enable the Accessibility Service.\n\nThis allows the app to:\n• Detect double-tap on status bar\n• Take screenshots\n• Show floating bubble\n\nNo other permissions needed!",
            textAlign = TextAlign.Center
        )
        Spacer(modifier = Modifier.height(32.dp))
        Button(onClick = {
            val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
            context.startActivity(intent)
        }) {
            Text("Enable Accessibility Service")
        }
        Spacer(modifier = Modifier.height(32.dp))
        
        Spacer(modifier = Modifier.height(32.dp))
        
        BubbleSwitch(context)

        // Support Icons Row
        Spacer(modifier = Modifier.height(48.dp))
        SocialLinksRow(context)
    }

    // Support Dialog
    if (showSupportDialog.value) {
        SupportDialog(
            onDismiss = {
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            onDonate = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/sponsors/aks-labs"))
                context.startActivity(intent)
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            dontShowAgain = dontShowAgain
        )
    }
}

@Composable
fun SocialLinksRow(context: android.content.Context) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Text(
            text = "Support & Connect",
            style = MaterialTheme.typography.labelMedium,
            color = Color.Gray,
            modifier = Modifier.padding(bottom = 12.dp)
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.Center,
            verticalAlignment = Alignment.CenterVertically
        ) {
            IconButton(onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs"))
                context.startActivity(intent)
            }, modifier = Modifier.padding(horizontal = 8.dp)) {
                Icon(
                    painter = androidx.compose.ui.res.painterResource(id = com.akslabs.circletosearch.R.drawable.github),
                    contentDescription = "Github",
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(8.dp)
                )
            }
            
            IconButton(onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/sponsors/akslabs"))
                context.startActivity(intent)
            }, modifier = Modifier.padding(horizontal = 8.dp)) {
                Icon(
                    painter = androidx.compose.ui.res.painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                    contentDescription = "Donate",
                    tint = MaterialTheme.colorScheme.primary,
//                    tint = Color(0xFFFF6B6B),
                    modifier = Modifier.padding(8.dp)
                )
            }
            
            IconButton(onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://t.me/akslabs"))
                context.startActivity(intent)
            }, modifier = Modifier.padding(horizontal = 8.dp)) {
                Icon(
                    painter = androidx.compose.ui.res.painterResource(id = com.akslabs.circletosearch.R.drawable.telegram),
                    contentDescription = "Telegram",
                    tint = MaterialTheme.colorScheme.primary,
//                    tint = Color(0xFF0088cc),
                    modifier = Modifier.padding(8.dp).size(23.dp)
                )
            }
        }
    }
}

@Composable
fun SupportDialog(
    onDismiss: () -> Unit,
    onDonate: () -> Unit,
    dontShowAgain: androidx.compose.runtime.MutableState<Boolean>
) {
    Dialog(
        onDismissRequest = onDismiss,
        properties = androidx.compose.ui.window.DialogProperties(
            dismissOnBackPress = true,
            dismissOnClickOutside = false
        )
    ) {
        Surface(
            modifier = Modifier
                .fillMaxWidth(0.85f)
                .padding(16.dp),
            shape = androidx.compose.foundation.shape.RoundedCornerShape(16.dp),
            color = MaterialTheme.colorScheme.surface
        ) {
            Column(
                modifier = Modifier.padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                // Close button
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End
                ) {
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Default.Close, contentDescription = "Close")
                    }
                }

                // Heart icon
                Icon(
                    Icons.Default.Favorite,
                    contentDescription = "Support",
                    tint = Color(0xFFFF6B6B),
                    modifier = Modifier
                        .size(48.dp)
                        .padding(bottom = 16.dp)
                )

                // Title
                Text(
                    text = "Support This Project",
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(bottom = 16.dp)
                )

                // Message
                Text(
                    text = "If my open-source work has ever made your day easier, smoother, or even a little calmer… consider supporting my journey. Your sponsorship keeps my projects alive, my motivation strong, and my chai hot. ☕✨",
                    style = MaterialTheme.typography.bodyMedium,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.padding(bottom = 24.dp),
                    lineHeight = 20.sp
                )

                // Don't show again checkbox
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(bottom = 20.dp),
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    Checkbox(
                        checked = dontShowAgain.value,
                        onCheckedChange = { dontShowAgain.value = it },
                        modifier = Modifier.padding(end = 8.dp)
                    )
                    Text(
                        text = "Don't show this again",
                        style = MaterialTheme.typography.labelSmall,
                        modifier = Modifier.padding(top = 2.dp)
                    )
                }

                // Buttons
                Row(
                    modifier = Modifier
                        .fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp)
                ) {
                    Button(
                        onClick = onDismiss,
                        modifier = Modifier
                            .weight(1f)
                            .height(44.dp),
                        colors = ButtonDefaults.outlinedButtonColors()
                    ) {
                        Text("Close", fontSize = 14.sp)
                    }
                    Button(
                        onClick = onDonate,
                        modifier = Modifier
                            .weight(1f)
                            .height(44.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = Color(0xFFFF6B6B)
                        )
                    ) {
                        Text("Donate", fontSize = 14.sp, color = Color.White)
                    }
                }
            }
        }
    }
}

@Composable
fun BubbleSwitch(context: android.content.Context) {
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    val isBubbleEnabled = remember { mutableStateOf(prefs.getBoolean("bubble_enabled", false)) }

    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
        modifier = Modifier
            .fillMaxWidth()
            .padding(horizontal = 32.dp)
    ) {
        Text("Enable Floating Bubble")
        Switch(
            checked = isBubbleEnabled.value,
            onCheckedChange = { enabled ->
                isBubbleEnabled.value = enabled
                prefs.edit().putBoolean("bubble_enabled", enabled).apply()
            }
        )
    }
}