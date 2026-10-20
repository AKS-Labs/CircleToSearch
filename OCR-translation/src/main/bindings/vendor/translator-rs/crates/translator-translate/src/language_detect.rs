use cld2::{Effort, Format, Hints, Reliable, detect_language_with_effort};
use translator_core::api::{LanguageCode, ScriptedLanguage};
use translator_core::script::Script;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DetectionResult {
    pub language: String,
    pub is_reliable: bool,
    pub confidence: i32,
}

pub fn detect_language(text: &str, hint: Option<&LanguageCode>) -> Option<DetectionResult> {
    let hints = Hints {
        content_language: hint.map(LanguageCode::as_str),
        ..Default::default()
    };
    let detected = detect_language_with_effort(text, Format::Text, &hints, Effort::Standard);
    let language = detected.language?.0.to_string();
    let is_reliable = detected.reliability == Reliable;
    let confidence = detected
        .scores
        .first()
        .map(|score| score.percent as i32)
        .unwrap_or(0);

    Some(DetectionResult {
        language,
        is_reliable,
        confidence,
    })
}

/// Fewer whitespace-separated words than this and same-script languages are
/// indistinguishable: a single word like `mango` or `teatro` exists verbatim in
/// a dozen Latin-script languages.
const MIN_WORDS: usize = 3;

/// The words the detector may score. While the user is still typing, the last
/// token is a fragment (`teatr`) that matches languages the finished word does
/// not, so it is dropped unless whitespace or punctuation shows it is complete.
fn complete_words(text: &str) -> Vec<&str> {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let ends_mid_word = text
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric());
    if ends_mid_word {
        words.pop();
    }
    words
}

pub fn detect_language_robust_code(
    text: &str,
    hint: Option<&LanguageCode>,
    available_languages: &[ScriptedLanguage],
) -> Option<LanguageCode> {
    if text.trim().is_empty() {
        return None;
    }

    // cld2 confuses same-script languages on short input (सुप्रभात scores as
    // Sanskrit, धन्यवाद as Marathi, नमस्ते as nothing), and hinting only echoes
    // whatever hint it is given, so it can't disambiguate. The text's own script
    // narrows the candidates to the supported languages that use it; when exactly
    // one does, that is the answer regardless of what cld2 thinks.
    let script = Script::dominant(text)?;
    let candidates: Vec<&LanguageCode> = available_languages
        .iter()
        .filter(|lang| lang.script == script)
        .map(|lang| &lang.code)
        .collect();
    let scored_text: String = match candidates.as_slice() {
        [] => return None,
        [only] => return Some((*only).clone()),
        _ if script.uses_word_spacing() => {
            let words = complete_words(text);
            if words.len() < MIN_WORDS {
                return None;
            }
            words.join(" ")
        }
        _ => text.to_string(),
    };

    let hints = Hints {
        content_language: hint.map(LanguageCode::as_str),
        ..Default::default()
    };
    let detected =
        detect_language_with_effort(&scored_text, Format::Text, &hints, Effort::Standard);
    if detected.reliability != Reliable {
        return None;
    }
    detected
        .scores
        .iter()
        .filter_map(|score| score.language)
        .find_map(|lang| {
            let code = catalog_code(lang.0);
            candidates
                .iter()
                .find(|candidate| candidate.as_str() == code)
        })
        .map(|code| (*code).clone())
}

