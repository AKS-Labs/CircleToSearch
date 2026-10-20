#[uniffi::export]
pub fn transliterate_with_policy_record(
    text: String,
    language_code: String,
    writing_system: translator::script::WritingSystem,
    japanese_dict_path: Option<String>,
    japanese_spaced: bool,
) -> Option<String> {
    #[cfg(feature = "transliterate")]
    {
        translator::transliterate::transliterate_with_policy_for_language(
            &text,
            &translator::LanguageCode::from(language_code),
            writing_system,
            japanese_dict_path.as_deref(),
            japanese_spaced,
        )
    }
    #[cfg(not(feature = "transliterate"))]
    {
        let _ = (
            text,
            language_code,
            writing_system,
            japanese_dict_path,
            japanese_spaced,
        );
        None
    }
}
