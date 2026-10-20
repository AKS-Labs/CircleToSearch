use std::fs;
use std::path::PathBuf;

use translator::TranslatorSession;
use translator::catalog::{MigrationAction, parse_and_validate_catalog};

fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("translator-migration-{tag}-{nanos}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn catalog_json(migrations: &str) -> String {
    format!(
        r#"{{
          "formatVersion":6,"generatedAt":1,"dictionaryVersion":1,
          "sources":{{"languageIndexVersion":1,"languageIndexUpdatedAt":1,
                      "dictionaryIndexVersion":1,"dictionaryIndexUpdatedAt":1}},
          "languages":{{}},"packs":{{}},
          "migrations":{migrations}
        }}"#
    )
}

#[test]
fn plan_filters_present_files_and_discard_removes_only_onnx() {
    let dir = unique_dir("discard");
    fs::write(dir.join("a.onnx"), b"x").unwrap();
    fs::write(dir.join("b.onnx"), b"x").unwrap();
    fs::write(dir.join("b.mnn"), b"x").unwrap();
    // c.onnx intentionally absent

    let json = catalog_json(
        r#"[
          {"onnx":"a.onnx","mnn":"a.mnn","quantBits":8,"onnxBytes":1,"mnnBytes":1,"feature":"tts"},
          {"onnx":"b.onnx","mnn":"b.mnn","quantBits":8,"onnxBytes":1,"mnnBytes":1,"feature":"tts"},
          {"onnx":"c.onnx","mnn":"c.mnn","quantBits":8,"onnxBytes":1,"mnnBytes":1,"feature":"tts"}
        ]"#,
    );
    let catalog = parse_and_validate_catalog(&json).unwrap();
    let session = TranslatorSession::from_catalog(catalog, dir.to_string_lossy().into_owned());

    let jobs = session.plan_migration();
    assert_eq!(jobs.len(), 2);
    assert_eq!(
        jobs.iter()
            .find(|job| job.entry.onnx == "a.onnx")
            .unwrap()
            .action,
        MigrationAction::Convert
    );
    assert_eq!(
        jobs.iter()
            .find(|job| job.entry.onnx == "b.onnx")
            .unwrap()
            .action,
        MigrationAction::CleanupOnly
    );

    session.discard_migration(&jobs);
    assert!(!dir.join("a.onnx").exists());
    assert!(!dir.join("b.onnx").exists());
    assert!(dir.join("b.mnn").exists(), "mnn must be left untouched");
    assert!(
        session.plan_migration().is_empty(),
        "nothing left to migrate once sources are gone"
    );

    fs::remove_dir_all(&dir).ok();
}
