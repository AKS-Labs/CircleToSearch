package dev.davidv.translator

import uniffi.bindings.transliterateWithPolicyRecord

class TransliterateBinding {
  fun transliterate(
    text: String,
    languageCode: String,
    writingSystem: WritingSystem,
    japaneseDictPath: String? = null,
    japaneseSpaced: Boolean = true,
  ): String? =
    transliterateWithPolicyRecord(
      text,
      languageCode,
      writingSystem,
      japaneseDictPath,
      japaneseSpaced,
    )
}
