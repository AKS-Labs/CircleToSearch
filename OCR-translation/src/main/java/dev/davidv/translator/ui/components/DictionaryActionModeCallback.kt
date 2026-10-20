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

import android.content.Context
import android.content.Intent
import android.util.Log
import android.view.ActionMode
import android.view.Menu
import android.view.MenuItem
import android.widget.TextView
import dev.davidv.translator.browser.BrowserActivity
import dev.davidv.translator.isWebUrl

class DictionaryActionModeCallback(
  private val context: Context,
  private val onDictionaryLookup: (String) -> Unit,
  // When set, an "Alternatives" item is offered — but only for selections that
  // actually have alternatives (per [hasAlternatives]). Clicking reports the
  // selection's char range so the caller can open the matching word's options.
  private val onAlternatives: ((Int, Int) -> Unit)? = null,
  private val hasAlternatives: ((Int, Int) -> Boolean)? = null,
) : ActionMode.Callback2() {
  private var currentTextView: TextView? = null

  fun setTextView(textView: TextView) {
    currentTextView = textView
  }

  override fun onCreateActionMode(
    mode: ActionMode?,
    menu: Menu?,
  ): Boolean {
    syncCustomItems(menu)
    return true
  }

  override fun onPrepareActionMode(
    mode: ActionMode?,
    menu: Menu?,
  ): Boolean {
    menu?.let { m ->
      val itemsToRemove = mutableListOf<Int>()
      for (i in 0 until m.size()) {
        val item = m.getItem(i)
        if (item.intent?.action == Intent.ACTION_TRANSLATE ||
          item.intent?.`package` == context.packageName ||
          item.title == "Translate"
        ) {
          itemsToRemove.add(item.itemId)
        }
      }
      itemsToRemove.forEach { m.removeItem(it) }

      syncCustomItems(m)
    }
    Log.d("DictionaryActionMode", "Show menu now")
    return true
  }

  private fun syncCustomItems(menu: Menu?) {
    menu ?: return
    val selectionIsUrl = isSelectionUrl()
    val hasDictionary = menu.findItem(DICTIONARY_ID) != null
    val hasTranslateUrl = menu.findItem(TRANSLATE_URL_ID) != null

    val hasAlternativesItem = menu.findItem(ALTERNATIVES_ID) != null
    val offerAlternatives = onAlternatives != null && selectionHasAlternatives()
    if (selectionIsUrl) {
      if (hasDictionary) menu.removeItem(DICTIONARY_ID)
      if (hasAlternativesItem) menu.removeItem(ALTERNATIVES_ID)
      if (!hasTranslateUrl) menu.add(0, TRANSLATE_URL_ID, 0, "Translate URL")
    } else {
      if (hasTranslateUrl) menu.removeItem(TRANSLATE_URL_ID)
      if (!hasDictionary) menu.add(0, DICTIONARY_ID, 0, "Dictionary")
      if (offerAlternatives && !hasAlternativesItem) {
        menu.add(0, ALTERNATIVES_ID, 1, "Alternatives")
      } else if (!offerAlternatives && hasAlternativesItem) {
        menu.removeItem(ALTERNATIVES_ID)
      }
    }
  }

  private fun selectionHasAlternatives(): Boolean {
    val tv = currentTextView ?: return false
    val check = hasAlternatives ?: return false
    val start = tv.selectionStart
    val end = tv.selectionEnd
    return start in 0 until end && check(start, end)
  }

  private fun isSelectionUrl(): Boolean = selectedText()?.let { isWebUrl(it) } ?: false

  private fun selectedText(): String? {
    val tv = currentTextView ?: return null
    val start = tv.selectionStart
    val end = tv.selectionEnd
    if (start < 0 || end <= start) return null
    return tv.text?.subSequence(start, end)?.toString()
  }

  override fun onActionItemClicked(
    mode: ActionMode?,
    item: MenuItem?,
  ): Boolean {
    Log.d("DictionaryActionMode", "Clicked '${item?.itemId}' == '${item?.title}'")
    return when (item?.itemId) {
      DICTIONARY_ID -> {
        val selected = selectedText().orEmpty()
        if (selected.isNotBlank()) {
          onDictionaryLookup(selected)
        }
        mode?.finish()
        true
      }

      ALTERNATIVES_ID -> {
        val tv = currentTextView
        val cb = onAlternatives
        if (tv != null && cb != null) {
          val start = tv.selectionStart
          val end = tv.selectionEnd
          if (start in 0 until end) cb(start, end)
        }
        mode?.finish()
        true
      }

      TRANSLATE_URL_ID -> {
        val url = selectedText()?.trim()
        if (!url.isNullOrBlank() && isWebUrl(url)) {
          val intent =
            Intent(context, BrowserActivity::class.java).apply {
              putExtra(BrowserActivity.EXTRA_URL, url)
              addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
          context.startActivity(intent)
        }
        mode?.finish()
        true
      }

      else -> false
    }
  }

  override fun onDestroyActionMode(mode: ActionMode?) {}

  companion object {
    private const val DICTIONARY_ID = 12345
    private const val TRANSLATE_URL_ID = 12346
    private const val ALTERNATIVES_ID = 12347
  }
}
