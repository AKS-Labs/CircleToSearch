package dev.davidv.translator

data class TranslatedText(
  val translated: String,
  val transliterated: String?,
  // Source text that produced this translation, needed to re-translate (steer)
  // when the user picks an alternative. Empty for paths that don't carry it.
  val source: String = "",
  // Per-word alternatives on the translated text (char offsets into `translated`).
  // Empty unless produced by the alternatives-bearing translation path.
  val alternatives: List<WordAlternatives> = emptyList(),
)
