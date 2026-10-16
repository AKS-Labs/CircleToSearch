#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

pub mod api;
pub mod catalog;
pub mod coords;
pub mod homography;
pub mod homography_ekf;
pub mod language;
pub mod ocr;
pub mod script;
pub mod script_normalize;
pub mod selection;
pub mod settings;
pub mod tts;

pub use settings::BackgroundMode;
