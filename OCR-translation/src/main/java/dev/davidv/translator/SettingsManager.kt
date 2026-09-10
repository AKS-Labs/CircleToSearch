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
import android.content.SharedPreferences
import android.content.pm.PackageManager
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject

class SettingsManager(
  context: Context,
) {
  private val appContext: Context = context.applicationContext
  private val prefs: SharedPreferences = context.getSharedPreferences("app_settings", Context.MODE_PRIVATE)

  private val modifiedSettings = mutableSetOf<String>()

  private val _settings = MutableStateFlow(loadSettings())
  val settings: StateFlow<AppSettings> = _settings.asStateFlow()

  private val prefsListener =
    SharedPreferences.OnSharedPreferenceChangeListener { _, _ ->
      _settings.value = loadSettings()
    }

  init {
    prefs.registerOnSharedPreferenceChangeListener(prefsListener)
  }

  private fun loadSettings(): AppSettings {
    val defaults = AppSettings()

    modifiedSettings.clear()
    prefs.all.keys.forEach { key ->
      modifiedSettings.add(key)
    }

    val defaultTargetLanguageCode = prefs.getString("default_target_language", null) ?: defaults.defaultTargetLanguageCode
    val defaultSourceLanguageCode = prefs.getString("default_source_language", null)

    val catalogIndexUrl =
      prefs.getString("catalog_index_url", null)
        ?: defaults.catalogIndexUrl

    val backgroundModeName = prefs.getString("background_mode", null)
    val backgroundMode =
      if (backgroundModeName != null) {
        try {
          BackgroundMode.valueOf(backgroundModeName)
        } catch (_: IllegalArgumentException) {
          defaults.backgroundMode
        }
      } else {
        defaults.backgroundMode
      }

    val minConfidence = prefs.getInt("min_confidence", defaults.minConfidence)
    val maxImageSize = prefs.getInt("max_image_size", defaults.maxImageSize).coerceIn(600, 2000)
    val disableCLD = prefs.getBoolean("disable_cld", defaults.disableCLD)
    val experimentalScreenTranslate =
      prefs.getBoolean("experimental_screen_translate", defaults.experimentalScreenTranslate)
    val enableOutputTransliteration = prefs.getBoolean("enable_output_transliteration", defaults.enableOutputTransliteration)
    val useExternalStorage = prefs.getBoolean("use_external_storage", defaults.useExternalStorage)
    val fontFactor = prefs.getFloat("font_factor", defaults.fontFactor)
    val showTransliterationOnInput = prefs.getBoolean("show_transliteration_on_input", defaults.showTransliterationOnInput)
    val onlyShowOutputOnReadonlyModal =
      prefs.getBoolean(
        "only_show_output_on_readonly_modal",
        defaults.onlyShowOutputOnReadonlyModal,
      )
    val readonlyModalOutputAlignmentName = prefs.getString("readonly_modal_output_alignment", null)
    val readonlyModalOutputAlignment =
      if (readonlyModalOutputAlignmentName != null) {
        try {
          ReadonlyModalOutputAlignment.valueOf(readonlyModalOutputAlignmentName)
        } catch (_: IllegalArgumentException) {
          defaults.readonlyModalOutputAlignment
        }
      } else {
        defaults.readonlyModalOutputAlignment
      }
    val readonlyModalCompactHeightFactor =
      prefs.getFloat(
        "readonly_modal_compact_height_factor",
        defaults.readonlyModalCompactHeightFactor,
      )
    val addSpacesForJapaneseTransliteration =
      prefs.getBoolean(
        "add_spaces_for_japanese_transliteration",
        defaults.addSpacesForJapaneseTransliteration,
      )
    val ttsPlaybackSpeed = prefs.getFloat("tts_playback_speed", defaults.ttsPlaybackSpeed)
    val ttsPlaybackSpeedOverrides =
      prefs
        .getString("tts_playback_speed_overrides", null)
        ?.let(::parsePlaybackSpeedOverrides)
        ?: defaults.ttsPlaybackSpeedOverrides
    val tapToTranslateEnabled = prefs.getBoolean("tap_to_translate_enabled", defaults.tapToTranslateEnabled)
    val ttsVoiceOverrides =
      prefs
        .getString("tts_voice_overrides", null)
        ?.let(::parseVoiceOverrides)
        ?: defaults.ttsVoiceOverrides
    val ttsReadUrlsAndHashtags =
      prefs.getBoolean("tts_read_urls_and_hashtags", defaults.ttsReadUrlsAndHashtags)
    val clearWebTranslatorDataOnClose =
      prefs.getBoolean(
        "clear_web_translator_data_on_close",
        defaults.clearWebTranslatorDataOnClose,
      )
    val translatePdfImages =
      prefs.getBoolean("translate_pdf_images", defaults.translatePdfImages)
    val liveCameraOverlayEnabled =
      prefs.getBoolean("live_camera_overlay_enabled", defaults.liveCameraOverlayEnabled)
    val registerAsBrowser =
      prefs.getBoolean("register_as_browser", defaults.registerAsBrowser)
    val multiTargetEnabled =
      prefs.getBoolean("multi_target_enabled", defaults.multiTargetEnabled)
    val httpServerEnabled = prefs.getBoolean("http_server_enabled", defaults.httpServerEnabled)
    val httpServerPort = prefs.getInt("http_server_port", defaults.httpServerPort).coerceIn(1, 65535)
    val httpServerBindModeName = prefs.getString("http_server_bind_mode", null)
    val httpServerBindMode =
      if (httpServerBindModeName != null) {
        try {
          HttpServerBindMode.valueOf(httpServerBindModeName)
        } catch (_: IllegalArgumentException) {
          defaults.httpServerBindMode
        }
      } else {
        defaults.httpServerBindMode
      }
    val assistantActionName = prefs.getString("assistant_action", null)
    val assistantAction =
      if (assistantActionName != null) {
        try {
          AssistantAction.valueOf(assistantActionName)
        } catch (_: IllegalArgumentException) {
          defaults.assistantAction
        }
      } else {
        defaults.assistantAction
      }

    return AppSettings(
      defaultTargetLanguageCode = defaultTargetLanguageCode,
      defaultSourceLanguageCode = defaultSourceLanguageCode,
      catalogIndexUrl = catalogIndexUrl,
      backgroundMode = backgroundMode,
      minConfidence = minConfidence,
      maxImageSize = maxImageSize,
      disableCLD = disableCLD,
      experimentalScreenTranslate = experimentalScreenTranslate,
      enableOutputTransliteration = enableOutputTransliteration,
      useExternalStorage = useExternalStorage,
      fontFactor = fontFactor,
      showTransliterationOnInput = showTransliterationOnInput,
      onlyShowOutputOnReadonlyModal = onlyShowOutputOnReadonlyModal,
      readonlyModalOutputAlignment = readonlyModalOutputAlignment,
      readonlyModalCompactHeightFactor = readonlyModalCompactHeightFactor,
      addSpacesForJapaneseTransliteration = addSpacesForJapaneseTransliteration,
      ttsPlaybackSpeed = ttsPlaybackSpeed,
      ttsPlaybackSpeedOverrides = ttsPlaybackSpeedOverrides,
      ttsVoiceOverrides = ttsVoiceOverrides,
      ttsReadUrlsAndHashtags = ttsReadUrlsAndHashtags,
      tapToTranslateEnabled = tapToTranslateEnabled,
      clearWebTranslatorDataOnClose = clearWebTranslatorDataOnClose,
      translatePdfImages = translatePdfImages,
      liveCameraOverlayEnabled = liveCameraOverlayEnabled,
      registerAsBrowser = registerAsBrowser,
      assistantAction = assistantAction,
      multiTargetEnabled = multiTargetEnabled,
      httpServerEnabled = httpServerEnabled,
      httpServerPort = httpServerPort,
      httpServerBindMode = httpServerBindMode,
    )
  }

  fun updateSettings(newSettings: AppSettings) {
    val currentSettings = _settings.value

    prefs.edit().apply {
      if (newSettings.defaultTargetLanguageCode != currentSettings.defaultTargetLanguageCode) {
        putString("default_target_language", newSettings.defaultTargetLanguageCode)
        modifiedSettings.add("default_target_language")
      }
      if (newSettings.defaultSourceLanguageCode != currentSettings.defaultSourceLanguageCode) {
        if (newSettings.defaultSourceLanguageCode != null) {
          putString("default_source_language", newSettings.defaultSourceLanguageCode)
        } else {
          remove("default_source_language")
        }
        modifiedSettings.add("default_source_language")
      }
      if (newSettings.catalogIndexUrl != currentSettings.catalogIndexUrl) {
        putString("catalog_index_url", newSettings.catalogIndexUrl)
        modifiedSettings.add("catalog_index_url")
      }
      if (newSettings.backgroundMode != currentSettings.backgroundMode) {
        putString("background_mode", newSettings.backgroundMode.name)
        modifiedSettings.add("background_mode")
      }
      if (newSettings.minConfidence != currentSettings.minConfidence) {
        putInt("min_confidence", newSettings.minConfidence)
        modifiedSettings.add("min_confidence")
      }
      if (newSettings.maxImageSize != currentSettings.maxImageSize) {
        putInt("max_image_size", newSettings.maxImageSize)
        modifiedSettings.add("max_image_size")
      }
      if (newSettings.disableCLD != currentSettings.disableCLD) {
        putBoolean("disable_cld", newSettings.disableCLD)
        modifiedSettings.add("disable_cld")
      }
      if (newSettings.experimentalScreenTranslate != currentSettings.experimentalScreenTranslate) {
        putBoolean("experimental_screen_translate", newSettings.experimentalScreenTranslate)
        modifiedSettings.add("experimental_screen_translate")
      }
      if (newSettings.enableOutputTransliteration != currentSettings.enableOutputTransliteration) {
        putBoolean("enable_output_transliteration", newSettings.enableOutputTransliteration)
        modifiedSettings.add("enable_output_transliteration")
      }
      if (newSettings.useExternalStorage != currentSettings.useExternalStorage) {
        putBoolean("use_external_storage", newSettings.useExternalStorage)
        modifiedSettings.add("use_external_storage")
      }
      if (newSettings.fontFactor != currentSettings.fontFactor) {
        putFloat("font_factor", newSettings.fontFactor)
        modifiedSettings.add("font_factor")
      }
      if (newSettings.showTransliterationOnInput != currentSettings.showTransliterationOnInput) {
        putBoolean("show_transliteration_on_input", newSettings.showTransliterationOnInput)
        modifiedSettings.add("show_transliteration_on_input")
      }
      if (newSettings.onlyShowOutputOnReadonlyModal != currentSettings.onlyShowOutputOnReadonlyModal) {
        putBoolean("only_show_output_on_readonly_modal", newSettings.onlyShowOutputOnReadonlyModal)
        modifiedSettings.add("only_show_output_on_readonly_modal")
      }
      if (newSettings.readonlyModalOutputAlignment != currentSettings.readonlyModalOutputAlignment) {
        putString("readonly_modal_output_alignment", newSettings.readonlyModalOutputAlignment.name)
        modifiedSettings.add("readonly_modal_output_alignment")
      }
      if (newSettings.readonlyModalCompactHeightFactor != currentSettings.readonlyModalCompactHeightFactor) {
        putFloat("readonly_modal_compact_height_factor", newSettings.readonlyModalCompactHeightFactor)
        modifiedSettings.add("readonly_modal_compact_height_factor")
      }
      if (newSettings.addSpacesForJapaneseTransliteration != currentSettings.addSpacesForJapaneseTransliteration) {
        putBoolean("add_spaces_for_japanese_transliteration", newSettings.addSpacesForJapaneseTransliteration)
        modifiedSettings.add("add_spaces_for_japanese_transliteration")
      }
      if (newSettings.ttsPlaybackSpeed != currentSettings.ttsPlaybackSpeed) {
        putFloat("tts_playback_speed", newSettings.ttsPlaybackSpeed)
        modifiedSettings.add("tts_playback_speed")
      }
      if (newSettings.ttsPlaybackSpeedOverrides != currentSettings.ttsPlaybackSpeedOverrides) {
        putString("tts_playback_speed_overrides", serializePlaybackSpeedOverrides(newSettings.ttsPlaybackSpeedOverrides))
        modifiedSettings.add("tts_playback_speed_overrides")
      }
      if (newSettings.ttsVoiceOverrides != currentSettings.ttsVoiceOverrides) {
        putString("tts_voice_overrides", serializeVoiceOverrides(newSettings.ttsVoiceOverrides))
        modifiedSettings.add("tts_voice_overrides")
        TranslatorTtsEngine.notifyVoiceDataChanged(appContext)
      }
      if (newSettings.ttsReadUrlsAndHashtags != currentSettings.ttsReadUrlsAndHashtags) {
        putBoolean("tts_read_urls_and_hashtags", newSettings.ttsReadUrlsAndHashtags)
        modifiedSettings.add("tts_read_urls_and_hashtags")
      }
      if (newSettings.tapToTranslateEnabled != currentSettings.tapToTranslateEnabled) {
        putBoolean("tap_to_translate_enabled", newSettings.tapToTranslateEnabled)
        modifiedSettings.add("tap_to_translate_enabled")
      }
      if (newSettings.clearWebTranslatorDataOnClose != currentSettings.clearWebTranslatorDataOnClose) {
        putBoolean("clear_web_translator_data_on_close", newSettings.clearWebTranslatorDataOnClose)
        modifiedSettings.add("clear_web_translator_data_on_close")
      }
      if (newSettings.translatePdfImages != currentSettings.translatePdfImages) {
        putBoolean("translate_pdf_images", newSettings.translatePdfImages)
        modifiedSettings.add("translate_pdf_images")
      }
      if (newSettings.liveCameraOverlayEnabled != currentSettings.liveCameraOverlayEnabled) {
        putBoolean("live_camera_overlay_enabled", newSettings.liveCameraOverlayEnabled)
        modifiedSettings.add("live_camera_overlay_enabled")
      }
      if (newSettings.registerAsBrowser != currentSettings.registerAsBrowser) {
        putBoolean("register_as_browser", newSettings.registerAsBrowser)
        modifiedSettings.add("register_as_browser")
        applyBrowserAliasState(newSettings.registerAsBrowser)
      }
      if (newSettings.assistantAction != currentSettings.assistantAction) {
        putString("assistant_action", newSettings.assistantAction.name)
        modifiedSettings.add("assistant_action")
      }
      if (newSettings.multiTargetEnabled != currentSettings.multiTargetEnabled) {
        putBoolean("multi_target_enabled", newSettings.multiTargetEnabled)
        modifiedSettings.add("multi_target_enabled")
      }
      if (newSettings.httpServerEnabled != currentSettings.httpServerEnabled) {
        putBoolean("http_server_enabled", newSettings.httpServerEnabled)
        modifiedSettings.add("http_server_enabled")
      }
      if (newSettings.httpServerPort != currentSettings.httpServerPort) {
        putInt("http_server_port", newSettings.httpServerPort)
        modifiedSettings.add("http_server_port")
      }
      if (newSettings.httpServerBindMode != currentSettings.httpServerBindMode) {
        putString("http_server_bind_mode", newSettings.httpServerBindMode.name)
        modifiedSettings.add("http_server_bind_mode")
      }
      remove("translation_models_base_url_v3")
      remove("tesseract_models_base_url")
      remove("dictionary_base_url")
      apply()
    }
    _settings.value = newSettings

    // The native server takes a config snapshot at start, so anything it
    // serves from settings restarts it.
    val serverConfigChanged =
      newSettings.httpServerPort != currentSettings.httpServerPort ||
        newSettings.httpServerBindMode != currentSettings.httpServerBindMode ||
        newSettings.maxImageSize != currentSettings.maxImageSize ||
        newSettings.minConfidence != currentSettings.minConfidence ||
        newSettings.backgroundMode != currentSettings.backgroundMode ||
        newSettings.translatePdfImages != currentSettings.translatePdfImages
    if (newSettings.httpServerEnabled != currentSettings.httpServerEnabled || serverConfigChanged) {
      applyHttpServerState(newSettings)
    }
  }

  fun applyHttpServerState(settings: AppSettings) {
    if (settings.httpServerEnabled) {
      TranslationApiService.start(appContext, settings.httpServerPort, settings.httpServerBindMode)
    } else {
      TranslationApiService.stop(appContext)
    }
  }

  fun applyBrowserAliasState(enabled: Boolean) {
    val component = ComponentName(appContext, "dev.davidv.translator.MainActivityBrowser")
    val desired =
      if (enabled) {
        PackageManager.COMPONENT_ENABLED_STATE_ENABLED
      } else {
        PackageManager.COMPONENT_ENABLED_STATE_DISABLED
      }
    appContext.packageManager.setComponentEnabledSetting(
      component,
      desired,
      PackageManager.DONT_KILL_APP,
    )
  }

  private fun parseVoiceOverrides(json: String): Map<String, String> =
    runCatching {
      val root = JSONObject(json)
      root.keys().asSequence().mapNotNull { languageCode ->
        root.optString(languageCode).takeIf { it.isNotBlank() }?.let { languageCode to it }
      }.toMap()
    }.getOrDefault(emptyMap())

  private fun serializeVoiceOverrides(overrides: Map<String, String>): String {
    val root = JSONObject()
    overrides.toSortedMap().forEach { (languageCode, voiceName) ->
      root.put(languageCode, voiceName)
    }
    return root.toString()
  }

  private fun parsePlaybackSpeedOverrides(json: String): Map<String, Float> =
    runCatching {
      val root = JSONObject(json)
      root.keys().asSequence().mapNotNull { voiceName ->
        val speed = root.optDouble(voiceName, Double.NaN)
        if (speed.isNaN()) {
          null
        } else {
          voiceName to speed.toFloat().coerceIn(0.5f, 2.0f)
        }
      }.toMap()
    }.getOrDefault(emptyMap())

  private fun serializePlaybackSpeedOverrides(overrides: Map<String, Float>): String {
    val root = JSONObject()
    overrides.toSortedMap().forEach { (voiceName, speed) ->
      root.put(voiceName, speed.coerceIn(0.5f, 2.0f))
    }
    return root.toString()
  }
}
