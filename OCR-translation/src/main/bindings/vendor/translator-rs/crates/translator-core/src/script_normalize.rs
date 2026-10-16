//! Post-OCR repair for Cyrillic/Latin script confusion.
//!
//! PP-OCR's `cyrillic` and `eslav` recognizers can decode an ambiguous glyph as
//! the Latin twin (e.g. `A` instead of `А`, `O` instead of `О`) even when the
//! surrounding word is clearly Cyrillic. The leaked Latin codepoint breaks
//! downstream language detection and translation — "ОПAСНО" with a Latin `A`
//! looks like a junk mixed-script token instead of Russian "ОПАСНО".
//!
//! The heuristic here is intentionally narrow: per whitespace-separated word,
//! only rewrite Latin letters to their Cyrillic visual twins if the word
//! already contains at least one Cyrillic letter *and* every Latin letter in
//! the word has a known confusable mapping. Words that are entirely Latin —
//! or that mix Cyrillic with Latin letters that have no visual twin (e.g.
//! genuine product names) — are left untouched.
//!
//! Intentionally mixed-script signs ("WC-2", "VIP-зал") therefore survive: the
//! Latin run is not a confusable, or there is no Cyrillic neighbour in the
//! same word.

/// Latin → Cyrillic visual twins that PP-OCR routinely confuses on Cyrillic
/// strips. Kept short on purpose; this is not the full Unicode confusables
/// table.
const LATIN_TO_CYRILLIC: &[(char, char)] = &[
    ('A', 'А'),
    ('B', 'В'),
    ('C', 'С'),
    ('E', 'Е'),
    ('H', 'Н'),
    ('I', 'І'),
    ('J', 'Ј'),
    ('K', 'К'),
    ('M', 'М'),
    ('O', 'О'),
    ('P', 'Р'),
    ('S', 'Ѕ'),
    ('T', 'Т'),
    ('X', 'Х'),
    ('Y', 'У'),
    ('a', 'а'),
    ('c', 'с'),
    ('e', 'е'),
    ('i', 'і'),
    ('j', 'ј'),
    ('o', 'о'),
    ('p', 'р'),
    ('s', 'ѕ'),
    ('x', 'х'),
    ('y', 'у'),
];

fn latin_to_cyrillic(ch: char) -> Option<char> {
    LATIN_TO_CYRILLIC
        .iter()
        .find(|(latin, _)| *latin == ch)
        .map(|(_, cyrl)| *cyrl)
}

fn is_cyrillic_letter(ch: char) -> bool {
    matches!(ch as u32, 0x0400..=0x04FF | 0x0500..=0x052F)
}

/// Rewrite Latin confusables inside otherwise-Cyrillic words.
///
/// See module docs for the heuristic. Whitespace, punctuation, digits and any
/// non-letter characters pass through unchanged. The output preserves the
/// original whitespace layout exactly.
pub fn repair_cyrillic_word_mixing(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    for ch in text.chars() {
        if ch.is_whitespace() {
            push_repaired_word(&word, &mut out);
            word.clear();
            out.push(ch);
        } else {
            word.push(ch);
        }
    }
    push_repaired_word(&word, &mut out);
    out
}

fn push_repaired_word(word: &str, out: &mut String) {
    let mut has_cyrillic = false;
    let mut latin_count = 0usize;
    let mut non_confusable_latin = false;
    for ch in word.chars() {
        if is_cyrillic_letter(ch) {
            has_cyrillic = true;
        } else if ch.is_ascii_alphabetic() {
            latin_count += 1;
            if latin_to_cyrillic(ch).is_none() {
                non_confusable_latin = true;
            }
        }
    }
    if !has_cyrillic || latin_count == 0 || non_confusable_latin {
        out.push_str(word);
        return;
    }
    for ch in word.chars() {
        if ch.is_ascii_alphabetic() {
            if let Some(replacement) = latin_to_cyrillic(ch) {
                out.push(replacement);
                continue;
            }
        }
        out.push(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_single_latin_letter_in_cyrillic_word() {
        // Latin A and Latin BOT inside Cyrillic words.
        assert_eq!(
            repair_cyrillic_word_mixing("ОПAСНО ЗА ЖИBOTA"),
            "ОПАСНО ЗА ЖИВОТА"
        );
    }

    #[test]
    fn leaves_pure_cyrillic_untouched() {
        assert_eq!(
            repair_cyrillic_word_mixing("ВНИМАНИЕ ВИСОКО НАПРЕЖЕНИЕ"),
            "ВНИМАНИЕ ВИСОКО НАПРЕЖЕНИЕ"
        );
    }

    #[test]
    fn leaves_pure_latin_untouched() {
        assert_eq!(repair_cyrillic_word_mixing("HELLO WORLD"), "HELLO WORLD");
    }

    #[test]
    fn preserves_intentional_mixed_script_when_latin_run_has_non_confusable() {
        // "WD" is not a confusable run (W has no Cyrillic twin), so the
        // mixed-script word is left as-is.
        assert_eq!(repair_cyrillic_word_mixing("WD-зал"), "WD-зал");
    }

    #[test]
    fn does_not_touch_lone_latin_words_next_to_cyrillic_words() {
        assert_eq!(
            repair_cyrillic_word_mixing("купи iPhone сегодня"),
            "купи iPhone сегодня"
        );
    }

    #[test]
    fn preserves_whitespace_layout() {
        assert_eq!(
            repair_cyrillic_word_mixing("  ОПAСНО\tЗА\nЖИBOTA  "),
            "  ОПАСНО\tЗА\nЖИВОТА  "
        );
    }

    #[test]
    fn handles_lowercase_confusables() {
        // "опacно" with Latin 'a' and 'c'.
        assert_eq!(repair_cyrillic_word_mixing("опacно"), "опасно");
    }

    #[test]
    fn leaves_punctuation_alone() {
        assert_eq!(repair_cyrillic_word_mixing("«ОПAСНО!»"), "«ОПАСНО!»");
    }
}
