//! Plain-text (`.txt`) translation with caller-controlled layout.
//!
//! Two layouts:
//!   - `Preserve`: every newline is kept and each non-empty line is
//!     translated independently. Lossless for structured text (lists,
//!     code, tables) where line breaks carry meaning.
//!   - `Reflow`: blank lines delimit paragraphs; the hard wraps inside a
//!     paragraph are collapsed so the sentence splitter sees whole
//!     paragraphs rather than wrap-width fragments. `wrap` re-wraps each
//!     translated paragraph to a column width on output (for the
//!     Gutenberg-style "keep it looking like a book" case).
//!
//! Word-wrapping splits on whitespace, so a space-less target script
//! (CJK) naturally falls through unwrapped — there is no inter-word break
//! point and we never split mid-glyph.

use std::num::NonZeroU32;

use translator_translate::document_translator::DocumentTranslator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxtLayout {
    Preserve,
    Reflow { wrap: Option<NonZeroU32> },
}

#[derive(Debug)]
pub enum TxtTranslateError {
    Translation(String),
    Cancelled,
}

/// Reconstruction plan paired with the translatable units it produced.
/// `Preserve` keeps the full line vector and the indices of the lines
/// that were sent for translation; `Reflow` keeps only the wrap width
/// since its units map one-to-one onto output paragraphs.
enum Plan {
    Preserve {
        lines: Vec<String>,
        unit_indices: Vec<usize>,
    },
    Reflow {
        wrap: Option<NonZeroU32>,
    },
}

/// Translate a `.txt` document in a single slimt call. `on_progress` receives a
/// smooth `[0.0, 1.0]` completion fraction (source-length weighted), reported
/// from slimt worker threads — it must be cheap, non-blocking and thread-safe.
/// Cancellation is requested out-of-band via
/// [`TranslatorSession::cancel_ongoing_work`] and surfaces here as
/// [`TxtTranslateError::Cancelled`].
pub fn translate_txt_with_progress(
    translator: &dyn DocumentTranslator,
    text: &str,
    source_code: &str,
    target_code: &str,
    layout: TxtLayout,
    on_progress: impl Fn(f32) + Sync,
) -> Result<String, TxtTranslateError> {
    translator.begin_document_translation();
    let (units, plan) = build_units(text, layout);
    if units.is_empty() {
        return Ok(String::new());
    }

    let translated = if source_code == target_code {
        units.clone()
    } else {
        let report = |done: usize, total: usize| {
            if total > 0 {
                on_progress(done as f32 / total as f32);
            }
        };
        translator
            .translate_texts_ctx(source_code, target_code, &units, &report)
            .map_err(|error| {
                if error.is_cancelled() {
                    TxtTranslateError::Cancelled
                } else {
                    TxtTranslateError::Translation(error.message)
                }
            })?
    };

    Ok(reconstruct(plan, translated))
}

fn build_units(text: &str, layout: TxtLayout) -> (Vec<String>, Plan) {
    match layout {
        TxtLayout::Preserve => {
            let lines: Vec<String> = text.split('\n').map(ToOwned::to_owned).collect();
            let unit_indices: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter_map(|(index, line)| (!line.trim().is_empty()).then_some(index))
                .collect();
            let units = unit_indices.iter().map(|&i| lines[i].clone()).collect();
            (
                units,
                Plan::Preserve {
                    lines,
                    unit_indices,
                },
            )
        }
        TxtLayout::Reflow { wrap } => {
            let paragraphs = split_paragraphs(text);
            (paragraphs, Plan::Reflow { wrap })
        }
    }
}

fn reconstruct(plan: Plan, translated: Vec<String>) -> String {
    match plan {
        Plan::Preserve {
            mut lines,
            unit_indices,
        } => {
            for (index, text) in unit_indices.into_iter().zip(translated) {
                lines[index] = text;
            }
            lines.join("\n")
        }
        Plan::Reflow { wrap } => translated
            .iter()
            .map(|paragraph| match wrap {
                Some(width) => word_wrap(paragraph, width.get() as usize),
                None => paragraph.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    }
}

/// Split into blank-line-delimited paragraphs. Each paragraph collapses
/// its hard-wrapped lines and runs of whitespace into single spaces.
/// Blank-only blocks are dropped, so the output paragraph count is the
/// number of content blocks.
fn split_paragraphs(text: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.trim().is_empty() {
            if !current.is_empty() {
                paragraphs.push(join_paragraph(&current));
                current.clear();
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        paragraphs.push(join_paragraph(&current));
    }
    paragraphs
}

fn join_paragraph(lines: &[&str]) -> String {
    lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Greedy word wrap at `width` columns (counted in `char`s, not display
/// cells). A token longer than `width` — or text with no spaces to break
/// on — overflows its line rather than being split.
fn word_wrap(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line_len = 0usize;
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if line_len == 0 {
            out.push_str(word);
            line_len = word_len;
        } else if line_len + 1 + word_len <= width {
            out.push(' ');
            out.push_str(word);
            line_len += 1 + word_len;
        } else {
            out.push('\n');
            out.push_str(word);
            line_len = word_len;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nz(value: u32) -> Option<NonZeroU32> {
        NonZeroU32::new(value)
    }

    #[test]
    fn preserve_keeps_blank_lines_and_line_count() {
        let text = "line one\n\nline two\nline three\n";
        let (units, plan) = build_units(text, TxtLayout::Preserve);
        assert_eq!(units, vec!["line one", "line two", "line three"]);
        // Round-trip with identity "translation" reproduces the input.
        let out = reconstruct(plan, units);
        assert_eq!(out, text);
    }

    #[test]
    fn reflow_joins_hard_wrapped_lines_into_paragraphs() {
        let text = "The quick brown\nfox jumps over\n\nThe lazy\ndog.";
        let (units, _) = build_units(text, TxtLayout::Reflow { wrap: None });
        assert_eq!(
            units,
            vec!["The quick brown fox jumps over", "The lazy dog."]
        );
    }

    #[test]
    fn reflow_collapses_repeated_blank_lines_and_whitespace() {
        let text = "a  b\n\n\n\nc   d";
        let (units, _) = build_units(text, TxtLayout::Reflow { wrap: None });
        assert_eq!(units, vec!["a b", "c d"]);
    }

    #[test]
    fn reflow_no_wrap_joins_paragraphs_with_blank_line() {
        let plan = Plan::Reflow { wrap: None };
        let out = reconstruct(plan, vec!["one two".to_string(), "three".to_string()]);
        assert_eq!(out, "one two\n\nthree");
    }

    #[test]
    fn reflow_wrap_breaks_on_word_boundaries() {
        let plan = Plan::Reflow { wrap: nz(10) };
        let out = reconstruct(plan, vec!["aaa bbb ccc ddd".to_string()]);
        assert_eq!(out, "aaa bbb\nccc ddd");
    }

    #[test]
    fn word_wrap_overflows_rather_than_splitting_long_token() {
        assert_eq!(
            word_wrap("supercalifragilistic ok", 5),
            "supercalifragilistic\nok"
        );
    }

    #[test]
    fn word_wrap_spaceless_text_stays_on_one_line() {
        let cjk = "日本語のテキストです";
        assert_eq!(word_wrap(cjk, 4), cjk);
    }
}
