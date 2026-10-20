use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::api::{LanguageCode, TranslatorError};
use crate::bergamot::{BergamotEngine, TranslateCtx};
use crate::catalog::{
    CatalogSnapshot, DeletePlan, DownloadPlan, FsPackInstallChecker, LanguageAvailabilityRow,
    LanguageOverview, MigrationJob, PackInstallChecker, build_catalog_snapshot,
    build_language_overview, language_rows_in_snapshot, parse_and_validate_catalog,
    plan_delete_dictionary, plan_delete_language, plan_delete_superseded_tts,
    plan_delete_support_by_kind, plan_delete_tts, plan_delete_tts_pack, plan_dictionary_download,
    plan_language_download, plan_migrations, plan_support_download_by_kind, plan_tts_download,
    select_best_catalog,
};
use crate::routing::MixedTextTranslationResult;
#[cfg(feature = "ppocr")]
use crate::settings::BackgroundMode;
use crate::translate::TranslationWithAlignment;
use crate::translate::TranslationWithAlternatives;
use crate::translate::Translator;

#[cfg(feature = "dictionary")]
use crate::tarkka::WordWithTaggedEntries;
#[cfg(feature = "dictionary")]
use translator_dictionary::engine::DictionaryEngine;

#[cfg(feature = "doc-align")]
use crate::doc_align::{DocumentDetection, DocumentQuad, WarpedImageRgba};
use crate::ocr::PreparedImageOverlay;
#[cfg(feature = "ppocr")]
use crate::ocr::{OcrSourceSelection, ReadingOrder};
#[cfg(feature = "ppocr")]
use crate::ocr_runtime::translate_image_rgba_ppocr_in_snapshot;
#[cfg(feature = "doc-align")]
use translator_align::engine::DocAlignEngine;
#[cfg(feature = "ppocr")]
use translator_ocr::engine::OcrEngine;

#[cfg(feature = "tts")]
use crate::api::VoiceName;
#[cfg(feature = "tts")]
use crate::catalog::{InstalledTtsPack, installed_tts_voices_for_language};
#[cfg(feature = "tts")]
use crate::tts::{PcmAudio, SpeechChunk, TtsVoiceOption, UrlsAndHashtags};
#[cfg(feature = "tts")]
use translator_tts::engine::TtsEngine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Feature {
    Core,
    Dictionary,
    Tts,
}

static BERGAMOT_ENGINE: OnceLock<Mutex<BergamotEngine>> = OnceLock::new();

pub struct TranslatorSession {
    snapshot: RwLock<Arc<CatalogSnapshot>>,
    /// Set true by `cancel_ongoing_work()` (from any thread) to abort the
    /// in-flight document translation. slimt worker threads poll it and stop
    /// within ~one batch. Document translations reset it to false at entry.
    /// Translations are serialized through the global engine mutex, so a
    /// single per-session flag unambiguously targets the active document.
    document_cancel: AtomicBool,
    #[cfg(feature = "tts")]
    tts: TtsEngine,
    #[cfg(feature = "dictionary")]
    dictionary: DictionaryEngine,
    #[cfg(feature = "doc-align")]
    doc_align: DocAlignEngine,
    #[cfg(feature = "ppocr")]
    ocr: OcrEngine,
}

impl TranslatorSession {
    pub fn from_snapshot(snapshot: CatalogSnapshot) -> Self {
        Self {
            snapshot: RwLock::new(Arc::new(snapshot)),
            document_cancel: AtomicBool::new(false),
            #[cfg(feature = "tts")]
            tts: TtsEngine::new(),
            #[cfg(feature = "dictionary")]
            dictionary: DictionaryEngine::new(),
            #[cfg(feature = "doc-align")]
            doc_align: DocAlignEngine::new(),
            #[cfg(feature = "ppocr")]
            ocr: OcrEngine::new(),
        }
    }

    pub fn from_catalog(catalog: crate::catalog::LanguageCatalog, base_dir: String) -> Self {
        let checker = FsPackInstallChecker::new(&base_dir);
        let snapshot = build_catalog_snapshot(catalog, base_dir, &checker);
        Self::from_snapshot(snapshot)
    }

