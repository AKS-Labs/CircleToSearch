/*
 * Copyright (C) 2024 David V
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

package dev.davidv.translator

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.ServiceConnection
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.IBinder
import android.util.Log
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.ViewModelProvider
import dev.davidv.translator.browser.BrowserActivity
import dev.davidv.translator.ui.TranslatorViewModel
import dev.davidv.translator.ui.TranslatorViewModelFactory
import dev.davidv.translator.ui.screens.TranslatorApp
import dev.davidv.translator.ui.theme.TranslatorTheme
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

class MainActivity : ComponentActivity() {
  companion object {
    const val EXTRA_OPEN_LANGUAGE_MANAGER = "dev.davidv.translator.OPEN_LANGUAGE_MANAGER"
  }

  // Identity in production; only the UI-export test flips UiExportRecorder.active to record which
  // R.string each rendered label resolved from.
  override fun attachBaseContext(newBase: Context) {
    super.attachBaseContext(UiExportRecorder.wrap(newBase))
  }

  private var textToTranslate: String = ""
  private var launchMode: LaunchMode = LaunchMode.Normal
  private var sharedImageUri: Uri? = null
  private var sharedDocumentUri: Uri? = null
  private var openLanguageManager: Boolean = false
  private lateinit var viewModel: TranslatorViewModel
  private var downloadService: DownloadService? = null
  private lateinit var serviceConnection: ServiceConnection
  private val _downloadServiceState = MutableStateFlow<DownloadService?>(null)
  private val downloadServiceState: StateFlow<DownloadService?> = _downloadServiceState.asStateFlow()

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    enableEdgeToEdge()
    handleIntent(intent)
    if (isFinishing) return

    val app = application as TranslatorApplication

    viewModel =
      ViewModelProvider(
        this,
        TranslatorViewModelFactory(
          appContext = applicationContext,
          translationCoordinator = app.translationCoordinator,
          settingsManager = app.settingsManager,
          filePathManager = app.filePathManager,
          languageMetadataManager = app.languageMetadataManager,
          initialText = textToTranslate,
          initialLaunchMode = launchMode,
        ),
      )[TranslatorViewModel::class.java]

    // Deliver any shared input now that the ViewModel exists.
    sharedImageUri?.let { viewModel.setSharedImageUri(it) }
    sharedDocumentUri?.let { viewModel.setSharedDocumentUri(it) }
    if (openLanguageManager) {
      viewModel.requestLanguageManager()
      openLanguageManager = false
    }

    setContent {
      TranslatorTheme {
        MigrationGate(
          filePathManager = app.filePathManager,
          onMigrated = {
            TranslatorTtsEngine.notifyVoiceDataChanged(applicationContext)
            // The Application holds its own catalog instance (used by e.g. the
            // doc-align gate) separate from LanguageStateManager's; refresh it too.
            app.reloadLanguageCatalog()
            viewModel.languageStateManager.refreshLanguageAvailability()
          },
        ) {
          TranslatorApp(
            viewModel = viewModel,
            downloadServiceState = downloadServiceState,
          )
        }
      }
    }

    serviceConnection =
      object : ServiceConnection {
        override fun onServiceConnected(
          name: ComponentName?,
          service: IBinder?,
        ) {
          Log.d("ServiceConnection", "download service done")
          val binder = service as DownloadService.DownloadBinder
          downloadService = binder.getService()
          _downloadServiceState.value = downloadService
        }

        override fun onServiceDisconnected(name: ComponentName?) {
          downloadService = null
          _downloadServiceState.value = null
        }
      }

    val serviceIntent = Intent(this, DownloadService::class.java)
    bindService(serviceIntent, serviceConnection, Context.BIND_AUTO_CREATE)
  }

  override fun onDestroy() {
    super.onDestroy()
    if (::serviceConnection.isInitialized) {
      unbindService(serviceConnection)
    }
    Log.i("MainActivity", "cleaning up")
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    handleIntent(intent)
    // Warm start (activity already created): deliver straight to the ViewModel,
    // since onCreate's one-shot delivery has already run.
    if (::viewModel.isInitialized) {
      sharedImageUri?.let {
        viewModel.setSharedImageUri(it)
        sharedImageUri = null
      }
      sharedDocumentUri?.let {
        viewModel.setSharedDocumentUri(it)
        sharedDocumentUri = null
      }
      if (openLanguageManager) {
        viewModel.requestLanguageManager()
        openLanguageManager = false
      }
    }
  }

  private fun handleIntent(intent: Intent?) {
    Log.d("MainActivity", "Got intent $intent")
    openLanguageManager = intent?.getBooleanExtra(EXTRA_OPEN_LANGUAGE_MANAGER, false) == true
    when (intent?.action) {
      Intent.ACTION_SEND -> {
        val text = intent.getStringExtra(Intent.EXTRA_TEXT)
        val streamUri =
          if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
          } else {
            @Suppress("DEPRECATION")
            intent.getParcelableExtra(Intent.EXTRA_STREAM)
          }

        if (text != null) {
          val trimmed = text.trim()
          if (isWebUrl(trimmed)) {
            if (isSupportedDocumentUrl(trimmed)) {
              Toast.makeText(this, getString(R.string.doc_url_unsupported), Toast.LENGTH_LONG).show()
            } else {
              openUrl(trimmed)
              return
            }
          } else {
            textToTranslate = text
          }
        } else if (streamUri != null) {
          // EXTRA_STREAM carries both shared images (→ crop) and shared documents
          // (→ the document drawer); route by MIME so a pdf/odt/txt/epub doesn't
          // get sent to the image cropper.
          if (isImageUri(this, streamUri)) {
            sharedImageUri = streamUri
          } else {
            sharedDocumentUri = streamUri
          }
          textToTranslate = ""
        }
      }
      Intent.ACTION_VIEW -> {
        val url = intent.data?.toString()
        if (!url.isNullOrBlank() && isWebUrl(url)) {
          if (isSupportedDocumentUrl(url)) {
            Toast.makeText(this, getString(R.string.doc_url_unsupported), Toast.LENGTH_LONG).show()
          } else {
            openUrl(url)
            return
          }
        }
      }
    }
  }

  private fun openUrl(url: String) {
    val browserIntent =
      Intent(this, BrowserActivity::class.java).apply {
        putExtra(BrowserActivity.EXTRA_URL, url)
      }
    startActivity(browserIntent)
    finish()
  }
}

internal fun isWebUrl(text: String): Boolean {
  val trimmed = text.trim()
  if (trimmed.contains(Regex("\\s"))) return false
  return trimmed.startsWith("http://") || trimmed.startsWith("https://")
}

private val SUPPORTED_DOCUMENT_URL_EXTENSIONS =
  setOf(
    "pdf", "odt", "epub",
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "heic", "heif", "tiff", "tif", "avif",
  )

internal fun isSupportedDocumentUrl(url: String): Boolean {
  val path =
    try {
      java.net.URI(url).path ?: return false
    } catch (_: Exception) {
      return false
    }
  val lastSlash = path.lastIndexOf('/')
  val lastSegment = if (lastSlash >= 0) path.substring(lastSlash + 1) else path
  val lastDot = lastSegment.lastIndexOf('.')
  if (lastDot <= 0) return false
  val ext = lastSegment.substring(lastDot + 1).lowercase()
  return ext in SUPPORTED_DOCUMENT_URL_EXTENSIONS
}
