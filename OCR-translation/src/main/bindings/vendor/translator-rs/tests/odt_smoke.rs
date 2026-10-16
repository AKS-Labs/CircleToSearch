//! Smoke test for English → Spanish ODT translation.
//!
//! Reads every `.odt` fixture in `files/odt` and writes translated packages to
//! `smoke-out/odt/<input file name>`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use translator::odt::translate_odt;
use translator::{FsPackInstallChecker, LanguageCode, TranslatorSession};

#[test]
fn smoke_translate_fixture_odts_en_to_es() {
    let input_dir = Path::new("files/odt");
    let output_dir = Path::new("smoke-out/odt");
    let bucket = env::var("ODT_SMOKE_BUCKET_DIR")
        .or_else(|_| env::var("PDF_SMOKE_BUCKET_DIR"))
        .unwrap_or_else(|_| ".test-assets/install".to_string());
    let source_lang = env::var("ODT_SMOKE_SOURCE_LANG").unwrap_or_else(|_| "en".to_string());
    let target_lang = env::var("ODT_SMOKE_TARGET_LANG").unwrap_or_else(|_| "es".to_string());

    let mut inputs = fs::read_dir(input_dir)
        .expect("read files/odt")
        .map(|entry| entry.expect("read files/odt entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "odt"))
        .collect::<Vec<_>>();
    inputs.sort();

    assert_eq!(inputs.len(), 4, "expected exactly 4 ODT fixtures");
    fs::create_dir_all(output_dir).expect("create smoke-out/odt");

    let bucket_path = PathBuf::from(&bucket);
    let catalog_path = bucket_path.join("index.json");
    let bundled_json = fs::read_to_string(&catalog_path).unwrap_or_else(|err| {
        panic!("read catalog {}: {err}", catalog_path.display());
    });
    let checker = FsPackInstallChecker::new(&bucket);
    let session = TranslatorSession::open(&bundled_json, None, bucket.clone(), &checker)
        .expect("open TranslatorSession");
    let available_langs: Vec<LanguageCode> = session
        .language_overview()
        .into_iter()
        .map(|row| LanguageCode::new(row.language.code))
        .collect();

    for input in inputs {
        let bytes = fs::read(&input).unwrap_or_else(|err| {
            panic!("read {}: {err}", input.display());
        });
        let translated = translate_odt(
            &session,
            &bytes,
            Some(&source_lang),
            &target_lang,
            &available_langs,
        )
        .unwrap_or_else(|err| {
            panic!("translate {}: {err}", input.display());
        });
        assert!(
            !translated.is_empty(),
            "{} translated to empty output",
            input.display()
        );

        let out_path = output_dir.join(file_name(&input));
        fs::write(&out_path, translated).unwrap_or_else(|err| {
            panic!("write {}: {err}", out_path.display());
        });
        eprintln!("[odt_smoke] wrote {}", out_path.display());
    }
}

fn file_name(path: &Path) -> PathBuf {
    path.file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing file name for {}", path.display()))
}
