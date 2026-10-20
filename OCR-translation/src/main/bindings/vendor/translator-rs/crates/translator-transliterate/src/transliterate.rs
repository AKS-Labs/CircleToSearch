use icu_experimental::transliterate::Transliterator;
use icu_locale_core::Locale;
use unicode_script::{Script, UnicodeScript};

use translator_core::api::LanguageCode;
use translator_core::script::{Script as CatalogScript, WritingSystem};

fn make_transliterator(source_script: &str) -> Option<Transliterator> {
    let locale_str = format!("und-Latn-t-und-{}", source_script.to_lowercase());
    let locale: Locale = locale_str.parse().ok()?;
    Transliterator::try_new(&locale).ok()
}

/// Malayalam dependent vowel signs that CLDR's `Malayalam-InterIndic` has no
/// rules for, so `und-Latn-t-und-mlym` romanizes them wrongly: `ൊ ോ ൌ` are
/// canonically decomposed by the transform's `::NFD;` step and then matched in
/// halves (`ൊ` → `eā`), while `ൄ ൢ ൣ` are not even admitted by its filter and
/// survive into the Latin output verbatim. Reported as CLDR-19646; drop this
/// once the rules land upstream.
///
/// Each entry is (vowel sign as it may appear in the input, marker, correct
/// romanization). Both the composed and NFD forms are listed for the three
/// decomposable signs, since either may reach us.
const MLYM_UNMAPPED_VOWEL_SIGNS: &[(&str, char, &str)] = &[
    ("\u{0D4A}", '\u{F8F0}', "o"),
    ("\u{0D46}\u{0D3E}", '\u{F8F0}', "o"),
    ("\u{0D4B}", '\u{F8F1}', "\u{014D}"),
    ("\u{0D47}\u{0D3E}", '\u{F8F1}', "\u{014D}"),
    ("\u{0D4C}", '\u{F8F2}', "au"),
    ("\u{0D46}\u{0D57}", '\u{F8F2}', "au"),
    ("\u{0D44}", '\u{F8F3}', "r\u{0325}\u{0304}"),
    ("\u{0D62}", '\u{F8F4}', "l\u{0325}"),
    ("\u{0D63}", '\u{F8F5}', "l\u{0325}\u{0304}"),
];

/// Stands in for an unmapped sign while the transform runs. A Malayalam vowel
/// sign rather than the Latin vowel itself, because a consonant carries an
/// inherent `a` that only a vowel sign displaces — dropping the sign entirely
/// would romanize `മൊഴി` as `maoḻi` instead of `moḻi`.
const MLYM_STAND_IN_SIGN: char = '\u{0D3F}';
const MLYM_STAND_IN_LATIN: &str = "i";

fn mask_malayalam_vowel_signs(text: &str) -> String {
    let stripped: String = text
        .chars()
        .filter(|ch| !MLYM_UNMAPPED_VOWEL_SIGNS.iter().any(|(_, m, _)| ch == m))
        .collect();

    MLYM_UNMAPPED_VOWEL_SIGNS
        .iter()
        .fold(stripped, |acc, (sign, marker, _)| {
            acc.replace(sign, &format!("{MLYM_STAND_IN_SIGN}{marker}"))
        })
}

fn restore_malayalam_vowel_signs(romanized: String) -> String {
    MLYM_UNMAPPED_VOWEL_SIGNS
        .iter()
        .fold(romanized, |acc, (_, marker, latin)| {
            acc.replace(&format!("{MLYM_STAND_IN_LATIN}{marker}"), latin)
        })
}

fn transliterate(text: &str, source: WritingSystem) -> Option<String> {
    match source {
        // CLDR publishes no `Jpan` transform: katakana and hiragana each have
        // their own, and mucab hands us readings in katakana.
        WritingSystem::Japanese => {
            let kana = make_transliterator("Kana")?;
            let hira = make_transliterator("Hira")?;
            let result = kana.transliterate(text.to_string());
            Some(hira.transliterate(result))
        }
        WritingSystem::Single(CatalogScript::Malayalam) => {
            let t = make_transliterator("Mlym")?;
            let masked = mask_malayalam_vowel_signs(text);
            Some(restore_malayalam_vowel_signs(t.transliterate(masked)))
        }
        other => {
            let t = make_transliterator(other.iso15924()?)?;
            Some(t.transliterate(text.to_string()))
        }
    }
}

