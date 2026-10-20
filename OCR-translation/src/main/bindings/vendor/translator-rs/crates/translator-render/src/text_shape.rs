//! Shared BiDi segmentation + OpenType shaping.
//!
//! Splits a string into runs that share both script and BiDi direction, and
//! shapes each run against a parsed `rustybuzz::Face` (Arabic joining, Indic
//! conjuncts, kerning, ligatures). The output glyphs are in *visual* order for
//! the run's direction, so a caller laying glyphs along a left-to-right cursor
//! gets correct ordering for RTL text.
//!
//! This is font-selection-agnostic: it takes a `Face` the caller already
//! resolved. The image renderer feeds it faces from its `FontProvider` chain;
//! the PDF overlay feeds it the embedded target font. Both share this module so
//! complex-script handling stays in one place.

use crate::text_runs::{ScriptRun, itemize};
use translator_core::script::Script;

use rustybuzz::{Direction, Face, UnicodeBuffer};
use unicode_bidi::{BidiInfo, Level};

#[derive(Debug, Clone)]
pub struct DirRun {
    /// Byte offsets into the source string.
    pub start: usize,
    pub end: usize,
    pub script: Script,
    pub rtl: bool,
    /// Position in the laid-out (visual) sequence. For LTR-only text this is
    /// the same as logical order.
    pub visual_index: usize,
}

/// Itemize `text` into runs that share both script and BiDi direction. The
/// returned vec is ordered logically; `visual_index` indicates the visual
/// order if a BiDi shuffle is needed.
pub fn segment_runs(text: &str) -> Vec<DirRun> {
    let bidi = BidiInfo::new(text, None);
    let script_runs = itemize(text);

    if bidi.paragraphs.is_empty() {
        return script_runs
            .into_iter()
            .enumerate()
            .map(|(i, r)| DirRun {
                start: r.start,
                end: r.end,
                script: r.script,
                rtl: r.script.is_rtl(),
                visual_index: i,
            })
            .collect();
    }

    let mut out: Vec<DirRun> = Vec::new();
    for para in &bidi.paragraphs {
        let para_range = para.range.clone();
        let para_runs: Vec<&ScriptRun> = script_runs
            .iter()
            .filter(|r| r.start < para_range.end && r.end > para_range.start)
            .collect();

        let mut split: Vec<DirRun> = Vec::new();
        for r in para_runs {
            let from = r.start.max(para_range.start);
            let to = r.end.min(para_range.end);
            split.extend(split_by_bidi_level(text, &bidi.levels, from, to, r.script));
        }

        // Reorder by visual position — unicode_bidi gives us the visual order
        // of the levels per paragraph.
        let (levels, level_runs) = bidi.visual_runs(para, para_range.clone());
        let mut visual_order: Vec<usize> = (0..split.len()).collect();
        visual_order.sort_by_key(|&i| {
            // find the visual index of this run's start byte.
            level_runs
                .iter()
                .position(|lr| lr.start <= split[i].start && split[i].start < lr.end)
                .unwrap_or(0)
        });
        let _ = levels;
        for (visual_index, logical_index) in visual_order.iter().enumerate() {
            let mut run = split[*logical_index].clone();
            run.visual_index = out.len() + visual_index;
            out.push(run);
        }
    }
    // Ensure logical order is by `start`; `visual_index` carries layout order.
    out.sort_by_key(|r| r.start);
    out
}

fn split_by_bidi_level(
    _text: &str,
    levels: &[Level],
    from: usize,
    to: usize,
    script: Script,
) -> Vec<DirRun> {
    if from >= to {
        return Vec::new();
    }
    let mut runs: Vec<DirRun> = Vec::new();
    let mut cursor = from;
    let mut current_level = levels.get(from).copied().unwrap_or_else(Level::ltr);
    for i in (from + 1)..to {
        let lvl = levels.get(i).copied().unwrap_or(current_level);
        if lvl != current_level {
            runs.push(DirRun {
                start: cursor,
                end: i,
                script,
                rtl: current_level.is_rtl(),
                visual_index: 0,
            });
            cursor = i;
            current_level = lvl;
        }
    }
    runs.push(DirRun {
        start: cursor,
        end: to,
        script,
        rtl: current_level.is_rtl(),
        visual_index: 0,
    });
    runs
}

#[derive(Debug, Clone, Copy)]
pub struct ShapedGlyph {
    /// Glyph ID in the font.
    pub gid: u16,
    /// Horizontal advance in font units.
    pub advance_x: i32,
    /// X offset from cursor in font units.
    pub offset_x: i32,
    /// Y offset from baseline in font units.
    pub offset_y: i32,
    /// Original cluster (byte offset in the run text) — kept for line breaking
    /// and source mapping.
    pub cluster: u32,
}

#[derive(Debug, Clone)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub units_per_em: i32,
    pub ascent: i32,
    pub descent: i32,
    pub rtl: bool,
    /// Byte offset of this run's first character in the source string passed to
    /// the shaper. Each glyph's `cluster` is relative to the run's slice; adding
    /// this gives the glyph's global byte position in the source.
    pub byte_start_in_text: usize,
}

