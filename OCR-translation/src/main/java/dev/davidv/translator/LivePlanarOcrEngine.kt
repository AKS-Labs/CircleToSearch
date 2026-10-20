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

import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import uniffi.bindings.PipelineTargetMode
import uniffi.bindings.PlanarTrackerState

private const val TAG_PLANAR = "LivePlanarOcrEngine"

/** Debug status snapshot for the on-screen tracker pill. */
data class TrackerStatus(
  val state: PlanarTrackerState,
  val anchorId: Long,
  /** Low-16 id of the chain root, straight from the per-frame result
   *  and never overwritten by acquire telemetry (unlike [anchorId],
   *  which the debug pill repoints at the last acquired anchor). The
   *  focus controller watches this to detect a full re-acquire — a new
   *  root means the scale baseline reset and focus must re-baseline. */
  val rootAnchorId: Long,
  val inliers: Int,
  /** Magnification of the tracked plane vs the chain root's acquire
   *  frame; `1.0` at acquire, larger as the camera nears the plane.
   *  `0` when not LOCKED. Drives the focus-distance estimate. */
  val scale: Float,
  val lastAcquireDet: Int,
  val lastAcquireRecOk: Int,
  val lastAcquireRecEmpty: Int,
  val lastAcquirePending: Int,
)

/** Thin Kotlin host for the Rust `LiveTrackerPipeline`. Owns the tracker
 *  + AF/overlay/language config; per-frame work happens entirely on the
 *  GL render thread inside `LiveGlSurfaceView` (driven by
 *  `SurfaceTexture.onFrameAvailable`), which invokes
 *  [LivePipelineJni.processFrameGl] directly with the tracker's raw
 *  address.
 *
 *  This class no longer touches per-frame camera bytes; the analyzer
 *  thread and the `FrameHandle` pool are both gone. It surfaces:
 *
 *   - [pipelinePtr] for the GL thread to drive the per-frame call
 *   - [onFrameResult] for the GL thread to publish the packed result
 *     back so the tracker pill flow stays in sync
 *   - language/focus/AF/overlay configuration knobs that translate
 *     directly into pipeline mutators (no per-frame state). */
class LivePlanarOcrEngine(
  private val catalog: LanguageCatalog,
  @Suppress("UNUSED_PARAMETER") workerScope: CoroutineScope,
) {
  private val tracker = uniffi.bindings.LivePlanarTracker(catalog.planarHandle())

  /** Raw pointer to the `LiveTrackerPipeline` so the GL render thread
   *  can drive per-frame `processFrameGl` calls without a uniffi hop.
   *  `0` if the tracker pointer couldn't be fetched. */
  val pipelinePtr: Long =
    runCatching { tracker.rawAddressForJni().toLong() }
      .onFailure { Log.w(TAG_PLANAR, "tracker.rawAddressForJni failed", it) }
      .getOrDefault(0L)

  private val _trackerStatus =
    MutableStateFlow(
      TrackerStatus(
        state = PlanarTrackerState.IDLE,
        anchorId = 0L,
        rootAnchorId = 0L,
        inliers = 0,
        scale = 0f,
        lastAcquireDet = -1,
        lastAcquireRecOk = 0,
        lastAcquireRecEmpty = 0,
        lastAcquirePending = 0,
      ),
    )
  val trackerStatus: StateFlow<TrackerStatus> = _trackerStatus.asStateFlow()

  @Volatile
  private var afScanning: Boolean = false

  @Volatile
  private var overlayEnabled: Boolean = true

  fun setLanguages(
    from: Language,
    to: Language,
    isAutoSource: Boolean,
  ) {
    runCatching { tracker.setLanguages(from.code, to.code, isAutoSource) }
      .onFailure { Log.w(TAG_PLANAR, "setLanguages failed", it) }
  }

  /** Tap-to-focus = fresh-start intent. Resetting the tracker bumps the
   *  Rust generation so any in-flight async job bails at its next
   *  gen-check. */
  fun resetTracker() {
    runCatching { tracker.reset() }
      .onFailure { Log.w(TAG_PLANAR, "tracker.reset failed", it) }
  }

  /** Receives the packed per-frame result from the GL thread. Updates
   *  the tracker status flow + pulls the latest acquire telemetry for
   *  the debug pill. */
  fun onFrameResult(packed: Long) {
    val result = LivePipelineJni.FrameResult.unpack(packed) ?: return
    val baseStatus =
      _trackerStatus.value.copy(
        state = result.state,
        anchorId = result.anchorIdLow16,
        rootAnchorId = result.anchorIdLow16,
        inliers = result.inliers,
        scale = result.scale,
      )
    val telemetry =
      runCatching { tracker.lastAcquireTelemetry() }.getOrNull()
    _trackerStatus.value =
      if (telemetry != null) {
        baseStatus.copy(
          anchorId = telemetry.anchorId.toLong(),
          lastAcquireDet = telemetry.detectedCount.toInt(),
          lastAcquireRecOk = telemetry.recOkCount.toInt(),
          lastAcquireRecEmpty = telemetry.recEmptyCount.toInt(),
          lastAcquirePending =
            (telemetry.detectedCount - telemetry.recOkCount - telemetry.recEmptyCount).toInt(),
        )
      } else {
        baseStatus
      }
  }

  fun onAfScanStart() {
    if (afScanning) return
    afScanning = true
    Log.i(TAG_PLANAR, "AF scan started → suspending pipeline")
    applyTargetMode()
  }

  fun onAfScanEnd() {
    if (!afScanning) return
    afScanning = false
    Log.i(TAG_PLANAR, "AF scan ended → resuming pipeline")
    applyTargetMode()
  }

  /** Toggle whether OCR/translation overlay work runs. When disabled,
   *  the tracker is held in SUPPRESSED so per-frame composites still
   *  produce camera pixels but no acquire/refresh worker is dispatched.
   *  This lets the camera keep rendering when models are missing or
   *  the user has turned the overlay off. */
  fun setOverlayEnabled(enabled: Boolean) {
    if (overlayEnabled == enabled) return
    overlayEnabled = enabled
    applyTargetMode()
  }

  private fun applyTargetMode() {
    val mode =
      if (overlayEnabled && !afScanning) PipelineTargetMode.ACTIVE else PipelineTargetMode.SUPPRESSED
    runCatching { tracker.setTargetMode(mode) }
      .onFailure { Log.w(TAG_PLANAR, "setTargetMode failed", it) }
  }

  fun clear() {
    runCatching { tracker.reset() }
    runCatching { tracker.clearOverlay() }
  }
}
