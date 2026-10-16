use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use piper_rs::{
    Backend, CoquiVitsModel, CotoviaVitsModel, GlowTtsHifiganModel, KokoroMnnModel, MmsModel,
    PiperModel, SherpaVitsModel,
};
use serde::Deserialize;
use translator::tts::{PhonemeChunk, SpeechChunk, SpeechChunkBoundary, plan_speech_chunks};

const DEFAULT_INDEX_PATH: &str = "~/AndroidStudioProjects/bucket/index_v6.json";
const DEFAULT_OUTPUT_DIR: &str = "samples";
const SAMPLE_TEXTS_JSON: &str = include_str!("../../data/sample_texts.json");

static SAMPLE_TEXTS: OnceLock<HashMap<String, String>> = OnceLock::new();

#[derive(Debug)]
struct Args {
    index_path: PathBuf,
    bucket_dir: PathBuf,
    output_dir: PathBuf,
    filter_language: Option<String>,
    filter_engine: Option<String>,
    jobs: usize,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct Catalog {
    packs: HashMap<String, Pack>,
}

#[derive(Debug, Deserialize)]
struct Pack {
    #[serde(default)]
    feature: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    quality: Option<String>,
    #[serde(default, rename = "defaultSpeakerId")]
    default_speaker_id: Option<i64>,
    #[serde(default)]
    files: Vec<PackFile>,
    #[serde(default, rename = "dependsOn")]
    depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PackFile {
    name: String,
    #[serde(default, rename = "installPath")]
    install_path: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Default)]
struct BatchTotals {
    succeeded: usize,
    failed: usize,
}

fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("{err}");
            eprintln!("{}", usage());
            std::process::exit(2);
        }
    };

    if let Err(err) = run(args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), String> {
    let catalog_file = File::open(&args.index_path).map_err(|err| {
        format!(
            "Failed to open catalog `{}`: {err}",
            args.index_path.display()
        )
    })?;
    let catalog: Catalog = serde_json::from_reader(catalog_file).map_err(|err| {
        format!(
            "Failed to parse catalog `{}`: {err}",
            args.index_path.display()
        )
    })?;
    let catalog = Arc::new(catalog);

    configure_espeak_data(&args.bucket_dir)?;
    fs::create_dir_all(&args.output_dir).map_err(|err| {
        format!(
            "Failed to create output directory `{}`: {err}",
            args.output_dir.display()
        )
    })?;

    let mut pack_keys: Vec<String> = catalog
        .packs
        .iter()
        .filter(|(_, pack)| pack.feature.as_deref() == Some("tts"))
        .filter(|(_, pack)| {
            args.filter_language
                .as_deref()
                .is_none_or(|lang| pack.language.as_deref() == Some(lang))
        })
        .filter(|(_, pack)| {
            args.filter_engine
                .as_deref()
                .is_none_or(|engine| pack.engine.as_deref() == Some(engine))
        })
        .map(|(key, _)| key.clone())
        .collect();

    pack_keys.sort_by(|left_key, right_key| {
        let left_pack = catalog.packs.get(left_key).expect("missing pack");
        let right_pack = catalog.packs.get(right_key).expect("missing pack");
        pack_sort_tuple(left_key, left_pack).cmp(&pack_sort_tuple(right_key, right_pack))
    });

    if let Some(limit) = args.limit {
        pack_keys.truncate(limit);
    }

    if pack_keys.is_empty() {
        return Err("No matching TTS packs found.".to_string());
    }

    let totals = process_packs(
        Arc::clone(&catalog),
        args.bucket_dir.clone(),
        args.output_dir.clone(),
        pack_keys,
        args.jobs,
    )?;

    println!(
        "Finished. succeeded={} failed={} output={}",
        totals.succeeded,
        totals.failed,
        args.output_dir.display()
    );

    if totals.failed > 0 {
        return Err(format!("{} pack(s) failed.", totals.failed));
    }

    Ok(())
}

