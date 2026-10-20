use translator::language_detect::{detect_language, detect_language_robust_code};

#[uniffi::export]
pub fn detect_language_record(
    text: String,
    hint: Option<String>,
) -> Option<translator::DetectionResult> {
    let hint = hint.map(translator::LanguageCode::from);
    detect_language(&text, hint.as_ref())
}

/// A detection candidate and the writing system the catalog records for it.
/// The script travels with the code because this entry point is free-standing
/// and has no catalog to resolve one against.
#[derive(uniffi::Record)]
pub struct AvailableLanguage {
    pub code: String,
    pub script: translator::script::Script,
}

#[uniffi::export]
pub fn detect_language_robust_code_record(
    text: String,
    hint: Option<String>,
    available_languages: Vec<AvailableLanguage>,
) -> Option<String> {
    let hint = hint.map(translator::LanguageCode::from);
    let available = available_languages
        .into_iter()
        .map(|language| translator::api::ScriptedLanguage {
            code: translator::LanguageCode::from(language.code),
            script: language.script,
        })
        .collect::<Vec<_>>();
    detect_language_robust_code(&text, hint.as_ref(), &available).map(|code| code.code)
}
