#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "doc-align")]
pub use translator_align::doc_align;
#[cfg(feature = "doc-align")]
pub use translator_align::doc_align_refine;
pub use translator_core::api;
pub use translator_core::catalog;
pub use translator_core::coords;
pub use translator_core::homography;
pub use translator_core::homography_ekf;
pub use translator_core::language;
pub use translator_core::ocr;
pub use translator_core::script;
pub use translator_core::script_normalize;
pub use translator_core::selection;
#[cfg(feature = "epub")]
pub use translator_doc::epub;
#[cfg(feature = "odt")]
pub use translator_doc::odt;
#[cfg(feature = "gpu")]
pub use translator_gpu::gl_renderer;
#[cfg(feature = "gpu")]
pub use translator_gpu::live_gpu_tick;
#[cfg(feature = "planar-tracker")]
pub use translator_live::live_screen;
#[cfg(feature = "planar-tracker")]
pub use translator_live::live_session;
#[cfg(feature = "planar-tracker")]
pub use translator_live::live_tracker_pipeline;
#[cfg(feature = "planar-tracker")]
pub use translator_live::live_worker;
#[cfg(feature = "mucab")]
pub use translator_mucab::mucab;
#[cfg(feature = "ppocr")]
pub use translator_ocr::ocr_runtime;
#[cfg(feature = "ppocr")]
pub use translator_ocr::ppocr;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_content;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_font_embed;
#[cfg(feature = "pdf-image-translate")]
pub use translator_pdf::pdf_image_translate;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_overlay;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_resources;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_surgery;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_text;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_text_overlay;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_translate;
#[cfg(feature = "pdf")]
pub use translator_pdf::pdf_write;
#[cfg(feature = "pdf")]
pub use translator_pdf::styled;
#[cfg(feature = "raster")]
pub use translator_raster::color_matting;
#[cfg(feature = "raster")]
pub use translator_raster::live_frame;
#[cfg(any(feature = "pdf", feature = "image-render"))]
pub use translator_render::font_metrics;
#[cfg(any(feature = "pdf", feature = "image-render"))]
pub use translator_render::font_provider;
#[cfg(feature = "image-render")]
pub use translator_render::image_render;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::coarse_tracker;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::klt;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::planar_engine;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::planar_tracker;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::screen_monitor;
pub use translator_translate::bergamot;
#[cfg(feature = "dom-translate")]
pub use translator_translate::dom_translate;
#[cfg(feature = "html")]
pub use translator_translate::html_translate;
pub use translator_translate::language_detect;
pub use translator_translate::routing;
pub use translator_translate::sentence_split;
#[cfg(any(feature = "pdf", feature = "image-render"))]
pub mod document;
#[cfg(feature = "http")]
pub mod http;
pub mod session;
pub use translator_core::settings;
pub use translator_core::tts;
#[cfg(feature = "dictionary")]
pub use translator_dictionary::tarkka;
pub use translator_doc::txt;
#[cfg(feature = "raster")]
pub use translator_raster::overlay;
#[cfg(feature = "raster")]
pub use translator_raster::text_metrics;
#[cfg(any(feature = "pdf", feature = "image-render"))]
pub use translator_render::text_runs;
#[cfg(any(feature = "pdf", feature = "image-render"))]
pub use translator_render::text_shape;
#[cfg(feature = "planar-tracker")]
pub use translator_tracker::surface_map;
pub use translator_translate::translate;
#[cfg(feature = "transliterate")]
pub use translator_transliterate::transliterate;
#[cfg(feature = "tts")]
pub use translator_tts::speech;

pub use api::{DictionaryCode, LanguageCode, ScriptCode, TranslatorError, TranslatorErrorKind};
pub use catalog::{
    CatalogSnapshot, DeletePlan, DictionaryInfo, DownloadPlan, DownloadTask, FileRole,
    FsPackInstallChecker, InstalledTtsPack, LanguageAvailabilityRow, LanguageCatalog,
    LanguageOverview, OcrEngine, OcrPack, PpocrScript, TtsSpeakerEntry, TtsVoicePickerRegion,
    available_ocr_engines_for_language, installed_ocr_engines_for_language,
    installed_tts_voice_picker_regions, language_rows_in_snapshot, ocr_engine_ready,
    parse_and_validate_catalog, plan_delete_superseded_files, plan_ocr_engine_download,
    plan_ocr_engine_downloads, plan_ocr_engine_upgrades, plan_repair, plan_translation_upgrades,
    translation_upgrade_language_codes,
};
pub use language_detect::DetectionResult;
pub use ocr::{
    DetectedTextBox, OcrSourceSelection, OrientedRect, OverlayColors, PreparedImageOverlay,
    ReadingOrder, RecognizedTextLine, Rect, sample_overlay_colors,
};
pub use routing::MixedTextTranslationResult;
pub use session::{Feature, TranslatorSession};
pub use settings::BackgroundMode;
pub use translate::{
    TokenAlignment, TranslationWithAlignment, TranslationWithAlternatives, WordAlternative,
    WordAlternatives,
};
pub use tts::{PcmAudio, SpeechChunk, TtsVoiceOption, UrlsAndHashtags};
