use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use translator_core::api::{DictionaryCode, LanguageCode, TranslatorError};
use translator_core::catalog::CatalogSnapshot;

pub use tarkka::WordWithTaggedEntries;
use tarkka::reader::DictionaryReader;

pub struct DictionaryCache {
    readers: HashMap<String, DictionaryReader<'static, File>>,
}

impl DictionaryCache {
    pub fn new() -> Self {
        Self {
            readers: HashMap::new(),
        }
    }

    pub fn close(&mut self, path: &str) {
        self.readers.remove(path);
    }

    fn lookup(&mut self, path: &str, word: &str) -> Result<Option<WordWithTaggedEntries>, String> {
        if !self.readers.contains_key(path) {
            let file = File::open(path)
                .map_err(|err| format!("failed to open dictionary file {path}: {err}"))?;
            let reader = DictionaryReader::open(file)
                .map_err(|err| format!("failed to open dictionary reader {path}: {err}"))?;
            eprintln!(
                "Dict version {}, timestamp {:?}",
                reader.version(),
                reader.created_at(),
            );
            self.readers.insert(path.to_string(), reader);
        }
        let reader = self
            .readers
            .get_mut(path)
            .ok_or_else(|| "dictionary reader missing after initialization".to_string())?;
        reader
            .lookup(word)
            .map_err(|err| format!("dictionary lookup failed: {err}"))
    }
}

impl Default for DictionaryCache {
    fn default() -> Self {
        Self::new()
    }
}

fn dictionary_path(base_dir: &str, dictionary_code: &str) -> Option<String> {
    let path = Path::new(base_dir)
        .join("dictionaries")
        .join(format!("{dictionary_code}.dict"));
    path.exists().then(|| path.to_string_lossy().into_owned())
}

pub fn lookup_dictionary_for_code(
    base_dir: &str,
    cache: &mut DictionaryCache,
    dictionary_code: &DictionaryCode,
    word: &str,
) -> Result<Option<WordWithTaggedEntries>, TranslatorError> {
    let normalized = word.trim();
    if normalized.is_empty() {
        return Ok(None);
    }

    let path = dictionary_path(base_dir, dictionary_code.as_str()).ok_or_else(|| {
        TranslatorError::missing_asset(format!(
            "dictionary not installed: {}",
            dictionary_code.as_str()
        ))
    })?;

    let lowered = normalized.to_lowercase();
    match cache.lookup(&path, normalized) {
        Ok(Some(word_data)) => Ok(Some(word_data)),
        Ok(None) if lowered != normalized => cache
            .lookup(&path, &lowered)
            .map_err(TranslatorError::dictionary),
        Ok(None) => Ok(None),
        Err(err) => Err(TranslatorError::dictionary(err)),
    }
}

fn dictionary_path_for_language(
    snapshot: &CatalogSnapshot,
    language_code: &LanguageCode,
) -> Option<String> {
    let language = snapshot.catalog.language_by_code(language_code)?;
    dictionary_path(&snapshot.base_dir, &language.dictionary_code)
}

pub fn lookup_dictionary_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut DictionaryCache,
    language_code: &LanguageCode,
    word: &str,
) -> Result<Option<WordWithTaggedEntries>, TranslatorError> {
    let language = snapshot
        .catalog
        .language_by_code(language_code)
        .ok_or_else(|| {
            TranslatorError::dictionary(format!(
                "unknown dictionary language: {}",
                language_code.as_str()
            ))
        })?;
    lookup_dictionary_for_code(
        &snapshot.base_dir,
        cache,
        &DictionaryCode::from(language.dictionary_code.clone()),
        word,
    )
}

pub fn close_dictionary_in_snapshot(
    snapshot: &CatalogSnapshot,
    cache: &mut DictionaryCache,
    language_code: &LanguageCode,
) {
    if let Some(path) = dictionary_path_for_language(snapshot, language_code) {
        cache.close(&path);
    }
}
