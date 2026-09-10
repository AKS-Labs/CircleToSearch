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

import android.app.Activity
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.speech.RecognizerIntent
import android.view.ViewTreeObserver
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.davidv.translator.Language
import dev.davidv.translator.R
import dev.davidv.translator.ReadingOrder
import dev.davidv.translator.TranslatorMessage
import dev.davidv.translator.browser.BrowserActivity
import dev.davidv.translator.isSupportedDocumentUrl
import dev.davidv.translator.isWebUrl

@Composable
fun ShareImage(onMessage: (TranslatorMessage) -> Unit) {
  ActionPillButton(
    iconRes = R.drawable.share,
    contentDescription = stringResource(R.string.a11y_share_image),
    showBackdrop = true,
    onClick = { onMessage(TranslatorMessage.ShareTranslatedImage) },
  )
}

@Composable
fun JapaneseOcrModeToggle(
  readingOrder: ReadingOrder?,
  onMessage: (TranslatorMessage) -> Unit,
) {
  val (iconRes, description) =
    when (readingOrder) {
      null -> R.drawable.text_rotation_auto to "Japanese OCR auto mode"
      ReadingOrder.TOP_TO_BOTTOM_RIGHT_TO_LEFT -> R.drawable.text_rotate_vertical to "Japanese OCR vertical mode"
      ReadingOrder.LEFT_TO_RIGHT -> R.drawable.text_rotation_none to "Japanese OCR horizontal mode"
    }
  ActionPillButton(
    iconRes = iconRes,
    contentDescription = description,
    showBackdrop = true,
    onClick = { onMessage(TranslatorMessage.ToggleJapaneseOcrMode) },
  )
}

@Composable
fun ClearInput(
  onMessage: (TranslatorMessage) -> Unit,
  showBackdrop: Boolean = false,
) {
  ActionPillButton(
    iconRes = R.drawable.cancel,
    contentDescription = stringResource(R.string.a11y_clear_input),
    showBackdrop = showBackdrop,
    onClick = { onMessage(TranslatorMessage.ClearInput) },
  )
}

// Paste the clipboard into the input. A pasted web URL opens the browser
// instead (unsupported document URLs toast); everything else becomes input text.
fun pasteFromClipboard(
  context: Context,
  onMessage: (TranslatorMessage) -> Unit,
) {
  val clipboardManager = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
  val clipData = clipboardManager.primaryClip ?: return
  if (clipData.itemCount == 0) return
  val text = clipData.getItemAt(0).text?.toString() ?: ""
  val trimmed = text.trim()
  if (!isWebUrl(trimmed)) {
    onMessage(TranslatorMessage.TextInput(text))
    return
  }
  if (isSupportedDocumentUrl(trimmed)) {
    Toast.makeText(context, context.getString(R.string.doc_url_unsupported), Toast.LENGTH_LONG).show()
    return
  }
  val browserIntent =
    Intent(context, BrowserActivity::class.java).apply {
      putExtra(BrowserActivity.EXTRA_URL, trimmed)
    }
  context.startActivity(browserIntent)
}

// Tracks whether the clipboard currently holds pasteable text, so a paste
// button can be hidden when there's nothing to paste. Only reads the clip
// description (MIME types), never the content, so it never trips the
// Android 12+ clipboard-access toast.
@Composable
fun rememberClipboardHasText(): Boolean {
  val context = LocalContext.current
  val view = LocalView.current
  val clipboardManager = remember { context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager }
  var hasText by remember { mutableStateOf(clipboardManager.clipboardHasText()) }

  DisposableEffect(clipboardManager, view) {
    val refresh = { hasText = clipboardManager.clipboardHasText() }
    val clipListener = ClipboardManager.OnPrimaryClipChangedListener { refresh() }
    clipboardManager.addPrimaryClipChangedListener(clipListener)
    // Clipboard reads are blocked until the window regains focus (Android 10+),
    // and a copy done in another app while we were backgrounded doesn't deliver
    // a clip-changed event. Re-read on focus gain, when access is permitted.
    val focusListener =
      ViewTreeObserver.OnWindowFocusChangeListener { hasFocus ->
        if (hasFocus) {
          refresh()
        }
      }
    view.viewTreeObserver.addOnWindowFocusChangeListener(focusListener)
    onDispose {
      clipboardManager.removePrimaryClipChangedListener(clipListener)
      view.viewTreeObserver.removeOnWindowFocusChangeListener(focusListener)
    }
  }
  return hasText
}

