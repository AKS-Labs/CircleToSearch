use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use piper_rs::{
    Backend, BoundaryAfter, CoquiVitsModel, CotoviaVitsModel, GlowTtsHifiganModel, KokoroMnnModel,
    MmsModel, PhonemeChunk as PiperPhonemeChunk, PiperModel, SherpaVitsModel,
};
use translator_core::api::{LanguageCode, TranslatorError, VoiceName};
use translator_core::catalog::{CatalogSnapshot, ResolvedTtsVoiceFiles};
use translator_core::script::Script;
use translator_core::tts::{
    PcmAudio, PhonemeChunk, SpeechChunk, SpeechChunkBoundary, TtsVoiceOption, UrlsAndHashtags,
    plan_speech_chunks,
};

fn log_debug(message: impl AsRef<str>) {
    log::debug!("{}", message.as_ref());
}

fn log_error(message: impl AsRef<str>) {
    log::error!("{}", message.as_ref());
}

pub struct SpeechCache {
    model: Option<CachedSpeechModel>,
}

impl SpeechCache {
    pub fn new() -> Self {
        Self { model: None }
    }

    pub fn clear(&mut self) {
        self.model = None;
    }
}

impl Default for SpeechCache {
    fn default() -> Self {
        Self::new()
    }
}

fn audio_duration_ms(sample_count: usize, sample_rate: u32) -> u64 {
    if sample_rate == 0 {
        return 0;
    }
    ((sample_count as u64) * 1000) / u64::from(sample_rate)
}

enum SpeechModel {
    Piper(PiperModel),
    KokoroMnn(KokoroMnnModel),
    Mms(MmsModel),
    CoquiVits(CoquiVitsModel),
    CotoviaVits(CotoviaVitsModel),
    SherpaVits(SherpaVitsModel),
    GlowTtsHifigan(GlowTtsHifiganModel),
}

/// Every file a voice loads from, resolved out of the catalog. They travel as
/// one value because each layer below needs the whole set, and which files a
/// voice has depends on its engine.
#[derive(Clone, PartialEq, Eq)]
struct SpeechModelPaths {
    engine: String,
    model: String,
    aux: String,
    /// Second network, for engines that split the acoustic model from the vocoder.
    vocoder: Option<String>,
    language_code: String,
    support_data_root: Option<String>,
}

impl SpeechModelPaths {
    fn support_root(&self) -> &str {
        self.support_data_root.as_deref().unwrap_or_default()
    }
}

struct CachedSpeechModel {
    paths: SpeechModelPaths,
    voices: Vec<(String, i64)>,
    default_voice: Option<(String, i64)>,
    model: SpeechModel,
}

struct ResolvedSpeechAssets {
    paths: SpeechModelPaths,
    script: Script,
    speaker_id: Option<i64>,
}

fn log_timing(step: &str, started_at: Instant) {
    log_debug(format!(
        "{step} took {} ms",
        started_at.elapsed().as_millis()
    ));
}

fn summarize_phoneme_chunk_sizes(chunks: &[PiperPhonemeChunk]) -> String {
    const MAX_CHUNKS_TO_LOG: usize = 6;

    let preview = chunks
        .iter()
        .take(MAX_CHUNKS_TO_LOG)
        .map(|chunk| chunk.phonemes.chars().count().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    if chunks.len() > MAX_CHUNKS_TO_LOG {
        format!("{preview}, ...")
    } else {
        preview
    }
}

fn normalized_language_family(language_code: &str) -> &str {
    language_code
        .split(['-', '_'])
        .next()
        .unwrap_or(language_code)
}

fn kokoro_voice_prefixes(language_code: &str) -> &'static [&'static str] {
    match normalized_language_family(language_code) {
        "en" => &["a", "b"],
        "es" => &["e"],
        "fr" => &["f"],
        "hi" => &["h"],
        "it" => &["i"],
        "ja" => &["j"],
        "ko" => &["j"],
        "pt" => &["p"],
        "zh" => &["z"],
        _ => &[],
    }
}

