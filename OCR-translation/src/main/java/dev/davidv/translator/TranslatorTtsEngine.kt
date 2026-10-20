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

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.media.AudioFormat
import android.os.Bundle
import android.speech.tts.SynthesisCallback
import android.speech.tts.SynthesisRequest
import android.speech.tts.TextToSpeech
import android.speech.tts.TextToSpeechService
import android.speech.tts.Voice
import android.util.Log
import java.util.Locale

class TranslatorTtsService : TextToSpeechService() {
  @Volatile
  private var stopped = false

  @Volatile
  private var currentVoice: TranslatorTtsVoice? = null

  override fun onGetVoices(): List<Voice> {
    val ttsVoices = TranslatorTtsEngine.systemTtsVoices(this)
    Log.i(
      "TranslatorTtsService",
      "onGetVoices: returning ${ttsVoices.size} voices: " +
        ttsVoices.joinToString { "${it.androidName}@${it.locale}" },
    )
    return ttsVoices.map { ttsVoice ->
      Voice(
        ttsVoice.androidName,
        ttsVoice.locale,
        ttsVoice.quality,
        Voice.LATENCY_NORMAL,
        false,
        setOf(TextToSpeech.Engine.KEY_FEATURE_EMBEDDED_SYNTHESIS),
      )
    }
  }

  override fun onIsValidVoiceName(voiceName: String): Int =
    if (TranslatorTtsEngine.isBootstrapVoiceName(voiceName) || TranslatorTtsEngine.isCatalogVoiceName(voiceName)) {
      TextToSpeech.SUCCESS
    } else {
      TextToSpeech.ERROR
    }

  override fun onLoadVoice(voiceName: String): Int {
    val voice = TranslatorTtsEngine.voiceFromAndroidName(voiceName) ?: return TextToSpeech.ERROR
    currentVoice = voice
    return TextToSpeech.SUCCESS
  }

  override fun onGetDefaultVoiceNameFor(
    lang: String,
    country: String,
    variant: String,
  ): String? {
    TranslatorTtsEngine.resolveUserPreferredVoice(this, lang)?.let { return it.androidName }
    val voices = TranslatorTtsEngine.systemTtsVoices(this)
    val match =
      voices.firstOrNull {
        it.locale.safeIso3Language() == lang && it.locale.safeIso3Country() == country
      }
        ?: voices.firstOrNull { it.locale.safeIso3Language() == lang }
    return match?.androidName
  }

  override fun onIsLanguageAvailable(
    lang: String,
    country: String,
    variant: String,
  ): Int = languageAvailability(lang, country)

  override fun onLoadLanguage(
    lang: String,
    country: String,
    variant: String,
  ): Int {
    currentVoice =
      TranslatorTtsEngine.resolveUserPreferredVoice(this, lang)
        ?: TranslatorTtsEngine.bootstrapVoice(lang, country)
    return languageAvailability(lang, country)
  }

  override fun onGetLanguage(): Array<String> {
    val locale = currentVoice?.locale ?: Locale.ENGLISH
    return arrayOf(
      locale.safeIso3Language(),
      locale.safeIso3Country(),
      locale.variant.orEmpty(),
    )
  }