private fun ClipboardManager.clipboardHasText(): Boolean {
  if (!hasPrimaryClip()) return false
  val description = primaryClipDescription ?: return false
  return description.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) ||
    description.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
}

@Composable
fun SpeechInputButton(
  input: String,
  from: Language,
  onMessage: (TranslatorMessage) -> Unit,
  modifier: Modifier = Modifier,
) {
  val context = LocalContext.current
  val isAvailable =
    remember {
      Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).resolveActivity(context.packageManager) != null
    }
  if (!isAvailable) {
    return
  }

  val launcher =
    rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
      if (result.resultCode != Activity.RESULT_OK) {
        return@rememberLauncherForActivityResult
      }
      val spoken =
        result.data
          ?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)
          ?.firstOrNull()
      if (!spoken.isNullOrBlank()) {
        val appended = if (input.isBlank()) spoken else "${input.trimEnd()} $spoken"
        onMessage(TranslatorMessage.TextInput(appended))
      }
    }

  ActionPillButton(
    iconRes = R.drawable.mic,
    contentDescription = stringResource(R.string.a11y_voice_input),
    modifier = modifier,
    onClick = {
      val intent =
        Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
          putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
          putExtra(RecognizerIntent.EXTRA_LANGUAGE, from.code)
        }
      launcher.launch(intent)
    },
  )
}

@Composable
fun ActionPillButton(
  iconRes: Int,
  contentDescription: String,
  showBackdrop: Boolean = false,
  modifier: Modifier = Modifier,
  enabled: Boolean = true,
  onClick: () -> Unit,
) {
  if (showBackdrop) {
    Surface(
      modifier = modifier,
      shape = CircleShape,
      color = Color(0xCC303030),
    ) {
      IconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.size(36.dp),
      ) {
        Icon(
          painterResource(id = iconRes),
          contentDescription = contentDescription,
          tint = Color.White,
        )
      }
    }
  } else {
    IconButton(
      onClick = onClick,
      enabled = enabled,
      modifier = modifier.size(36.dp),
    ) {
      Icon(
        painterResource(id = iconRes),
        contentDescription = contentDescription,
        tint =
          if (enabled) {
            MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
          } else {
            MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
          },
      )
    }
  }
}

// A card-header tool button. The active state is shown as a soft squircle
// background behind the icon rather than by tinting the glyph.
@Composable
fun ToolIconButton(
  iconRes: Int,
  contentDescription: String,
  onClick: () -> Unit,
  modifier: Modifier = Modifier,
  active: Boolean = false,
  enabled: Boolean = true,
  // Renders greyed-out like a disabled button but stays clickable, so the tap
  // can explain why the feature is unavailable (e.g. a toast).
  available: Boolean = true,
) {
  val background =
    if (active) MaterialTheme.colorScheme.primary.copy(alpha = 0.14f) else Color.Transparent
  val tint =
    when {
      !enabled || !available -> MaterialTheme.colorScheme.onSurface.copy(alpha = 0.3f)
      active -> MaterialTheme.colorScheme.primary
      else -> MaterialTheme.colorScheme.onSurfaceVariant
    }
  Box(
    modifier =
      modifier
        .size(40.dp)
        .clip(RoundedCornerShape(12.dp))
        .background(background)
        .clickable(
          enabled = enabled,
          role = Role.Button,
          onClick = onClick,
        ),
    contentAlignment = Alignment.Center,
  ) {
    Icon(
      painterResource(id = iconRes),
      contentDescription = contentDescription,
      tint = tint,
      modifier = Modifier.size(24.dp),
    )
  }
}
