use translator::LanguageCode;
use translator::api::ScriptedLanguage;
use translator::language_detect::{detect_language, detect_language_robust_code};
use translator::script::Script;

fn scripted(codes: &[&str]) -> Vec<ScriptedLanguage> {
    codes
        .iter()
        .map(|code| ScriptedLanguage {
            code: LanguageCode::from(*code),
            script: match *code {
                "ja" => Script::Hiragana,
                _ => Script::Latin,
            },
        })
        .collect()
}

fn run(text: &str, hint: Option<&str>) {
    let hint_owned = hint.map(LanguageCode::from);
    let _ = detect_language(text, hint_owned.as_ref());
}

#[test]
fn matches_bergamot_shape_with_hint() {
    let text = "this is some text";
    for _ in 0..256 {
        run(text, Some("en"));
    }
}

#[test]
fn matches_bergamot_shape_no_hint() {
    let text = "this is some text";
    for _ in 0..256 {
        run(text, None);
    }
}

#[test]
fn robust_code_with_available_list() {
    let available = scripted(&["en", "es", "fr", "de", "ja"]);
    for _ in 0..64 {
        let _ =
            detect_language_robust_code("hello world", Some(&LanguageCode::from("en")), &available);
        let _ = detect_language_robust_code("hola mundo", None, &available);
        let _ =
            detect_language_robust_code("こんにちは", Some(&LanguageCode::from("ja")), &available);
    }
}

#[test]
fn robust_code_unreliable_falls_through_loop() {
    let available = scripted(&["en", "es", "fr", "de"]);
    for _ in 0..64 {
        let _ = detect_language_robust_code("xyz", Some(&LanguageCode::from("en")), &available);
        let _ = detect_language_robust_code("a", Some(&LanguageCode::from("en")), &available);
    }
}

#[test]
fn many_short_inputs_with_hint() {
    let inputs = [
        "", " ", "a", "hi", "ok", "no", "si", "hello", "world", "test", "??", "...", "1234", "the",
        "and", "or", "but",
    ];
    for _ in 0..32 {
        for s in inputs.iter() {
            run(s, Some("en"));
            run(s, None);
        }
    }
}
