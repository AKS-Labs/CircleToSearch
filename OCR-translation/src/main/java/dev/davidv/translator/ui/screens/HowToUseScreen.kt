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

package dev.davidv.translator.ui.screens

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import dev.davidv.translator.R
import dev.davidv.translator.ui.components.AppCard
import dev.davidv.translator.ui.theme.TranslatorTheme

private data class Tip(
  val iconRes: Int,
  @StringRes val titleRes: Int,
  @StringRes val bodyRes: Int,
)

private data class TipSection(
  @StringRes val titleRes: Int,
  val tips: List<Tip>,
)

private val sections =
  listOf(
    TipSection(
      titleRes = R.string.howto_section_other_apps,
      tips =
        listOf(
          Tip(
            iconRes = R.drawable.share,
            titleRes = R.string.howto_share_title,
            bodyRes = R.string.howto_share_body,
          ),
          Tip(
            iconRes = R.drawable.copy,
            titleRes = R.string.howto_copy_title,
            bodyRes = R.string.howto_copy_body,
          ),
        ),
    ),
    TipSection(
      titleRes = R.string.howto_section_in_app,
      tips =
        listOf(
          Tip(
            iconRes = R.drawable.draft,
            titleRes = R.string.howto_docs_title,
            bodyRes = R.string.howto_docs_body,
          ),
          Tip(
            iconRes = R.drawable.dictionary,
            titleRes = R.string.howto_dict_title,
            bodyRes = R.string.howto_dict_body,
          ),
          Tip(
            iconRes = R.drawable.alt_route,
            titleRes = R.string.howto_alt_title,
            bodyRes = R.string.howto_alt_body,
          ),
          Tip(
            iconRes = R.drawable.volume_up,
            titleRes = R.string.howto_voice_title,
            bodyRes = R.string.howto_voice_body,
          ),
        ),
    ),
  )

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HowToUseScreen(onBack: () -> Unit) {
  Scaffold(
    containerColor = MaterialTheme.colorScheme.surfaceContainer,
    topBar = {
      TopAppBar(
        title = { Text(stringResource(R.string.howto_title)) },
        navigationIcon = {
          IconButton(onClick = onBack) {
            Icon(
              painter = painterResource(id = R.drawable.arrow_back),
              contentDescription = stringResource(R.string.a11y_back),
            )
          }
        },
      )
    },
  ) { paddingValues ->
    Column(
      modifier =
        Modifier
          .fillMaxSize()
          .padding(paddingValues)
          .padding(horizontal = 16.dp)
          .verticalScroll(rememberScrollState()),
    ) {
      Spacer(modifier = Modifier.height(8.dp))

      sections.forEach { section ->
        Text(
          text = stringResource(section.titleRes),
          style = MaterialTheme.typography.titleSmall,
          color = MaterialTheme.colorScheme.primary,
          modifier = Modifier.padding(top = 8.dp, bottom = 8.dp),
        )
        AppCard(
          modifier = Modifier.fillMaxWidth().testTag("export-section:" + stringResource(section.titleRes)),
        ) {
          Column(
            modifier = Modifier.padding(vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
          ) {
            section.tips.forEach { tip -> TipRow(tip) }
          }
        }
        Spacer(modifier = Modifier.height(8.dp))
      }

      Spacer(modifier = Modifier.height(16.dp))
    }
  }
}

@Composable
private fun TipRow(tip: Tip) {
  Row(
    modifier =
      Modifier
        .fillMaxWidth()
        .padding(horizontal = 16.dp, vertical = 12.dp),
    verticalAlignment = Alignment.Top,
  ) {
    Icon(
      painter = painterResource(id = tip.iconRes),
      contentDescription = null,
      tint = MaterialTheme.colorScheme.primary,
      modifier = Modifier.size(24.dp).padding(top = 2.dp),
    )
    Column(
      modifier = Modifier.padding(start = 16.dp),
    ) {
      Text(
        text = stringResource(tip.titleRes),
        style = MaterialTheme.typography.bodyMedium,
        fontWeight = FontWeight.SemiBold,
        color = MaterialTheme.colorScheme.onSurface,
      )
      Text(
        text = stringResource(tip.bodyRes),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(top = 4.dp),
      )
    }
  }
}

@Preview(showBackground = true)
@Composable
fun HowToUseScreenPreview() {
  TranslatorTheme {
    HowToUseScreen(onBack = {})
  }
}

@Preview(
  showBackground = true,
  uiMode = android.content.res.Configuration.UI_MODE_NIGHT_YES,
)
@Composable
fun HowToUseScreenDarkPreview() {
  TranslatorTheme {
    HowToUseScreen(onBack = {})
  }
}