    fn engine(&self) -> &'static Mutex<BergamotEngine> {
        BERGAMOT_ENGINE.get_or_init(|| Mutex::new(BergamotEngine::new()))
    }

    pub fn open<C>(
        bundled_json: &str,
        disk_json: Option<&str>,
        base_dir: String,
        install_checker: &C,
    ) -> Result<Self, TranslatorError>
    where
        C: PackInstallChecker,
    {
        let catalog = parse_selected_catalog(bundled_json, disk_json).ok_or_else(|| {
            TranslatorError::new(
                crate::api::TranslatorErrorKind::InvalidInput,
                "failed to parse any catalog",
            )
        })?;
        let snapshot = build_catalog_snapshot(catalog, base_dir, install_checker);
        Ok(Self::from_snapshot(snapshot))
    }

    pub fn snapshot(&self) -> Arc<CatalogSnapshot> {
        self.snapshot
            .read()
            .expect("snapshot lock poisoned")
            .clone()
    }

    fn set_snapshot(&self, snapshot: CatalogSnapshot) {
        *self.snapshot.write().expect("snapshot lock poisoned") = Arc::new(snapshot);
    }

    pub fn refresh_snapshot(&self) {
        let current = self.snapshot();
        let catalog = current.catalog.clone();
        let base_dir = current.base_dir.clone();
        drop(current);
        let checker = FsPackInstallChecker::new(&base_dir);
        self.set_snapshot(build_catalog_snapshot(catalog, base_dir, &checker));
    }

    pub fn apply_delete_plan(&self, plan: &DeletePlan) {
        use std::collections::HashSet;
        use std::path::Path;

        let base_dir = self.snapshot().base_dir.clone();
        let base = Path::new(&base_dir);

        let mut files = plan
            .file_paths
            .iter()
            .map(|path| base.join(path))
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        for file in files {
            let _ = std::fs::remove_file(file);
        }

        let mut directories = plan
            .directory_paths
            .iter()
            .map(|path| base.join(path))
            .collect::<Vec<_>>();
        directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
        let mut seen = HashSet::new();
        for directory in directories {
            if seen.insert(directory.clone()) {
                let _ = std::fs::remove_dir_all(directory);
            }
        }

        self.refresh_snapshot();
    }

    /// The on-device ONNX→MNN conversions needed to migrate this install to the
    /// MNN-only runtime: only entries whose `.onnx` is present on disk.
    pub fn plan_migration(&self) -> Vec<MigrationJob> {
        let snap = self.snapshot();
        let checker = FsPackInstallChecker::new(&snap.base_dir);
        plan_migrations(&snap.catalog, &checker)
    }

    /// Delete the source `.onnx` of each job without converting (either the user
    /// opted to drop these models instead of migrating them, or a cleanup-only
    /// job whose `.mnn` already exists). The actual ONNX→MNN conversion runs in a
    /// separate link unit (the `translator-convert` crate / its own native lib)
    /// because the MNN converter and slimt's sentencepiece vendor incompatible
    /// protobufs that cannot coexist in one binary.
    pub fn discard_migration(&self, jobs: &[MigrationJob]) {
        let base_dir = self.snapshot().base_dir.clone();
        let base = std::path::Path::new(&base_dir);
        for job in jobs {
            let _ = std::fs::remove_file(base.join(&job.entry.onnx));
        }
        self.refresh_snapshot();
    }

    pub fn language_rows(&self) -> Vec<LanguageAvailabilityRow> {
        language_rows_in_snapshot(&self.snapshot())
    }

    pub fn language_overview(&self) -> Vec<LanguageOverview> {
        build_language_overview(&self.snapshot())
    }

    /// Every available language paired with its catalog script, for callers
    /// that pick fonts or detect a source language.
    pub fn available_languages(&self) -> Vec<translator_core::api::ScriptedLanguage> {
        self.language_rows()
            .into_iter()
            .map(|row| row.language.scripted())
            .collect()
    }

    pub fn scripted_language(
        &self,
        code: &LanguageCode,
    ) -> Option<translator_core::api::ScriptedLanguage> {
        self.snapshot().catalog.scripted_language(code)
    }

    pub fn warm(&self, from_code: &str, to_code: &str) -> Result<(), TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap)
            .warm(&LanguageCode::from(from_code), &LanguageCode::from(to_code))
    }

    pub fn translate_text(
        &self,
        from_code: &str,
        to_code: &str,
        text: &str,
    ) -> Result<String, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_text(
            &LanguageCode::from(from_code),
            &LanguageCode::from(to_code),
            text,
        )
    }

    pub fn translate_text_with_alternatives(
        &self,
        from_code: &str,
        to_code: &str,
        text: &str,
    ) -> Result<TranslationWithAlternatives, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_text_with_alternatives(
            &LanguageCode::from(from_code),
            &LanguageCode::from(to_code),
            text,
        )
    }

    pub fn steer(
        &self,
        from_code: &str,
        to_code: &str,
        source: &str,
        forced_prefix: &str,
    ) -> Result<TranslationWithAlternatives, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).steer_text(
            &LanguageCode::from(from_code),
            &LanguageCode::from(to_code),
            source,
            forced_prefix,
        )
    }

    /// Request cancellation of the in-flight document translation. Safe to call
    /// from any thread (e.g. a UI "cancel" tap) while a `*_ctx` translation is
    /// running on another thread — slimt workers observe it within ~one batch.
    pub fn cancel_ongoing_work(&self) {
        self.document_cancel.store(true, Ordering::Relaxed);
    }

    /// Clear the cancellation flag. Called once at the start of a document
    /// translation so a previous cancel does not abort the next one.
    pub fn begin_document_translation(&self) {
        self.document_cancel.store(false, Ordering::Relaxed);
    }

    /// Cancellable, progress-reporting [`Self::translate_texts`]. `on_progress`
    /// is invoked from slimt worker threads with `(sentences_done, total)`;
    /// cancellation is driven by [`Self::cancel_ongoing_work`].
    pub(crate) fn translate_texts_ctx(
        &self,
        from_code: &str,
        to_code: &str,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Vec<String>, TranslatorError> {
        let ctx = TranslateCtx {
            cancel: &self.document_cancel,
            on_progress,
        };
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_texts_ctx(
            &LanguageCode::from(from_code),
            &LanguageCode::from(to_code),
            texts,
            &ctx,
        )
    }

    pub fn translate_html_fragments(
        &self,
        from_code: &str,
        to_code: &str,
        fragments: &[String],
    ) -> Result<Vec<String>, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_html_fragments(
            &LanguageCode::from(from_code),
            &LanguageCode::from(to_code),
            fragments,
        )
    }

    pub fn translate_mixed_texts(
        &self,
        inputs: &[String],
        forced_source_code: Option<&str>,
        target_code: &str,
        available_language_codes: &[LanguageCode],
    ) -> Result<MixedTextTranslationResult, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_mixed_texts(
            inputs,
            forced_source_code.map(LanguageCode::from).as_ref(),
            &LanguageCode::from(target_code),
            available_language_codes,
        )
    }

    #[cfg(feature = "planar-tracker")]
    pub(crate) fn translate_mixed_texts_with_alignment(
        &self,
        inputs: &[String],
        forced_source_code: Option<&str>,
        target_code: &str,
        available_language_codes: &[LanguageCode],
    ) -> Result<Vec<crate::translate::TranslationWithAlignment>, TranslatorError> {
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap).translate_mixed_texts_with_alignment(
            inputs,
            forced_source_code.map(LanguageCode::from).as_ref(),
            &LanguageCode::from(target_code),
            available_language_codes,
        )
    }

    /// Cancellable, progress-reporting alignment translation for documents.
    pub(crate) fn translate_texts_with_alignment_ctx(
        &self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Option<Vec<TranslationWithAlignment>>, TranslatorError> {
        let ctx = TranslateCtx {
            cancel: &self.document_cancel,
            on_progress,
        };
        let snap = self.snapshot();
        let mut engine = self.engine().lock().expect("engine lock poisoned");
        Translator::new(&mut engine, &snap)
            .translate_texts_with_alignment_ctx(from_code, to_code, texts, &ctx)
    }

    #[cfg(feature = "ppocr")]
    #[allow(clippy::too_many_arguments)]
    pub fn translate_image_rgba(
        &self,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        max_image_size: u32,
        source_selection: OcrSourceSelection,
        target_code: &str,
        min_confidence: u32,
        reading_order: Option<ReadingOrder>,
        background_mode: BackgroundMode,
        // Optional boxes from a prior `detect_image_boxes` call; when `Some`, detection is skipped.
        detection: Option<Vec<crate::ocr::DetectedTextBox>>,
    ) -> Result<PreparedImageOverlay, TranslatorError> {
        let still =
            crate::ocr_runtime::build_still_image(rgba_bytes, width, height, max_image_size)?;
        self.translate_from_still(
            &still,
            rgba_bytes,
            width,
            height,
            source_selection,
            target_code,
            min_confidence,
            reading_order,
            background_mode,
            detection,
        )
    }

    /// Build the still image's RGB derivatives once; the `OcrImage` handle caches the result so a
    /// staged detect→ocr pass over the same owned pixels rebuilds nothing.
    #[cfg(feature = "ppocr")]
    pub fn build_still_image(
        &self,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        max_image_size: u32,
    ) -> Result<crate::live_frame::StillImage, TranslatorError> {
        crate::ocr_runtime::build_still_image(rgba_bytes, width, height, max_image_size)
    }

    #[cfg(feature = "ppocr")]
    pub fn detect_boxes_from_still(
        &self,
        still: &crate::live_frame::StillImage,
        width: u32,
        height: u32,
    ) -> Result<Vec<crate::ocr::DetectedTextBox>, TranslatorError> {
        let snap = self.snapshot();
        let ppocr = self.ocr.engine(&snap)?;
        crate::ocr_runtime::detect_boxes_from_still(&ppocr, still, width, height)
    }

    #[cfg(feature = "ppocr")]
    #[allow(clippy::too_many_arguments)]
    pub fn translate_from_still(
        &self,
        still: &crate::live_frame::StillImage,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        source_selection: OcrSourceSelection,
        target_code: &str,
        min_confidence: u32,
        reading_order: Option<ReadingOrder>,
        background_mode: BackgroundMode,
        detection: Option<Vec<crate::ocr::DetectedTextBox>>,
    ) -> Result<PreparedImageOverlay, TranslatorError> {
        let snap = self.snapshot();
        let tgt = LanguageCode::from(target_code);
        let ppocr = self.ocr.engine(&snap)?;
        log::info!("ocr engine: ppocr ({:?})", source_selection);
        translate_image_rgba_ppocr_in_snapshot(
            self.engine(),
            &ppocr,
            &snap,
            still,
            rgba_bytes,
            width,
            height,
            &source_selection,
            &tgt,
            min_confidence,
            background_mode,
            reading_order,
            detection,
        )
        .map_err(|e| {
            if e.message.to_lowercase().contains("no text found") {
                TranslatorError::ocr("No text found in image (engine=ppocr)")
            } else {
                e
            }
        })
    }

    /// Detect-only pass over a still RGBA image: returns the text boxes without recognizing or
    /// translating them, for the UI to pill + animate. Feed the result back into
    /// `translate_image_rgba` (as `detection`) so the detector runs only once.
    #[cfg(feature = "ppocr")]
    pub fn detect_image_boxes(
        &self,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        max_image_size: u32,
    ) -> Result<Vec<crate::ocr::DetectedTextBox>, TranslatorError> {
        let snap = self.snapshot();
        let ppocr = self.ocr.engine(&snap)?;
        crate::ocr_runtime::detect_image_boxes_ppocr(
            &ppocr,
            rgba_bytes,
            width,
            height,
            max_image_size,
        )
    }

    #[cfg(feature = "planar-tracker")]
    pub(crate) fn ocr(&self) -> &OcrEngine {
        &self.ocr
    }

    pub fn retranslate_prepared_overlay(
        &self,
        prepared: PreparedImageOverlay,
        source_code: &str,
        target_code: &str,
    ) -> Result<PreparedImageOverlay, TranslatorError> {
        #[cfg(feature = "ppocr")]
        {
            let snap = self.snapshot();
            let src = LanguageCode::from(source_code);
            let tgt = LanguageCode::from(target_code);
            let mut prepared = prepared;
            crate::ocr_runtime::retranslate_overlay(
                self.engine(),
                &snap,
                &mut prepared,
                &src,
                &tgt,
            )?;
            Ok(prepared)
        }
        // Image overlays require the OCR pipeline; without it there's nothing to retranslate.
        #[cfg(not(feature = "ppocr"))]
        {
            let _ = (source_code, target_code);
            Ok(prepared)
        }
    }

    pub fn plan_download(
        &self,
        language_code: &str,
        feature: Feature,
        selected_tts_pack_id: Option<&str>,
    ) -> Option<DownloadPlan> {
        let snap = self.snapshot();
        let code = LanguageCode::from(language_code);
        match feature {
            Feature::Core => Some(plan_language_download(&snap, &code)),
            Feature::Dictionary => plan_dictionary_download(&snap, &code),
            Feature::Tts => plan_tts_download(&snap, &code, selected_tts_pack_id),
        }
    }

    pub fn plan_support_download_by_kind(&self, support_kind: &str) -> Option<DownloadPlan> {
        let snap = self.snapshot();
        plan_support_download_by_kind(&snap, support_kind)
    }

    pub fn prepare_delete(&self, language_code: &str, feature: Feature) -> DeletePlan {
        let snap = self.snapshot();
        let code = LanguageCode::from(language_code);
        match feature {
            Feature::Core => {
                if let Some(engine) = BERGAMOT_ENGINE.get() {
                    engine
                        .lock()
                        .expect("engine lock poisoned")
                        .evict_involving(language_code);
                }
                #[cfg(feature = "dictionary")]
                self.close_dictionary(&snap, &code);
                #[cfg(feature = "tts")]
                self.clear_speech_cache();
                plan_delete_language(&snap, &code)
            }
            Feature::Dictionary => {
                #[cfg(feature = "dictionary")]
                self.close_dictionary(&snap, &code);
                plan_delete_dictionary(&snap, &code)
            }
            Feature::Tts => {
                #[cfg(feature = "tts")]
                self.clear_speech_cache();
                plan_delete_tts(&snap, &code)
            }
        }
    }

    pub fn prepare_delete_support_by_kind(&self, support_kind: &str) -> DeletePlan {
        let snap = self.snapshot();
        plan_delete_support_by_kind(&snap, support_kind)
    }

    #[cfg(feature = "dictionary")]
    fn close_dictionary(&self, snapshot: &CatalogSnapshot, language_code: &LanguageCode) {
        self.dictionary.close(snapshot, language_code);
    }

    pub fn prepare_delete_superseded_tts(
        &self,
        language_code: &str,
        selected_pack_id: &str,
    ) -> DeletePlan {
        let snap = self.snapshot();
        let code = LanguageCode::from(language_code);
        #[cfg(feature = "tts")]
        self.clear_speech_cache();
        plan_delete_superseded_tts(&snap, &code, selected_pack_id)
    }

    pub fn prepare_delete_tts_pack(&self, pack_id: &str) -> DeletePlan {
        let snap = self.snapshot();
        #[cfg(feature = "tts")]
        self.clear_speech_cache();
        plan_delete_tts_pack(&snap, pack_id)
    }

    #[cfg(feature = "tts")]
    fn clear_speech_cache(&self) {
        self.tts.clear();
    }

    pub fn size_bytes(&self, language_code: &str, feature: Feature) -> u64 {
        let snap = self.snapshot();
        let code = LanguageCode::from(language_code);
        match feature {
            Feature::Core => snap.catalog.translation_size_bytes_for_language(&code),
            Feature::Tts => snap.catalog.tts_size_bytes_for_language(&code),
            Feature::Dictionary => snap
                .catalog
                .language_by_code(&code)
                .and_then(|lang| {
                    snap.catalog
                        .dictionary_info(&crate::api::DictionaryCode::from(
                            lang.dictionary_code.clone(),
                        ))
                })
                .map(|info| info.size as u64)
                .unwrap_or(0),
        }
    }

    pub fn support_size_bytes_by_kind(&self, support_kind: &str) -> u64 {
        self.snapshot()
            .catalog
            .support_size_bytes_by_kind(support_kind)
    }

    #[cfg(feature = "dictionary")]
    pub fn lookup_dictionary(
        &self,
        language_code: &str,
        word: &str,
    ) -> Result<Option<WordWithTaggedEntries>, TranslatorError> {
        self.dictionary
            .lookup(&self.snapshot(), &LanguageCode::from(language_code), word)
    }

    #[cfg(feature = "tts")]
    pub fn available_tts_voices(
        &self,
        language_code: &str,
    ) -> Result<Vec<TtsVoiceOption>, TranslatorError> {
        self.tts
            .available_voices(&self.snapshot(), &LanguageCode::from(language_code))
    }

    #[cfg(feature = "tts")]
    pub fn installed_tts_voices(&self, language_code: &str) -> Vec<InstalledTtsPack> {
        let snap = self.snapshot();
        installed_tts_voices_for_language(&snap, &LanguageCode::from(language_code))
    }

    #[cfg(feature = "tts")]
    pub fn warm_tts_model(&self, language_code: &str) -> Result<(), TranslatorError> {
        self.tts
            .warm_model(&self.snapshot(), &LanguageCode::from(language_code))
    }

    #[cfg(feature = "tts")]
    pub fn plan_speech_chunks(
        &self,
        language_code: &str,
        text: &str,
        pack_id: Option<&str>,
        urls_and_hashtags: UrlsAndHashtags,
    ) -> Result<Vec<SpeechChunk>, TranslatorError> {
        self.tts.plan_speech_chunks(
            &self.snapshot(),
            &LanguageCode::from(language_code),
            text,
            pack_id,
            urls_and_hashtags,
        )
    }

    #[cfg(feature = "tts")]
    pub fn synthesize_pcm(
        &self,
        language_code: &str,
        text: &str,
        speech_speed: f32,
        voice_name: Option<&str>,
        is_phonemes: bool,
        pack_id: Option<&str>,
    ) -> Result<PcmAudio, TranslatorError> {
        self.tts.synthesize_pcm(
            &self.snapshot(),
            &LanguageCode::from(language_code),
            text,
            speech_speed,
            voice_name.map(VoiceName::from).as_ref(),
            is_phonemes,
            pack_id,
        )
    }

    pub fn transliterate(&self, text: &str, language_code: &str) -> Option<String> {
        #[cfg(feature = "transliterate")]
        {
            let snap = self.snapshot();
            let code = LanguageCode::from(language_code);
            // The declared writing system, not the inventory `script_for`
            // collapses it to: CLDR keys its romanizations on `Hans`/`Jpan`,
            // and there is no transform under the collapsed `Hani`/`Hira`.
            let source = snap.catalog.writing_system_for(&code)?;
            let mucab_path = std::path::Path::new(&snap.base_dir)
                .join("bin")
                .join("mucab.bin");
            let mucab_path_str = mucab_path
                .exists()
                .then(|| mucab_path.to_string_lossy().into_owned());
            crate::transliterate::transliterate_with_policy_for_language(
                text,
                &code,
                source,
                mucab_path_str.as_deref(),
                true,
            )
        }
        #[cfg(not(feature = "transliterate"))]
        {
            let _ = (text, language_code);
            None
        }
    }

    #[cfg(feature = "doc-align")]
    pub fn detect_document_quad(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Option<DocumentDetection>, TranslatorError> {
        self.doc_align
            .detect_document_quad(&self.snapshot(), rgba, width, height)
    }

    #[cfg(feature = "doc-align")]
    pub fn warp_document_rgba(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
        quad: &DocumentQuad,
        out_width: Option<u32>,
        out_height: Option<u32>,
        postprocess: bool,
    ) -> Result<WarpedImageRgba, TranslatorError> {
        let (default_w, default_h) = crate::doc_align::suggested_output_dims(quad);
        let out_w = out_width.unwrap_or(default_w);
        let out_h = out_height.unwrap_or(default_h);
        crate::doc_align::warp(rgba, width, height, quad, out_w, out_h, postprocess)
    }

    #[cfg(feature = "doc-align")]
    pub fn suggested_warp_dims(&self, quad: &DocumentQuad) -> (u32, u32) {
        crate::doc_align::suggested_output_dims(quad)
    }
}

#[cfg(feature = "planar-tracker")]
impl translator_live::live_session::LiveRecognizer for TranslatorSession {
    fn recognize(
        &self,
        oriented: &crate::live_frame::OrientedImage,
        boxes: &[crate::ocr::DetectedTextBox],
        source_selection: &OcrSourceSelection,
        canonical_quadrant: Option<crate::coords::Quadrant>,
    ) -> Result<Vec<crate::ocr::RecognizedTextLine>, String> {
        let snap = self.snapshot();
        let ppocr = self.ocr().engine(&snap).map_err(|e| format!("{e:?}"))?;
        let script = match source_selection {
            OcrSourceSelection::Auto => None,
            OcrSourceSelection::Specific { language_code } => Some(
                crate::ocr_runtime::recognizer_script_for_language(&snap, language_code)
                    .map_err(|e| format!("{e:?}"))?,
            ),
        };
        let mut lines = translator_ocr::engine::recognize_oriented(
            &ppocr,
            oriented,
            boxes,
            script,
            canonical_quadrant,
        )
        .map_err(|e| format!("{e:?}"))?;
        if matches!(source_selection, OcrSourceSelection::Auto) {
            if let Some(code) = crate::ocr_runtime::ocr_source_for_lines(&snap, &lines, None) {
                let code = code.as_str().to_owned();
                for line in &mut lines {
                    if !line.text.trim().is_empty() {
                        line.source_code = Some(code.clone());
                    }
                }
            }
        }
        Ok(lines)
    }
}

#[cfg(feature = "planar-tracker")]
impl translator_live::live_session::LiveTranslator for TranslatorSession {
    fn translate_mixed_texts_with_alignment(
        &self,
        inputs: &[String],
        forced_source_code: Option<&str>,
        target_code: &str,
        available_language_codes: &[LanguageCode],
    ) -> Result<Vec<crate::translate::TranslationWithAlignment>, String> {
        self.translate_mixed_texts_with_alignment(
            inputs,
            forced_source_code,
            target_code,
            available_language_codes,
        )
        .map_err(|e| format!("{e:?}"))
    }
}

#[cfg(feature = "planar-tracker")]
impl translator_live::live_session::LiveOcrHost for TranslatorSession {
    fn ppocr_engine(&self) -> Result<Arc<crate::ppocr::PpocrEngine>, String> {
        self.ocr()
            .engine(&self.snapshot())
            .map_err(|e| format!("{e:?}"))
    }

    fn orient_script(&self, from_lang: &str, is_auto_source: bool) -> Option<crate::PpocrScript> {
        if is_auto_source {
            return None;
        }
        crate::ocr_runtime::recognizer_script_for_language(
            &self.snapshot(),
            &LanguageCode::from(from_lang),
        )
        .ok()
    }

    fn available_language_codes(&self) -> Vec<LanguageCode> {
        self.language_rows()
            .into_iter()
            .map(|row| LanguageCode::from(row.language.code.as_str()))
            .collect()
    }
}

#[cfg(feature = "pdf-image-translate")]
impl translator_pdf::pdf_image_translate::ImageTranslator for TranslatorSession {
    fn translate_image_rgba(
        &self,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        max_image_size: u32,
        source_selection: OcrSourceSelection,
        target_code: &str,
        min_confidence: u32,
        reading_order: Option<ReadingOrder>,
        background_mode: BackgroundMode,
        detection: Option<Vec<crate::ocr::DetectedTextBox>>,
    ) -> Result<crate::ocr::PreparedImageOverlay, TranslatorError> {
        self.translate_image_rgba(
            rgba_bytes,
            width,
            height,
            max_image_size,
            source_selection,
            target_code,
            min_confidence,
            reading_order,
            background_mode,
            detection,
        )
    }
}

impl translator_translate::document_translator::DocumentTranslator for TranslatorSession {
    fn begin_document_translation(&self) {
        self.begin_document_translation();
    }

    fn translate_texts_ctx(
        &self,
        from_code: &str,
        to_code: &str,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Vec<String>, TranslatorError> {
        self.translate_texts_ctx(from_code, to_code, texts, on_progress)
    }

    fn translate_texts_with_alignment_ctx(
        &self,
        from_code: &LanguageCode,
        to_code: &LanguageCode,
        texts: &[String],
        on_progress: &(dyn Fn(usize, usize) + Sync),
    ) -> Result<Option<Vec<crate::translate::TranslationWithAlignment>>, TranslatorError> {
        self.translate_texts_with_alignment_ctx(from_code, to_code, texts, on_progress)
    }
}

pub fn parse_selected_catalog(
    bundled_json: &str,
    disk_json: Option<&str>,
) -> Option<crate::catalog::LanguageCatalog> {
    let preferred = select_best_catalog(bundled_json, disk_json).ok()?;
    let fallback = if std::ptr::eq(preferred, bundled_json) {
        disk_json
    } else {
        Some(bundled_json)
    };

    parse_and_validate_catalog(preferred)
        .ok()
        .or_else(|| fallback.and_then(|json| parse_and_validate_catalog(json).ok()))
}