fn parse_args() -> Result<Args, String> {
    let mut index_path = expand_tilde(DEFAULT_INDEX_PATH);
    let mut bucket_dir = None;
    let mut output_dir = PathBuf::from(DEFAULT_OUTPUT_DIR);
    let mut filter_language = None;
    let mut filter_engine = None;
    let mut jobs = 8usize;
    let mut limit = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--index" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --index".to_string())?;
                index_path = expand_tilde(&value);
            }
            "--bucket" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --bucket".to_string())?;
                bucket_dir = Some(expand_tilde(&value));
            }
            "--output" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --output".to_string())?;
                output_dir = expand_tilde(&value);
            }
            "--lang" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --lang".to_string())?;
                filter_language = Some(value);
            }
            "--engine" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --engine".to_string())?;
                filter_engine = Some(value);
            }
            "--jobs" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --jobs".to_string())?;
                jobs = value
                    .parse::<usize>()
                    .map_err(|err| format!("Invalid --jobs value `{value}`: {err}"))?;
                if jobs == 0 {
                    return Err("--jobs must be at least 1".to_string());
                }
            }
            "--limit" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --limit".to_string())?;
                limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|err| format!("Invalid --limit value `{value}`: {err}"))?,
                );
            }
            "--help" | "-h" => return Err("".to_string()),
            _ => return Err(format!("Unknown argument `{arg}`")),
        }
    }

    let bucket_dir = bucket_dir.unwrap_or_else(|| {
        index_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });

    Ok(Args {
        index_path,
        bucket_dir,
        output_dir,
        filter_language,
        filter_engine,
        jobs,
        limit,
    })
}

fn usage() -> &'static str {
    "Usage: cargo run --bin bucket_samples -- [--index PATH] [--bucket DIR] [--output DIR] [--lang CODE] [--engine NAME] [--jobs N] [--limit N]"
}

fn process_packs(
    catalog: Arc<Catalog>,
    bucket_dir: PathBuf,
    output_dir: PathBuf,
    pack_keys: Vec<String>,
    jobs: usize,
) -> Result<BatchTotals, String> {
    let total_packs = pack_keys.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(pack_keys)));
    let totals = Arc::new(Mutex::new(BatchTotals::default()));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let workers = jobs.max(1).min(total_packs.max(1));

    let mut handles = Vec::with_capacity(workers);
    for worker_id in 0..workers {
        let queue = Arc::clone(&queue);
        let totals = Arc::clone(&totals);
        let errors = Arc::clone(&errors);
        let catalog = Arc::clone(&catalog);
        let bucket_dir = bucket_dir.clone();
        let output_dir = output_dir.clone();

        handles.push(thread::spawn(move || -> Result<(), String> {
            loop {
                let Some(pack_key) = ({
                    let mut queue = queue
                        .lock()
                        .map_err(|_| "Worker queue lock poisoned".to_string())?;
                    queue.pop_front()
                }) else {
                    return Ok(());
                };

                let pack = catalog
                    .packs
                    .get(&pack_key)
                    .ok_or_else(|| format!("Pack `{pack_key}` not found in catalog"))?;
                let language = pack.language.as_deref().unwrap_or("unknown");
                let engine = pack.engine.as_deref().unwrap_or("unknown");
                let voice = pack.voice.as_deref().unwrap_or("unknown");
                let quality = pack.quality.as_deref().unwrap_or("unknown");
                println!("[worker {worker_id}] {language} | {engine} | {voice} | {quality}");

                match synthesize_pack_sample(&pack_key, pack, &catalog, &bucket_dir, &output_dir) {
                    Ok(path) => {
                        println!("[worker {worker_id}]   saved {}", path.display());
                        let mut totals = totals
                            .lock()
                            .map_err(|_| "Totals lock poisoned".to_string())?;
                        totals.succeeded += 1;
                    }
                    Err(err) => {
                        eprintln!("[worker {worker_id}]   failed: {err}");
                        {
                            let mut totals = totals
                                .lock()
                                .map_err(|_| "Totals lock poisoned".to_string())?;
                            totals.failed += 1;
                        }
                        let mut errors = errors
                            .lock()
                            .map_err(|_| "Errors lock poisoned".to_string())?;
                        errors.push(format!("{pack_key}: {err}"));
                    }
                }
            }
        }));
    }

    for handle in handles {
        match handle.join() {
            Ok(Ok(())) => {}
            Ok(Err(err)) => return Err(err),
            Err(_) => return Err("A worker thread panicked".to_string()),
        }
    }

    let totals = {
        let mut guard = totals
            .lock()
            .map_err(|_| "Totals lock poisoned".to_string())?;
        std::mem::take(&mut *guard)
    };
    let errors = {
        let mut guard = errors
            .lock()
            .map_err(|_| "Errors lock poisoned".to_string())?;
        std::mem::take(&mut *guard)
    };

    if !errors.is_empty() {
        eprintln!("Failures:");
        for err in errors {
            eprintln!("  {err}");
        }
    }

    Ok(totals)
}

