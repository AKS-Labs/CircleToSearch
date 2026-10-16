use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;
use translator::bergamot::{BergamotEngine, ModelPaths};

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(flatten)]
    first: Hop,
    #[serde(default)]
    pivot: Option<Hop>,
}

#[derive(Debug, Deserialize)]
struct Hop {
    model: PathBuf,
    vocabulary: PathBuf,
    #[serde(default)]
    shortlist: Option<PathBuf>,
    #[serde(default)]
    target_vocabulary: Option<PathBuf>,
}

#[derive(Debug)]
struct Args {
    config: PathBuf,
    text: Option<String>,
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            if !err.is_empty() {
                eprintln!("{err}");
            }
            eprintln!("{}", usage());
            return ExitCode::from(2);
        }
    };

    if let Err(err) = run(args) {
        eprintln!("error: {err}");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn run(args: Args) -> Result<(), String> {
    let config_text = fs::read_to_string(&args.config)
        .map_err(|err| format!("reading {}: {err}", args.config.display()))?;
    let config: Config = toml::from_str(&config_text)
        .map_err(|err| format!("parsing {}: {err}", args.config.display()))?;

    let base = args
        .config
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let first = resolve_paths(&config.first, &base);
    let pivot = config.pivot.as_ref().map(|hop| resolve_paths(hop, &base));

    let input = match args.text {
        Some(text) => text,
        None => {
            let mut buf = String::new();
            io::stdin()
                .read_to_string(&mut buf)
                .map_err(|err| format!("reading stdin: {err}"))?;
            buf
        }
    };

    let translated = translate(&first, pivot.as_ref(), &input)?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(translated.as_bytes())
        .map_err(|err| format!("writing stdout: {err}"))?;
    if !translated.ends_with('\n') {
        stdout
            .write_all(b"\n")
            .map_err(|err| format!("writing stdout: {err}"))?;
    }
    Ok(())
}

fn translate(
    first: &ModelPaths,
    pivot: Option<&ModelPaths>,
    input: &str,
) -> Result<String, String> {
    if input.trim().is_empty() {
        return Ok(input.to_string());
    }

    let mut engine = BergamotEngine::new();
    engine.load_model_into_cache(first, "first")?;
    if let Some(p) = pivot {
        engine.load_model_into_cache(p, "pivot")?;
    }

    let lines: Vec<String> = input.split('\n').map(str::to_owned).collect();
    let non_empty: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| (!line.trim().is_empty()).then_some(i))
        .collect();
    if non_empty.is_empty() {
        return Ok(input.to_string());
    }
    let to_translate: Vec<String> = non_empty.iter().map(|&i| lines[i].clone()).collect();

    let translated = match pivot {
        Some(_) => engine.pivot_multiple("first", "pivot", &to_translate)?,
        None => engine.translate_multiple(&to_translate, "first")?,
    };

    let mut merged = lines;
    for (i, value) in non_empty.into_iter().zip(translated.into_iter()) {
        merged[i] = value;
    }
    Ok(merged.join("\n"))
}

fn resolve_paths(hop: &Hop, base: &Path) -> ModelPaths {
    ModelPaths {
        model: resolve_relative(&hop.model, base),
        vocabulary: resolve_relative(&hop.vocabulary, base),
        shortlist: hop.shortlist.as_ref().map(|p| resolve_relative(p, base)),
        target_vocabulary: hop
            .target_vocabulary
            .as_ref()
            .map(|p| resolve_relative(p, base)),
    }
}

fn resolve_relative(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn parse_args() -> Result<Args, String> {
    let mut config: Option<PathBuf> = None;
    let mut text: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value for --config".to_string())?;
                config = Some(PathBuf::from(value));
            }
            "--text" => {
                text = Some(
                    args.next()
                        .ok_or_else(|| "Missing value for --text".to_string())?,
                );
            }
            "--help" | "-h" => return Err(String::new()),
            other => return Err(format!("Unknown argument `{other}`")),
        }
    }

    let config = config.ok_or_else(|| "Missing required --config".to_string())?;
    Ok(Args { config, text })
}

fn usage() -> &'static str {
    "Usage: translate --config CONFIG.toml [--text TEXT]\n\
     Config has a top-level direct hop and an optional [pivot] table for the second hop.\n\
     If --text is omitted, input is read from stdin."
}
