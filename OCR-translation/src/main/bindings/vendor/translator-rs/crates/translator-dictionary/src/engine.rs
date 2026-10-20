use std::sync::Mutex;

use translator_core::api::{LanguageCode, TranslatorError};
use translator_core::catalog::CatalogSnapshot;

use crate::tarkka::{
    DictionaryCache, WordWithTaggedEntries, close_dictionary_in_snapshot,
    lookup_dictionary_in_snapshot,
};

pub struct DictionaryEngine {
    cache: Mutex<DictionaryCache>,
}

impl Default for DictionaryEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DictionaryEngine {
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(DictionaryCache::new()),
        }
    }

    pub fn lookup(
        &self,
        snap: &CatalogSnapshot,
        language: &LanguageCode,
        word: &str,
    ) -> Result<Option<WordWithTaggedEntries>, TranslatorError> {
        let mut cache = self.cache.lock().expect("dictionary cache poisoned");
        lookup_dictionary_in_snapshot(snap, &mut cache, language, word)
    }

    pub fn close(&self, snap: &CatalogSnapshot, language: &LanguageCode) {
        let mut cache = self.cache.lock().expect("dictionary cache poisoned");
        close_dictionary_in_snapshot(snap, &mut cache, language);
    }
}