fn configure_espeak_data(bucket_dir: &Path) -> Result<(), String> {
    for candidate in [
        bucket_dir.join("tts/1"),
        bucket_dir.join("bin"),
        bucket_dir.to_path_buf(),
        ".".into(),
    ] {
        let espeak_dir = candidate.join("espeak-ng-data");
        if has_complete_espeak_data(&espeak_dir) {
            piper_rs::init_espeak(&candidate)
                .map_err(|err| format!("Failed to initialize eSpeak: {err}"))?;
            println!("Using eSpeak data from {}", espeak_dir.display());
            return Ok(());
        }
    }

    Err(format!(
        "Could not find a pre-extracted `espeak-ng-data/` under `{}`. \
        If you only have `espeak-ng-data.zip`, unzip it next to itself first.",
        bucket_dir.display()
    ))
}

fn has_complete_espeak_data(espeak_dir: &Path) -> bool {
    espeak_dir.join("phontab").is_file()
        && espeak_dir.join("phondata").is_file()
        && espeak_dir.join("intonations").exists()
}

fn synthesize_pack_sample(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    output_dir: &Path,
) -> Result<PathBuf, String> {
    let engine = pack
        .engine
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `engine`"))?;
    let language = pack
        .language
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `language`"))?;
    let voice = pack
        .voice
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `voice`"))?;
    let quality = pack
        .quality
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `quality`"))?;
    let text = sample_text_for_language(language)
        .ok_or_else(|| format!("No translated sample text for language `{language}`"))?;

    let output_path = output_dir.join(language).join(format!(
        "{}_{}.wav",
        sanitize_filename(voice),
        sanitize_filename(quality)
    ));
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create output directory `{}`: {err}",
                parent.display()
            )
        })?;
    }

    let (samples, sample_rate) = match engine {
        "piper" => synthesize_piper(pack_key, pack, catalog, bucket_dir, text)?,
        "mimic3" => synthesize_mimic3(pack_key, pack, catalog, bucket_dir, text)?,
        "mms" => synthesize_mms(pack_key, pack, catalog, bucket_dir, text)?,
        "coqui_vits" => synthesize_coqui(pack_key, pack, catalog, bucket_dir, text)?,
        "cotovia_vits" => synthesize_cotovia(pack_key, catalog, bucket_dir, text)?,
        "glowtts_hifigan" => synthesize_glowtts_hifigan(pack_key, catalog, bucket_dir, text)?,
        "sherpa_vits" => synthesize_sherpa(pack_key, catalog, bucket_dir, text)?,
        "kokoro_mnn" => synthesize_kokoro_mnn(pack_key, pack, catalog, bucket_dir, text)?,
        "kokoro" => {
            return Err(format!(
                "`{pack_key}` uses the legacy ort kokoro engine (superseded by kokoro_mnn); skipped"
            ));
        }
        other => return Err(format!("Unsupported engine `{other}` for `{pack_key}`")),
    };

    let samples_i16: Vec<i16> = samples
        .iter()
        .map(|&sample| (sample * i16::MAX as f32) as i16)
        .collect();

    let mut output_file = File::create(&output_path).map_err(|err| {
        format!(
            "Failed to create output file `{}`: {err}",
            output_path.display()
        )
    })?;
    write_wav(&mut output_file, &samples_i16, sample_rate, 1)?;
    Ok(output_path)
}

