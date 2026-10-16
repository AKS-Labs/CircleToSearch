use std::hash::{Hash, Hasher};

#[derive(Debug, Clone)]
pub struct ModelFile {
    pub name: String,
    pub size_bytes: u64,
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct LanguageDirection {
    pub model: ModelFile,
    pub src_vocab: ModelFile,
    pub tgt_vocab: ModelFile,
    pub lex: ModelFile,
}

impl LanguageDirection {
    pub fn all_files(&self) -> Vec<&ModelFile> {
        let mut files: Vec<&ModelFile> = Vec::with_capacity(4);
        for file in [&self.model, &self.src_vocab, &self.tgt_vocab, &self.lex] {
            if files.iter().all(|existing| existing.name != file.name) {
                files.push(file);
            }
        }
        files
    }

    pub fn total_size(&self) -> u64 {
        self.all_files()
            .into_iter()
            .map(|file| file.size_bytes)
            .sum()
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Language {
    pub code: String,
    pub display_name: String,
    pub short_display_name: String,
    pub script: crate::script::Script,
    /// The subtag the catalog declared, kept alongside the inventory `script`
    /// collapses it to: romanization is keyed on the writing system, not on the
    /// glyphs it is drawn with.
    pub writing_system: crate::script::WritingSystem,
    pub dictionary_code: String,
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl Eq for Language {}

impl Hash for Language {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.code.hash(state);
    }
}

impl Language {
    pub fn is_english(&self) -> bool {
        self.code == "en"
    }

    pub fn scripted(&self) -> crate::api::ScriptedLanguage {
        crate::api::ScriptedLanguage {
            code: crate::api::LanguageCode::from(self.code.as_str()),
            script: self.script,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, LanguageDirection, ModelFile};

    fn file(name: &str, size_bytes: u64) -> ModelFile {
        ModelFile {
            name: name.to_string(),
            size_bytes,
            path: format!("bin/{name}"),
        }
    }

    #[test]
    fn deduplicates_language_direction_files_by_name() {
        let direction = LanguageDirection {
            model: file("model.bin", 10),
            src_vocab: file("vocab.spm", 20),
            tgt_vocab: file("vocab.spm", 20),
            lex: file("lex.bin", 30),
        };

        assert_eq!(direction.all_files().len(), 3);
        assert_eq!(direction.total_size(), 60);
    }

    #[test]
    fn exposes_language_helpers() {
        let language = Language {
            code: "en".to_string(),
            display_name: "English".to_string(),
            short_display_name: "English".to_string(),
            script: crate::script::Script::Latin,
            writing_system: crate::script::WritingSystem::Single(crate::script::Script::Latin),
            dictionary_code: "en".to_string(),
        };

        assert!(language.is_english());
        assert_eq!(language.scripted().script, crate::script::Script::Latin);
    }
}
