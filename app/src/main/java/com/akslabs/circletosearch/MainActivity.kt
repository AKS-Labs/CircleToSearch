package com.akslabs.circletosearch

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.akslabs.circletosearch.ui.theme.CircleToSearchTheme
import kotlinx.coroutines.launch

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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetupScreen() {
    val context = LocalContext.current
    val lifecycleOwner = androidx.lifecycle.compose.LocalLifecycleOwner.current
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    
    // Permission States
    var isAccessibilityEnabled by remember { mutableStateOf(isAccessibilityServiceEnabled(context)) }
    var isDefaultAssistant by remember { mutableStateOf(isDefaultAssistant(context)) }
    
    // Check permissions on Resume
    DisposableEffect(lifecycleOwner) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == androidx.lifecycle.Lifecycle.Event.ON_RESUME) {
                isAccessibilityEnabled = isAccessibilityServiceEnabled(context)
                isDefaultAssistant = isDefaultAssistant(context)
            }
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    val showSupportDialog = remember { mutableStateOf(!prefs.getBoolean("support_dialog_dismissed", false)) }
    val dontShowAgain = remember { mutableStateOf(false) }
    
    var showSupportSheet by remember { mutableStateOf(false) }
    val sheetState = rememberModalBottomSheetState()
    val scope = rememberCoroutineScope()

    Scaffold(
        containerColor = MaterialTheme.colorScheme.background
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(horizontal = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            Spacer(modifier = Modifier.height(32.dp))
            
            // 1. Header
            Text(
                text = "Circle to Search",
                style = MaterialTheme.typography.displaySmall.copy(fontWeight = FontWeight.Bold),
                color = MaterialTheme.colorScheme.primary
            )
            Spacer(modifier = Modifier.height(16.dp))
            Text(
                text = "Search anything on your screen instantly.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center
            )
            
            Spacer(modifier = Modifier.height(32.dp))

            // 2. REQUIRED: Accessibility Service
            Text(
                text = "REQUIRED",
                style = MaterialTheme.typography.labelSmall,
                color = if (isAccessibilityEnabled) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.error,
                modifier = Modifier.align(Alignment.Start).padding(bottom = 8.dp)
            )
            
            if (isAccessibilityEnabled) {
                // Granted State
                Card(
                     colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.primaryContainer.copy(alpha = 0.6f)
                    ),
                    shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Default.CheckCircle,
                            contentDescription = "Granted",
                            tint = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.size(24.dp)
                        )
                        Spacer(modifier = Modifier.width(16.dp))
                        Column {
                            Text(
                                text = "Accessibility Service Active",
                                style = MaterialTheme.typography.titleMedium,
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            Text(
                                text = "Double tap status bar to launch CircleToSearch",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            } else {
                 // Action Needed State
                Card(
                    onClick = {
                        val intent = Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)
                        context.startActivity(intent)
                    },
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.errorContainer
                    ),
                    shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Default.Accessibility,
                            contentDescription = null,
                            modifier = Modifier.size(24.dp),
                            tint = MaterialTheme.colorScheme.onErrorContainer
                        )
                        Spacer(modifier = Modifier.width(16.dp))
                        Column {
                            Text(
                                text = "Enable Accessibility",
                                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                                color = MaterialTheme.colorScheme.onErrorContainer
                            )
                             Text(
                                text = "To do its job properly, the app needs this permission.\n" +
                                        "Tap allow and we’re good to go! \uD83D\uDC4D",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onErrorContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            }
            
            Spacer(modifier = Modifier.height(24.dp))

            // 3. OPTIONAL: Default Assistant
            Text(
                text = "OPTIONAL: For Home Button Trigger",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.align(Alignment.Start).padding(bottom = 8.dp)
            )

            if (isDefaultAssistant) {
                 // Granted State
                Card(
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.secondaryContainer.copy(alpha = 0.6f)
                    ),
                    shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Default.CheckCircle,
                            contentDescription = "Active",
                            tint = MaterialTheme.colorScheme.secondary,
                            modifier = Modifier.size(24.dp)
                        )
                        Spacer(modifier = Modifier.width(16.dp))
                         Text(
                            text = "Default Assistant Set",
                            style = MaterialTheme.typography.titleMedium,
                             color = MaterialTheme.colorScheme.onSecondaryContainer
                        )
                    }
                }
            } else {
                 // Action State
                Card(
                    onClick = {
                        val intent = Intent(Settings.ACTION_VOICE_INPUT_SETTINGS)
                        context.startActivity(intent)
                    },
                    colors = CardDefaults.cardColors(
                        containerColor = MaterialTheme.colorScheme.primaryContainer
                    ),
                    shape = androidx.compose.foundation.shape.RoundedCornerShape(24.dp),
                    modifier = Modifier.fillMaxWidth()
                ) {
                    Row(
                        modifier = Modifier.padding(16.dp),
                        verticalAlignment = Alignment.CenterVertically
                    ) {
                        Icon(
                            imageVector = Icons.Default.TouchApp,
                            contentDescription = null,
                            modifier = Modifier.size(24.dp),
                            tint = MaterialTheme.colorScheme.onPrimaryContainer
                        )
                        Spacer(modifier = Modifier.width(16.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "Set as Default Assistant",
                                style = MaterialTheme.typography.titleMedium.copy(fontWeight = FontWeight.Bold),
                                color = MaterialTheme.colorScheme.onPrimaryContainer
                            )
                            Text(
                                text = "Hold the Home button or edge-swipe up to summon CircleToSearch — like calling your Pokémon. \uD83D\uDD0D\uD83D\uDE06",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f)
                            )
                        }
                    }
                }
            }

            Spacer(modifier = Modifier.height(24.dp))

            // 4. Settings (Bubble)
            Text(
                text = "OPTIONAL",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.secondary,
                modifier = Modifier.align(Alignment.Start)
            )
            BubbleSwitch(context)

            Spacer(modifier = Modifier.height(25.dp))

            // Privacy Note
            Text(
                text = "That’s it. No more permissions.\n We’re not trying to adopt your phone. \uD83D\uDE02.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.7f),
                textAlign = TextAlign.Center
            )

            Spacer(modifier = Modifier.weight(1f))


            // 5. Footer
            SocialLinksRow(
                context = context,
                onDonateClick = { showSupportSheet = true }
            )
            Spacer(modifier = Modifier.height(20.dp))
            Box(
                modifier = Modifier
                    .clip(androidx.compose.foundation.shape.RoundedCornerShape(8.dp))
                    .clickable {
                        val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs"))
                        context.startActivity(intent)
                    }
                    .padding(8.dp),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = "From AKS-Labs With ❤️",
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.primary.copy(alpha = 0.8f),
                    textAlign = TextAlign.Center,
                    fontWeight = FontWeight.Bold
                )
            }
            Spacer(modifier = Modifier.height(2.dp))
        }
    }

    // Support Dialog & Sheet
    if (showSupportDialog.value) {
        SupportDialog(
            onDismiss = {
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            onDonate = {
                showSupportSheet = true
                showSupportDialog.value = false
                if (dontShowAgain.value) {
                    prefs.edit().putBoolean("support_dialog_dismissed", true).apply()
                }
            },
            dontShowAgain = dontShowAgain
        )
    }
    
    if (showSupportSheet) {
        SupportSheet(
            sheetState = sheetState,
            onDismissRequest = {
                scope.launch {
                    sheetState.hide()
                }.invokeOnCompletion {
                    if (!sheetState.isVisible) {
                        showSupportSheet = false
                    }
                }
            }
        )
    }
}

