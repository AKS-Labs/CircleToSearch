use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LanguageCode {
    pub code: String,
}

impl LanguageCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self { code: value.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl From<&str> for LanguageCode {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LanguageCode {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for LanguageCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for LanguageCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.code.fmt(f)
    }
}

/// A language together with the writing system the catalog records for it.
///
/// Resolved once where the catalog is in scope, so crates that pick fonts or
/// decide on romanization receive the script instead of inferring one from the
/// language tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptedLanguage {
    pub code: LanguageCode,
    pub script: crate::script::Script,
}

impl ScriptedLanguage {
    pub fn as_str(&self) -> &str {
        self.code.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ScriptCode {
    pub code: String,
}

impl ScriptCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self { code: value.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl From<&str> for ScriptCode {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ScriptCode {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for ScriptCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ScriptCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.code.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct VoiceName {
    pub name: String,
}

impl VoiceName {
    pub fn new(value: impl Into<String>) -> Self {
        Self { name: value.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl From<&str> for VoiceName {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for VoiceName {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for VoiceName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for VoiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DictionaryCode {
    pub code: String,
}

impl DictionaryCode {
    pub fn new(value: impl Into<String>) -> Self {
        Self { code: value.into() }
    }

    pub fn as_str(&self) -> &str {
        &self.code
    }
}

impl From<&str> for DictionaryCode {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DictionaryCode {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for DictionaryCode {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for DictionaryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.code.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum TranslatorErrorKind {
    Translation,
    Ocr,
    Tts,
    Dictionary,
    Transliterate,
    InvalidInput,
    Internal,
    MissingAsset,
    /// User-initiated cancellation of an in-flight document translation.
    /// Not a failure — callers should stop silently.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslatorError {
    pub kind: TranslatorErrorKind,
    pub message: String,
}

impl TranslatorError {
    pub fn new(kind: TranslatorErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn translation(message: impl Into<String>) -> Self {
        Self::new(TranslatorErrorKind::Translation, message)
    }

    pub fn ocr(message: impl Into<String>) -> Self {
        Self::new(TranslatorErrorKind::Ocr, message)
    }

    pub fn tts(message: impl Into<String>) -> Self {
        Self::new(TranslatorErrorKind::Tts, message)
    }

    pub fn dictionary(message: impl Into<String>) -> Self {
        Self::new(TranslatorErrorKind::Dictionary, message)
    }

    pub fn missing_asset(message: impl Into<String>) -> Self {
        Self::new(TranslatorErrorKind::MissingAsset, message)
    }

    pub fn cancelled() -> Self {
        Self::new(TranslatorErrorKind::Cancelled, "cancelled")
    }

    pub fn is_missing_asset(&self) -> bool {
        self.kind == TranslatorErrorKind::MissingAsset
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == TranslatorErrorKind::Cancelled
    }
}

impl fmt::Display for TranslatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

impl std::error::Error for TranslatorError {}
