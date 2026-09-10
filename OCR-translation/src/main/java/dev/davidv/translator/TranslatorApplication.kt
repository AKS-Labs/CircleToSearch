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

import android.app.Application
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Build
import android.os.UserManager
import android.util.Log
import dev.davidv.translator.adblock.AdblockManager

class TranslatorApplication : Application() {
  lateinit var settingsManager: SettingsManager
  lateinit var languageMetadataManager: LanguageMetadataManager
  lateinit var filePathManager: FilePathManager
  lateinit var imageProcessor: ImageProcessor
  lateinit var translationService: TranslationService
  lateinit var speechService: SpeechService
  lateinit var languageDetector: LanguageDetector
  lateinit var translationCoordinator: TranslationCoordinator
  lateinit var adblockManager: AdblockManager
  val languagesFlow = kotlinx.coroutines.flow.MutableStateFlow<List<Language>>(emptyList())
  var languageCatalog: LanguageCatalog? = null
    private set

  /** Re-open the app-level catalog after on-disk model files change (e.g. the
   *  ONNX→MNN migration), so gates like doc-align see the new state. */
  fun reloadLanguageCatalog() {
    languageCatalog = filePathManager.reloadCatalog()
    languagesFlow.value = languageCatalog?.languageList ?: emptyList()
  }

  private var servicesInitialized = false

  override fun onCreate() {
    super.onCreate()

    // The assistant VoiceInteractionService is directBootAware, so the system starts
    // this process before the user unlocks. Service init below touches credential-encrypted
    // SharedPreferences, which throw until unlock — so defer it until the user is unlocked.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N && !isUserUnlocked()) {
      Log.d("TranslatorApplication", "User locked (direct boot); deferring service init until unlock")
      registerUnlockReceiver()
      return
    }

    initializeServices()
  }

  private fun isUserUnlocked(): Boolean = getSystemService(UserManager::class.java).isUserUnlocked

  private fun registerUnlockReceiver() {
    val receiver =
      object : BroadcastReceiver() {
        override fun onReceive(
          context: Context,
          intent: Intent,
        ) {
          Log.d("TranslatorApplication", "User unlocked; running deferred service init")
          initializeServices()
          unregisterReceiver(this)
        }
      }
    registerReceiver(receiver, IntentFilter(Intent.ACTION_USER_UNLOCKED))
  }

  private fun initializeServices() {
    if (servicesInitialized) {
      return
    }
    servicesInitialized = true
    Log.d("TranslatorApplication", "Initializing application services")

    settingsManager = SettingsManager(this)
    filePathManager = FilePathManager(this, settingsManager.settings)

    if (isTtsEngineProcess()) {
      Log.d("TranslatorApplication", "Initialized lightweight TTS engine process")
      return
    }

    languageCatalog = filePathManager.loadCatalog()
    languagesFlow.value = languageCatalog?.languageList ?: emptyList()
    languageMetadataManager = LanguageMetadataManager(this, languagesFlow)
    imageProcessor = ImageProcessor(this, filePathManager)
    translationService = TranslationService(settingsManager, filePathManager)
    speechService = SpeechService(settingsManager, filePathManager)
    languageDetector = LanguageDetector { code -> languageCatalog?.languageByCode(code) }
    translationCoordinator =
      TranslationCoordinator(translationService, speechService, languageDetector, imageProcessor, settingsManager)
    adblockManager = AdblockManager(filePathManager)

    if (settingsManager.settings.value.tapToTranslateEnabled) {
      TapToTranslateNotification.show(this)
    }

    if (settingsManager.settings.value.httpServerEnabled) {
      settingsManager.applyHttpServerState(settingsManager.settings.value)
    }

    settingsManager.applyBrowserAliasState(settingsManager.settings.value.registerAsBrowser)
  }

  private fun isTtsEngineProcess(): Boolean = currentProcessName().endsWith(":tts")

  private fun currentProcessName(): String =
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
      getProcessName()
    } else {
      try {
        java.io.File("/proc/self/cmdline").readText().trimEnd('\u0000')
      } catch (_: Exception) {
        packageName
      }
    }
}
