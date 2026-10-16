//! Lightweight sentence splitter — replaces slimt's PCRE2-backed Splitter.
//!
//! Splits on terminal punctuation (`.`, `!`, `?`, including ellipses) when
//! followed by whitespace and a Unicode uppercase letter — optionally with an
//! opening delimiter (`"`, `“`, `(`, `¿`, `«`, …) in front of that letter, so
//! a new sentence that opens with quoted dialogue still splits (`bed. “In
//! judging…”`). This matches ssplit-cpp's regex-fallback heuristic (the path it
//! takes when no abbreviation prefix file is loaded), which works well on
//! modern web / ebook prose, plus a curated nonbreaking-prefix list so titles
//! and initials ("Mr. Smith", "J. K. Rowling") don't split — a bare "Mr."
//! fragment fed to the es model comes back as "¿Sr.", corrupting mundane
//! sentences. The split layer used to live in slimt and pulled PCRE2 into the
//! build (~1 MB binary, 13 MB source); moving it here removes both.
//!
//! The splitter returns `&str` slices borrowed from the input. Callers
//! recover each sentence's byte offset via pointer arithmetic
//! (`slice.as_ptr() - input.as_ptr()`); the alignment-stitching code in
//! `BergamotEngine` uses that to map per-sentence alignments back into
//! the original input's coordinate space.

use std::sync::OnceLock;

use regex::Regex;

fn boundary_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // One or more of `.`, `!`, `?` (handles ellipses and `!?` mixes),
        // optionally followed by *closing* delimiters that belong to the
        // sentence just ended (`wake!”`, `(done.)`), then whitespace, then the
        // start of the next sentence: any run of *opening* delimiters
        // (straight/curly quotes, parens, brackets, Spanish `¿`/`¡`,
        // guillemets) and then the first non-space character (group 3).
        //
        // Whether group 3 must be uppercase is decided in `split_sentences`,
        // not here: `!`/`?` split regardless of case (casual lowercase input
        // like "how are you? i'm fine" is common), while a lone `.`/ellipsis
        // stays guarded by an uppercase requirement against false splits on
        // the overloaded `.` (decimals `3.14`, `e.g.`, `(or so I thought)`).
        // The delimiter runs only let quotes/brackets sit around the boundary.
        // Group 1 ends after the closing delimiters (they stay with the current
        // sentence); group 2 is the opening-delimiter run and group 3 the first
        // real character — the next sentence begins at group 2's start.
        Regex::new(r#"([.!?।॥։۔؟።፧]+["'”’)\]»]*)\s+(["'“‘(\[¿¡«]*)(\S)"#)
            .expect("valid sentence-boundary regex")
    })
}

/// Words that end with a period mid-sentence (titles, street/legal
/// abbreviations), trimmed from the Moses/ssplit nonbreaking-prefix lists
/// for the app's source languages to forms that realistically precede a
/// capitalized word. Case-sensitive; matched against the word immediately
/// before a lone `.` boundary. Single uppercase letters (initials,
/// "J. K. Rowling") are handled in `is_nonbreaking_prefix` instead of
/// being enumerated here.
#[rustfmt::skip]
static NONBREAKING_PREFIXES: &[&str] = &[
    // en titles / ranks
    "Mr", "Mrs", "Ms", "Dr", "Drs", "Prof", "Rev", "Hon", "St", "Ste", "Fr",
    "Pres", "Gov", "Sen", "Rep", "Gen", "Col", "Maj", "Capt", "Cmdr", "Lt",
    "Sgt", "Cpl", "Pvt", "Adm", "Messrs", "Jr", "Sr", "Bros",
    // en misc
    "vs", "v", "Mt", "Ft", "Ave", "Blvd", "Rd", "Dept", "Univ", "Inc", "Ltd",
    "Corp", "Co", "Est", "Ph.D", "M.D", "B.A", "M.A", "D.C", "e.g", "i.e",
    // es
    "Sra", "Srta", "Ud", "Uds", "Vd", "Vds", "Dña", "Excmo", "Ilmo", "Avda",
    // fr
    "MM", "Mme", "Mmes", "Mlle", "Mlles", "Me", "Mgr",
    // de
    "Hr", "Frau", "Nr", "Str", "z.B", "bzw", "usw", "ca",
    // pl (lowercase forms commonly precede capitalized names: "ul. Krakowska")
    "ul", "al", "prof", "dr", "mgr", "inż", "hab", "im", "św", "ks", "płk",
    // ka — needed now that Georgian splits at `.` at all; without these,
    // "ელ. ფოსტა" (email) is cut in half.
    "ე.წ", "ელ", "მაგ", "იხ", "წმ", "დაახლ", "თ.წ", "ე.ი",
    "ძვ", "მდ", "გვ", "სთ",
];