/// Writing system that romanizes a given run, or `None` for runs we leave
/// untouched (Latin, punctuation, marks). Scripts ICU has no transform for
/// still fall back to pass-through via `transliterate` returning `None`.
fn romanizable_source(script: Script) -> Option<WritingSystem> {
    match CatalogScript::from(script) {
        CatalogScript::Latin => None,
        // Both kana share the Japanese transform (katakana then hiragana).
        CatalogScript::Hiragana | CatalogScript::Katakana => Some(WritingSystem::Japanese),
        // A Han run carries no simplified/traditional marker of its own, and
        // CLDR romanizes both with the same pinyin rules, so either subtag
        // reaches them.
        CatalogScript::Han => Some(WritingSystem::HanSimplified),
        // Punctuation, marks and unenumerated scripts have no subtag, so they
        // drop out of `transliterate` rather than needing their own arm.
        other => Some(WritingSystem::Single(other)),
    }
}

/// Punctuation, whitespace, digits and combining marks carry no script of their
/// own; they belong to whichever run surrounds them.
fn is_neutral(script: Script) -> bool {
    matches!(script, Script::Common | Script::Inherited)
}

/// Assign every char a concrete script, letting neutral chars inherit from the
/// run they sit in (preceding run first, leading neutrals from the following
/// one).
fn resolve_char_scripts(text: &str) -> Vec<(char, Script)> {
    let mut chars: Vec<(char, Script)> = text.chars().map(|ch| (ch, ch.script())).collect();

    let mut last_concrete: Option<Script> = None;
    for (_, script) in chars.iter_mut() {
        if is_neutral(*script) {
            if let Some(prev) = last_concrete {
                *script = prev;
            }
        } else {
            last_concrete = Some(*script);
        }
    }

    if let Some(first_concrete) = chars.iter().map(|(_, s)| *s).find(|s| !is_neutral(*s)) {
        for (_, script) in chars.iter_mut() {
            if !is_neutral(*script) {
                break;
            }
            *script = first_concrete;
        }
    }

    chars
}

/// Romanize the non-Latin runs of a mixed-script string, leaving Latin text,
/// punctuation and whitespace as they are. Used when the requested output is a
/// Latin-script voice but the text carries foreign names ("Your line 'Сливница
/// - Летище София' is arriving" → "Your line 'Slivnitsa - Letishte Sofiya' is
/// arriving").
pub fn transliterate_mixed_to_latin(text: &str) -> String {
    let resolved = resolve_char_scripts(text);

    let mut out = String::with_capacity(text.len());
    let mut run = String::new();
    let mut run_script: Option<Script> = None;

    for (ch, script) in resolved {
        if run_script != Some(script) {
            flush_run(&mut out, &run, run_script);
            run.clear();
            run_script = Some(script);
        }
        run.push(ch);
    }
    flush_run(&mut out, &run, run_script);

    out
}

fn flush_run(out: &mut String, run: &str, run_script: Option<Script>) {
    if run.is_empty() {
        return;
    }
    let romanized = run_script
        .and_then(romanizable_source)
        .and_then(|source| transliterate(run, source));
    match romanized {
        Some(romanized) => out.push_str(&romanized),
        None => out.push_str(run),
    }
}

fn transliterate_with_policy(
    text: &str,
    language_code: &LanguageCode,
    source: WritingSystem,
    japanese_preprocessed: Option<&str>,
) -> Option<String> {
    if source.script() == CatalogScript::Latin {
        return None;
    }

    let input = match language_code.as_str() {
        "ja" => japanese_preprocessed.unwrap_or(text),
        _ => text,
    };

    transliterate(input, source)
}

/// Romanize `text`, written in `source`. `None` when the text is already Latin
/// or when CLDR has no transform for the writing system.
pub fn transliterate_with_policy_for_language(
    text: &str,
    language_code: &LanguageCode,
    source: WritingSystem,
    japanese_dict_path: Option<&str>,
    japanese_spaced: bool,
) -> Option<String> {
    let normalized = text.trim();
    if normalized.is_empty() || normalized.is_ascii() {
        return None;
    }

    let japanese_preprocessed = if language_code.as_str() == "ja" {
        preprocess_japanese(normalized, japanese_dict_path, japanese_spaced)
    } else {
        None
    };

    transliterate_with_policy(
        normalized,
        language_code,
        source,
        japanese_preprocessed.as_deref(),
    )
}

