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
import android.net.Uri
import android.provider.OpenableColumns
import java.io.File

// Shared content-URI import helpers used by both the file picker
// (ImageCaptureComponent) and shared-document intents (MainActivity →
// TranslatorViewModel), so a picked file and a shared file reach the document
// drawer through the same code.

internal fun displayNameForUri(
  context: Context,
  uri: Uri,
): String? =
  context.contentResolver
    .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
    ?.use { cursor ->
      if (cursor.moveToFirst()) {
        val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (index >= 0) cursor.getString(index) else null
      } else {
        null
      }
    }

internal fun sizeBytesForUri(
  context: Context,
  uri: Uri,
): Long? =
  context.contentResolver
    .query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)
    ?.use { cursor ->
      if (cursor.moveToFirst()) {
        val index = cursor.getColumnIndex(OpenableColumns.SIZE)
        if (index >= 0 && !cursor.isNull(index)) cursor.getLong(index) else null
      } else {
        null
      }
    }

internal fun documentFileExtension(
  context: Context,
  uri: Uri,
): String {
  val displayName = displayNameForUri(context, uri)
  val displayExtension =
    displayName
      ?.substringAfterLast('.', missingDelimiterValue = "")
      ?.takeIf { it.isNotBlank() && it.length <= 8 }
  if (displayExtension != null) return ".$displayExtension"

  return when (context.contentResolver.getType(uri)) {
    "application/pdf" -> ".pdf"
    "text/plain" -> ".txt"
    "application/vnd.oasis.opendocument.text" -> ".odt"
    "application/epub+zip" -> ".epub"
    else -> ".bin"
  }
}

internal fun copyDocumentUriToCache(
  context: Context,
  uri: Uri,
): File {
  val outputFile = File.createTempFile("input_document_", documentFileExtension(context, uri), context.cacheDir)
  context.contentResolver.openInputStream(uri).use { input ->
    requireNotNull(input) { "Unable to open selected document" }
    outputFile.outputStream().use { output ->
      input.copyTo(output)
    }
  }
  return outputFile
}

internal fun isImageUri(
  context: Context,
  uri: Uri,
): Boolean = context.contentResolver.getType(uri)?.startsWith("image/") == true
