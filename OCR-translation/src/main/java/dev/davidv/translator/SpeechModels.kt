package dev.davidv.translator

data class TtsVoiceOption(
  val name: String,
  val speakerId: Int,
  val displayName: String,
)

data class SpeechChunkPlan(
  val content: String,
  val isPhonemes: Boolean,
  val pauseAfterMs: Int?,
)