#[cfg(feature = "mucab")]
fn preprocess_japanese(
    text: &str,
    dict_path: Option<&str>,
    japanese_spaced: bool,
) -> Option<String> {
    let dict_path = dict_path?;
    if dict_path.is_empty() {
        return None;
    }
    translator_mucab::mucab::transliterate_with_path(dict_path, text, japanese_spaced).ok()
}

#[cfg(not(feature = "mucab"))]
fn preprocess_japanese(
    _text: &str,
    _dict_path: Option<&str>,
    _japanese_spaced: bool,
) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writing_system(subtag: &str) -> WritingSystem {
        WritingSystem::from_iso15924(subtag).expect("test subtag parses")
    }

    fn translit(subtag: &str, text: &str) -> String {
        transliterate(text, writing_system(subtag)).unwrap()
    }

    #[test]
    fn test_cyrillic() {
        assert_eq!(translit("Cyrl", "Привет мир"), "Privet mir");
    }

    #[test]
    fn test_arabic() {
        assert_eq!(translit("Arab", "مرحبا"), "mrḥbạ");
    }

    #[test]
    fn test_greek() {
        assert_eq!(translit("Grek", "Αθήνα"), "Athḗna");
    }

    #[test]
    fn test_devanagari() {
        assert_eq!(translit("Deva", "नमस्ते"), "namastē");
    }

    #[test]
    fn test_hangul() {
        assert_eq!(translit("Hang", "안녕하세요"), "annyeonghaseyo");
    }

    #[test]
    fn test_hebrew() {
        assert_eq!(translit("Hebr", "שלום"), "şlwm");
    }

    #[test]
    fn test_bengali() {
        assert_eq!(translit("Beng", "নমস্কার"), "namaskāra");
    }

    #[test]
    fn test_tamil() {
        assert_eq!(translit("Taml", "வணக்கம்"), "vaṇakkam");
    }

    #[test]
    fn test_telugu() {
        assert_eq!(translit("Telu", "నమస్కారం"), "namaskāraṁ");
    }

    #[test]
    fn test_malayalam() {
        assert_eq!(translit("Mlym", "നമസ്കാരം"), "namaskāraṁ");
        assert_eq!(translit("Mlym", "മലയാളം"), "malayāḷaṁ");
    }

    #[test]
    fn test_malayalam_unmapped_vowel_signs() {
        assert_eq!(translit("Mlym", "മൊഴി"), "moḻi");
        assert_eq!(translit("Mlym", "മൊ"), "mo");
        assert_eq!(translit("Mlym", "മോ"), "mō");
        assert_eq!(translit("Mlym", "മൌ"), "mau");
        assert_eq!(translit("Mlym", "മൄ"), "mr̥̄");
        assert_eq!(translit("Mlym", "മൢ"), "ml̥");
        assert_eq!(translit("Mlym", "മൣ"), "ml̥̄");
    }

    #[test]
    fn test_malayalam_decomposed_input_matches_composed() {
        assert_eq!(
            translit("Mlym", "\u{0D2E}\u{0D46}\u{0D3E}"),
            translit("Mlym", "\u{0D2E}\u{0D4A}")
        );
        assert_eq!(
            translit("Mlym", "\u{0D2E}\u{0D47}\u{0D3E}"),
            translit("Mlym", "\u{0D2E}\u{0D4B}")
        );
    }

    #[test]
    fn test_malayalam_working_vowel_signs_untouched() {
        assert_eq!(translit("Mlym", "മാ"), "mā");
        assert_eq!(translit("Mlym", "മി"), "mi");
        assert_eq!(translit("Mlym", "മെ"), "me");
        assert_eq!(translit("Mlym", "മേ"), "mē");
        assert_eq!(translit("Mlym", "മൈ"), "mai");
        assert_eq!(translit("Mlym", "മൃ"), "mr̥");
    }

    #[test]
    fn test_malayalam_independent_vowels_untouched() {
        assert_eq!(translit("Mlym", "ഒരു"), "oru");
        assert_eq!(translit("Mlym", "ഓട്ടം"), "ōṭṭaṁ");
    }

    #[test]
    fn test_mixed_malayalam_in_latin_sentence() {
        assert_eq!(
            transliterate_mixed_to_latin("the word മൊഴി means speech"),
            "the word moḻi means speech"
        );
    }

    #[test]
    fn test_malayalam_marker_in_input_does_not_leak() {
        assert_eq!(translit("Mlym", "മൊ\u{F8F0}ഴി"), "moḻi");
    }

    #[test]
    fn test_han_simplified() {
        assert_eq!(translit("Hans", "你好世界"), "nǐ hǎo shì jiè");
    }

    #[test]
    fn test_han_traditional() {
        assert_eq!(translit("Hant", "你好世界"), "nǐ hǎo shì jiè");
    }

    #[test]
    fn test_japanese_hiragana() {
        assert_eq!(translit("Jpan", "こんにちは"), "kon'nichiha");
    }

    #[test]
    fn test_japanese_katakana() {
        assert_eq!(translit("Jpan", "カタカナ"), "katakana");
    }

    #[test]
    fn test_japanese_mixed_kana() {
        let result = translit("Jpan", "ひらがなカタカナ");
        assert!(result.contains("hiragana"));
        assert!(result.contains("katakana"));
    }

    #[test]
    fn test_jpan_preserves_kanji() {
        // After mucab, some kanji may remain unconverted.
        // Verify they pass through unchanged.
        assert_eq!(translit("Jpan", "東京 の ひと"), "東京 no hito");
    }

    #[test]
    fn test_jpan_simulated_mucab_output() {
        // mucab converts kanji→hiragana and adds spaces.
        // Simulate: "東京タワー" → mucab → "とうきょう タワー"
        // Then ICU should produce: "toukyou tawā"
        assert_eq!(translit("Jpan", "とうきょう タワー"), "toukyou tawā");
    }

    #[test]
    fn test_mixed_cyrillic_in_latin_sentence() {
        assert_eq!(
            transliterate_mixed_to_latin("Your line 'Сливница - Летище София' is arriving"),
            "Your line 'Slivnica - Letiŝe Sofiâ' is arriving"
        );
    }

    #[test]
    fn test_mixed_pure_latin_unchanged() {
        assert_eq!(
            transliterate_mixed_to_latin("Just a plain ASCII sentence."),
            "Just a plain ASCII sentence."
        );
    }

    #[test]
    fn test_mixed_pure_cyrillic() {
        assert_eq!(transliterate_mixed_to_latin("Привет мир"), "Privet mir");
    }

    #[test]
    fn test_mixed_multiple_scripts() {
        assert_eq!(
            transliterate_mixed_to_latin("hello Привет and Αθήνα done"),
            "hello Privet and Athḗna done"
        );
    }

    #[test]
    fn test_latin_is_none() {
        assert!(transliterate("Hello", writing_system("Latn")).is_none());
    }

    #[test]
    fn test_policy_skips_latin_source() {
        assert!(
            transliterate_with_policy(
                "Hello",
                &LanguageCode::from("en"),
                writing_system("Latn"),
                None
            )
            .is_none()
        );
    }

    #[test]
    fn test_policy_uses_japanese_preprocessed_text() {
        assert_eq!(
            transliterate_with_policy(
                "東京タワー",
                &LanguageCode::from("ja"),
                writing_system("Jpan"),
                Some("とうきょう タワー")
            )
            .unwrap(),
            "toukyou tawā"
        );
    }

    /// mucab reports readings in katakana, so the Japanese arm has to romanize
    /// katakana before hiragana. Keying on the `Hira` inventory instead left
    /// the reading in katakana.
    #[test]
    fn test_policy_romanizes_katakana_reading_from_mucab() {
        assert_eq!(
            transliterate_with_policy(
                "こんにちは",
                &LanguageCode::from("ja"),
                writing_system("Jpan"),
                Some("コンニチワ")
            )
            .unwrap(),
            "kon'nichiwa"
        );
    }

    /// CLDR has no `Hani` transform: pinyin is published under the
    /// simplified/traditional subtags, which is why the catalog's own subtag
    /// has to survive to here.
    #[test]
    fn test_chinese_subtags_romanize_to_pinyin() {
        assert_eq!(
            transliterate_with_policy(
                "你好世界",
                &LanguageCode::from("zh"),
                writing_system("Hans"),
                None
            )
            .unwrap(),
            "nǐ hǎo shì jiè"
        );
        assert_eq!(
            transliterate_with_policy(
                "你好世界",
                &LanguageCode::from("zh-Hant"),
                writing_system("Hant"),
                None
            )
            .unwrap(),
            "nǐ hǎo shì jiè"
        );
    }

    #[test]
    fn test_mixed_han_in_latin_sentence() {
        assert_eq!(
            transliterate_mixed_to_latin("the city 北京 is large"),
            "the city běi jīng is large"
        );
    }
}
