use std::collections::{HashMap, HashSet};

use crate::api::{DictionaryCode, LanguageCode};
use crate::language::{Language, LanguageDirection, ModelFile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSourcesV2 {
    pub language_index_version: i32,
    pub language_index_updated_at: i64,
    pub dictionary_index_version: i32,
    pub dictionary_index_updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguageTtsRegionV2 {
    pub display_name: String,
    pub voices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguageTtsV2 {
    pub default_region: Option<String>,
    pub regions: Vec<(String, LanguageTtsRegionV2)>,
    pub sample_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LanguageResources {
    pub translation_root_packs: Vec<String>,
    pub ocr_packs: Vec<(String, String)>,
    pub dictionary_pack_id: Option<String>,
    pub support_root_packs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanguageInfo {
    pub language: Language,
    pub resources: LanguageResources,
    pub tts: Option<LanguageTtsV2>,
}

/// Groups interchangeable files within a pack: files sharing a role are
/// alternatives for the same slot, ranked by `AssetFileV2::priority`.
///
/// The named variants are the roles this build dispatches on (OCR sub-models,
/// translation model/vocab/lex, TTS voice model + engine-specific aux). Any
/// other role a catalog defines becomes `Other`, kept opaque so grouping still
/// works by equality — a newer catalog can introduce roles without breaking
/// install/upgrade logic on older clients.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileRole {
    Detector,
    ScriptClassifier,
    TextlineOrientation,
    Recognizer,
    Keys,
    Ink,
    Model,
    Lex,
    SrcVocab,
    TgtVocab,
    Vocab,
    Other(String),
}

impl FileRole {
    pub fn new(value: impl AsRef<str>) -> Self {
        match value.as_ref() {
            "detector" => Self::Detector,
            "scriptClassifier" => Self::ScriptClassifier,
            "textlineOrientation" => Self::TextlineOrientation,
            "recognizer" => Self::Recognizer,
            "keys" => Self::Keys,
            "ink" => Self::Ink,
            "model" => Self::Model,
            "lex" => Self::Lex,
            "srcVocab" => Self::SrcVocab,
            "tgtVocab" => Self::TgtVocab,
            "vocab" => Self::Vocab,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Detector => "detector",
            Self::ScriptClassifier => "scriptClassifier",
            Self::TextlineOrientation => "textlineOrientation",
            Self::Recognizer => "recognizer",
            Self::Keys => "keys",
            Self::Ink => "ink",
            Self::Model => "model",
            Self::Lex => "lex",
            Self::SrcVocab => "srcVocab",
            Self::TgtVocab => "tgtVocab",
            Self::Vocab => "vocab",
            Self::Other(value) => value,
        }
    }
}

/// Whether a catalog file is needed for its pack to count as installed. A
/// `Required` file that is absent marks the pack incomplete (the language shows
/// as missing and the file is force-downloaded). An `Optional` file that is
/// absent leaves the pack installed and is surfaced as an improvement upgrade
/// instead, so a newly-added optional model (e.g. the ink matte) never demotes
/// an already-installed language on app upgrade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileRequirement {
    #[default]
    Required,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFileV2 {
    pub name: String,
    pub size_bytes: u64,
    pub install_path: String,
    pub url: String,
    pub source_path: Option<String>,
    pub archive_format: Option<String>,
    pub extract_to: Option<String>,
    pub delete_after_extract: bool,
    pub install_marker_path: Option<String>,
    pub install_marker_version: Option<i32>,
    pub role: FileRole,
    pub priority: i32,
    pub requirement: FileRequirement,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssetPackMetadataV2 {
    pub date: Option<i64>,
    pub type_name: Option<String>,
    pub word_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRecord {
    pub id: String,
    pub files: Vec<AssetFileV2>,
    pub depends_on: Vec<String>,
    pub kind: PackKind,
}

impl PackRecord {
    /// Files filling the given role, best (highest priority) first.
    /// Ties keep catalog order.
    pub fn role_alternatives(&self, role: &FileRole) -> Vec<&AssetFileV2> {
        let mut files = self
            .files
            .iter()
            .filter(|file| &file.role == role)
            .collect::<Vec<_>>();
        files.sort_by_key(|file| std::cmp::Reverse(file.priority));
        files
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackKind {
    Translation(TranslationPack),
    Ocr(OcrPack),
    Tts(TtsPack),
    Dictionary(DictionaryPack),
    Support(SupportPack),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationPack {
    pub from: String,
    pub to: String,
    pub experimental: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrPack {
    PpocrDetector,
    PpocrRecognizer { script: PpocrScript },
}

impl OcrPack {
    pub fn engine(&self) -> OcrEngine {
        match self {
            OcrPack::PpocrDetector | OcrPack::PpocrRecognizer { .. } => OcrEngine::Ppocr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OcrEngine {
    Ppocr,
}

impl OcrEngine {
    pub fn as_str(&self) -> &'static str {
        match self {
            OcrEngine::Ppocr => "ppocr",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "ppocr" => Some(OcrEngine::Ppocr),
            _ => None,
        }
    }
}

/// PPOCR recognizer script slugs.
///
/// One PPOCR recognizer model covers every language that uses the script.
/// `Cj` is the shared Chinese + Japanese model (Korean uses its own).
/// `Eslav` is the East-Slavic specialization (be/ru/uk); `Cyrillic` is the
/// non-East-Slavic Cyrillic fallback (bg/sr).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PpocrScript {
    Arabic,
    Cj,
    Cyrillic,
    Devanagari,
    El,
    Eslav,
    /// Georgian. Mkhedruli plus Mtavruli, which is a separate codepoint block rather
    /// than a case styling, so the recognizer emits whichever the page carries. The
    /// script is unicameral — case applies to a whole run, never to a leading letter.
    Georgian,
    /// Hebrew. The fine-tuned rec model emits glyphs in visual (RTL) order; logical
    /// order is recovered downstream by the bidi pass, same as Arabic.
    Hebrew,
    /// Merged Bengali + Gujarati + Kannada + Malayalam recognizer (one model over the
    /// four non-Devanagari Indic scripts; Devanagari/hi keeps its own recognizer).
    Indic,
    Korean,
    Latin,
    Ta,
    Te,
    Th,
}

impl PpocrScript {
    pub fn as_slug(&self) -> &'static str {
        match self {
            PpocrScript::Arabic => "arabic",
            PpocrScript::Cj => "cj",
            PpocrScript::Cyrillic => "cyrillic",
            PpocrScript::Devanagari => "devanagari",
            PpocrScript::El => "el",
            PpocrScript::Eslav => "eslav",
            PpocrScript::Georgian => "georgian",
            PpocrScript::Hebrew => "hebrew",
            PpocrScript::Indic => "indic",
            PpocrScript::Korean => "korean",
            PpocrScript::Latin => "latin",
            PpocrScript::Ta => "ta",
            PpocrScript::Te => "te",
            PpocrScript::Th => "th",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        match value {
            "arabic" => Some(PpocrScript::Arabic),
            "cj" => Some(PpocrScript::Cj),
            "cyrillic" => Some(PpocrScript::Cyrillic),
            "devanagari" => Some(PpocrScript::Devanagari),
            "el" => Some(PpocrScript::El),
            "eslav" => Some(PpocrScript::Eslav),
            "georgian" => Some(PpocrScript::Georgian),
            "hebrew" => Some(PpocrScript::Hebrew),
            "indic" => Some(PpocrScript::Indic),
            "korean" => Some(PpocrScript::Korean),
            "latin" => Some(PpocrScript::Latin),
            "ta" => Some(PpocrScript::Ta),
            "te" => Some(PpocrScript::Te),
            "th" => Some(PpocrScript::Th),
            _ => None,
        }
    }

    /// True for scripts whose recognizer emits glyphs in visual (left-to-right strip)
    /// order, so the decoded line needs a visual→logical bidi pass. PaddleOCR's Arabic
    /// model and our fine-tuned Hebrew model are both trained this way.
    pub fn is_rtl(&self) -> bool {
        matches!(self, PpocrScript::Arabic | PpocrScript::Hebrew)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsPack {
    pub language: String,
    pub engine: Option<String>,
    pub aux_role: FileRole,
    pub locale: Option<String>,
    pub region: Option<String>,
    pub voice: Option<String>,
    pub quality: Option<String>,
    pub num_speakers: Option<i32>,
    pub default_speaker_id: Option<i32>,
    pub sample_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryPack {
    pub language: Option<String>,
    pub dictionary_code: String,
    pub metadata: Option<AssetPackMetadataV2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SupportPack {
    pub language: Option<String>,
    pub languages: Vec<String>,
    pub aliases: Vec<String>,
    pub kind: Option<String>,
    pub metadata: Option<AssetPackMetadataV2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DictionaryInfo {
    pub date: i64,
    pub filename: String,
    pub size: u64,
    pub type_name: String,
    pub word_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageFeature {
    Translation,
    Ocr,
    Dictionary,
    Support,
    Tts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TtsVoicePackInfo {
    pub pack_id: String,
    pub display_name: String,
    pub quality: Option<String>,
    pub size_bytes: u64,
    pub sample_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TtsSpeakerEntry {
    pub name: String,
    pub speaker_id: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct InstalledTtsPack {
    pub pack_id: String,
    pub display_name: String,
    pub voices: Vec<TtsSpeakerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct TtsVoicePickerRegion {
    pub code: String,
    pub display_name: String,
    pub voices: Vec<TtsVoicePackInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTtsVoiceFiles {
    pub engine: String,
    pub model_install_path: String,
    pub aux_install_path: String,
    /// Second network, for engines that split the acoustic model from the vocoder.
    pub vocoder_install_path: Option<String>,
    pub language_code: String,
    pub speaker_id: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageCatalog {
    pub format_version: i32,
    pub generated_at: i64,
    pub dictionary_version: i32,
    pub sources: CatalogSourcesV2,
    pub(crate) languages: HashMap<String, LanguageInfo>,
    pub packs: HashMap<String, PackRecord>,
    pub(crate) translation_pack_ids: HashMap<(String, String), String>,
    pub(crate) dictionary_pack_ids_by_code: HashMap<String, String>,
    pub(crate) root_pack_ids_by_language_feature: HashMap<(String, LanguageFeature), Vec<String>>,
    pub migrations: Vec<MigrationEntry>,
}

/// One ONNX→MNN conversion the app may run on-device to migrate an install that
/// predates the MNN-only runtime. Self-contained (both sizes carried) so the UI
/// can show reclaimed space without cross-referencing the pack file entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationEntry {
    pub onnx: String,
    pub mnn: String,
    pub quant_bits: i32,
    pub onnx_bytes: u64,
    pub mnn_bytes: u64,
    pub feature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationAction {
    Convert,
    CleanupOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationJob {
    pub entry: MigrationEntry,
    pub action: MigrationAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DownloadTask {
    pub pack_id: String,
    pub install_path: String,
    pub url: String,
    pub size_bytes: u64,
    pub decompress: bool,
    pub archive_format: Option<String>,
    pub extract_to: Option<String>,
    pub delete_after_extract: bool,
    pub install_marker_path: Option<String>,
    pub install_marker_version: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DownloadPlan {
    pub total_size: u64,
    pub tasks: Vec<DownloadTask>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DeletePlan {
    pub file_paths: Vec<String>,
    pub directory_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LangAvailability {
    pub has_from_english: bool,
    pub has_to_english: bool,
    pub ocr_files: bool,
    pub dictionary_files: bool,
    pub tts_files: bool,
}

impl LangAvailability {
    pub fn translator_files(&self) -> bool {
        self.has_from_english || self.has_to_english
    }
}

// A model that shares one vocabulary for both directions publishes it under the
// `vocab` role; separate source/target vocabularies use `srcVocab`/`tgtVocab`.
pub fn translation_direction_from_files(files: &[AssetFileV2]) -> Option<LanguageDirection> {
    let best = |role: &FileRole| -> Option<&AssetFileV2> {
        files
            .iter()
            .filter(|file| &file.role == role)
            .max_by_key(|file| file.priority)
    };

    let model = best(&FileRole::Model)?;
    let lex = best(&FileRole::Lex)?;
    let src_vocab = best(&FileRole::SrcVocab).or_else(|| best(&FileRole::Vocab))?;
    let tgt_vocab = best(&FileRole::TgtVocab)
        .or_else(|| best(&FileRole::Vocab))
        .unwrap_or(src_vocab);

    let to_model_file = |file: &AssetFileV2| ModelFile {
        name: file.name.clone(),
        size_bytes: file.size_bytes,
        path: file.install_path.clone(),
    };

    Some(LanguageDirection {
        model: to_model_file(model),
        src_vocab: to_model_file(src_vocab),
        tgt_vocab: to_model_file(tgt_vocab),
        lex: to_model_file(lex),
    })
}

impl LanguageCatalog {
    pub(crate) fn language_info(&self, code: &str) -> Option<&LanguageInfo> {
        self.languages.get(code)
    }

    pub fn pack(&self, pack_id: &str) -> Option<&PackRecord> {
        self.packs.get(pack_id)
    }

    pub fn language_list(&self) -> Vec<Language> {
        let mut codes = self.languages.keys().cloned().collect::<Vec<_>>();
        codes.sort();
        codes
            .into_iter()
            .filter_map(|code| self.languages.get(&code).map(|info| info.language.clone()))
            .collect()
    }

    pub fn language_by_code(&self, code: &LanguageCode) -> Option<Language> {
        self.languages
            .get(code.as_str())
            .map(|info| info.language.clone())
    }

    /// The writing system the catalog records for a language. `None` only when
    /// the language is absent; an unrecognized script parses as
    /// [`Script::Other`], never as a guess.
    pub fn script_for(&self, code: &LanguageCode) -> Option<crate::script::Script> {
        self.languages
            .get(code.as_str())
            .map(|info| info.language.script)
    }

    /// The writing system the catalog declares for a language, composite
    /// subtags intact. [`LanguageCatalog::script_for`] answers which glyphs to
    /// draw it with; this answers which writing system it is.
    pub fn writing_system_for(&self, code: &LanguageCode) -> Option<crate::script::WritingSystem> {
        self.languages
            .get(code.as_str())
            .map(|info| info.language.writing_system)
    }

    pub fn scripted_language(&self, code: &LanguageCode) -> Option<crate::api::ScriptedLanguage> {
        Some(crate::api::ScriptedLanguage {
            code: code.clone(),
            script: self.script_for(code)?,
        })
    }

    pub fn english(&self) -> Option<Language> {
        self.language_by_code(&LanguageCode::from("en"))
    }

    pub fn dictionary_info_for(&self, language: &Language) -> Option<DictionaryInfo> {
        self.dictionary_info(&DictionaryCode::from(language.dictionary_code.clone()))
    }

    pub fn dictionary_info(&self, dictionary_code: &DictionaryCode) -> Option<DictionaryInfo> {
        let pack_id = self
            .dictionary_pack_ids_by_code
            .get(dictionary_code.as_str())?;
        let pack = self.packs.get(pack_id)?;
        let file = pack.files.first()?;
        let metadata = match &pack.kind {
            PackKind::Dictionary(dictionary) => dictionary.metadata.as_ref(),
            PackKind::Support(support) => support.metadata.as_ref(),
            _ => None,
        };
        Some(DictionaryInfo {
            date: metadata.and_then(|m| m.date).unwrap_or(0),
            filename: file.name.clone(),
            size: file.size_bytes,
            type_name: metadata
                .and_then(|m| m.type_name.clone())
                .unwrap_or_default(),
            word_count: metadata.and_then(|m| m.word_count).unwrap_or(0),
        })
    }

    pub(crate) fn tts(&self, language_code: &LanguageCode) -> Option<&LanguageTtsV2> {
        self.language_info(language_code.as_str())?.tts.as_ref()
    }

    pub fn tts_pack_ids_for_language(&self, language_code: &LanguageCode) -> Vec<String> {
        let Some(tts) = self.tts(language_code) else {
            return Vec::new();
        };
        tts_pack_ids_from_config(tts)
    }

    pub fn tts_sample_text(&self, language_code: &LanguageCode) -> Option<String> {
        self.tts(language_code)?.sample_text.clone()
    }

    pub fn ordered_tts_regions_for_language(
        &self,
        language_code: &LanguageCode,
    ) -> Vec<(String, LanguageTtsRegionV2)> {
        let Some(tts) = self.tts(language_code) else {
            return Vec::new();
        };

        let mut ordered_codes = Vec::new();
        if let Some(default_region) = &tts.default_region {
            if tts.regions.iter().any(|(code, _)| code == default_region) {
                ordered_codes.push(default_region.clone());
            }
        }

        let mut remaining_codes = tts
            .regions
            .iter()
            .map(|(code, _)| code.clone())
            .filter(|code| Some(code) != tts.default_region.as_ref())
            .collect::<Vec<_>>();
        remaining_codes.sort();
        ordered_codes.extend(remaining_codes);

        ordered_codes
            .into_iter()
            .filter_map(|region_code| {
                tts.regions
                    .iter()
                    .find(|(code, _)| code == &region_code)
                    .map(|(_, region)| (region_code, region.clone()))
            })
            .collect()
    }

    pub fn default_tts_pack_id_for_language(&self, language_code: &LanguageCode) -> Option<String> {
        let tts = self.tts(language_code)?;
        let region = tts
            .default_region
            .as_ref()
            .and_then(|code| {
                tts.regions
                    .iter()
                    .find(|(region_code, _)| region_code == code)
            })
            .map(|(_, region)| region)
            .or_else(|| tts.regions.first().map(|(_, region)| region))?;
        region.voices.first().cloned()
    }

    pub fn tts_size_bytes_for_language(&self, language_code: &LanguageCode) -> u64 {
        self.default_tts_pack_id_for_language(language_code)
            .map(|pack_id| self.pack_size_bytes(&pack_id))
            .unwrap_or(0)
    }

    pub fn has_tts_voices(&self, language_code: &LanguageCode) -> bool {
        !self
            .root_pack_ids_for_feature(language_code, LanguageFeature::Tts)
            .is_empty()
    }

    pub fn translation_size_bytes_for_language(&self, language_code: &LanguageCode) -> u64 {
        self.root_pack_ids_for_feature(language_code, LanguageFeature::Translation)
            .iter()
            .map(|pack_id| self.pack_size_bytes(pack_id))
            .sum()
    }

    pub fn tts_voice_pack_info(&self, pack_id: &str) -> Option<TtsVoicePackInfo> {
        let pack = self.pack(pack_id)?;
        let PackKind::Tts(tts) = &pack.kind else {
            return None;
        };
        Some(TtsVoicePackInfo {
            pack_id: pack.id.clone(),
            display_name: tts.voice.clone().unwrap_or_else(|| pack.id.clone()),
            quality: tts.quality.clone(),
            size_bytes: self.pack_size_bytes(pack_id),
            sample_url: tts.sample_url.clone(),
        })
    }

    pub fn tts_voice_picker_regions(
        &self,
        language_code: &LanguageCode,
    ) -> Vec<TtsVoicePickerRegion> {
        self.ordered_tts_regions_for_language(language_code)
            .into_iter()
            .map(|(code, region)| TtsVoicePickerRegion {
                code,
                display_name: region.display_name,
                voices: region
                    .voices
                    .into_iter()
                    .filter_map(|pack_id| self.tts_voice_pack_info(&pack_id))
                    .collect(),
            })
            .filter(|region| !region.voices.is_empty())
            .collect()
    }

    pub fn pack_size_bytes(&self, pack_id: &str) -> u64 {
        self.unique_files_in_dependency_closure([pack_id])
            .into_iter()
            .map(|file| file.size_bytes)
            .sum()
    }

    pub fn translation_pack_id(&self, from: &str, to: &str) -> Option<String> {
        self.translation_pack_ids
            .get(&(from.to_string(), to.to_string()))
            .cloned()
    }

    pub fn can_swap_languages(&self, from_code: &LanguageCode, to_code: &LanguageCode) -> bool {
        let to_can_be_source =
            to_code.as_str() == "en" || self.translation_pack_id(to_code.as_str(), "en").is_some();
        let from_can_be_target = from_code.as_str() == "en"
            || self.translation_pack_id("en", from_code.as_str()).is_some();
        to_can_be_source && from_can_be_target
    }

    pub(crate) fn root_pack_ids_for_feature(
        &self,
        language_code: &LanguageCode,
        feature: LanguageFeature,
    ) -> &[String] {
        self.root_pack_ids_by_language_feature
            .get(&(language_code.as_str().to_string(), feature))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn core_pack_ids_for_language(&self, language_code: &str) -> HashSet<String> {
        let language_code = LanguageCode::from(language_code);
        let mut pack_ids = HashSet::new();
        if language_code.as_str() != "en" {
            pack_ids.extend(
                self.root_pack_ids_for_feature(&language_code, LanguageFeature::Translation)
                    .iter()
                    .cloned(),
            );
        }
        pack_ids.extend(
            self.root_pack_ids_for_feature(&language_code, LanguageFeature::Ocr)
                .iter()
                .cloned(),
        );
        pack_ids.extend(
            self.root_pack_ids_for_feature(&language_code, LanguageFeature::Support)
                .iter()
                .cloned(),
        );
        pack_ids
    }

    pub(crate) fn dictionary_pack_id_for_language(&self, language_code: &str) -> Option<String> {
        self.root_pack_ids_for_feature(
            &LanguageCode::from(language_code),
            LanguageFeature::Dictionary,
        )
        .first()
        .cloned()
    }

    pub(crate) fn dependency_closure<'a, I>(&self, root_pack_ids: I) -> Vec<String>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut resolved = Vec::new();
        let mut seen = HashSet::new();

        fn visit(
            catalog: &LanguageCatalog,
            pack_id: &str,
            resolved: &mut Vec<String>,
            seen: &mut HashSet<String>,
        ) {
            if !seen.insert(pack_id.to_string()) {
                return;
            }
            resolved.push(pack_id.to_string());
            if let Some(pack) = catalog.packs.get(pack_id) {
                for dep_id in &pack.depends_on {
                    visit(catalog, dep_id, resolved, seen);
                }
            }
        }

        for pack_id in root_pack_ids {
            visit(self, pack_id, &mut resolved, &mut seen);
        }
        resolved
    }

    pub fn pack_files_with_dependencies(&self, root_pack_id: &str) -> Vec<&AssetFileV2> {
        self.dependency_closure([root_pack_id])
            .into_iter()
            .filter_map(|pack_id| self.packs.get(&pack_id))
            .flat_map(|pack| pack.files.iter())
            .collect()
    }

    pub fn support_files_by_kind(&self, support_kind: &str) -> Vec<AssetFileV2> {
        let mut packs = self
            .packs
            .values()
            .filter(|pack| match &pack.kind {
                PackKind::Support(support) => support.kind.as_deref() == Some(support_kind),
                _ => false,
            })
            .collect::<Vec<_>>();
        packs.sort_by(|left, right| left.id.cmp(&right.id));

        let mut seen_install_paths = HashSet::new();
        let mut files = Vec::new();
        for pack in packs {
            for file in self.pack_files_with_dependencies(&pack.id) {
                if seen_install_paths.insert(file.install_path.clone()) {
                    files.push(file.clone());
                }
            }
        }
        files
    }

    pub fn support_pack_ids_by_kind(&self, support_kind: &str) -> Vec<String> {
        let mut pack_ids = self
            .packs
            .values()
            .filter(|pack| match &pack.kind {
                PackKind::Support(support) => support.kind.as_deref() == Some(support_kind),
                _ => false,
            })
            .map(|pack| pack.id.clone())
            .collect::<Vec<_>>();
        pack_ids.sort();
        pack_ids
    }

    pub fn support_size_bytes_by_kind(&self, support_kind: &str) -> u64 {
        let pack_ids = self.support_pack_ids_by_kind(support_kind);
        self.unique_files_in_dependency_closure(pack_ids.iter().map(String::as_str))
            .into_iter()
            .map(|file| file.size_bytes)
            .sum()
    }

    pub fn ocr_engines_for_language(&self, language_code: &LanguageCode) -> Vec<(String, String)> {
        self.languages
            .get(language_code.as_str())
            .map(|info| info.resources.ocr_packs.clone())
            .unwrap_or_default()
    }

    pub fn ocr_pack_id_for_engine(
        &self,
        language_code: &LanguageCode,
        engine: &str,
    ) -> Option<String> {
        self.languages
            .get(language_code.as_str())?
            .resources
            .ocr_packs
            .iter()
            .find(|(pack_engine, _)| pack_engine == engine)
            .map(|(_, pack_id)| pack_id.clone())
    }

    pub(crate) fn unique_files_in_dependency_closure<'a, I>(
        &'a self,
        root_pack_ids: I,
    ) -> Vec<&'a AssetFileV2>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut seen_install_paths = HashSet::new();
        let mut files = Vec::new();
        for pack_id in self.dependency_closure(root_pack_ids) {
            let Some(pack) = self.packs.get(&pack_id) else {
                continue;
            };
            for file in &pack.files {
                if seen_install_paths.insert(file.install_path.clone()) {
                    files.push(file);
                }
            }
        }
        files
    }
}

pub(crate) fn tts_pack_ids_from_config(tts: &LanguageTtsV2) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut pack_ids = Vec::new();
    for (_, region) in &tts.regions {
        for voice in &region.voices {
            if seen.insert(voice.clone()) {
                pack_ids.push(voice.clone());
            }
        }
    }
    pack_ids
}
