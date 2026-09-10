package dev.davidv.translator

import android.util.Log
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.bindings.MigrationJobRecord
import java.io.File

private const val TAG = "MigrationGate"

// The doc-detection model (docaligner) is required and tiny, so it converts
// silently; TTS voices are user-facing and big, so they go through a mandatory
// prompt with a delete-all alternative.
private const val FEATURE_DOC_DETECT = "doc_detect"

sealed interface MigrationUiState {
  /** Planning / converting the required (doc-detect) models. */
  data object Working : MigrationUiState

  data class Converting(val current: Int, val total: Int, val label: String) : MigrationUiState

  /** Mandatory choice for TTS voices before the app can be used. */
  data class AwaitTtsDecision(val ttsCount: Int, val savedBytes: Long) : MigrationUiState

  /** Nothing to migrate, or migration finished — show the app. */
  data object Passed : MigrationUiState
}

private fun jobLabel(job: MigrationJobRecord): String = File(job.mnnPath).name

private fun savedBytes(jobs: List<MigrationJobRecord>): Long =
  jobs.sumOf { (it.onnxBytes.toLong() - it.mnnBytes.toLong()).coerceAtLeast(0) }

/**
 * Convert each job's `.onnx` to `.mnn` (cleanup-only jobs are passed through),
 * reporting N-of-M progress. Each job's source `.onnx` is dropped via [onJobDone]
 * the moment that job is safe — right after its own successful conversion, or
 * immediately for cleanup-only jobs whose `.mnn` already exists. Failed
 * conversions keep their `.onnx` so the next launch retries.
 */
private fun convertGroup(
  baseDir: String,
  jobs: List<MigrationJobRecord>,
  onProgress: (Int, Int, String) -> Unit,
  onJobDone: (MigrationJobRecord) -> Unit,
) {
  // Jobs already satisfied — cleanup-only, or whose `.mnn` already exists from a
  // prior/duplicate run — need no conversion and must NOT drive the progress bar.
  // Otherwise an intermittent re-run (e.g. the gate re-mounting on an activity
  // recreate) visibly sweeps the bar again over already-migrated models.
  val toConvert =
    jobs.filter { job ->
      val done = !job.needsConvert || File(baseDir, job.mnnPath).exists()
      if (done) onJobDone(job)
      !done
    }
  Log.i(TAG, "convertGroup ENTER total=${jobs.size} toConvert=${toConvert.size}")
  toConvert.forEachIndexed { index, job ->
    onProgress(index, toConvert.size, jobLabel(job))
    val onnx = File(baseDir, job.onnxPath).absolutePath
    val error = ModelConverterJni.convert(onnx, File(baseDir, job.mnnPath).absolutePath, job.quantBits)
    if (error == null) {
      onJobDone(job)
    } else {
      Log.e(TAG, "convert failed for ${job.onnxPath}: $error")
    }
  }
}

/**
 * Blocks the app on first launch after the ONNX→MNN migration until every needed
 * model is converted (or the user drops the TTS voices). Once nothing is left to
 * migrate this is a cheap no-op and renders [content] directly.
 */