/// cld2 spells script variants BCP-47 style (`zh-Hant`); the catalog keys
/// them as `zh_hant`.
fn catalog_code(cld2_code: &str) -> String {
    cld2_code.to_ascii_lowercase().replace('-', "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Routes through the same ISO 15924 parse production uses, so the
    /// composite subtags (`Jpan`, `Hans`) resolve here exactly as they do from
    /// a real catalog.
    fn languages(list: &[&str]) -> Vec<ScriptedLanguage> {
        list.iter()
            .map(|code| ScriptedLanguage {
                code: LanguageCode::from(*code),
                script: translator_core::script::WritingSystem::from_iso15924(match *code {
                    "hi" => "Deva",
                    "bn" => "Beng",
                    "el" => "Grek",
                    "he" => "Hebr",
                    "th" => "Thai",
                    "ko" => "Hang",
                    "ja" => "Jpan",
                    "zh" => "Hans",
                    "zh_hant" => "Hant",
                    "ru" | "uk" | "bg" => "Cyrl",
                    _ => "Latn",
                })
                .expect("catalog script parses")
                .script(),
            })
            .collect()
    }

    fn detect(text: &str, available: &[&str]) -> Option<String> {
        detect_language_robust_code(text, None, &languages(available))
            .map(|c| c.as_str().to_string())
    }

    #[test]
    fn single_script_language_wins_over_cld2_guess() {
        // cld2 alone scores these as Sanskrit / Marathi / nothing; Devanagari maps
        // to only `hi` among the supported languages, so the script decides.
        let available = ["en", "es", "hi", "fr"];
        assert_eq!(detect("सुप्रभात", &available).as_deref(), Some("hi"));
        assert_eq!(detect("धन्यवाद", &available).as_deref(), Some("hi"));
        assert_eq!(detect("नमस्ते", &available).as_deref(), Some("hi"));
    }

    #[test]
    fn other_single_script_languages() {
        let available = ["en", "hi", "bn", "el", "he", "th", "ko"];
        assert_eq!(detect("ধন্যবাদ", &available).as_deref(), Some("bn"));
        assert_eq!(detect("ευχαριστώ", &available).as_deref(), Some("el"));
        assert_eq!(detect("תודה", &available).as_deref(), Some("he"));
        assert_eq!(detect("ขอบคุณ", &available).as_deref(), Some("th"));
        assert_eq!(detect("고맙습니다", &available).as_deref(), Some("ko"));
    }

    #[test]
    fn japanese_kana_beats_han_collision() {
        // Kanji outnumber kana here, but any kana marks it Japanese rather than
        // colliding with Chinese on the shared Han script.
        let available = ["en", "ja", "zh"];
        assert_eq!(
            detect("日本語を話します", &available).as_deref(),
            Some("ja")
        );
    }

    #[test]
    fn unsupported_script_returns_none() {
        // Devanagari text but `hi` is not installed: nothing to route it to.
        assert_eq!(detect("सुप्रभात", &["en", "es"]), None);
    }

    #[test]
    fn typed_fragment_is_not_scored() {
        // Typing "mango" passes through "mang", which cld2 would happily call
        // Malay when forced; "teatr" and "páj" likewise land on Slovak and Czech.
        let available = ["en", "es", "ms", "sk", "cs", "de"];
        assert_eq!(detect("mang", &available), None);
        assert_eq!(detect("teatr", &available), None);
        assert_eq!(detect("páj", &available), None);
        assert_eq!(detect("vamos al teatr", &available), None);
        assert_eq!(detect("vamos al teatro", &available), None);
        assert_eq!(
            detect("vamos al teatro esta noch", &available).as_deref(),
            Some("es")
        );
        assert_eq!(
            detect("vamos al teatro esta noche.", &available).as_deref(),
            Some("es")
        );
    }

    #[test]
    fn fewer_than_three_words_is_not_evidence() {
        let available = ["en", "es", "fr", "de"];
        assert_eq!(detect("hola como", &available), None);
        assert_eq!(detect("hola, como está", &available), None);
        assert_eq!(
            detect("hola, como estás?", &available).as_deref(),
            Some("es")
        );
    }

    #[test]
    fn unspaced_scripts_skip_the_word_gate() {
        let available = ["en", "zh", "zh_hant"];
        assert_eq!(detect("你好吗", &available).as_deref(), Some("zh"));
        assert_eq!(
            detect("火車站在哪裡", &available).as_deref(),
            Some("zh_hant")
        );
    }

    #[test]
    fn multi_language_script_defers_to_cld2() {
        // Latin and Cyrillic each cover several supported languages, so cld2's
        // ranking still decides within the script.
        let available = ["en", "es", "fr", "de", "ru", "uk"];
        assert_eq!(
            detect("The quick brown fox jumps over the lazy dog", &available).as_deref(),
            Some("en")
        );
        assert_eq!(
            detect("Съешь же ещё этих мягких французских булочек", &available).as_deref(),
            Some("ru")
        );
    }
}