/// Shape `run` (a byte range of `text`) against `face`. Returns glyphs in visual
/// order for the run's direction.
pub fn shape_run(text: &str, run: &DirRun, face: &Face) -> ShapedRun {
    let units_per_em = face.units_per_em() as i32;
    let ascent = face.ascender() as i32;
    let descent = face.descender() as i32;

    let mut buf = UnicodeBuffer::new();
    buf.push_str(&text[run.start..run.end]);
    buf.set_direction(if run.rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    buf.set_script(map_script(run.script));
    let glyph_buffer = rustybuzz::shape(face, &[], buf);
    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    for (info, pos) in infos.iter().zip(positions.iter()) {
        glyphs.push(ShapedGlyph {
            gid: info.glyph_id as u16,
            advance_x: pos.x_advance,
            offset_x: pos.x_offset,
            offset_y: pos.y_offset,
            cluster: info.cluster,
        });
    }

    ShapedRun {
        glyphs,
        units_per_em,
        ascent,
        descent,
        rtl: run.rtl,
        byte_start_in_text: run.start,
    }
}

pub fn map_script(s: Script) -> rustybuzz::Script {
    use rustybuzz::script;
    match s {
        Script::Latin => script::LATIN,
        Script::Cyrillic => script::CYRILLIC,
        Script::Greek => script::GREEK,
        Script::Armenian => script::ARMENIAN,
        Script::Hebrew => script::HEBREW,
        Script::Arabic => script::ARABIC,
        Script::Devanagari => script::DEVANAGARI,
        Script::Bengali => script::BENGALI,
        Script::Gurmukhi => script::GURMUKHI,
        Script::Gujarati => script::GUJARATI,
        Script::Oriya => script::ORIYA,
        Script::Tamil => script::TAMIL,
        Script::Telugu => script::TELUGU,
        Script::Kannada => script::KANNADA,
        Script::Malayalam => script::MALAYALAM,
        Script::Sinhala => script::SINHALA,
        Script::Thai => script::THAI,
        Script::Lao => script::LAO,
        Script::Tibetan => script::TIBETAN,
        Script::Myanmar => script::MYANMAR,
        Script::Georgian => script::GEORGIAN,
        Script::Ethiopic => script::ETHIOPIC,
        Script::Khmer => script::KHMER,
        Script::Han => script::HAN,
        Script::Hiragana => script::HIRAGANA,
        Script::Katakana => script::KATAKANA,
        Script::Hangul => script::HANGUL,
        Script::Common | Script::Inherited | Script::Other => script::COMMON,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arabic_face_bytes() -> Option<Vec<u8>> {
        for p in [
            "/usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf",
            "/home/david/Android/Sdk/platforms/android-28/data/fonts/NotoNaskhArabic-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansArabicUI-Regular.ttf",
        ] {
            if let Ok(b) = std::fs::read(p) {
                return Some(b);
            }
        }
        None
    }

    #[test]
    fn arabic_word_joins_and_reverses() {
        let Some(bytes) = arabic_face_bytes() else {
            eprintln!("no arabic font available; skipping");
            return;
        };
        let face = Face::from_slice(&bytes, 0).unwrap();
        // "مرحبا" (marhaba) — 5 letters, cursive joining.
        let text = "مرحبا";
        let runs = segment_runs(text);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].rtl, "arabic run must be RTL");
        let shaped = shape_run(text, &runs[0], &face);
        assert!(!shaped.glyphs.is_empty());
        // The first emitted glyph (leftmost, visual order) must correspond to
        // the logically-last character — i.e. clusters descend across output.
        let first = shaped.glyphs.first().unwrap().cluster;
        let last = shaped.glyphs.last().unwrap().cluster;
        assert!(
            first > last,
            "RTL shaping should emit glyphs in visual (reversed) order: first cluster {first} > last {last}"
        );
        // Joining: the shaped glyph ids differ from the isolated cmap glyphs.
        let isolated: Vec<u16> = text
            .chars()
            .map(|c| face.glyph_index(c).map(|g| g.0).unwrap_or(0))
            .collect();
        let shaped_gids: Vec<u16> = shaped.glyphs.iter().map(|g| g.gid).collect();
        assert_ne!(
            isolated, shaped_gids,
            "shaped (joined) gids must differ from isolated cmap gids"
        );
    }

    #[test]
    fn latin_run_is_ltr_source_order() {
        let Some(bytes) = arabic_face_bytes() else {
            return;
        };
        let face = Face::from_slice(&bytes, 0).unwrap();
        let text = "abc";
        let runs = segment_runs(text);
        let shaped = shape_run(text, &runs[0], &face);
        assert!(!runs[0].rtl);
        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        let mut sorted = clusters.clone();
        sorted.sort();
        assert_eq!(clusters, sorted, "LTR glyphs stay in source order");
    }
}