  override fun onSynthesizeText(
    request: SynthesisRequest,
    callback: SynthesisCallback,
  ) {
    stopped = false
    val text = request.charSequenceText?.toString().orEmpty()
    if (text.isBlank()) {
      callback.error()
      callback.done()
      return
    }

    val explicitVoice =
      request.voiceName
        ?.takeIf(TranslatorTtsEngine::isCatalogVoiceName)
        ?.let(TranslatorTtsEngine::voiceFromAndroidName)
    val voice =
      explicitVoice
        ?: TranslatorTtsEngine.resolveUserPreferredVoice(this, request.language.orEmpty())
        ?: currentVoice
        ?: TranslatorTtsEngine.bootstrapVoice(request.language.orEmpty(), request.country.orEmpty())

    val settings = (application as TranslatorApplication).settingsManager.settings.value
    val baseSpeed =
      voice.catalogVoiceName
        ?.let { vname -> settings.ttsPlaybackSpeedOverrides["${voice.languageCode}:$vname"] }
        ?: settings.ttsPlaybackSpeed
    val finalSpeed = (baseSpeed * (request.speechRate / 100f)).coerceIn(0.5f, 2.0f)

    try {
      synthesizeVoice(
        voice = voice,
        text = text,
        speechSpeed = finalSpeed,
        callback = callback,
      )
    } catch (error: Exception) {
      Log.e("TranslatorTtsService", "System TTS synthesis failed", error)
      callback.error()
    } finally {
      callback.done()
    }
  }

  override fun onStop() {
    stopped = true
  }

  private fun synthesizeVoice(
    voice: TranslatorTtsVoice,
    text: String,
    speechSpeed: Float,
    callback: SynthesisCallback,
  ) {
    val catalog = applicationServices().filePathManager.loadCatalog() ?: error("Catalog unavailable")
    val readUrlsAndHashtags = applicationServices().settingsManager.settings.value.ttsReadUrlsAndHashtags
    val chunks = catalog.planSpeechChunks(voice.languageCode, text, voice.packId, readUrlsAndHashtags)
    if (chunks.isEmpty()) {
      error("No speech chunks planned")
    }

    var started = false
    var sampleRate: Int? = null
    for (chunk in chunks) {
      if (stopped) return
      val audio =
        catalog.synthesizeSpeechPcm(
          languageCode = voice.languageCode,
          text = chunk.content,
          speechSpeed = speechSpeed,
          voiceName = voice.catalogVoiceName?.takeIf(String::isNotBlank),
          isPhonemes = chunk.isPhonemes,
          packId = voice.packId,
        )

      if (!started) {
        callback.start(audio.sampleRate, AudioFormat.ENCODING_PCM_16BIT, 1)
        started = true
        sampleRate = audio.sampleRate
      } else if (sampleRate != audio.sampleRate) {
        error("TTS sample rate changed from $sampleRate to ${audio.sampleRate}")
      }

      if (!writePcm(audio.pcmSamples, callback)) return

      val pause = chunk.pauseAfterMs
      if (pause != null && pause > 0) {
        val silence = PcmAudio.silence(audio.sampleRate, pause)
        if (!writePcm(silence.pcmSamples, callback)) return
      }
    }
  }

  private fun writePcm(
    samples: ShortArray,
    callback: SynthesisCallback,
  ): Boolean {
    val maxSamples = (callback.maxBufferSize / Short.SIZE_BYTES).coerceAtLeast(1)
    var offset = 0
    while (offset < samples.size) {
      if (stopped) return false
      val count = minOf(maxSamples, samples.size - offset)
      val buffer = ByteArray(count * Short.SIZE_BYTES)
      var byteIndex = 0
      for (sampleIndex in offset until offset + count) {
        val sample = samples[sampleIndex].toInt()
        buffer[byteIndex++] = sample.toByte()
        buffer[byteIndex++] = (sample shr 8).toByte()
      }
      if (callback.audioAvailable(buffer, 0, buffer.size) != TextToSpeech.SUCCESS) {
        return false
      }
      offset += count
    }
    return true
  }

  private fun languageAvailability(
    lang: String,
    country: String,
  ): Int = TranslatorTtsEngine.languageAvailability(lang, country)

  private fun applicationServices(): TranslatorApplication = application as TranslatorApplication
}

class TranslatorTtsDataActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    when (intent?.action) {
      TextToSpeech.Engine.ACTION_CHECK_TTS_DATA -> finishCheckTtsData()
      TextToSpeech.Engine.ACTION_GET_SAMPLE_TEXT -> finishSampleText()
      TextToSpeech.Engine.ACTION_INSTALL_TTS_DATA -> finishInstallTtsData()
      else -> finish()
    }
  }

  private fun finishCheckTtsData() {
    val installedLanguages =
      TranslatorTtsEngine.installedTtsLocales(this).map { it.language }.distinct()
    val systemDefault = Locale.getDefault()
    val voices =
      installedLanguages
        .mapNotNull { language ->
          if (language == systemDefault.language) {
            systemDefault.toTtsDataVoiceName()
          } else {
            Locale(language).toTtsDataVoiceName()
          }
        }
        .distinct()
        .let(::ArrayList)

    val data =
      Intent()
        .putStringArrayListExtra(TextToSpeech.Engine.EXTRA_AVAILABLE_VOICES, voices)
        .putStringArrayListExtra(TextToSpeech.Engine.EXTRA_UNAVAILABLE_VOICES, arrayListOf())
    setResult(
      if (voices.isEmpty()) TextToSpeech.Engine.CHECK_VOICE_DATA_FAIL else TextToSpeech.Engine.CHECK_VOICE_DATA_PASS,
      data,
    )
    finish()
  }

  private fun finishSampleText() {
    val iso3Lang = intent?.getStringExtra("language").orEmpty()
    val sample =
      TranslatorTtsEngine.sampleText(this, iso3Lang)
        ?: getString(R.string.tts_sample_fallback)
    val data = Intent().putExtra(TextToSpeech.Engine.EXTRA_SAMPLE_TEXT, sample)
    setResult(TextToSpeech.LANG_AVAILABLE, data)
    finish()
  }

  private fun finishInstallTtsData() {
    val intent =
      Intent(this, MainActivity::class.java)
        .addFlags(
          Intent.FLAG_ACTIVITY_NEW_TASK or
            Intent.FLAG_ACTIVITY_CLEAR_TOP or
            Intent.FLAG_ACTIVITY_SINGLE_TOP,
        ).putExtra(MainActivity.EXTRA_OPEN_LANGUAGE_MANAGER, true)
    startActivity(intent)
    setResult(Activity.RESULT_OK)
    finish()
  }
}

class TranslatorTtsSettingsRelayActivity : Activity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    startActivity(
      Intent(this, MainActivity::class.java)
        .addFlags(
          Intent.FLAG_ACTIVITY_NEW_TASK or
            Intent.FLAG_ACTIVITY_CLEAR_TOP or
            Intent.FLAG_ACTIVITY_SINGLE_TOP,
        ).putExtra(MainActivity.EXTRA_OPEN_LANGUAGE_MANAGER, true),
    )
    finish()
  }
}

data class TranslatorTtsVoice(
  val androidName: String,
  val languageCode: String,
  val packId: String?,
  val catalogVoiceName: String?,
  val locale: Locale,
  val quality: Int,
)

object TranslatorTtsEngine {
  private const val BOOTSTRAP_VOICE_PREFIX = "default|"
  private const val VOICE_SEPARATOR = "|"

  @Volatile
  private var cachedInstalledLocales: List<Locale>? = null

  fun sampleText(
    context: Context,
    iso3Language: String,
  ): String? {
    val app = context.applicationContext as? TranslatorApplication ?: return null
    val catalog = app.filePathManager.loadCatalog() ?: return null
    val iso2 = iso3LanguageToIso2(iso3Language).ifBlank { iso3Language }
    return catalog.ttsSampleText(iso2)
  }

  fun installedTtsLocales(context: Context): List<Locale> {
    cachedInstalledLocales?.let { return it }
    val app = context.applicationContext as? TranslatorApplication ?: return emptyList()
    val catalog = app.filePathManager.loadCatalog() ?: return emptyList()
    val result = mutableListOf<Locale>()
    for (row in catalog.languageRows) {
      if (!row.availability.ttsFiles) continue
      val installedRegions = catalog.installedTtsVoicePickerRegions(row.language.code)
      if (installedRegions.isEmpty()) {
        result += Locale(row.language.code)
      } else {
        for (region in installedRegions) {
          result += Locale(row.language.code, region.code)
        }
      }
    }
    cachedInstalledLocales = result
    return result
  }