// Scripts that do not mark sentence starts with a capital. The `.` boundary
// guard below requires an uppercase opener, which is a Latin/Cyrillic/Greek
// assumption: applied to these scripts it silently disables period-splitting
// altogether, so a whole paragraph reaches a sentence-trained model as one
// unit. Georgian is listed even though Unicode gives Mkhedruli lowercase
// status with an uppercase mapping into Mtavruli (U+1C90..U+1CBF) -- that
// mapping is for all-caps display only and never appears sentence-initially,
// so `char::is_uppercase` is false for every letter of ordinary Georgian prose.
#[rustfmt::skip]
static UNICAMERAL_RANGES: &[(char, char)] = &[
    ('\u{10A0}', '\u{10FF}'), ('\u{1C90}', '\u{1CBF}'), ('\u{2D00}', '\u{2D2F}'),
    ('\u{0590}', '\u{05FF}'), ('\u{FB1D}', '\u{FB4F}'),
    ('\u{0600}', '\u{06FF}'), ('\u{0750}', '\u{077F}'), ('\u{08A0}', '\u{08FF}'),
    ('\u{FB50}', '\u{FDFF}'), ('\u{FE70}', '\u{FEFF}'),
    ('\u{0700}', '\u{074F}'), ('\u{0780}', '\u{07BF}'),
    ('\u{0900}', '\u{097F}'), ('\u{0980}', '\u{09FF}'), ('\u{0A00}', '\u{0A7F}'),
    ('\u{0A80}', '\u{0AFF}'), ('\u{0B00}', '\u{0B7F}'), ('\u{0B80}', '\u{0BFF}'),
    ('\u{0C00}', '\u{0C7F}'), ('\u{0C80}', '\u{0CFF}'), ('\u{0D00}', '\u{0D7F}'),
    ('\u{0D80}', '\u{0DFF}'),
    ('\u{0E00}', '\u{0E7F}'), ('\u{0E80}', '\u{0EFF}'), ('\u{0F00}', '\u{0FFF}'),
    ('\u{1000}', '\u{109F}'), ('\u{1200}', '\u{137F}'), ('\u{1780}', '\u{17FF}'),
    ('\u{1100}', '\u{11FF}'), ('\u{3040}', '\u{30FF}'), ('\u{3400}', '\u{4DBF}'),
    ('\u{4E00}', '\u{9FFF}'), ('\u{AC00}', '\u{D7AF}'), ('\u{F900}', '\u{FAFF}'),
];

fn is_unicameral(c: char) -> bool {
    UNICAMERAL_RANGES.iter().any(|&(lo, hi)| c >= lo && c <= hi)
}

fn is_nonbreaking_prefix(before_period: &str) -> bool {
    let word = before_period
        .rsplit(char::is_whitespace)
        .next()
        .unwrap_or("")
        .trim_start_matches(|c: char| !c.is_alphanumeric());
    if word.is_empty() {
        return false;
    }
    let mut chars = word.chars();
    if let (Some(first), None) = (chars.next(), chars.next()) {
        // A lone letter before a period is an initial. `is_uppercase` alone misses
        // this for every caseless script, splitting a Georgian name mid-way at its
        // initial -- the same assumption the boundary guard above had to drop.
        if first.is_uppercase() || is_unicameral(first) {
            return true;
        }
    }
    NONBREAKING_PREFIXES.contains(&word)
}

