#[path = "catalog_model.rs"]
mod model;
#[path = "catalog_planner.rs"]
mod planner;
#[path = "catalog_wire.rs"]
mod wire;

pub use model::{
    AssetFileV2, AssetPackMetadataV2, CatalogSourcesV2, DeletePlan, DictionaryInfo, DownloadPlan,
    DownloadTask, FileRequirement, FileRole, InstalledTtsPack, LangAvailability, LanguageCatalog,
    LanguageFeature, LanguageTtsRegionV2, LanguageTtsV2, MigrationAction, MigrationEntry,
    MigrationJob, OcrEngine, OcrPack, PackKind, PackRecord, PpocrScript, ResolvedTtsVoiceFiles,
    TtsSpeakerEntry, TtsVoicePackInfo, TtsVoicePickerRegion, translation_direction_from_files,
};
pub use planner::{
    CatalogSnapshot, FsPackInstallChecker, LanguageAvailabilityRow, LanguageOverview,
    PackInstallChecker, PackInstallStatus, TtsVoiceOverview, TtsVoiceRegionOverview,
    available_ocr_engines_for_language, build_catalog_snapshot, build_language_overview,
    can_translate, installed_ocr_engines_for_language, installed_tts_voice_picker_regions,
    installed_tts_voices_for_language, language_rows_in_snapshot, ocr_engine_ready,
    plan_delete_dictionary, plan_delete_language, plan_delete_superseded_files,
    plan_delete_superseded_tts, plan_delete_support_by_kind, plan_delete_tts, plan_delete_tts_pack,
    plan_dictionary_download, plan_language_download, plan_migrations, plan_ocr_engine_download,
    plan_ocr_engine_downloads, plan_ocr_engine_upgrades, plan_repair,
    plan_support_download_by_kind, plan_translation_upgrades, plan_tts_download,
    resolve_tts_voice_files, resolve_tts_voice_files_for_pack, translation_upgrade_language_codes,
};
pub use wire::{parse_and_validate_catalog, parse_language_catalog, select_best_catalog};

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