  fun invalidateInstalledLocales() {
    cachedInstalledLocales = null
  }

  fun notifyVoiceDataChanged(context: Context) {
    cachedInstalledLocales = null
    context.applicationContext.sendBroadcast(
      android.content.Intent(TextToSpeech.Engine.ACTION_TTS_DATA_INSTALLED),
    )
  }

  fun bootstrapVoices(context: Context): List<TranslatorTtsVoice> {
    val seenLanguages = mutableSetOf<String>()
    return installedTtsLocales(context).mapNotNull { locale ->
      val iso3Language = locale.safeIso3Language()
      if (iso3Language.isBlank() || !seenLanguages.add(iso3Language)) return@mapNotNull null

      TranslatorTtsVoice(
        androidName = bootstrapVoiceName(iso3Language, "") ?: return@mapNotNull null,
        languageCode = locale.language,
        packId = null,
        catalogVoiceName = null,
        locale = Locale(locale.language),
        quality = Voice.QUALITY_NORMAL,
      )
    }
  }

  fun systemTtsVoices(context: Context): List<TranslatorTtsVoice> {
    val app = context.applicationContext as? TranslatorApplication ?: return emptyList()
    val catalog = app.filePathManager.loadCatalog() ?: return emptyList()
    val voices = mutableListOf<TranslatorTtsVoice>()
    for (row in catalog.languageRows) {
      if (!row.availability.ttsFiles) continue
      val lang = row.language.code
      val packs = catalog.installedTtsVoices(lang)
      if (packs.isEmpty()) continue
      val locale = localeFor(lang, null)
      for (pack in packs) {
        for (speaker in pack.voices) {
          voices +=
            TranslatorTtsVoice(
              androidName = androidVoiceName(lang, pack.packId, speaker.name),
              languageCode = lang,
              packId = pack.packId,
              catalogVoiceName = speaker.name,
              locale = locale,
              quality = qualityFor(speaker.name),
            )
        }
      }
    }
    return voices
  }

  fun resolveUserPreferredVoice(
    context: Context,
    language: String,
  ): TranslatorTtsVoice? {
    if (language.isBlank()) return null
    val app = context.applicationContext as? TranslatorApplication ?: return null
    val catalog = app.filePathManager.loadCatalog() ?: return null
    val iso2 = iso3LanguageToIso2(language).ifBlank { language }
    val packs = catalog.installedTtsVoices(iso2)
    if (packs.isEmpty()) return null

    val override = parseVoiceOverride(app.settingsManager.settings.value.ttsVoiceOverrides[iso2])
    val chosenPack =
      override?.packId?.let { pid -> packs.firstOrNull { it.packId == pid } }
        ?: override?.voiceName?.let { vn -> packs.firstOrNull { p -> p.voices.any { it.name == vn } } }
        ?: packs.first()
    val chosenSpeaker =
      override?.voiceName?.let { vn -> chosenPack.voices.firstOrNull { it.name == vn } }
        ?: chosenPack.voices.firstOrNull()
        ?: return null

    return TranslatorTtsVoice(
      androidName = androidVoiceName(iso2, chosenPack.packId, chosenSpeaker.name),
      languageCode = iso2,
      packId = chosenPack.packId,
      catalogVoiceName = chosenSpeaker.name,
      locale = localeFor(iso2, null),
      quality = qualityFor(chosenSpeaker.name),
    )
  }

  fun languageAvailability(
    lang: String,
    country: String,
  ): Int {
    if (country.isBlank()) {
      return TextToSpeech.LANG_AVAILABLE
    }
    return TextToSpeech.LANG_COUNTRY_AVAILABLE
  }

