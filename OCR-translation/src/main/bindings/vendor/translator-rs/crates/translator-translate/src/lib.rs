pub mod document_translator;
#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod bergamot;
pub mod language_detect;
pub mod routing;
pub mod sentence_split;
pub mod translate;

#[cfg(feature = "dom-translate")]
pub mod dom_translate;
#[cfg(feature = "html")]
pub mod html_translate;
