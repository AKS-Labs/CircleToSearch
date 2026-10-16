use std::hint::black_box;

use translator::{
    BackgroundMode, Feature, FsPackInstallChecker, LanguageCode, ReadingOrder, Rect,
    TranslatorSession,
};

fn main() {
    let checker = FsPackInstallChecker::new("");
    let session = TranslatorSession::open("{}", None, String::new(), &checker);

    if black_box(false) {
        if let Ok(session) = session {
            let languages = vec![LanguageCode::from("en")];
            let scripted = vec![translator::api::ScriptedLanguage {
                code: LanguageCode::from("en"),
                script: translator::script::Script::Latin,
            }];
            let target = translator::api::ScriptedLanguage {
                code: LanguageCode::from("nl"),
                script: translator::script::Script::Latin,
            };
            let texts = vec!["hello".to_string()];
            let _ = session.language_rows();
            let _ = session.language_overview();
            let _ = session.warm("en", "nl");
            let _ = session.translate_text("en", "nl", "hello");
            let _ = session.translate_html_fragments("en", "nl", &texts);
            let _ = session.translate_mixed_texts(&texts, Some("en"), "nl", &languages);
            let _ = session.translate_image_rgba(
                &[],
                1,
                1,
                u32::MAX,
                translator::OcrSourceSelection::specific(translator::LanguageCode::from("en")),
                "nl",
                75,
                Some(ReadingOrder::LeftToRight),
                BackgroundMode::AutoDetect,
                None,
            );
            let _ = session.plan_download("en", Feature::Core, None);
            let _ = session.plan_download("en", Feature::Dictionary, None);
            let _ = session.plan_download("en", Feature::Tts, None);
            let _ = session.plan_support_download_by_kind("mucab");
            let _ = session.prepare_delete("en", Feature::Core);
            let _ = session.prepare_delete_support_by_kind("mucab");
            let _ = session.prepare_delete_superseded_tts("en", "voice");
            let _ = session.size_bytes("en", Feature::Core);
            let _ = session.support_size_bytes_by_kind("mucab");
            let _ = session.lookup_dictionary("en", "hello");
            let _ = session.available_tts_voices("en");
            let _ = session.warm_tts_model("en");
            let _ =
                session.plan_speech_chunks("en", "hello", None, translator::UrlsAndHashtags::Skip);
            let _ = session.synthesize_pcm("en", "hello", 1.0, None, false, None);
            let _ = session.transliterate("hello", "ja");

            let _ = translator::sample_overlay_colors(
                &[],
                1,
                1,
                Rect::default(),
                BackgroundMode::AutoDetect,
                None,
            );
            let _ = translator::odt::translate_odt(&session, &[], Some("en"), "nl", &scripted);
            let _ = translator::pdf_translate::translate_pdf(
                &session,
                &[],
                Some("en"),
                &target,
                &scripted,
            );
            let _ = translator::pdf_write::write_translated_pdf(
                &[],
                &[],
                &translator::font_provider::NoFontProvider,
            );
        }
    }
}
