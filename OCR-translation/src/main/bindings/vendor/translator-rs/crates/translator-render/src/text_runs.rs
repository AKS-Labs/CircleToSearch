//! Script-run itemizer following UAX #24.
//!
//! Given a string, produce contiguous runs each tagged with a [`Script`].
//! `Common` (digits, ASCII punctuation, spaces) and `Inherited` (combining
//! marks) are folded into the surrounding strong-script run so we don't
//! fragment shaping unnecessarily.
//!
//! This is a shared service between the image renderer (which then asks the
//! `FontProvider` per run) and, eventually, the PDF overlay's mixed-run
//! splitter.

use translator_core::script::Script;
use unicode_script::UnicodeScript;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptRun {
    /// Byte offset in the source string where this run starts.
    pub start: usize,
    /// Byte offset where this run ends (exclusive).
    pub end: usize,
    pub script: Script,
}

impl ScriptRun {
    pub fn slice<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

/// Itemize `text` into script runs. The output covers the whole input string
/// with no gaps; consecutive same-script runs are coalesced.
///
/// Handles UAX #24 carry-over for `Common` and `Inherited`:
/// - `Inherited` always carries from the preceding strong script (or the
///   following one if it appears at the start).
/// - `Common` carries from the surrounding strong script when both sides
///   agree, otherwise is its own run (rare in practice; we choose the
///   preceding script as the tiebreaker, matching HarfBuzz).
pub fn itemize(text: &str) -> Vec<ScriptRun> {
    if text.is_empty() {
        return Vec::new();
    }

    let mut raw: Vec<(usize, usize, Script)> = Vec::new();
    for (idx, ch) in text.char_indices() {
        let s = Script::from(ch.script());
        let end = idx + ch.len_utf8();
        raw.push((idx, end, s));
    }

    resolve_common_inherited(&mut raw, text);

    let mut runs: Vec<ScriptRun> = Vec::new();
    for (start, end, script) in raw {
        match runs.last_mut() {
            Some(last) if last.script == script && last.end == start => {
                last.end = end;
            }
            _ => runs.push(ScriptRun { start, end, script }),
        }
    }
    runs
}

/// In-place carry-over. After this pass, no codepoint is left tagged
/// `Inherited`; `Common` becomes the surrounding strong script when one
/// neighbour is strong, otherwise stays `Common`.
fn resolve_common_inherited(items: &mut [(usize, usize, Script)], _text: &str) {
    // Forward pass: anything Inherited / Common adopts the previous strong
    // script when one exists.
    let mut last_strong: Option<Script> = None;
    for entry in items.iter_mut() {
        match entry.2 {
            Script::Inherited => {
                if let Some(s) = last_strong {
                    entry.2 = s;
                }
            }
            Script::Common => {
                if let Some(s) = last_strong {
                    entry.2 = s;
                }
            }
            other => {
                last_strong = Some(other);
            }
        }
    }
    // Backward pass: anything still Inherited or Common (i.e. there was no
    // preceding strong script) adopts the next strong script. Leftovers stay
    // Common — that's fine, the renderer falls back to the document language.
    let mut next_strong: Option<Script> = None;
    for entry in items.iter_mut().rev() {
        match entry.2 {
            Script::Inherited => {
                if let Some(s) = next_strong {
                    entry.2 = s;
                }
            }
            Script::Common => {
                if let Some(s) = next_strong {
                    entry.2 = s;
                }
            }
            other => {
                next_strong = Some(other);
            }
        }
    }
    // Any Inherited that survives both passes (string of pure combining
    // marks) becomes Common — easier to handle downstream than a third
    // category.
    for entry in items.iter_mut() {
        if entry.2 == Script::Inherited {
            entry.2 = Script::Common;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_latin_is_one_run() {
        let runs = itemize("Hello, world!");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].script, Script::Latin);
        assert_eq!(runs[0].slice("Hello, world!"), "Hello, world!");
    }

    #[test]
    fn cyrillic_with_latin_splits_at_script_boundary() {
        let text = "Hello мир";
        let runs = itemize(text);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].script, Script::Latin);
        assert_eq!(runs[0].slice(text), "Hello ");
        assert_eq!(runs[1].script, Script::Cyrillic);
        assert_eq!(runs[1].slice(text), "мир");
    }

    #[test]
    fn devanagari_with_inherited_marks_is_one_run() {
        // नमस्ते = na + ma + s + te (with combining marks). All Devanagari +
        // Inherited; should coalesce.
        let text = "नमस्ते";
        let runs = itemize(text);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].script, Script::Devanagari);
    }

    #[test]
    fn punctuation_glues_to_surrounding_script() {
        let text = "abc, мир.";
        let runs = itemize(text);
        // Latin "abc, " then Cyrillic "мир." — comma+space adopts Latin,
        // period adopts Cyrillic.
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].script, Script::Latin);
        assert_eq!(runs[0].slice(text), "abc, ");
        assert_eq!(runs[1].script, Script::Cyrillic);
        assert_eq!(runs[1].slice(text), "мир.");
    }

    #[test]
    fn arabic_with_latin_splits() {
        let text = "Hello مرحبا";
        let runs = itemize(text);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].script, Script::Latin);
        assert_eq!(runs[1].script, Script::Arabic);
    }

    #[test]
    fn pure_punctuation_stays_common() {
        let runs = itemize("?!");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].script, Script::Common);
    }

    #[test]
    fn empty_string_yields_no_runs() {
        let runs = itemize("");
        assert!(runs.is_empty());
    }
}
