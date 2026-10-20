package dev.davidv.translator

import androidx.annotation.StringRes

typealias ReadingOrder = uniffi.translator_core.ReadingOrder
typealias Script = uniffi.translator_core.Script
typealias WritingSystem = uniffi.translator_core.WritingSystem

// A typealias does not expose the nested classifiers of what it points at, so
// `WritingSystem.Single` cannot be reached through the alias above the way
// `Script.LATIN` can -- enum entries resolve through an alias, nested classes do not.
fun singleScript(script: Script): WritingSystem = uniffi.translator_core.WritingSystem.Single(script)
typealias SelectionView = uniffi.translator_core.SelectionView
typealias WritingAxis = uniffi.translator_core.WritingAxis
typealias Point = uniffi.translator_core.Point
typealias UrlsAndHashtags = uniffi.translator_core.UrlsAndHashtags
typealias NothingReason = uniffi.translator_translate.NothingReason
typealias BackgroundMode = uniffi.translator_core.BackgroundMode
typealias PreparedImageOverlay = uniffi.translator_core.PreparedImageOverlay
typealias TokenAlignment = uniffi.translator_translate.TokenAlignment
typealias TranslationWithAlignment = uniffi.translator_translate.TranslationWithAlignment
typealias WordAlternative = uniffi.translator_translate.WordAlternative
typealias WordAlternatives = uniffi.translator_translate.WordAlternatives
typealias TranslationWithAlternatives = uniffi.translator_translate.TranslationWithAlternatives
typealias Feature = uniffi.translator.Feature
typealias DownloadPlan = uniffi.translator_core.DownloadPlan
typealias DownloadTask = uniffi.translator_core.DownloadTask
typealias DeletePlan = uniffi.translator_core.DeletePlan
typealias TtsVoicePackInfo = uniffi.translator_core.TtsVoicePackInfo
typealias TtsVoicePickerRegion = uniffi.translator_core.TtsVoicePickerRegion

@get:StringRes
val BackgroundMode.labelRes: Int
  get() =
    when (this) {
      BackgroundMode.WHITE_ON_BLACK -> R.string.background_mode_white_on_black
      BackgroundMode.BLACK_ON_WHITE -> R.string.background_mode_black_on_white
      BackgroundMode.AUTO_DETECT -> R.string.background_mode_auto_detect
    }