  fun bootstrapVoiceName(
    lang: String,
    country: String,
  ): String? {
    if (lang.isBlank()) return null
    return "$BOOTSTRAP_VOICE_PREFIX$lang"
  }

  fun isBootstrapVoiceName(voiceName: String): Boolean = voiceName.startsWith(BOOTSTRAP_VOICE_PREFIX)

  fun isCatalogVoiceName(voiceName: String): Boolean = voiceName.count { it == '|' } == 2

  fun voiceFromAndroidName(voiceName: String): TranslatorTtsVoice? {
    if (isBootstrapVoiceName(voiceName)) {
      return bootstrapVoice(voiceName.removePrefix(BOOTSTRAP_VOICE_PREFIX), "")
    }
    if (!isCatalogVoiceName(voiceName)) return null
    val parts = voiceName.split(VOICE_SEPARATOR)
    val languageCode = parts[0]
    val packId = parts[1].takeIf(String::isNotBlank)
    val catalogVoiceName = parts[2]
    if (languageCode.isBlank() || catalogVoiceName.isBlank()) return null
    return TranslatorTtsVoice(
      androidName = voiceName,
      languageCode = languageCode,
      packId = packId,
      catalogVoiceName = catalogVoiceName,
      locale = localeFor(languageCode, null),
      quality = qualityFor(catalogVoiceName),
    )
  }

  fun bootstrapVoice(
    lang: String,
    country: String,
  ): TranslatorTtsVoice {
    val languageCode = iso3LanguageToIso2(lang).ifBlank { lang }
    val countryCode = iso3CountryToIso2(country).ifBlank { country }
    return TranslatorTtsVoice(
      androidName = bootstrapVoiceName(lang, country) ?: "${BOOTSTRAP_VOICE_PREFIX}und$VOICE_SEPARATOR",
      languageCode = languageCode,
      packId = null,
      catalogVoiceName = null,
      locale = localeFor(languageCode, countryCode),
      quality = Voice.QUALITY_NORMAL,
    )
  }

  private fun androidVoiceName(
    languageCode: String,
    packId: String?,
    catalogVoiceName: String,
  ): String = "$languageCode$VOICE_SEPARATOR${packId.orEmpty()}$VOICE_SEPARATOR$catalogVoiceName"

  private fun localeFor(
    languageCode: String,
    regionCode: String?,
  ): Locale {
    val languageTag = languageCode.replace('_', '-')
    val tag = listOfNotNull(languageTag, regionCode?.takeIf(String::isNotBlank)).joinToString("-")
    return Locale.forLanguageTag(tag)
  }

  private fun qualityFor(voiceName: String): Int =
    when {
      voiceName.contains("-high") -> Voice.QUALITY_HIGH
      voiceName.contains("-x-low") -> Voice.QUALITY_LOW
      else -> Voice.QUALITY_NORMAL
    }

  private fun iso3LanguageToIso2(iso3Language: String): String =
    Locale.getISOLanguages().firstOrNull { language ->
      Locale(language).safeIso3Language() == iso3Language
    }.orEmpty()

  private fun iso3CountryToIso2(iso3Country: String): String =
    Locale.getISOCountries().firstOrNull { country ->
      Locale("", country).safeIso3Country() == iso3Country
    }.orEmpty()
}

private fun Locale.safeIso3Language(): String =
  try {
    getISO3Language().orEmpty()
  } catch (_: Exception) {
    ""
  }

private fun Locale.safeIso3Country(): String =
  try {
    getISO3Country().orEmpty()
  } catch (_: Exception) {
    ""
  }

private fun Locale.toTtsDataVoiceName(): String? {
  val language = safeIso3Language()
  if (language.isBlank()) return null

  val country = safeIso3Country()
  val variant = variant.orEmpty()
  return listOf(language, country, variant)
    .filter(String::isNotBlank)
    .joinToString("-")
}