// Helper Functions
fun isAccessibilityServiceEnabled(context: android.content.Context): Boolean {
    val expectedComponentName = android.content.ComponentName(context, CircleToSearchAccessibilityService::class.java)
    val enabledServicesSetting = Settings.Secure.getString(
        context.contentResolver,
        Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES
    ) ?: return false
    
    val colonSplitter = android.text.TextUtils.SimpleStringSplitter(':')
    colonSplitter.setString(enabledServicesSetting)
    
    while (colonSplitter.hasNext()) {
        val componentNameString = colonSplitter.next()
        val enabledComponent = android.content.ComponentName.unflattenFromString(componentNameString)
        if (enabledComponent != null && enabledComponent == expectedComponentName)
            return true
    }
    return false
}

fun isDefaultAssistant(context: android.content.Context): Boolean {
    // Basic check: triggers the settings intent, but actual "is default" check is complex
    // on some Android versions. For simplified UI, we might relay on user return, 
    // or use RoleManager on Android 10+. 
    // For now, let's keep it simple or check specific secure settings if possible.
    // A reliable check is to see if we are arguably the voice interaction service.
    
    val assistant = Settings.Secure.getString(context.contentResolver, "voice_interaction_service")
    val component = android.content.ComponentName(context, CircleToSearchRecognitionService::class.java)
    val myComponentString = component.flattenToString()
    
    // Fallback: Checks if our service is the one set.
    return assistant == myComponentString
}