fn plan_for_text<P>(
    engine: &str,
    pack_key: &str,
    text: &str,
    mut phonemize_fn: P,
) -> Result<Vec<SpeechChunk>, String>
where
    P: FnMut(&str) -> Result<String, String>,
{
    plan_speech_chunks(text, |chunk_text| {
        let phonemes = phonemize_fn(chunk_text)?;
        Ok(vec![PhonemeChunk {
            content: phonemes,
            boundary_after: SpeechChunkBoundary::Paragraph,
        }])
    })
    .map_err(|err| format!("Failed to plan speech chunks for {engine} `{pack_key}`: {err}"))
}

fn synthesize_plan<S>(
    plan: Vec<SpeechChunk>,
    mut synthesize_fn: S,
) -> Result<(Vec<f32>, u32), String>
where
    S: FnMut(&str, bool) -> Result<(Vec<f32>, u32), String>,
{
    let mut combined: Vec<f32> = Vec::new();
    let mut sample_rate: u32 = 0;
    for chunk in plan {
        let (samples, sr) = synthesize_fn(&chunk.content, chunk.is_phonemes)?;
        if sample_rate == 0 {
            sample_rate = sr;
        }
        combined.extend(samples);
        if let Some(pause_ms) = chunk.pause_after_ms {
            let pause_samples = (sr as u64 * pause_ms.max(0) as u64 / 1000) as usize;
            combined.resize(combined.len() + pause_samples, 0.0);
        }
    }
    Ok((combined, sample_rate))
}

fn synthesize_piper(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".mnn")
    })?;
    let config_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".onnx.json")
    })?;

    let mut model = PiperModel::new(&model_path, &config_path, &Backend::Cpu)
        .map_err(|err| format!("Failed to load Piper model `{pack_key}`: {err}"))?;
    let speaker_id = model
        .voices()
        .map(|voices| resolve_speaker_id(pack_key, pack, voices))
        .transpose()?
        .flatten();

    let plan = plan_for_text("Piper", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize Piper voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, speaker_id)
        } else {
            model.synthesize(content, speaker_id)
        }
        .map_err(|err| format!("Failed to synthesize Piper voice `{pack_key}`: {err}"))
    })
}

fn synthesize_cotovia(
    pack_key: &str,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".mnn")
    })?;
    // the shared cotovia lexicon lives in a dependency pack
    let lexicon_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with("lexicon.txt.zst")
    })?;
    let mut model = CotoviaVitsModel::new(&model_path, &lexicon_path, &Backend::Cpu)
        .map_err(|err| format!("Failed to load Cotovia VITS model `{pack_key}`: {err}"))?;
    // cotovia_vits tokenizes internally; synthesize the whole sample text.
    model
        .synthesize(text, None, None)
        .map_err(|err| format!("Failed to synthesize Cotovia VITS voice `{pack_key}`: {err}"))
}

fn synthesize_glowtts_hifigan(
    pack_key: &str,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with("glowtts.mnn")
    })?;
    let vocoder_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with("hifigan.mnn")
    })?;
    let lexicon_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with("lexicon.bin")
    })?;
    let mut model =
        GlowTtsHifiganModel::new(&model_path, &vocoder_path, &lexicon_path, &Backend::Cpu)
            .map_err(|err| format!("Failed to load GlowTTS + HiFiGAN model `{pack_key}`: {err}"))?;
    model
        .synthesize(text, None, None)
        .map_err(|err| format!("Failed to synthesize GlowTTS + HiFiGAN voice `{pack_key}`: {err}"))
}

