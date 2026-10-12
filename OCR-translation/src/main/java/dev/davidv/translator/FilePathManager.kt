package dev.davidv.translator

import android.content.Context
import android.os.Build
import android.os.Environment
import android.util.Log
import kotlinx.coroutines.flow.StateFlow
import org.json.JSONObject
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class FilePathManager(
  val context: Context,
  private val settingsFlow: StateFlow<AppSettings>,
) {
  private val catalogLock = Any()

  // Kotlin treats LanguageCatalog as an immutable snapshot.
  // FilePathManager owns the current cached snapshot reference and swaps it on reload.
  private var cachedCatalog: LanguageCatalog? = null
  private var cachedCatalogBaseDir: String? = null

  // Bundled-OCR seeding runs once per install on a background thread; catalog loads
  // wait for it (with a timeout) so a fresh install can detect text offline.
  private val seedLatch = CountDownLatch(1)
  @Volatile private var seedStarted = false

  companion object {
    private const val TAG = "FilePathManager"
    private const val BUNDLED_OCR_ASSET_DIR = "ppocr"
    // v2: v1 wrote model files into same-named directories (broken layout); bumping
    // forces a repairing re-seed on devices that got the broken copy.
    private const val BUNDLED_OCR_VERSION = 2
    private const val BUNDLED_OCR_SEED_TIMEOUT_SEC = 60L
  }

  private val baseDir: File
    get() =
      if (settingsFlow.value.useExternalStorage) {
        if (Build.VERSION.SDK_INT == Build.VERSION_CODES.Q) {
          val ext = context.getExternalFilesDir(null)
          ext ?: context.filesDir
        } else {
          val documentsDir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOCUMENTS)
          File(documentsDir, "dev.davidv.translator").also { dir ->
            if (!dir.exists()) {
              dir.mkdirs()
            }
          }
        }
      } else {
        context.filesDir
      }

  fun currentBaseDir(): File = baseDir

  /**
   * Copies the text-recognition model packs bundled in APK assets (`assets/ppocr/…`)
   * into the install base dir, so a fresh install can detect text offline immediately
   * instead of hitting the "text recognition models are missing" error until the user
   * downloads packs. Idempotent: skipped once the version marker exists. Safe to call
   * from any thread; re-runs are cheap no-ops.
   */
  private fun seedBundledOcrAssetsInternal() {
    try {
      val marker = File(baseDir, ".bundled_ocr_v$BUNDLED_OCR_VERSION")
      if (marker.exists()) return
      copyAssetTree(BUNDLED_OCR_ASSET_DIR, File(baseDir, BUNDLED_OCR_ASSET_DIR))
      marker.parentFile?.mkdirs()
      marker.writeText(BUNDLED_OCR_VERSION.toString())
      Log.i(TAG, "Seeded bundled OCR assets into ${baseDir.absolutePath}")
    } catch (e: Exception) {
      Log.e(TAG, "Failed to seed bundled OCR assets", e)
    } finally {
      seedLatch.countDown()
    }
  }

  /**
   * Starts seeding on a dedicated background thread (once). [target] is the
   * destination root; for leaf assets it is the destination FILE path.
   */
  fun seedBundledOcrAssetsAsync() {
    if (seedStarted) return
    seedStarted = true
    Thread({ seedBundledOcrAssetsInternal() }, "bundled-ocr-seed").start()
  }

  private fun awaitBundledOcrSeed() {
    if (!seedStarted) return
    try {
      seedLatch.await(BUNDLED_OCR_SEED_TIMEOUT_SEC, TimeUnit.SECONDS)
    } catch (_: InterruptedException) {
      Thread.currentThread().interrupt()
    }
  }

  // Copies an asset file or directory tree to the filesystem. Directories are
  // identified by assets.list() returning children; a leaf entry maps directly
  // to [targetFile]. A leftover directory at a file's path (from an older buggy
  // seed) is removed first so re-seeding repairs the layout.
  private fun copyAssetTree(assetPath: String, targetFile: File) {
    val children = context.assets.list(assetPath).orEmpty()
    if (children.isEmpty()) {
      if (targetFile.isDirectory) {
        targetFile.deleteRecursively()
      }
      targetFile.parentFile?.mkdirs()
      context.assets.open(assetPath).use { input ->
        targetFile.outputStream().use { output -> input.copyTo(output) }
      }
      return
    }
    targetFile.mkdirs()
    for (child in children) {
      copyAssetTree("$assetPath/$child", File(targetFile, child))
    }
  }

  fun getTranslatedDocumentsDir(): File =
    File(context.cacheDir, "translated_documents").also { dir ->
      if (!dir.exists()) {
        dir.mkdirs()
      }
    }

  fun getDataDir(): File = File(baseDir, "bin")

  fun getDictionariesDir(): File = File(baseDir, "dictionaries")

  fun getAdblockDir(): File = File(baseDir, "adblock")

  fun resolveInstallPath(relativePath: String): File = File(baseDir, relativePath)

  fun getDictionaryFile(language: Language): File = File(getDictionariesDir(), "${language.dictionaryCode}.dict")

  fun getCatalogFile(): File = File(baseDir, "index_v6.json")

  fun getMucabFile(): File = File(getDataDir(), "mucab.bin")

  fun hasInstallMarker(
    relativePath: String,
    expectedVersion: Int,
  ): Boolean {
    val markerFile = resolveInstallPath(relativePath)
    if (!markerFile.exists()) return false
    return try {
      val root = JSONObject(markerFile.readText())
      root.optInt("version", -1) == expectedVersion
    } catch (_: Exception) {
      false
    }
  }

  fun writeInstallMarker(
    relativePath: String,
    version: Int,
  ) {
    val markerFile = resolveInstallPath(relativePath)
    markerFile.parentFile?.mkdirs()
    markerFile.writeText(
      JSONObject()
        .put("version", version)
        .toString(),
    )
    invalidateCatalog()
  }

  fun applyDeletePlan(plan: DeletePlan) {
    plan.directoryPaths.forEach { relativePath ->
      val directory = resolveInstallPath(relativePath)
      if (directory.exists() && directory.deleteRecursively()) {
        Log.i("FilePathManager", "Deleted directory $relativePath")
      }
    }

    plan.filePaths.forEach { relativePath ->
      val file = resolveInstallPath(relativePath)
      if (file.exists() && file.delete()) {
        Log.i("FilePathManager", "Deleted file $relativePath")
      }
    }

    invalidateCatalog()
  }

  /**
   * Loads the catalog. [awaitOcrSeed] (default) blocks until bundled OCR assets have
   * been copied on first install, so availability checks never race the seed copy.
   * Application startup passes false so launching isn't delayed by the copy.
   */
  fun loadCatalog(awaitOcrSeed: Boolean = true): LanguageCatalog? {
    if (awaitOcrSeed) {
      // First catalog use on a fresh install must wait for the bundled OCR models to
      // land on disk, otherwise availability checks report them as missing.
      awaitBundledOcrSeed()
    }
    val baseDirPath = currentBaseDir().absolutePath
    synchronized(catalogLock) {
      cachedCatalog?.takeIf { cachedCatalogBaseDir == baseDirPath }?.let { return it }
      val catalog = openCatalog(baseDirPath)
      replaceCachedCatalogLocked(catalog, baseDirPath)
      return catalog
    }
  }

  fun reloadCatalog(): LanguageCatalog? =
    synchronized(catalogLock) {
      val baseDirPath = currentBaseDir().absolutePath
      val catalog = openCatalog(baseDirPath)
      replaceCachedCatalogLocked(catalog, baseDirPath)
      catalog
    }

  fun invalidateCatalog() {
    synchronized(catalogLock) {
      replaceCachedCatalogLocked(null, null)
    }
  }

  private fun openCatalog(baseDirPath: String): LanguageCatalog? {
    // Older catalog formats served under earlier filenames stay served for old
    // app versions, but their on-disk copies are stale once this build moves to
    // a newer one. Delete every superseded index_v*.json, keeping only the one
    // this build reads.
    val currentCatalogName = getCatalogFile().name
    baseDir
      .listFiles { file -> file.name.matches(Regex("index_v\\d+\\.json")) && file.name != currentCatalogName }
      ?.forEach { it.delete() }
    // The kokoro ONNX engine is gone (dropped from v5, unloadable under MNN). Its
    // 92 MB core blob is no longer in any pack, so reclaim it directly here.
    File(baseDir, "bin/kokoro/kokoro-v1.0.int8.onnx").delete()
    val bundledJson =
      try {
        context.assets.open("index_v6.json").bufferedReader().readText()
      } catch (e: Exception) {
        Log.e("FilePathManager", "Error reading bundled catalog index", e)
        null
      }

    val diskJson =
      try {
        getCatalogFile().takeIf(File::exists)?.readText()
      } catch (e: Exception) {
        Log.w("FilePathManager", "Error reading cached catalog index", e)
        null
      }

    val catalog =
      try {
        when {
          bundledJson != null -> LanguageCatalog.open(bundledJson, diskJson, baseDirPath)
          diskJson != null -> LanguageCatalog.open(diskJson, null, baseDirPath)
          else -> null
        }
      } catch (e: Exception) {
        Log.e("FilePathManager", "Error loading catalog index", e)
        null
      }

    if (catalog == null) {
      Log.e("FilePathManager", "No valid catalog found")
    }
    return catalog
  }

  private fun replaceCachedCatalogLocked(
    newCatalog: LanguageCatalog?,
    baseDirPath: String?,
  ) {
    if (cachedCatalog === newCatalog && cachedCatalogBaseDir == baseDirPath) return
    cachedCatalog = newCatalog
    cachedCatalogBaseDir = baseDirPath
  }
}