@Composable
fun MigrationGate(
  filePathManager: FilePathManager,
  onMigrated: () -> Unit,
  content: @Composable () -> Unit,
) {
  var state by remember { mutableStateOf<MigrationUiState>(MigrationUiState.Working) }
  var ttsJobs by remember { mutableStateOf<List<MigrationJobRecord>>(emptyList()) }
  // Guards against the Accept/Delete tap registering more than once: without it a
  // second invocation re-runs convertGroup over already-migrated jobs (whose
  // `.onnx` is gone), flashing the bar through a second time.
  var ttsHandled by remember { mutableStateOf(false) }
  val scope = rememberCoroutineScope()

  val ttsProgress: (Int, Int, String) -> Unit = { current, total, label ->
    state = MigrationUiState.Converting(current = current, total = total, label = label)
  }

  // Phase 1: plan + convert the required doc-detect models silently (no per-file
  // bar — just the spinner). Then either pass (no TTS) or surface the mandatory
  // TTS decision. `migrated` distinguishes "did real work" from "nothing to do"
  // so we only refresh availability when something actually changed.
  LaunchedEffect(Unit) {
    Log.i(TAG, "phase1 LaunchedEffect ENTER")
    val (outcome, migrated) =
      withContext(Dispatchers.IO) {
        val catalog =
          filePathManager.loadCatalog() ?: return@withContext MigrationUiState.Passed to false
        val plan = catalog.planMigration()
        Log.i(TAG, "phase1 plan size=${plan.size}")
        if (plan.isEmpty()) return@withContext MigrationUiState.Passed to false

        val (docDetect, tts) = plan.partition { it.feature == FEATURE_DOC_DETECT }
        if (docDetect.isNotEmpty()) {
          convertGroup(catalog.baseDir, docDetect, { _, _, _ -> }) { catalog.discardMigration(listOf(it)) }
        }
        ttsJobs = tts
        if (tts.isEmpty()) {
          filePathManager.invalidateCatalog()
          MigrationUiState.Passed to true
        } else {
          MigrationUiState.AwaitTtsDecision(
            ttsCount = tts.count { it.needsConvert },
            savedBytes = savedBytes(tts.filter { it.needsConvert }),
          ) to false
        }
      }
    state = outcome
    if (migrated) onMigrated()
  }

  val finishTts: (convert: Boolean) -> Unit = { convert ->
    Log.i(TAG, "finishTts ENTER convert=$convert ttsHandled=$ttsHandled ttsJobs=${ttsJobs.size}")
    if (!ttsHandled) {
      ttsHandled = true
      scope.launch {
        withContext(Dispatchers.IO) {
          val catalog = filePathManager.loadCatalog()
          if (catalog != null) {
            if (convert) {
              convertGroup(catalog.baseDir, ttsJobs, ttsProgress) { catalog.discardMigration(listOf(it)) }
            } else {
              // Drop the voices: delete every source .onnx without converting.
              catalog.discardMigration(ttsJobs)
            }
          }
          filePathManager.invalidateCatalog()
        }
        onMigrated()
        state = MigrationUiState.Passed
      }
    }
  }

  when (val current = state) {
    MigrationUiState.Passed -> content()
    else ->
      MigrationBlockingScreen(
        state = current,
        onConvert = { finishTts(true) },
        onDeleteAll = { finishTts(false) },
      )
  }
}

@Composable
internal fun MigrationBlockingScreen(
  state: MigrationUiState,
  onConvert: () -> Unit,
  onDeleteAll: () -> Unit,
) {
  Surface(modifier = Modifier.fillMaxSize()) {
    Column(
      modifier = Modifier.fillMaxSize().padding(32.dp),
      horizontalAlignment = Alignment.CenterHorizontally,
      verticalArrangement = Arrangement.Center,
    ) {
      when (state) {
        is MigrationUiState.AwaitTtsDecision -> {
          val savedMb = state.savedBytes / 1_000_000.0
          Text(
            text = stringResource(R.string.migration_updating_speech_models),
            textAlign = TextAlign.Center,
          )
          Spacer(Modifier.height(12.dp))
          Text(
            text =
              pluralStringResource(
                R.plurals.migration_convert_prompt,
                state.ttsCount,
                state.ttsCount,
                6 * state.ttsCount,
                savedMb,
              ),
            textAlign = TextAlign.Center,
          )
          Spacer(Modifier.height(24.dp))
          Button(modifier = Modifier.fillMaxWidth(), onClick = onConvert) {
            Text(stringResource(R.string.migration_accept_convert))
          }
          Spacer(Modifier.height(8.dp))
          OutlinedButton(modifier = Modifier.fillMaxWidth(), onClick = onDeleteAll) {
            Text(stringResource(R.string.migration_delete_all))
          }
        }

        is MigrationUiState.Converting -> {
          // Each `.onnx`→`.mnn` step blocks for seconds, so the bar value changes
          // only ~once per model. A static window lets the surface's BufferQueue
          // fill with those few stale frames, which the hand-off to the app then
          // replays as a phantom re-sweep. Breathing the bar colour keeps a fresh
          // frame flowing every vsync, so there is nothing stale to re-present.
          val breathing = rememberInfiniteTransition(label = "convert-breath")
          val breathAlpha by breathing.animateFloat(
            initialValue = 0.72f,
            targetValue = 1f,
            animationSpec = infiniteRepeatable(tween(1800), RepeatMode.Reverse),
            label = "convert-breath-alpha",
          )
          Text(
            text = stringResource(R.string.migration_converting, state.current + 1, state.total),
            textAlign = TextAlign.Center,
          )
          Spacer(Modifier.height(8.dp))
          Text(text = state.label, textAlign = TextAlign.Center)
          Spacer(Modifier.height(24.dp))
          LinearProgressIndicator(
            progress = { (state.current + 1).toFloat() / state.total.coerceAtLeast(1) },
            color = MaterialTheme.colorScheme.primary.copy(alpha = breathAlpha),
            modifier = Modifier.fillMaxWidth(),
          )
        }

        else -> {
          CircularProgressIndicator()
          Spacer(Modifier.height(16.dp))
          Text(text = stringResource(R.string.migration_optimizing), textAlign = TextAlign.Center)
        }
      }
    }
  }
}
