//! On-image word selection. The geometry, the covered range, the highlight shapes and the
//! copied text all live in translator-rs so this app and the Qt one behave identically; these
//! are plain pass-throughs taking image-pixel coordinates.

use translator::ocr::PositionedWord;
use translator::selection::{self, SelectionView, WritingAxis};

#[uniffi::export]
pub fn selection_word_at(words: Vec<PositionedWord>, x: f32, y: f32) -> Option<u32> {
    selection::hit_test_word(&words, x, y)
}

#[uniffi::export]
pub fn selection_nearest_word(
    words: Vec<PositionedWord>,
    x: f32,
    y: f32,
    axis: Option<WritingAxis>,
) -> Option<u32> {
    selection::nearest_word(&words, x, y, axis)
}

#[uniffi::export]
pub fn selection_word_axis(words: Vec<PositionedWord>, index: u32) -> Option<WritingAxis> {
    selection::word_axis(&words, index)
}

#[uniffi::export]
pub fn selection_resolve(
    words: Vec<PositionedWord>,
    start: u32,
    end: u32,
) -> Option<SelectionView> {
    selection::resolve_selection(&words, start, end)
}
