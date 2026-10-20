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

import android.media.MediaPlayer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

sealed interface SamplePlaybackState {
  data object Idle : SamplePlaybackState

  data class Loading(
    val packId: String,
  ) : SamplePlaybackState

  data class Playing(
    val packId: String,
  ) : SamplePlaybackState
}

class SamplePlayer(
  private val cacheRoot: File,
  private val scope: CoroutineScope,
) {
  var state by mutableStateOf<SamplePlaybackState>(SamplePlaybackState.Idle)
    private set

  private var player: MediaPlayer? = null
  private var fetchJob: Job? = null

  fun isCached(packId: String): Boolean = cacheFile(packId).exists()

  fun toggle(
    packId: String,
    sampleUrl: String,
  ) {
    val current = state
    when {
      current is SamplePlaybackState.Playing && current.packId == packId -> stopAndReset()
      current is SamplePlaybackState.Loading && current.packId == packId -> stopAndReset()
      else -> play(packId, sampleUrl)
    }
  }

  fun release() = stopAndReset()

  private fun play(
    packId: String,
    sampleUrl: String,
  ) {
    stopAndReset()
    state = SamplePlaybackState.Loading(packId)
    fetchJob =
      scope.launch {
        val file = ensureCached(packId, sampleUrl)
        if (file == null) {
          state = SamplePlaybackState.Idle
          return@launch
        }
        if (state !is SamplePlaybackState.Loading || (state as SamplePlaybackState.Loading).packId != packId) {
          return@launch
        }
        startPlayback(packId, file)
      }
  }

  private fun stopAndReset() {
    fetchJob?.cancel()
    fetchJob = null
    player?.let { mp ->
      runCatching { if (mp.isPlaying) mp.stop() }
      mp.release()
    }
    player = null
    state = SamplePlaybackState.Idle
  }

  private suspend fun ensureCached(
    packId: String,
    sampleUrl: String,
  ): File? =
    withContext(Dispatchers.IO) {
      val target = cacheFile(packId)
      if (target.exists() && target.length() > 0) return@withContext target
      target.parentFile?.mkdirs()
      val tmp = File(target.parentFile, "${target.name}.tmp")
      runCatching {
        val connection =
          (URL(sampleUrl).openConnection() as HttpURLConnection).apply {
            connectTimeout = 10_000
            readTimeout = 15_000
          }
        connection.inputStream.use { input ->
          tmp.outputStream().use { output -> input.copyTo(output) }
        }
        connection.disconnect()
        if (!tmp.renameTo(target)) {
          tmp.delete()
          null
        } else {
          target
        }
      }.onFailure { tmp.delete() }.getOrNull()
    }

  private fun startPlayback(
    packId: String,
    file: File,
  ) {
    val mp =
      MediaPlayer().apply {
        setDataSource(file.absolutePath)
        setOnPreparedListener { start() }
        setOnCompletionListener {
          if ((this@SamplePlayer.state as? SamplePlaybackState.Playing)?.packId == packId) {
            this@SamplePlayer.state = SamplePlaybackState.Idle
          }
          it.release()
          if (player === it) player = null
        }
        setOnErrorListener { mp, _, _ ->
          if ((this@SamplePlayer.state as? SamplePlaybackState.Playing)?.packId == packId) {
            this@SamplePlayer.state = SamplePlaybackState.Idle
          }
          mp.release()
          if (player === mp) player = null
          true
        }
        prepareAsync()
      }
    player = mp
    state = SamplePlaybackState.Playing(packId)
  }

  private fun cacheFile(packId: String): File = File(cacheRoot, "$packId.opus")
}