fn synthesize_mimic3(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".mnn")
    })?;
    let config_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".onnx.json")
    })?;

    let mut model = PiperModel::from_mimic3(&model_path, &config_path, &Backend::Cpu)
        .map_err(|err| format!("Failed to load Mimic3 model `{pack_key}`: {err}"))?;
    let speaker_id = model
        .voices()
        .map(|voices| resolve_speaker_id(pack_key, pack, voices))
        .transpose()?
        .flatten();

    let plan = plan_for_text("Mimic3", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize Mimic3 voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, speaker_id)
        } else {
            model.synthesize(content, speaker_id)
        }
        .map_err(|err| format!("Failed to synthesize Mimic3 voice `{pack_key}`: {err}"))
    })
}

fn synthesize_mms(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let language = pack
        .language
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `language`"))?;
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "model.mnn"
    })?;
    let tokens_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "tokens.txt"
    })?;

    let mut model = MmsModel::new(&model_path, &tokens_path, language, &Backend::Cpu)
        .map_err(|err| format!("Failed to load MMS model `{pack_key}`: {err}"))?;

    let plan = plan_for_text("MMS", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize MMS voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, None, None)
        } else {
            model.synthesize(content, None, None)
        }
        .map_err(|err| format!("Failed to synthesize MMS voice `{pack_key}`: {err}"))
    })
}

fn synthesize_coqui(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".mnn")
    })?;
    let config_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "config.json"
    })?;
    let language = pack
        .language
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `language`"))?;

    let mut model = CoquiVitsModel::new(&model_path, &config_path, language, &Backend::Cpu)
        .map_err(|err| format!("Failed to load Coqui VITS model `{pack_key}`: {err}"))?;
    let speaker_id = model
        .voices()
        .map(|voices| resolve_speaker_id(pack_key, pack, voices))
        .transpose()?
        .flatten();

    let plan = plan_for_text("Coqui VITS", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize Coqui VITS voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, speaker_id, None)
        } else {
            model.synthesize(content, speaker_id, None)
        }
        .map_err(|err| format!("Failed to synthesize Coqui VITS voice `{pack_key}`: {err}"))
    })
}

fn synthesize_sherpa(
    pack_key: &str,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name.ends_with(".mnn")
    })?;
    let config_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "config.json"
    })?;

    let mut model = SherpaVitsModel::new(&model_path, &config_path, &Backend::Cpu)
        .map_err(|err| format!("Failed to load Sherpa VITS model `{pack_key}`: {err}"))?;

    let plan = plan_for_text("Sherpa VITS", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize Sherpa VITS voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, None, None)
        } else {
            model.synthesize(content, None, None)
        }
        .map_err(|err| format!("Failed to synthesize Sherpa VITS voice `{pack_key}`: {err}"))
    })
}

fn synthesize_kokoro_mnn(
    pack_key: &str,
    pack: &Pack,
    catalog: &Catalog,
    bucket_dir: &Path,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let model_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "kokoro.mnn"
    })?;
    let voices_path = find_file_path(pack_key, catalog, bucket_dir, |file| {
        file.name == "voices-v1.0.bin"
    })?;
    let language = pack
        .language
        .as_deref()
        .ok_or_else(|| format!("Pack `{pack_key}` is missing `language`"))?;

    let mut model = KokoroMnnModel::new(&model_path, &voices_path, language)
        .map_err(|err| format!("Failed to load Kokoro model `{pack_key}`: {err}"))?;

    load_kokoro_language_support(&mut model, pack_key, language, catalog, bucket_dir)?;

    let speaker_id = model
        .voices()
        .map(|voices| resolve_speaker_id(pack_key, pack, voices))
        .transpose()?
        .flatten();

    let plan = plan_for_text("Kokoro", pack_key, text, |t| {
        model
            .phonemize(t)
            .map_err(|err| format!("Failed to phonemize Kokoro voice `{pack_key}`: {err}"))
    })?;

    synthesize_plan(plan, |content, is_phonemes| {
        if is_phonemes {
            model.synthesize_phonemes(content, speaker_id, None)
        } else {
            model.synthesize(content, speaker_id, None)
        }
        .map_err(|err| format!("Failed to synthesize Kokoro voice `{pack_key}`: {err}"))
    })
}

