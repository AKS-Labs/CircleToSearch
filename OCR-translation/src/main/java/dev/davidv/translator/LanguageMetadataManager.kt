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

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "language_metadata")

class LanguageMetadataManager(
  private val context: Context,
  private val languagesFlow: StateFlow<List<Language>>,
) {
  private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

  val metadata: StateFlow<Map<Language, LanguageMetadata>> =
    combine(context.dataStore.data, languagesFlow) { prefs, languages ->
      languages.associateWith { lang ->
        (
          prefs[stringPreferencesKey("lang_${lang.code}")]?.let {
            Json.decodeFromString<LanguageMetadata>(it)
          } ?: LanguageMetadata()
        )
      }
    }.stateIn(
      scope = scope,
      started = SharingStarted.Eagerly,
      initialValue = emptyMap(),
    )

  fun updateLanguage(
    language: Language,
    metadata: LanguageMetadata,
  ) {
    scope.launch {
      context.dataStore.edit { prefs ->
        prefs[stringPreferencesKey("lang_${language.code}")] = Json.encodeToString(metadata)
      }
    }
  }
}