// ... SocialLinksRow, SupportDialog, BubbleSwitch same as before ...
@Composable
fun SocialLinksRow(
    context: android.content.Context,
    onDonateClick: () -> Unit
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.Center,
        verticalAlignment = Alignment.CenterVertically
    ) {
        IconButton(
            onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/aks-labs"))
                context.startActivity(intent)
            }, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.github),
                contentDescription = "Github",
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp)
            )
        }
        
        IconButton(
            onClick = onDonateClick, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                contentDescription = "Donate",
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp)
            )
        }
        
        IconButton(
            onClick = {
                val intent = Intent(Intent.ACTION_VIEW, Uri.parse("https://t.me/akslabs"))
                context.startActivity(intent)
            }, 
            colors = IconButtonDefaults.filledTonalIconButtonColors(),
            modifier = Modifier.padding(horizontal = 8.dp)
        ) {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.telegram),
                contentDescription = "Telegram",
                tint = MaterialTheme.colorScheme.onSecondaryContainer,
                modifier = Modifier.padding(8.dp).size(23.dp)
            )
        }
    }
}

@Composable
fun SupportDialog(
    onDismiss: () -> Unit,
    onDonate: () -> Unit,
    dontShowAgain: MutableState<Boolean>
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = {
            Icon(
                painter = painterResource(id = com.akslabs.circletosearch.R.drawable.donation),
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
                modifier = Modifier.size(48.dp)
            )
        },
        title = {
            Text(
                text = "Support App Developer",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold
            )
        },
        text = {
            Column {
                Text(
                    text = "If anything I’ve made has ever made your day a little easier or smoother,\n" +
                            "\n" +
                            "I’d be grateful if you considered supporting my work ☺\uFE0F.\n" +
                            "\n" +
                            "Your support helps me keep improving these apps\n" +
                            "and stay motivated,\n" +
                            
                            "\n" +
                            "Thank you for supporting independent developers.",
                    style = MaterialTheme.typography.bodyMedium,
                    lineHeight = 20.sp
                )
                
                Spacer(modifier = Modifier.height(16.dp))
                
//                 Don't show again checkbox
//                Row(
//                    verticalAlignment = Alignment.CenterVertically,
//                    modifier = Modifier.fillMaxWidth().clickable { dontShowAgain.value = !dontShowAgain.value }
//                ) {
//                    Checkbox(
//                        checked = dontShowAgain.value,
//                        onCheckedChange = { dontShowAgain.value = it }
//                    )
//                    Text(
//                        text = "Don't show this again",
//                        style = MaterialTheme.typography.bodySmall,
//                        modifier = Modifier.padding(start = 8.dp)
//                    )
//                }
            }
        },
        confirmButton = {
            Button(onClick = onDonate) {
                Text("Donate")
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text("Close")
            }
        }
    )
}

@Composable
fun BubbleSwitch(context: android.content.Context) {
    val prefs = context.getSharedPreferences("app_prefs", android.content.Context.MODE_PRIVATE)
    val isBubbleEnabled = remember { mutableStateOf(prefs.getBoolean("bubble_enabled", false)) }

    ListItem(
        headlineContent = { Text("Floating Bubble") },
        supportingContent = { Text("Show a floating button triggers search") },
        trailingContent = {
            Switch(
                checked = isBubbleEnabled.value,
                onCheckedChange = { enabled ->
                    isBubbleEnabled.value = enabled
                    prefs.edit().putBoolean("bubble_enabled", enabled).apply()
                }
            )
        },
        colors = ListItemDefaults.colors(
            containerColor = Color.Transparent
        )
    )
}