fn load_kokoro_language_support(
    model: &mut KokoroMnnModel,
    pack_key: &str,
    language: &str,
    catalog: &Catalog,
    bucket_dir: &Path,
) -> Result<(), String> {
    if language == "ja" {
        let mucab_path = find_language_support_file(catalog, bucket_dir, "ja", "mucab", "mucab.bin")
            .ok_or_else(|| format!("Japanese Kokoro voice `{pack_key}` requires `mucab.bin`, but no `support`/`mucab` pack was found in the index"))?;
        let mucab_str = mucab_path.to_str().ok_or_else(|| {
            format!(
                "Japanese dictionary path is not valid UTF-8: `{}`",
                mucab_path.display()
            )
        })?;
        model.load_japanese_dict(mucab_str).map_err(|err| {
            format!(
                "Failed to load Japanese dictionary for `{pack_key}` from `{}`: {err}",
                mucab_path.display()
            )
        })?;
        println!(
            "Loaded Japanese dictionary for {} from {}",
            pack_key,
            mucab_path.display()
        );
    }

    Ok(())
}

fn find_language_support_file(
    catalog: &Catalog,
    bucket_dir: &Path,
    language: &str,
    kind: &str,
    file_name: &str,
) -> Option<PathBuf> {
    catalog.packs.iter().find_map(|(_, pack)| {
        (pack.feature.as_deref() == Some("support")
            && pack.language.as_deref() == Some(language)
            && pack.kind.as_deref() == Some(kind))
        .then(|| {
            pack.files
                .iter()
                .find(|file| file.name == file_name)
                .and_then(|file| resolve_local_file_path(bucket_dir, file))
        })
        .flatten()
    })
}

fn resolve_speaker_id(
    pack_key: &str,
    pack: &Pack,
    voices: &HashMap<String, i64>,
) -> Result<Option<i64>, String> {
    if voices.is_empty() {
        return Ok(None);
    }

    if let Some(requested_voice_name) = pack.voice.as_deref() {
        if let Some(id) = lookup_voice_id(voices, requested_voice_name) {
            println!(
                "Resolved speaker for {}: {} -> {}",
                pack_key, requested_voice_name, id
            );
            return Ok(Some(id));
        }
    }

    if let Some(default_speaker_id) = pack.default_speaker_id {
        println!(
            "Resolved speaker for {} via defaultSpeakerId -> {}",
            pack_key, default_speaker_id
        );
        return Ok(Some(default_speaker_id));
    }

    if let Some(id) = voices.get("neutral").copied() {
        println!("Resolved speaker for {} via `neutral` -> {}", pack_key, id);
        return Ok(Some(id));
    }

    if let Some(id) = voices.get("default").copied() {
        println!("Resolved speaker for {} via `default` -> {}", pack_key, id);
        return Ok(Some(id));
    }

    if voices.len() == 1 {
        let id = voices.values().next().copied();
        if let Some(id) = id {
            println!(
                "Resolved speaker for {} via single speaker -> {}",
                pack_key, id
            );
        }
        return Ok(id);
    }

    let requested = pack.voice.as_deref().unwrap_or("<none>");
    let mut available: Vec<&str> = voices.keys().map(String::as_str).collect();
    available.sort_unstable();
    let preview = available
        .into_iter()
        .take(10)
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "Could not resolve named speaker `{}` for `{}`. Available speakers include: {}",
        requested, pack_key, preview
    ))
}

