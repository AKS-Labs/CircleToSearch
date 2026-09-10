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

package dev.davidv.translator.ui.components

import android.content.res.Configuration
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.davidv.translator.LangAvailability
import dev.davidv.translator.Language
import dev.davidv.translator.LanguageAvailabilityEntry
import dev.davidv.translator.LanguageAvailabilityState
import dev.davidv.translator.LanguageMetadata
import dev.davidv.translator.R
import dev.davidv.translator.Script
import dev.davidv.translator.TranslatorMessage
import dev.davidv.translator.singleScript
import dev.davidv.translator.ui.theme.TranslatorTheme

@Composable
fun LanguageSelectionRow(
  from: Language,
  to: Language,
  canSwap: Boolean,
  languageState: LanguageAvailabilityState,
  languageMetadata: Map<Language, LanguageMetadata>,
  onMessage: (TranslatorMessage) -> Unit,
  onSettings: (() -> Unit)?,
  drawable: Pair<String, Int>,
  isAutoSource: Boolean = false,
  detectedInstalled: Language? = null,
  showAutoOption: Boolean = false,
) {
  val titleStyle =
    MaterialTheme.typography.titleMedium.copy(
      fontWeight = FontWeight.SemiBold,
      fontSize = 18.sp,
    )

  Row(
    modifier = Modifier.fillMaxWidth(),
    horizontalArrangement = Arrangement.spacedBy(2.dp),
    verticalAlignment = Alignment.CenterVertically,
  ) {
    val fromLanguages =
      languageState.allLanguages().filter { x ->
        (isAutoSource || x != from) && (languageState.availabilityFor(x)?.hasToEnglish == true || x.isEnglish)
      }
    val toLanguages =
      languageState.allLanguages().filter { x ->
        (isAutoSource || x != from) && x != to && (languageState.availabilityFor(x)?.hasFromEnglish == true || x.isEnglish)
      }

    LanguageSelector(
      selectedLanguage = from,
      availableLanguages = fromLanguages,
      languageMetadata = languageMetadata,
      onLanguageSelected = { language ->
        onMessage(TranslatorMessage.FromLang(language))
      },
      modifier = Modifier.widthIn(max = 150.dp),
      isAutoSource = isAutoSource,
      detectedInstalled = detectedInstalled,
      showAutoOption = showAutoOption,
      onAutoSelected = { onMessage(TranslatorMessage.EnableAutoSource) },
      centered = false,
      textStyle = titleStyle,
    )
    LanguageSwapButton(
      onClick = { onMessage(TranslatorMessage.SwapLanguages) },
      enabled = canSwap && !(isAutoSource && detectedInstalled == null),
    )

    LanguageSelector(
      selectedLanguage = to,
      availableLanguages = toLanguages,
      languageMetadata = languageMetadata,
      onLanguageSelected = { language ->
        onMessage(TranslatorMessage.ToLang(language))
      },
      modifier = Modifier.widthIn(max = 150.dp),
      centered = false,
      textStyle = titleStyle,
    )

    Spacer(modifier = Modifier.weight(1f))

    if (onSettings != null) {
      IconButton(onClick = onSettings, modifier = Modifier.size(36.dp)) {
        Icon(
          painterResource(id = drawable.second),
          contentDescription = drawable.first,
        )
      }
    } else {
      Spacer(modifier = Modifier.size(48.dp))
    }
  }
}

@Composable
fun LanguageSwapButton(
  onClick: () -> Unit,
  enabled: Boolean = true,
) {
  IconButton(
    onClick = onClick,
    enabled = enabled,
    modifier = Modifier.size(36.dp),
  ) {
    Icon(
      painterResource(id = R.drawable.compare),
      contentDescription = stringResource(R.string.a11y_reverse_translation_direction),
      tint = MaterialTheme.colorScheme.primary,
      modifier = Modifier.size(20.dp),
    )
  }
}

private fun previewLanguage(
  code: String,
  name: String,
) = Language(
  code = code,
  displayName = name,
  shortDisplayName = name,
  script = Script.LATIN,
  writingSystem = singleScript(Script.LATIN),
  dictionaryCode = code,
)

@Preview(showBackground = true)
@Composable
fun LanguageSelectionRowPreview() {
  TranslatorTheme {
    LanguageSelectionRow(
      from = previewLanguage("en", "English"),
      to = previewLanguage("es", "Spanish"),
      canSwap = true,
      languageState = previewAvailability(),
      languageMetadata = mapOf(previewLanguage("es", "Spanish") to LanguageMetadata(favorite = true)),
      onMessage = {},
      onSettings = {},
      drawable = Pair("Settings", R.drawable.settings),
    )
  }
}

private fun previewAvailability() =
  LanguageAvailabilityState(
    hasLanguages = true,
    availableLanguages =
      listOf(
        LanguageAvailabilityEntry(
          previewLanguage("en", "English"),
          LangAvailability(hasFromEnglish = true, hasToEnglish = true, ocrFiles = true, dictionaryFiles = false),
        ),
        LanguageAvailabilityEntry(
          previewLanguage("es", "Spanish"),
          LangAvailability(hasFromEnglish = true, hasToEnglish = true, ocrFiles = true, dictionaryFiles = false),
        ),
        LanguageAvailabilityEntry(
          previewLanguage("fr", "French"),
          LangAvailability(hasFromEnglish = true, hasToEnglish = true, ocrFiles = true, dictionaryFiles = false),
        ),
        LanguageAvailabilityEntry(
          previewLanguage("de", "German"),
          LangAvailability(hasFromEnglish = true, hasToEnglish = true, ocrFiles = true, dictionaryFiles = false),
        ),
      ),
    isChecking = false,
  )

@Preview(
  showBackground = true,
  uiMode = Configuration.UI_MODE_NIGHT_YES,
)
@Composable
fun LanguageSelectionRowDarkPreview() {
  TranslatorTheme {
    LanguageSelectionRow(
      from = previewLanguage("fr", "French"),
      to = previewLanguage("de", "German"),
      canSwap = true,
      languageState = previewAvailability(),
      languageMetadata = mapOf(previewLanguage("fr", "French") to LanguageMetadata(favorite = true)),
      onMessage = {},
      onSettings = {},
      drawable = Pair("Settings", R.drawable.settings),
    )
  }
}