/// Split `text` into sentence-sized substrings, each ending after its
/// terminal punctuation. Inter-sentence whitespace is dropped from the
/// output (the boundary point is where the next sentence's first
/// uppercase letter begins). Returns an empty vec if `text` is empty
/// or whitespace-only; otherwise returns at least one slice.
pub fn split_sentences(text: &str) -> Vec<&str> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut last_end = 0usize;
    for caps in boundary_regex().captures_iter(text) {
        let punct = caps.get(1).expect("group 1 always present");
        let openers = caps.get(2).expect("group 2 always present");
        let first = caps.get(3).expect("group 3 always present");
        // `!` and `?` are unambiguous terminators: split regardless of the next
        // sentence's case ("how are you? i'm fine" — casual lowercase input is
        // common and must split). A lone `.`/ellipsis stays conservative: only
        // split before an uppercase letter (guards decimals `3.14`, `e.g.`,
        // lowercase parentheticals `(or so I thought)`) and never after a
        // title/initial abbreviation.
        let strong = punct
            .as_str()
            .chars()
            .any(|c| matches!(c, '!' | '?' | '\u{061F}'));
        if !strong {
            if punct.as_str() == "." && is_nonbreaking_prefix(&text[..punct.start()]) {
                continue;
            }
            let first = first
                .as_str()
                .chars()
                .next()
                .expect("group 3 is one character");
            if !is_unicameral(first) && !first.is_uppercase() {
                continue;
            }
        }
        let piece = &text[last_end..punct.end()];
        if !piece.trim().is_empty() {
            out.push(piece);
        }
        last_end = openers.start();
    }
    if last_end < text.len() {
        let tail = &text[last_end..];
        if !tail.trim().is_empty() {
            out.push(tail);
        }
    }
    if out.is_empty() {
        out.push(text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte_offset(slice: &str, original: &str) -> usize {
        let base = original.as_ptr() as usize;
        let here = slice.as_ptr() as usize;
        here - base
    }

    #[test]
    fn empty_returns_empty() {
        assert!(split_sentences("").is_empty());
        assert!(split_sentences("   \n  ").is_empty());
    }

    #[test]
    fn single_sentence_round_trips() {
        let s = "Hello world.";
        assert_eq!(split_sentences(s), vec![s]);
    }

    #[test]
    fn paragraph_with_three_sentences() {
        let p = "The cat eats fish. Hello world. Done.";
        let parts = split_sentences(p);
        assert_eq!(parts.len(), 3);
        // Slices retain their byte offsets within the original string;
        // the alignment-combine logic relies on this.
        assert_eq!(byte_offset(parts[0], p), 0);
        assert_eq!(parts[0], "The cat eats fish.");
        assert_eq!(parts[1], "Hello world.");
        assert_eq!(parts[2], "Done.");
        // The space between sentences is dropped from output.
        assert_eq!(
            byte_offset(parts[1], p),
            "The cat eats fish. ".len(),
            "second sentence must start where the original 'Hello' is"
        );
    }

    #[test]
    fn ellipsis_counts_as_one_boundary() {
        // "..." followed by whitespace + Capital is one boundary, not three.
        let p = "Wait... Then it happened.";
        let parts = split_sentences(p);
        assert_eq!(parts, vec!["Wait...", "Then it happened."]);
    }

    #[test]
    fn mixed_terminal_punctuation() {
        let p = "Really?! No way. Yes.";
        let parts = split_sentences(p);
        assert_eq!(parts, vec!["Really?!", "No way.", "Yes."]);
    }

    #[test]
    fn lowercase_after_bang_or_question_splits() {
        // `?` / `!` are unambiguous terminators — split even before a lowercase
        // next word (casual typing: "how are you? i'm fine").
        assert_eq!(
            split_sentences("hello, how are you? i am fine, thanks."),
            vec!["hello, how are you?", "i am fine, thanks."]
        );
        assert_eq!(
            split_sentences("wait! it works now."),
            vec!["wait!", "it works now."]
        );
        // A lone `.` / ellipsis stays conservative: lowercase next → no split.
        assert_eq!(
            split_sentences("wait. it happened."),
            vec!["wait. it happened."]
        );
        assert_eq!(
            split_sentences("wait... it happened."),
            vec!["wait... it happened."]
        );
    }

    #[test]
    fn unicode_uppercase_starts_new_sentence() {
        // Spanish-style accented uppercase. Without `\p{Lu}` matching, the
        // regex would miss boundaries before words starting with Á/É.
        let p = "Hola mundo. Él vino. Última cosa.";
        let parts = split_sentences(p);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], "Él vino.");
        assert_eq!(parts[2], "Última cosa.");
    }

    #[test]
    fn lowercase_after_period_does_not_split() {
        // "e.g." inside a sentence shouldn't trigger a split because the
        // following character is lowercase. We don't try to handle the
        // "Mr. Smith" case (capital after) — that's the known limitation.
        let p = "We use abbreviations e.g. like this.";
        assert_eq!(split_sentences(p), vec![p]);
    }

    #[test]
    fn trailing_punctuation_no_capital_keeps_one_sentence() {
        let p = "He said hi. ok";
        // No capital after the period, so no split.
        assert_eq!(split_sentences(p), vec![p]);
    }

    #[test]
    fn unterminated_tail_returned() {
        // Last sentence has no terminal punctuation — still gets emitted.
        let p = "First. Second";
        assert_eq!(split_sentences(p), vec!["First.", "Second"]);
    }

    #[test]
    fn title_abbreviations_do_not_split() {
        let p = "Mr. Smith arrived.";
        assert_eq!(split_sentences(p), vec![p]);
        let p = "Dr. Brzezinski prescribed amoxicillin for the laryngitis.";
        assert_eq!(split_sentences(p), vec![p]);
        let p = "El Sr. García llegó tarde.";
        assert_eq!(split_sentences(p), vec![p]);
        let p = "Mieszka przy ul. Krakowskiej.";
        assert_eq!(split_sentences(p), vec![p]);
    }

    #[test]
    fn initials_do_not_split() {
        let p = "J. K. Rowling wrote it.";
        assert_eq!(split_sentences(p), vec![p]);
    }

    #[test]
    fn sentence_ending_before_abbreviation_still_splits() {
        let p = "He met Mr. Smith. Then he left.";
        assert_eq!(
            split_sentences(p),
            vec!["He met Mr. Smith.", "Then he left."]
        );
    }

    #[test]
    fn known_limitation_single_letter_sentence_end() {
        // The initials rule absorbs a sentence genuinely ending in a single
        // capital letter; "plan B. Next" reads as an initial. Accepted
        // tradeoff, same as the Moses/ssplit prefix lists.
        let p = "We go with plan B. Next question.";
        assert_eq!(split_sentences(p), vec![p]);
    }

    #[test]
    fn abbreviation_with_question_mark_still_splits() {
        // Only a lone `.` is guarded; other terminals end the sentence even
        // after a prefix-looking word.
        let p = "Did you call Mr.? Yes.";
        assert_eq!(split_sentences(p), vec!["Did you call Mr.?", "Yes."]);
    }

    #[test]
    fn quoted_dialogue_after_period_splits() {
        // The dialogue case that previously merged: an opening curly quote
        // sits between the period and the capital. The quote joins the new
        // sentence.
        let p = "He toasts for bed. “In judging of that wind,” he said.";
        let parts = split_sentences(p);
        assert_eq!(
            parts,
            vec!["He toasts for bed.", "“In judging of that wind,” he said."]
        );
        assert!(
            parts[1].starts_with('“'),
            "quote belongs to the new sentence"
        );
    }

    #[test]
    fn bang_then_quote_splits() {
        // Splits twice: at the opening quote, and again after the closing
        // quote — the bracketed exclamation is its own sentence.
        let p = "with a tomahawk! “Queequeg!” At length he woke.";
        assert_eq!(
            split_sentences(p),
            vec!["with a tomahawk!", "“Queequeg!”", "At length he woke."]
        );
    }

    #[test]
    fn closing_quote_then_capital_splits() {
        // Terminal punctuation inside a closing quote (`wake!” At…`): the
        // close-quote stays with the sentence that ended.
        let p = "“Queequeg, wake!” At length he stirred.";
        let parts = split_sentences(p);
        assert_eq!(parts, vec!["“Queequeg, wake!”", "At length he stirred."]);
        assert!(
            parts[0].ends_with('”'),
            "closing quote stays with its sentence"
        );
    }

    #[test]
    fn straight_quote_and_paren_openers_split() {
        assert_eq!(
            split_sentences("Done. \"Next one starts here.\""),
            vec!["Done.", "\"Next one starts here.\""]
        );
        assert_eq!(
            split_sentences("He left. (See the footnote.)"),
            vec!["He left.", "(See the footnote.)"]
        );
    }

    #[test]
    fn spanish_inverted_opener_splits() {
        // `¿`/`¡` precede the capital in Spanish; both should still split.
        assert_eq!(
            split_sentences("Dijo algo. ¿Vienes conmigo?"),
            vec!["Dijo algo.", "¿Vienes conmigo?"]
        );
    }

    #[test]
    fn lowercase_opener_does_not_split() {
        // The reason the boundary is "openers* THEN capital" rather than
        // "capital OR punctuation": a delimiter followed by lowercase is a
        // mid-sentence aside, not a new sentence. A plain `\p{Punct}` branch
        // would wrongly split all of these.
        assert_eq!(
            split_sentences("I saw it. (or so I thought) and moved on."),
            vec!["I saw it. (or so I thought) and moved on."]
        );
        assert_eq!(
            split_sentences("She paused. \"and then nothing\""),
            vec!["She paused. \"and then nothing\""]
        );
    }

    #[test]
    fn caseless_scripts_split_on_period() {
        // These have no uppercase, so an is_uppercase() opener check silently
        // disables period-splitting and hands the model a whole paragraph.
        for (script, text) in [
            (
                "ka",
                "კარი დაკეტილია. გთხოვთ გამოიყენოთ გვერდითი შესასვლელი.",
            ),
            ("he", "הדלת סגורה. אנא השתמש בכניסה הצדדית."),
            ("ar", "الباب مغلق. الرجاء استخدام المدخل الجانبي."),
            ("bn", "দরজা বন্ধ. পাশের প্রবেশপথ ব্যবহার করুন."),
            ("th", "ประตูปิดอยู่. กรุณาใช้ทางเข้าด้านข้าง."),
        ] {
            assert_eq!(split_sentences(text).len(), 2, "{script} must split");
        }
    }

    #[test]
    fn non_latin_terminators_split() {
        assert_eq!(split_sentences("दरवाज़ा बंद है। कृपया आएं।").len(), 2);
        assert_eq!(split_sentences("أين هو؟ إنه هنا.").len(), 2);
    }

    #[test]
    fn georgian_abbreviations_do_not_split() {
        assert_eq!(
            split_sentences("ეს არის ელ. ფოსტა."),
            vec!["ეს არის ელ. ფოსტა."]
        );
        assert_eq!(
            split_sentences("მაგ. წყალი და პური."),
            vec!["მაგ. წყალი და პური."]
        );
        assert_eq!(
            split_sentences("ეს არის ე.წ. პრობლემა. მეორე წინადადება.").len(),
            2
        );
    }

    #[test]
    fn cased_scripts_keep_the_uppercase_guard() {
        // The guard must still suppress the Latin false positives it was
        // written for; unicameral handling is additive, not a relaxation.
        assert_eq!(split_sentences("The value is 3.14 and no more.").len(), 1);
        assert_eq!(split_sentences("Bring gear, e.g. rope and boots.").len(), 1);
        assert_eq!(split_sentences("He left. (or so I thought)").len(), 1);
        assert_eq!(split_sentences("It was J. K. Rowling.").len(), 1);
    }

    #[test]
    fn latin_eg_ie_do_not_split_before_a_capital() {
        // The uppercase guard hides this only while the next word is lowercase;
        // "e.g. Paris" broke mid-sentence for every language pair.
        assert_eq!(
            split_sentences("Visit a city, e.g. Paris in spring.").len(),
            1
        );
        assert_eq!(
            split_sentences("The result, i.e. Total revenue, fell.").len(),
            1
        );
    }

    #[test]
    fn georgian_etc_terminates_a_sentence() {
        // `ა.შ.` is "and so on"; like the deliberately-absent Latin `etc`, it ends
        // a sentence, and suppressing the break desynchronises it from English.
        assert_eq!(
            split_sentences("წიგნები, ჟურნალები და ა.შ. შემდეგი წინადადება.").len(),
            2
        );
    }

    #[test]
    fn caseless_initials_and_abbreviations_do_not_split() {
        // `is_uppercase` is false for every letter of a caseless script, so the
        // lone-initial rule never fired and Georgian names split mid-way.
        for text in [
            "სტენლი ვ. კუბრიკი იყო რეჟისორი.",
            "ძვ. წ. 500 წელს დაარსდა ქალაქი.",
            "ქ. თბილისი დედაქალაქია.",
            "მდ. მტკვარი კვეთს ქალაქს.",
            "იხ. გვ. 42 დამატებითი ინფორმაციისთვის.",
        ] {
            assert_eq!(split_sentences(text).len(), 1, "must not split: {text}");
        }
        assert_eq!(
            split_sentences("წიგნი დაიწერა 1990 წელს. ავტორი გარდაიცვალა.").len(),
            2
        );
    }
}
