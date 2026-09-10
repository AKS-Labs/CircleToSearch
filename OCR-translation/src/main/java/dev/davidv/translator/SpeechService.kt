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

import android.util.Log
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.withContext

class SpeechService(
  private val settingsManager: SettingsManager,
  private val filePathManager: FilePathManager,
) {
  suspend fun synthesizeSpeech(
    language: Language,
    text: String,
  ): SpeechSynthesisResult =
    withContext(Dispatchers.IO) {
      if (text.isBlank()) {
        return@withContext SpeechSynthesisResult.Error(SpeechError.NothingToSpeak)
      }

      val catalog =
        filePathManager.loadCatalog()
          ?: return@withContext SpeechSynthesisResult.Error(SpeechError.CatalogUnavailable)
      if (!catalog.hasTtsVoices(language.code)) {
        return@withContext SpeechSynthesisResult.Error(SpeechError.NoVoiceInstalled(language))
      }

      val settings = settingsManager.settings.value
      val parsed = parseVoiceOverride(settings.ttsVoiceOverrides[language.code])
      val selectedVoiceName = parsed?.voiceName
      val selectedPackId = parsed?.packId
      val speechSpeedVoiceName =
        selectedVoiceName
          ?: catalog.availableTtsVoices(language.code).firstOrNull()?.name
      val speechSpeed =
        speechSpeedVoiceName
          ?.let { voiceName -> settings.ttsPlaybackSpeedOverrides["${language.code}:$voiceName"] }
          ?: settings.ttsPlaybackSpeed
      val boundedSpeechSpeed = speechSpeed.coerceIn(0.5f, 2.0f)
      Log.d(
        "SpeechService",
        "Using TTS pack=$selectedPackId voice=$selectedVoiceName speed=$boundedSpeechSpeed lang=${language.code}",
      )
      val chunkRequests =
        try {
          catalog.planSpeechChunks(
            languageCode = language.code,
            text = text,
            packId = selectedPackId,
            readUrlsAndHashtags = settings.ttsReadUrlsAndHashtags,
          )
        } catch (e: uniffi.bindings.CatalogException) {
          Log.e("SpeechService", "planSpeechChunks failed lang=${language.code} pack=$selectedPackId", e)
          return@withContext SpeechSynthesisResult.Error(
            SpeechError.SynthesisError(language, e.message ?: e.toString()),
          )
        }
      if (chunkRequests.isEmpty()) {
        return@withContext SpeechSynthesisResult.Error(SpeechError.SynthesisFailed(language))
      }

      SpeechSynthesisResult.Success(
        flow {
          for ((index, chunkRequest) in chunkRequests.withIndex()) {
            currentCoroutineContext().ensureActive()
            Log.d(
              "SpeechService",
              "Speech chunk ${index + 1}/${chunkRequests.size}: synth start isPhonemes=${chunkRequest.isPhonemes} textLen=${chunkRequest.content.length} pauseAfterMs=${chunkRequest.pauseAfterMs}",
            )
            val pcmAudio =
              try {
                catalog.synthesizeSpeechPcm(
                  languageCode = language.code,
                  text = chunkRequest.content,
                  speechSpeed = boundedSpeechSpeed,
                  voiceName = selectedVoiceName,
                  isPhonemes = chunkRequest.isPhonemes,
                  packId = selectedPackId,
                )
              } catch (e: uniffi.bindings.CatalogException) {
                Log.e("SpeechService", "synthesizeSpeechPcm failed lang=${language.code} pack=$selectedPackId", e)
                throw IllegalStateException(
                  "Speech synthesis failed for ${language.displayName}: ${e.message ?: e.toString()}",
                  e,
                )
              }
            val audioDurationMs = (pcmAudio.pcmSamples.size * 1000L) / pcmAudio.sampleRate
            Log.d(
              "SpeechService",
              "Speech chunk ${index + 1}/${chunkRequests.size}: synth ready samples=${pcmAudio.pcmSamples.size} sampleRate=${pcmAudio.sampleRate} audioMs=$audioDurationMs",
            )
            currentCoroutineContext().ensureActive()
            emit(pcmAudio)

            val silenceChunk =
              chunkRequest.pauseAfterMs?.let { pauseMs ->
                PcmAudio.silence(pcmAudio.sampleRate, pauseMs)
              }
            if (silenceChunk != null) {
              emit(silenceChunk)
            }
          }
        },
      )
    }

  suspend fun availableTtsVoices(language: Language): List<TtsVoiceOption> =
    withContext(Dispatchers.IO) {
      val catalog = filePathManager.loadCatalog() ?: return@withContext emptyList()
      catalog.availableTtsVoices(language.code)
    }

  suspend fun installedTtsVoices(language: Language): List<uniffi.translator_core.InstalledTtsPack> =
    withContext(Dispatchers.IO) {
      val catalog = filePathManager.loadCatalog() ?: return@withContext emptyList()
      catalog.installedTtsVoices(language.code)
    }
}

data class VoiceOverride(
  val packId: String?,
  val voiceName: String,
)

private const val OVERRIDE_SEPARATOR = "|"

fun encodeVoiceOverride(
  packId: String,
  voiceName: String,
): String = "$packId$OVERRIDE_SEPARATOR$voiceName"

fun parseVoiceOverride(raw: String?): VoiceOverride? {
  if (raw.isNullOrBlank()) return null
  val sepIndex = raw.indexOf(OVERRIDE_SEPARATOR)
  return if (sepIndex < 0) {
    VoiceOverride(packId = null, voiceName = raw)
  } else {
    VoiceOverride(
      packId = raw.substring(0, sepIndex).takeIf(String::isNotBlank),
      voiceName = raw.substring(sepIndex + 1),
    )
  }
}

sealed class SpeechSynthesisResult {
  data class Success(
    val audioChunks: Flow<PcmAudio>,
  ) : SpeechSynthesisResult()

  data class Error(
    val reason: SpeechError,
  ) : SpeechSynthesisResult()
}

sealed interface SpeechError {
  data object NothingToSpeak : SpeechError

  data object CatalogUnavailable : SpeechError

  data class NoVoiceInstalled(
    val language: Language,
  ) : SpeechError

  data class SynthesisFailed(
    val language: Language,
  ) : SpeechError

  data class SynthesisError(
    val language: Language,
    val reason: String,
  ) : SpeechError
}