fn lookup_voice_id(voices: &HashMap<String, i64>, requested_voice_name: &str) -> Option<i64> {
    voices.get(requested_voice_name).copied().or_else(|| {
        let requested = normalize_voice_name(requested_voice_name);
        voices
            .iter()
            .find_map(|(name, id)| (normalize_voice_name(name) == requested).then_some(*id))
    })
}

fn normalize_voice_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn find_file_path<F>(
    pack_key: &str,
    catalog: &Catalog,
    bucket_dir: &Path,
    matcher: F,
) -> Result<PathBuf, String>
where
    F: Fn(&PackFile) -> bool + Copy,
{
    let mut stack = vec![pack_key.to_string()];
    let mut visited = HashSet::new();

    while let Some(current_key) = stack.pop() {
        if !visited.insert(current_key.clone()) {
            continue;
        }

        let pack = catalog
            .packs
            .get(&current_key)
            .ok_or_else(|| format!("Pack `{current_key}` not found in catalog"))?;

        for file in &pack.files {
            if matcher(file) {
                return resolve_local_file_path(bucket_dir, file).ok_or_else(|| {
                    format!(
                        "Matched file `{}` for `{current_key}`, but could not resolve a local path",
                        file.name
                    )
                });
            }
        }

        stack.extend(pack.depends_on.iter().cloned());
    }

    Err(format!("No matching file found for `{pack_key}`"))
}

fn resolve_local_file_path(bucket_dir: &Path, file: &PackFile) -> Option<PathBuf> {
    if let Some(install_path) = &file.install_path {
        let candidate = bucket_dir.join(install_path);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    if let Some(url) = &file.url {
        let relative = relative_path_from_url(url)?;
        let candidate = bucket_dir.join(relative);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn relative_path_from_url(url: &str) -> Option<&str> {
    if let Some(rest) = url.strip_prefix("https://") {
        return rest.split_once('/').map(|(_, path)| path);
    }
    if let Some(rest) = url.strip_prefix("http://") {
        return rest.split_once('/').map(|(_, path)| path);
    }
    url.strip_prefix('/')
}

fn pack_sort_tuple<'a>(pack_key: &'a str, pack: &'a Pack) -> (&'a str, &'a str, &'a str, &'a str) {
    (
        pack.language.as_deref().unwrap_or(""),
        pack.voice.as_deref().unwrap_or(""),
        pack.quality.as_deref().unwrap_or(""),
        pack_key,
    )
}

fn sanitize_filename(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut last_was_sep = false;

    for ch in value.chars() {
        let keep = ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.');
        if keep {
            output.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            output.push('_');
            last_was_sep = true;
        }
    }

    output.trim_matches('_').to_string()
}

fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(input));
    }

    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }

    PathBuf::from(input)
}

fn write_wav(
    writer: &mut impl Write,
    samples: &[i16],
    sample_rate: u32,
    channels: u16,
) -> Result<(), String> {
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * channels as u32 * 2;
    writer
        .write_all(b"RIFF")
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&(36 + data_len).to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(b"WAVEfmt ")
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&16u32.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&1u16.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&channels.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&sample_rate.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&byte_rate.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&(channels * 2).to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&16u16.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(b"data")
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;
    writer
        .write_all(&data_len.to_le_bytes())
        .map_err(|err| format!("Failed to write WAV header: {err}"))?;

    for sample in samples {
        writer
            .write_all(&sample.to_le_bytes())
            .map_err(|err| format!("Failed to write WAV samples: {err}"))?;
    }

    Ok(())
}

fn sample_text_for_language(language: &str) -> Option<&'static str> {
    sample_texts().get(language).map(String::as_str)
}

fn sample_texts() -> &'static HashMap<String, String> {
    SAMPLE_TEXTS.get_or_init(|| {
        serde_json::from_str(SAMPLE_TEXTS_JSON).expect("sample_texts.json must be valid")
    })
}
