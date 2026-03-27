package com.akslabs.circletosearch.ui

import android.content.Context
import android.net.Uri
import android.util.Log
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Language
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import java.io.File
import java.io.FileOutputStream

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OcrSettingsScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val prefs = context.getSharedPreferences("OcrSettings", Context.MODE_PRIVATE)

    var currentLang by remember { mutableStateOf(prefs.getString("selected_lang", "eng") ?: "eng") }
    var availableModels by remember { mutableStateOf(getAvailableModels(context)) }

    val filePickerLauncher = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.GetContent()
    ) { uri: Uri? ->
        if (uri != null) {
            importModel(context, uri) { success, msg ->
                Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
                if (success) {
                    availableModels = getAvailableModels(context)
                }
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("OCR Language Models", fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                    titleContentColor = MaterialTheme.colorScheme.onSurface
                )
            )
        },
        floatingActionButton = {
            ExtendedFloatingActionButton(
                onClick = { filePickerLauncher.launch("*/*") },
                icon = { Icon(Icons.Default.Add, contentDescription = "Import Model") },
                text = { Text("Import Model") }
            )
        }
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
        ) {
            Text(
                text = "Tesseract Optical Character Recognition uses .traineddata models to accurately read text from screen captures. English is bundled by default.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(16.dp)
            )

            LazyColumn(modifier = Modifier.fillMaxWidth()) {
                items(availableModels) { lang ->
                    val isSelected = lang == currentLang
                    ListItem(
                        headlineContent = { Text(lang.uppercase(), fontWeight = FontWeight.SemiBold) },
                        supportingContent = { Text("Model file: $lang.traineddata") },
                        leadingContent = {
                            Icon(Icons.Default.Language, contentDescription = null)
                        },
                        trailingContent = {
                            if (isSelected) {
                                Icon(Icons.Default.Check, contentDescription = "Selected", tint = MaterialTheme.colorScheme.primary)
                            }
                        },
                        modifier = Modifier.clickable {
                            currentLang = lang
                            prefs.edit().putString("selected_lang", lang).apply()
                        },
                        colors = ListItemDefaults.colors(
                            containerColor = if (isSelected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface
                        )
                    )
                    HorizontalDivider(color = MaterialTheme.colorScheme.surfaceVariant)
                }
            }
        }
    }
}

private fun getAvailableModels(context: Context): List<String> {
    val dir = File(context.filesDir, "tessdata")
    if (!dir.exists()) {
        // Ensure bundle copied
        com.akslabs.circletosearch.ocr.TesseractEngine.prepareTessData(context)
    }
    
    val files = dir.listFiles() ?: return listOf("eng")
    return files.filter { it.name.endsWith(".traineddata") }
        .map { it.name.removeSuffix(".traineddata") }
        .sorted()
}

private fun importModel(context: Context, uri: Uri, callback: (Boolean, String) -> Unit) {
    try {
        val cursor = context.contentResolver.query(uri, null, null, null, null)
        var fileName = "unknown.traineddata"
        if (cursor != null && cursor.moveToFirst()) {
            val nameIndex = cursor.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (nameIndex != -1) {
                fileName = cursor.getString(nameIndex)
            }
            cursor.close()
        }

        if (!fileName.endsWith(".traineddata")) {
            callback(false, "File must be a .traineddata Tesseract model.")
            return
        }

        val tessDir = File(context.filesDir, "tessdata")
        if (!tessDir.exists()) tessDir.mkdirs()

        val destFile = File(tessDir, fileName)
        
        context.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(destFile).use { output ->
                input.copyTo(output)
            }
        }

        Log.d("OcrSettings", "Imported model to ${destFile.absolutePath}")
        callback(true, "Successfully imported ${fileName.removeSuffix(".traineddata").uppercase()} model!")

    } catch (e: Exception) {
        Log.e("OcrSettings", "Error importing model: ${e.message}")
        callback(false, "Failed to import model")
    }
}