fn available_voices(model: &SpeechModel) -> Vec<(String, i64)> {
    let mut voices = match model {
        SpeechModel::Piper(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::KokoroMnn(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::Mms(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::CoquiVits(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::CotoviaVits(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::GlowTtsHifigan(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
        SpeechModel::SherpaVits(model) => model
            .voices()
            .map(|voices| {
                voices
                    .iter()
                    .map(|(name, id)| (name.clone(), *id))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    };
    voices.sort_by(|left, right| left.0.cmp(&right.0));
    voices
}

fn is_kokoro_voice_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(first), Some(second), Some('_'))
            if first.is_ascii_lowercase() && second.is_ascii_lowercase()
    )
}

fn format_voice_display_name(name: &str) -> String {
    let trimmed = if is_kokoro_voice_name(name) {
        &name[3..]
    } else {
        name
    };

    trimmed
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => {
                    let mut formatted = String::new();
                    formatted.extend(first.to_uppercase());
                    formatted.push_str(chars.as_str());
                    formatted
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn visible_voices(
    engine: &str,
    language_code: &str,
    voices: &[(String, i64)],
) -> Vec<TtsVoiceOption> {
    let filtered = if engine == "kokoro" || engine == "kokoro_mnn" {
        let prefixes = kokoro_voice_prefixes(language_code);
        let matching = voices
            .iter()
            .filter(|(name, _)| prefixes.iter().any(|prefix| name.starts_with(prefix)))
            .cloned()
            .collect::<Vec<_>>();
        if matching.is_empty() {
            voices.to_vec()
        } else {
            matching
        }
    } else {
        voices.to_vec()
    };

    filtered
        .into_iter()
        .map(|(name, speaker_id)| TtsVoiceOption {
            display_name: format_voice_display_name(&name),
            name,
            speaker_id,
        })
        .collect()
}

fn select_default_voice(
    engine: &str,
    language_code: &str,
    voices: &[(String, i64)],
) -> Option<(String, i64)> {
    if voices.is_empty() {
        return None;
    }

    if engine != "kokoro" && engine != "kokoro_mnn" {
        return None;
    }

    for prefix in kokoro_voice_prefixes(language_code) {
        if let Some((name, id)) = voices.iter().find(|(name, _)| name.starts_with(prefix)) {
            return Some((name.clone(), *id));
        }
    }

    Some(voices[0].clone())
}

fn resolve_requested_voice(
    cached_model: &CachedSpeechModel,
    requested_voice_name: Option<&str>,
    speaker_id: Option<i64>,
) -> Option<(String, i64)> {
    if let Some(requested_voice_name) = requested_voice_name.filter(|value| !value.is_empty()) {
        if let Some((name, id)) = cached_model
            .voices
            .iter()
            .find(|(name, _)| name == requested_voice_name)
        {
            return Some((name.clone(), *id));
        }
        log_error(format!(
            "requested voice `{requested_voice_name}` was not found for engine={} language={}; falling back",
            cached_model.paths.engine, cached_model.paths.language_code
        ));
    }

    if let Some(speaker_id) = speaker_id {
        if let Some((name, id)) = cached_model.voices.iter().find(|(_, id)| *id == speaker_id) {
            return Some((name.clone(), *id));
        }
    }

    cached_model
        .default_voice
        .clone()
        .or_else(|| speaker_id.map(|id| (format!("speaker_{id}"), id)))
}

fn clamp_speech_speed(speech_speed: f32) -> f32 {
    speech_speed.clamp(0.5, 2.0)
}

fn support_data_root(snapshot: &CatalogSnapshot, _files: &ResolvedTtsVoiceFiles) -> Option<String> {
    let data_dir = Path::new(&snapshot.base_dir).join("bin");
    data_dir
        .join("espeak-ng-data")
        .is_dir()
        .then(|| data_dir.display().to_string())
}

fn absolute_install_path(snapshot: &CatalogSnapshot, relative_path: &str) -> String {
    Path::new(&snapshot.base_dir)
        .join(relative_path)
        .display()
        .to_string()
}

fn resolve_speech_assets(
    snapshot: &CatalogSnapshot,
    language_code: &LanguageCode,
) -> Option<ResolvedSpeechAssets> {
    resolve_speech_assets_for_pack(snapshot, language_code, None)
}

fn resolve_speech_assets_for_pack(
    snapshot: &CatalogSnapshot,
    language_code: &LanguageCode,
    pack_id: Option<&str>,
) -> Option<ResolvedSpeechAssets> {
    let files = translator_core::catalog::resolve_tts_voice_files_for_pack(
        snapshot,
        language_code,
        pack_id,
    )?;
    let support_data_root = support_data_root(snapshot, &files);
    let model_path = absolute_install_path(snapshot, &files.model_install_path);
    if !Path::new(&model_path).exists() {
        return None;
    }
    let aux_path = absolute_install_path(snapshot, &files.aux_install_path);
    if !Path::new(&aux_path).exists() {
        return None;
    }
    let vocoder = match &files.vocoder_install_path {
        Some(relative) => {
            let path = absolute_install_path(snapshot, relative);
            if !Path::new(&path).exists() {
                return None;
            }
            Some(path)
        }
        None => None,
    };
    Some(ResolvedSpeechAssets {
        paths: SpeechModelPaths {
            engine: files.engine,
            model: model_path,
            aux: aux_path,
            vocoder,
            language_code: files.language_code,
            support_data_root,
        },
        script: snapshot.catalog.script_for(language_code)?,
        speaker_id: files.speaker_id.map(i64::from),
    })
}

fn piper_length_scale_for_speed(speech_speed: f32) -> f32 {
    1.0 / clamp_speech_speed(speech_speed)
}

/// When the TTS voice writes a Latin script, romanize any foreign-script runs
/// in the text so the voice pronounces them instead of skipping them. A voice
/// renders exactly one script, so anything outside it is dropped at
/// tokenization; voices that natively write the source script keep the text
/// intact.
#[cfg(feature = "transliterate")]
fn romanize_foreign_runs_for_voice(text: &str, script: Script) -> String {
    if script != Script::Latin {
        return text.to_owned();
    }
    translator_transliterate::transliterate::transliterate_mixed_to_latin(text)
}

#[cfg(not(feature = "transliterate"))]
fn romanize_foreign_runs_for_voice(text: &str, _script: Script) -> String {
    text.to_owned()
}

fn missing_tts_asset(language_code: &LanguageCode) -> TranslatorError {
    TranslatorError::missing_asset(format!(
        "TTS voice not installed for {}",
        language_code.as_str()
    ))
}

pub fn available_tts_voices_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut SpeechCache,
    language_code: &LanguageCode,
) -> Result<Vec<TtsVoiceOption>, TranslatorError> {
    let assets = resolve_speech_assets(snapshot, language_code)
        .ok_or_else(|| missing_tts_asset(language_code))?;
    list_voices(cache, &assets.paths).map_err(TranslatorError::tts)
}

pub fn warm_tts_model_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut SpeechCache,
    language_code: &LanguageCode,
) -> Result<(), TranslatorError> {
    available_tts_voices_in_snapshot(snapshot, cache, language_code).map(|_| ())
}

pub fn plan_speech_chunks_for_text_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut SpeechCache,
    language_code: &LanguageCode,
    text: &str,
    pack_id: Option<&str>,
    urls_and_hashtags: UrlsAndHashtags,
) -> Result<Vec<SpeechChunk>, TranslatorError> {
    let assets = resolve_speech_assets_for_pack(snapshot, language_code, pack_id)
        .ok_or_else(|| missing_tts_asset(language_code))?;
    plan_speech_chunks_for_text(cache, &assets.paths, assets.script, text, urls_and_hashtags)
        .map_err(TranslatorError::tts)
}

pub fn synthesize_pcm_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut SpeechCache,
    language_code: &LanguageCode,
    text: &str,
    speech_speed: f32,
    voice_name: Option<&VoiceName>,
    is_phonemes: bool,
    pack_id: Option<&str>,
) -> Result<PcmAudio, TranslatorError> {
    let assets = resolve_speech_assets_for_pack(snapshot, language_code, pack_id)
        .ok_or_else(|| missing_tts_asset(language_code))?;
    synthesize_pcm(
        cache,
        &assets.paths,
        assets.script,
        text,
        speech_speed,
        voice_name.map(VoiceName::as_str),
        assets.speaker_id,
        is_phonemes,
    )
    .map_err(TranslatorError::tts)
}

fn japanese_dict_path(support_data_root: &str) -> Result<PathBuf, String> {
    if support_data_root.is_empty() {
        return Err(
            "Japanese speech needs the mucab dictionary, but no support data is installed"
                .to_owned(),
        );
    }

    let candidate = Path::new(support_data_root).join("mucab.bin");
    if !candidate.exists() {
        return Err(format!(
            "Japanese speech needs the mucab dictionary at {}, which is not installed",
            candidate.display()
        ));
    }
    Ok(candidate)
}

fn load_speech_model(paths: &SpeechModelPaths) -> Result<SpeechModel, String> {
    let SpeechModelPaths {
        engine,
        model: model_path,
        aux: aux_path,
        vocoder,
        language_code,
        ..
    } = paths;
    if aux_path.is_empty() {
        return Err(format!("Missing auxiliary path for TTS engine `{engine}`"));
    }

    match engine.as_str() {
        "kokoro_mnn" => {
            let mut model =
                KokoroMnnModel::new(Path::new(model_path), Path::new(aux_path), language_code)
                    .map_err(|err| format!("Failed to load Kokoro MNN voice: {err}"))?;
            if language_code == "ja" {
                let dict_path = japanese_dict_path(paths.support_root())?;
                model
                    .load_japanese_dict(dict_path.to_string_lossy().as_ref())
                    .map_err(|err| format!("Failed to load Japanese dictionary: {err}"))?;
            }
            Ok(SpeechModel::KokoroMnn(model))
        }
        "mms" => MmsModel::new(
            Path::new(model_path),
            Path::new(aux_path),
            language_code,
            &Backend::Cpu,
        )
        .map(SpeechModel::Mms)
        .map_err(|err| format!("Failed to load MMS voice: {err}")),
        "coqui_vits" => CoquiVitsModel::new(
            Path::new(model_path),
            Path::new(aux_path),
            language_code,
            &Backend::Cpu,
        )
        .map(SpeechModel::CoquiVits)
        .map_err(|err| format!("Failed to load Coqui VITS voice: {err}")),
        "cotovia_vits" => {
            CotoviaVitsModel::new(Path::new(model_path), Path::new(aux_path), &Backend::Cpu)
                .map(SpeechModel::CotoviaVits)
                .map_err(|err| format!("Failed to load Cotovia VITS voice: {err}"))
        }
        "glowtts_hifigan" => {
            let vocoder = vocoder
                .as_ref()
                .ok_or_else(|| format!("Missing vocoder path for TTS engine `{engine}`"))?;
            GlowTtsHifiganModel::new(
                Path::new(model_path),
                Path::new(vocoder),
                Path::new(aux_path),
                &Backend::Cpu,
            )
            .map(SpeechModel::GlowTtsHifigan)
            .map_err(|err| format!("Failed to load GlowTTS + HiFiGAN voice: {err}"))
        }
        "sherpa_vits" => {
            SherpaVitsModel::new(Path::new(model_path), Path::new(aux_path), &Backend::Cpu)
                .map(SpeechModel::SherpaVits)
                .map_err(|err| format!("Failed to load Sherpa VITS voice: {err}"))
        }
        "mimic3" => {
            PiperModel::from_mimic3(Path::new(model_path), Path::new(aux_path), &Backend::Cpu)
                .map(SpeechModel::Piper)
                .map_err(|err| format!("Failed to load Mimic3 voice: {err}"))
        }
        "piper" => PiperModel::new(Path::new(model_path), Path::new(aux_path), &Backend::Cpu)
            .map(SpeechModel::Piper)
            .map_err(|err| format!("Failed to load Piper voice: {err}")),
        other => Err(format!("Unsupported TTS engine `{other}`")),
    }
}

fn with_cached_model<T>(
    cache: &mut SpeechCache,
    paths: &SpeechModelPaths,
    f: impl FnOnce(&mut CachedSpeechModel) -> Result<T, String>,
) -> Result<T, String> {
    let load_started_at = Instant::now();
    let engine = paths.engine.as_str();
    let language_code = paths.language_code.as_str();

    let cache_hit = cache
        .model
        .as_ref()
        .map(|cached| &cached.paths == paths)
        .unwrap_or(false);

    if !cache_hit {
        eprintln!(
            "tts.model: cache_miss engine={} language={} model={} aux={} support_root={}",
            engine,
            language_code,
            paths.model,
            paths.aux,
            paths.support_root()
        );
        let model_load_started_at = Instant::now();
        eprintln!(
            "tts.model: load.start engine={} language={}",
            engine, language_code
        );
        let model = load_speech_model(paths)?;
        eprintln!(
            "tts.model: load.done engine={} language={} took_ms={}",
            engine,
            language_code,
            model_load_started_at.elapsed().as_millis()
        );
        let voices = available_voices(&model);
        let default_voice = select_default_voice(engine, language_code, &voices);
        if voices.is_empty() {
            eprintln!(
                "tts.model: voices.loaded engine={} language={} count=0",
                engine, language_code
            );
        } else if let Some((name, id)) = default_voice.as_ref() {
            eprintln!(
                "tts.model: voices.loaded engine={} language={} count={} default={}={}",
                engine,
                language_code,
                voices.len(),
                name,
                id
            );
        } else {
            eprintln!(
                "tts.model: voices.loaded engine={} language={} count={} default=<none>",
                engine,
                language_code,
                voices.len()
            );
        }
        cache.model = Some(CachedSpeechModel {
            paths: paths.clone(),
            voices,
            default_voice,
            model,
        });

        eprintln!(
            "tts.model: ready engine={} language={} total_wait_ms={}",
            engine,
            language_code,
            load_started_at.elapsed().as_millis()
        );
    }

    let cached = cache
        .model
        .as_mut()
        .ok_or_else(|| "TTS model cache was unexpectedly empty".to_owned())?;
    f(cached)
}

fn configure_support_data_root(support_data_root: Option<&str>) {
    let Some(support_data_root) = support_data_root.filter(|path| !path.is_empty()) else {
        return;
    };

    let data_root = Path::new(support_data_root);
    let required = ["phondata", "phonindex", "phontab", "intonations"];
    let direct_layout_ok = required.iter().all(|name| data_root.join(name).exists());
    let nested_layout_ok = required
        .iter()
        .all(|name| data_root.join("espeak-ng-data").join(name).exists());

    log_debug(format!(
        "eSpeak data probe root={support_data_root} direct_layout_ok={direct_layout_ok} nested_layout_ok={nested_layout_ok}"
    ));

    // piper-rs's espeak expects the PARENT directory of `espeak-ng-data/`;
    // if the caller passed us the parent directly, use it, otherwise strip.
    let init_dir = if nested_layout_ok {
        data_root.to_path_buf()
    } else {
        data_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| data_root.to_path_buf())
    };

    match piper_rs::init_espeak(&init_dir) {
        Ok(()) => log_debug(format!(
            "Configured eSpeak data directory at {}",
            init_dir.display()
        )),
        Err(err) => log_error(format!(
            "Failed to initialize eSpeak with data directory `{}`: {err}",
            init_dir.display()
        )),
    }
}

fn phonemize(model: &mut SpeechModel, text: &str) -> Result<String, String> {
    match model {
        SpeechModel::Piper(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::KokoroMnn(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::Mms(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::CoquiVits(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::CotoviaVits(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::SherpaVits(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::GlowTtsHifigan(model) => model
            .phonemize(text)
            .map_err(|err| format!("Speech synthesis failed: {err}")),
    }
}

fn synthesize(
    cached_model: &mut CachedSpeechModel,
    text: &str,
    speech_speed: f32,
    voice_name: Option<&str>,
    speaker_id: Option<i64>,
    is_phonemes: bool,
) -> Result<(Vec<f32>, u32), String> {
    let selected_voice = resolve_requested_voice(cached_model, voice_name, speaker_id);
    let effective_speaker_id = selected_voice.as_ref().map(|(_, id)| *id);
    let clamped_speech_speed = clamp_speech_speed(speech_speed);
    match &mut cached_model.model {
        SpeechModel::Piper(model) => {
            if let Some((name, id)) = selected_voice.as_ref() {
                log_debug(format!(
                    "using voice {name}={id} for engine={} language={} speech_speed={} length_scale={}",
                    cached_model.paths.engine,
                    cached_model.paths.language_code,
                    clamped_speech_speed,
                    piper_length_scale_for_speed(clamped_speech_speed)
                ));
            }
            if is_phonemes {
                model
                    .synthesize_phonemes_with_options(
                        text,
                        effective_speaker_id,
                        Some(piper_length_scale_for_speed(clamped_speech_speed)),
                    )
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize_with_options(
                        text,
                        effective_speaker_id,
                        Some(piper_length_scale_for_speed(clamped_speech_speed)),
                    )
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
        SpeechModel::KokoroMnn(model) => {
            if let Some((name, id)) = selected_voice.as_ref() {
                log_debug(format!(
                    "using voice {name}={id} for engine={} language={} speech_speed={} ({} available)",
                    cached_model.paths.engine,
                    cached_model.paths.language_code,
                    clamped_speech_speed,
                    cached_model.voices.len()
                ));
            }
            if is_phonemes {
                model
                    .synthesize_phonemes(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
        SpeechModel::Mms(model) => {
            if is_phonemes {
                model
                    .synthesize_phonemes(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
        SpeechModel::CoquiVits(model) => {
            if is_phonemes {
                model
                    .synthesize_phonemes(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
        SpeechModel::SherpaVits(model) => {
            if is_phonemes {
                model
                    .synthesize_phonemes(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
        SpeechModel::CotoviaVits(model) => model
            .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
            .map_err(|err| format!("Speech synthesis failed: {err}")),
        SpeechModel::GlowTtsHifigan(model) => {
            if is_phonemes {
                model
                    .synthesize_phonemes(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            } else {
                model
                    .synthesize(text, effective_speaker_id, Some(clamped_speech_speed))
                    .map_err(|err| format!("Speech synthesis failed: {err}"))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn synthesize_pcm(
    cache: &mut SpeechCache,
    paths: &SpeechModelPaths,
    script: Script,
    text: &str,
    speech_speed: f32,
    voice_name: Option<&str>,
    speaker_id: Option<i64>,
    is_phonemes: bool,
) -> Result<PcmAudio, String> {
    if text.trim().is_empty() {
        return Err("Text is empty".to_owned());
    }

    let total_started_at = Instant::now();
    configure_support_data_root(paths.support_data_root.as_deref());

    let romanized = (!is_phonemes).then(|| romanize_foreign_runs_for_voice(text, script));
    let text = romanized.as_deref().unwrap_or(text);

    log_debug(format!(
        "Synthesizing speech with engine={} model={}",
        paths.engine, paths.model
    ));
    let (samples, sample_rate) = with_cached_model(cache, paths, |cached_model| {
        if is_phonemes {
            log_debug(format!(
                "synthesizing direct phoneme chunk with {} phoneme char(s)",
                text.chars().count()
            ));
        } else {
            let phonemize_started_at = Instant::now();
            let phonemes = phonemize(&mut cached_model.model, text)?;
            log_debug(format!(
                "phonemize produced 1 chunk, {} phoneme char(s)",
                phonemes.chars().count(),
            ));
            log_timing("phonemize", phonemize_started_at);
        }

        let synth_started_at = Instant::now();
        let result = synthesize(
            cached_model,
            text,
            speech_speed,
            voice_name,
            speaker_id,
            is_phonemes,
        )?;
        log_timing("infer", synth_started_at);
        Ok(result)
    })?;

    let convert_started_at = Instant::now();
    let pcm_samples: Vec<i16> = samples
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
        .collect();
    let duration_ms = audio_duration_ms(pcm_samples.len(), sample_rate);
    log_debug(format!(
        "convert_to_pcm produced {} sample(s) at {} Hz (~{} ms audio)",
        pcm_samples.len(),
        sample_rate,
        duration_ms,
    ));
    log_timing("convert_to_pcm", convert_started_at);
    log_timing("synthesize_total", total_started_at);

    Ok(PcmAudio {
        sample_rate: sample_rate as i32,
        pcm_samples,
    })
}

fn list_voices(
    cache: &mut SpeechCache,
    paths: &SpeechModelPaths,
) -> Result<Vec<TtsVoiceOption>, String> {
    configure_support_data_root(paths.support_data_root.as_deref());

    with_cached_model(cache, paths, |cached_model| {
        Ok(visible_voices(
            &cached_model.paths.engine,
            &cached_model.paths.language_code,
            &cached_model.voices,
        ))
    })
}

fn phonemize_chunks(
    cache: &mut SpeechCache,
    paths: &SpeechModelPaths,
    text: &str,
) -> Result<Vec<PiperPhonemeChunk>, String> {
    if text.trim().is_empty() {
        return Err("Text is empty".to_owned());
    }

    configure_support_data_root(paths.support_data_root.as_deref());

    log_debug(format!(
        "Phonemizing text with engine={} model={}",
        paths.engine, paths.model
    ));

    with_cached_model(cache, paths, |cached_model| {
        let phonemize_started_at = Instant::now();
        let phonemes = phonemize(&mut cached_model.model, text)?;
        let phoneme_chunks = vec![PiperPhonemeChunk {
            phonemes,
            boundary_after: BoundaryAfter::Paragraph,
        }];
        log_debug(format!(
            "phonemize produced {} chunk(s), {} phoneme char(s), chunk sizes [{}]",
            phoneme_chunks.len(),
            phoneme_chunks[0].phonemes.chars().count(),
            summarize_phoneme_chunk_sizes(&phoneme_chunks),
        ));
        log_timing("phonemize", phonemize_started_at);
        Ok(phoneme_chunks)
    })
}

fn to_phoneme_chunk(chunk: PiperPhonemeChunk) -> PhonemeChunk {
    PhonemeChunk {
        content: chunk.phonemes,
        boundary_after: match chunk.boundary_after {
            BoundaryAfter::None => SpeechChunkBoundary::None,
            BoundaryAfter::Sentence => SpeechChunkBoundary::Sentence,
            BoundaryAfter::Paragraph => SpeechChunkBoundary::Paragraph,
        },
    }
}

fn is_url(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.")
}

fn strip_urls_and_hashtags(text: &str) -> String {
    text.split_whitespace()
        .filter(|word| !is_url(word) && !word.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ")
}

fn plan_speech_chunks_for_text(
    cache: &mut SpeechCache,
    paths: &SpeechModelPaths,
    script: Script,
    text: &str,
    urls_and_hashtags: UrlsAndHashtags,
) -> Result<Vec<SpeechChunk>, String> {
    let filtered = match urls_and_hashtags {
        UrlsAndHashtags::Read => Cow::Borrowed(text),
        UrlsAndHashtags::Skip => Cow::Owned(strip_urls_and_hashtags(text)),
    };
    let text = romanize_foreign_runs_for_voice(&filtered, script);
    plan_speech_chunks(&text, |chunk_text| {
        phonemize_chunks(cache, paths, chunk_text)
            .map(|chunks| chunks.into_iter().map(to_phoneme_chunk).collect::<Vec<_>>())
    })
}

#[cfg(test)]
mod tests {
    use super::strip_urls_and_hashtags;

    #[test]
    fn strips_urls_and_hashtags_keeping_surrounding_words() {
        let input = "Check https://example.com/path now #cool and www.foo.org too";
        assert_eq!(strip_urls_and_hashtags(input), "Check now and too");
    }

    #[test]
    fn keeps_plain_text_untouched() {
        assert_eq!(
            strip_urls_and_hashtags("just plain words"),
            "just plain words"
        );
    }

    #[test]
    fn keeps_hash_only_when_not_leading() {
        assert_eq!(strip_urls_and_hashtags("C# is fine"), "C# is fine");
    }
}
