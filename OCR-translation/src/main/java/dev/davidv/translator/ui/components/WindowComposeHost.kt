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
import android.view.View
import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.ComposeView
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.LifecycleRegistry
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.setViewTreeLifecycleOwner
import androidx.lifecycle.setViewTreeViewModelStoreOwner
import androidx.savedstate.SavedStateRegistry
import androidx.savedstate.SavedStateRegistryController
import androidx.savedstate.SavedStateRegistryOwner
import androidx.savedstate.setViewTreeSavedStateRegistryOwner

/**
 * Minimal lifecycle/owner scaffolding so a [ComposeView] can run inside a non-Activity window
 * (the accessibility and assistant overlays, which add Views to a WindowManager/Dialog window with
 * no ViewTree owners). The host is created RESUMED; call [dispose] when the window goes away.
 */
class WindowComposeHost(context: Context) : LifecycleOwner, ViewModelStoreOwner, SavedStateRegistryOwner {
  private val lifecycleRegistry = LifecycleRegistry(this)
  private val store = ViewModelStore()
  private val savedStateController = SavedStateRegistryController.create(this)

  override val lifecycle: Lifecycle get() = lifecycleRegistry
  override val viewModelStore: ViewModelStore get() = store
  override val savedStateRegistry: SavedStateRegistry get() = savedStateController.savedStateRegistry

  val view: ComposeView =
    ComposeView(context).also { composeView ->
      savedStateController.performRestore(null)
      lifecycleRegistry.currentState = Lifecycle.State.RESUMED
      installOn(composeView)
    }

  /**
   * Compose resolves its window recomposer from the *top-level* view of the window (the first child
   * of `android.R.id.content`, or the topmost view when there is none, as in a WindowManager/Dialog
   * window) and reads the ViewTree owners off it. Setting them only on the [ComposeView] is not
   * enough: the owners must live on whatever view roots this window, so install them there too.
   */
  fun installOn(target: View) {
    target.setViewTreeLifecycleOwner(this)
    target.setViewTreeViewModelStoreOwner(this)
    target.setViewTreeSavedStateRegistryOwner(this)
  }

  fun setContent(content: @Composable () -> Unit) = view.setContent(content)

  fun dispose() {
    lifecycleRegistry.currentState = Lifecycle.State.DESTROYED
    store.clear()
  }
}
