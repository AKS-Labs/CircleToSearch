//! Extract text + bounding boxes from a PDF via mupdf's stext API. mupdf
//! reports per-character colour, bold flag and font on every emitted glyph,
//! which is enough to split a line into runs of consecutive same-style
//! characters in a single pass — no second content-stream parser needed.

use std::collections::HashMap;

use mupdf::text_page::TextBlockType;
use mupdf::{Document, Page, TextCharFlags, TextPageFlags};

use crate::pdf::{PageDims, PdfError};
use crate::styled::{StyledFragment, TextStyle};
use translator_core::ocr::Rect;

#[derive(Debug, Clone)]
pub struct PageTextFragments {
    pub page_index: usize,
    pub page: PageDims,
    pub fragments: Vec<StyledFragment>,
}

const STEXT_FLAGS: TextPageFlags = TextPageFlags::from_bits_truncate(
    TextPageFlags::PRESERVE_LIGATURES.bits()
        | TextPageFlags::PRESERVE_WHITESPACE.bits()
        | TextPageFlags::DEHYPHENATE.bits()
        | TextPageFlags::ACCURATE_BBOXES.bits(),
);

pub fn extract_text(pdf_bytes: &[u8]) -> Result<Vec<PageTextFragments>, PdfError> {
    let document = Document::from_bytes(pdf_bytes, "application/pdf")?;
    let page_count = document.page_count()?;
    let mut pages = Vec::with_capacity(page_count as usize);
    for i in 0..page_count {
        let page = document.load_page(i)?;
        pages.push(extract_page(&page, i as usize)?);
    }
    Ok(pages)
}

fn extract_page(page: &Page, page_index: usize) -> Result<PageTextFragments, PdfError> {
    let bounds = page.bounds()?;
    let dims = PageDims {
        width_pts: bounds.x1 - bounds.x0,
        height_pts: bounds.y1 - bounds.y0,
    };

    let stext = page.to_text_page(STEXT_FLAGS)?;
    // Cache italic + bold lookups per font name. fz_font_name is stable
    // per font and calling fz_font_is_italic / fz_font_is_bold per char
    // is wasteful when a paragraph reuses the same handful of fonts.
    //
    // Bold detection has to go through `Font::is_bold` rather than the
    // per-char `TextCharFlags::BOLD` flag: mupdf reserves that flag for
    // *synthetic* bold (drawn via double-stroke), so a producer that
    // names its bold font `Times-Bold` (or carries a `Bold`
    // FontDescriptor flag) but emits real glyphs leaves the per-char
    // flag at 0. mupdf's own HTML output uses the same `fz_font_is_bold`
    // path; without it our headings round-trip as regular weight.
    let mut italic_by_font: HashMap<String, bool> = HashMap::new();
    let mut bold_by_font: HashMap<String, bool> = HashMap::new();

    // First pass: collect every non-empty line, with an upfront math-class
    // classification per line. We cannot finalise the `opaque` flag yet —
    // mupdf reports inline subscripts/superscripts (a single CMMI `i`, a big
    // CMEX paren) as their own one-char "lines" because they sit at a
    // different baseline than the surrounding glyphs, and a 1-char
    // math-class line in isolation is just an inline glyph belonging to
    // prose, not a display equation. So we defer the decision until we can
    // see the whole sequence and only mark Weak math runs opaque when they
    // sit inside a contiguous run that includes at least one Strong (≥3
    // math-class chars) line — i.e. an actual display equation.
    let mut pre_lines: Vec<PreLine> = Vec::new();
    for (block_index, block) in stext
        .blocks()
        .filter(|b| matches!(b.r#type(), TextBlockType::Text))
        .enumerate()
    {
        for line in block.lines() {
            let line_rect = rect_from_mupdf(line.bounds());
            let mut typed_chars: Vec<TypedChar> = Vec::new();
            for ch in line.chars() {
                let Some(c) = ch.char() else {
                    continue;
                };
                let font = ch.font();
                let font_name = font
                    .as_ref()
                    .map(|f| f.name().to_string())
                    .unwrap_or_default();
                let italic = font
                    .as_ref()
                    .map(|f| {
                        *italic_by_font
                            .entry(font_name.clone())
                            .or_insert_with(|| f.is_italic())
                    })
                    .unwrap_or(false);
                let font_bold = font
                    .as_ref()
                    .map(|f| {
                        *bold_by_font
                            .entry(font_name.clone())
                            .or_insert_with(|| f.is_bold())
                    })
                    .unwrap_or(false);
                typed_chars.push(TypedChar {
                    c,
                    x: ch.origin().x,
                    y: ch.origin().y,
                    bbox: rect_from_quad(ch.quad()),
                    font_size: ch.size(),
                    style: PdfCharStyle {
                        bold: font_bold || ch.flags().contains(TextCharFlags::BOLD),
                        italic,
                        fill_argb: ch.argb(),
                        text_size: None,
                        baseline_shift: None,
                    },
                    font_name,
                });
            }
            if typed_chars.iter().all(|tc| tc.c.is_whitespace()) {
                continue;
            }
            let math_kind = classify_math_line(&typed_chars);
            pre_lines.push(PreLine {
                block_index,
                typed_chars,
                line_rect,
                math_kind,
            });
        }
    }

    // Second pass: walk pre_lines and mark a math-class line opaque iff its
    // contiguous run of math-class lines (broken by Prose) contains at least
    // one Strong line. This keeps lone subscripts (mupdf splits a `τ qc i +
    // δ` paragraph into the prose body + a one-char `i` line + more prose)
    // tied to the surrounding prose, while still tagging the multi-line
    // display equation on page 7 (`Bk ←` + big-paren `(` + `bk, P(Bk−1)`
    // + big-paren `)` + trailing `,`) as a single opaque run.
    let mut opaque_flags = compute_opaque_flags(&pre_lines);
    // Lines whose Unicode mapping is entirely U+FFFD (replacement char,
    // i.e. mupdf couldn't decode the glyph back to a codepoint) get
    // marked opaque so the writer preserves the original text-show
    // program. Bullet glyphs from FM-style PDFs land here: their font
    // emits a glyph code with no /ToUnicode entry, so mupdf returns
    // U+FFFD, bergamot strips it on translation, and the bullet
    // disappears. With opaque=true the writer reuses the captured
    // glyph program at the original position.
    for (i, pre) in pre_lines.iter().enumerate() {
        if !pre.typed_chars.is_empty() && pre.typed_chars.iter().all(|tc| tc.c == '\u{fffd}') {
            opaque_flags[i] = true;
        }
    }

    // Third pass: drop non-opaque lines that look like display math under the
    // text-only heuristic (catch-all for non-TeX producers), then emit
    // fragments. `group_id` bumps on every mupdf-block boundary AND on every
    // opaqueness transition so opaque math gets its own TranslatableBlock:
    // the writer can preserve its original PDF text-show program, while the
    // prose around it is translated and rendered normally.
    let mut fragments = Vec::new();
    let mut group_id: u32 = 0;
    let mut prev_block_index: Option<usize> = None;
    let mut prev_opaque: Option<bool> = None;
    for (i, pre) in pre_lines.iter().enumerate() {
        let opaque = opaque_flags[i];
        if !opaque {
            let line_text: String = pre.typed_chars.iter().map(|tc| tc.c).collect();
            if should_skip_pdf_line(&line_text, pre.line_rect, dims) {
                continue;
            }
        }
        let block_changed = prev_block_index.is_some_and(|prev| prev != pre.block_index);
        let opaque_changed = prev_opaque.is_some_and(|prev| prev != opaque);
        if block_changed || opaque_changed {
            group_id += 1;
        }
        prev_block_index = Some(pre.block_index);
        prev_opaque = Some(opaque);

        for run in split_line_by_style(&pre.typed_chars, pre.line_rect) {
            if run.text.trim().is_empty() {
                continue;
            }
            fragments.push(StyledFragment {
                text: run.text,
                bounding_box: run.bbox,
                style: Some(run.style.into()),
                layout_group: 0,
                translation_group: group_id,
                cluster_group: group_id,
                opaque,
            });
        }
    }

    Ok(PageTextFragments {
        page_index,
        page: dims,
        fragments,
    })
}

struct PreLine {
    block_index: usize,
    typed_chars: Vec<TypedChar>,
    line_rect: Rect,
    math_kind: MathKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MathKind {
    /// No math-class chars, or math is a minority alongside text-class
    /// chars. Treated like prose for opaqueness.
    Prose,
    /// At least 3 chars and a strict majority are math-class — a real
    /// display equation fragment. Always opaque on its own.
    Strong,
    /// Math-class chars only (no text-class chars), but too few to be a
    /// Strong line on its own. Could be a one-char inline subscript that
    /// mupdf split out, or a big delimiter from CMEX that's part of a
    /// larger display equation. Becomes opaque only when adjacent (within
    /// a contiguous non-Prose run) to a Strong line.
    Weak,
}

fn classify_math_line(chars: &[TypedChar]) -> MathKind {
    let mut math = 0usize;
    let mut text = 0usize;
    for tc in chars {
        if tc.c.is_whitespace() {
            continue;
        }
        if is_extensible_math_replacement(tc) {
            // MuPDF cannot map some TeX extensible delimiters (large
            // parentheses/brackets from CMEX) to Unicode, so it reports
            // U+FFFD. Redrawing that text can only reproduce the replacement
            // character; keep the original PDF glyphs instead.
            return MathKind::Strong;
        }
        if is_math_font(&tc.font_name) {
            math += 1;
        } else if is_text_font(&tc.font_name) {
            text += 1;
        }
        // Unknown font families don't vote either way.
    }
    if math == 0 {
        return MathKind::Prose;
    }
    let counted = math + text;
    // Strong iff math chars make up at least half of the voting chars.
    // The previous 70% threshold rejected fraction-numerator lines like
    // `(τ_2 − τ_1)` where the digits/parens land in CMR (text) even
    // though the line is unambiguously part of a display equation, and
    // 50/50 lines like `−2 = Ω` (where `2`, `=` are CMR but `−`, `Ω`
    // are math) — both showed up as Prose. A 50% floor catches those
    // while still keeping ordinary prose (where `Lemma 21`-style
    // references contribute one inline math char against many text
    // ones) classified as Prose.
    if counted >= 3 && math * 2 >= counted {
        return MathKind::Strong;
    }
    if text == 0 {
        return MathKind::Weak;
    }
    MathKind::Prose
}

fn compute_opaque_flags(pre_lines: &[PreLine]) -> Vec<bool> {
    let mut flags = vec![false; pre_lines.len()];
    if pre_lines.is_empty() {
        return flags;
    }
    // Walk the lines one mupdf-block at a time and find each contiguous
    // math run (consecutive Strong/Weak lines, broken by Prose). A run is
    // marked opaque iff it contains at least one Strong line. That keeps
    // a lone subscript line (a single CMMI `i` mupdf split out from
    // surrounding prose) bound to its prose neighbours — but tags a real
    // multi-line display equation (or an inline `Ω(τ_2-τ_1)/δ` whose
    // numerator/denominator/big-paren chunks span several mupdf "lines"
    // at slightly offset baselines) as one opaque unit. The per-block
    // boundaries matter because group_id later flips on every mupdf-block
    // and every opaqueness change, so a math run inside a prose paragraph
    // ends up as its own TranslatableBlock with the prose runs around it
    // becoming their own — and the writer's cursor-coordination logic
    // chains them back together on the visual row.
    let mut block_start = 0;
    while block_start < pre_lines.len() {
        let block_idx = pre_lines[block_start].block_index;
        let mut block_end = block_start + 1;
        while block_end < pre_lines.len() && pre_lines[block_end].block_index == block_idx {
            block_end += 1;
        }
        let mut i = block_start;
        while i < block_end {
            if matches!(pre_lines[i].math_kind, MathKind::Prose) {
                i += 1;
                continue;
            }
            let run_start = i;
            let mut has_strong = false;
            while i < block_end && !matches!(pre_lines[i].math_kind, MathKind::Prose) {
                if matches!(pre_lines[i].math_kind, MathKind::Strong) {
                    has_strong = true;
                }
                i += 1;
            }
            if has_strong {
                for j in run_start..i {
                    flags[j] = true;
                }
            }
        }
        block_start = block_end;
    }
    flags
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PdfCharStyle {
    bold: bool,
    italic: bool,
    fill_argb: u32,
    text_size: Option<f32>,
    baseline_shift: Option<f32>,
}

impl From<PdfCharStyle> for TextStyle {
    fn from(s: PdfCharStyle) -> Self {
        TextStyle {
            text_color: Some(s.fill_argb),
            bg_color: None,
            text_size: s.text_size,
            baseline_shift: s.baseline_shift,
            bold: s.bold,
            italic: s.italic,
            underline: false,
            strikethrough: false,
        }
    }
}

struct TypedChar {
    c: char,
    x: f32,
    y: f32,
    bbox: Rect,
    font_size: f32,
    style: PdfCharStyle,
    /// mupdf-reported font name. We look at this to decide whether the line
    /// is display math (drawn in a math-class font like CMSY/CMMI) versus
    /// prose. Cheap to populate — we already have the font handle in scope.
    font_name: String,
}

#[derive(Debug)]
struct LineRun {
    text: String,
    style: PdfCharStyle,
    bbox: Rect,
}

/// Group consecutive chars into runs of identical style. Whitespace is
/// treated as style-neutral: it inherits whichever run it falls into and
/// never triggers a transition. That way the trailing space of "bold word "
/// stays inside the bold run instead of starting a new one — and downstream
/// concatenation in `build_block` sees the space and joins fragments
/// correctly.
fn split_line_by_style(chars: &[TypedChar], line_rect: Rect) -> Vec<LineRun> {
    let mut runs: Vec<LineRun> = Vec::new();
    let mut run_text = String::new();
    let mut run_start_x: Option<f32> = None;
    let mut run_end_x: f32 = line_rect.left as f32;
    let mut run_style: Option<PdfCharStyle> = None;
    let script_context = ScriptContext::for_line(chars);

    for tc in chars {
        let is_ws = tc.c.is_whitespace();
        let script = script_context.kind_for(tc);
        let effective_style = script_context.style_for(tc, script);
        let style_changed = !is_ws && run_style.is_some_and(|s| s != effective_style);
        if style_changed && !run_text.is_empty() {
            runs.push(finish_run(
                &run_text,
                run_style.expect("run_style set when run_text non-empty"),
                run_start_x,
                tc.x,
                line_rect,
            ));
            run_text.clear();
            run_start_x = None;
        }
        if run_start_x.is_none() {
            run_start_x = Some(tc.x);
        }
        run_end_x = run_end_x.max(tc.x);
        if !is_ws {
            run_style = Some(effective_style);
        }
        run_text.push(tc.c);
    }
    if !run_text.is_empty() {
        let style = run_style.unwrap_or(PdfCharStyle {
            bold: false,
            italic: false,
            fill_argb: 0xFF00_0000,
            text_size: None,
            baseline_shift: None,
        });
        runs.push(finish_run(
            &run_text,
            style,
            run_start_x,
            line_rect.right as f32,
            line_rect,
        ));
    }
    runs
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScriptKind {
    Normal,
    Subscript,
    Superscript,
}

struct ScriptContext {
    main_size: f32,
    main_top: f32,
    main_bottom: f32,
    main_baseline: f32,
}

impl ScriptContext {
    fn for_line(chars: &[TypedChar]) -> Self {
        let main_size = chars
            .iter()
            .filter(|tc| !tc.c.is_whitespace())
            .map(|tc| tc.font_size)
            .fold(0.0, f32::max);
        let mut main_tops = Vec::new();
        let mut main_bottoms = Vec::new();
        let mut main_baselines = Vec::new();
        for tc in chars {
            if tc.c.is_whitespace() || tc.font_size < main_size * 0.95 {
                continue;
            }
            main_tops.push(tc.bbox.top);
            main_bottoms.push(tc.bbox.bottom);
            main_baselines.push(tc.y);
        }
        main_tops.sort_unstable();
        main_bottoms.sort_unstable();
        main_baselines.sort_by(|a, b| a.total_cmp(b));
        let main_top = main_tops
            .get(main_tops.len() / 2)
            .copied()
            .unwrap_or_default() as f32;
        let main_bottom = main_bottoms
            .get(main_bottoms.len() / 2)
            .copied()
            .unwrap_or_default() as f32;
        let main_baseline = main_baselines
            .get(main_baselines.len() / 2)
            .copied()
            .unwrap_or_default();
        Self {
            main_size,
            main_top,
            main_bottom,
            main_baseline,
        }
    }

    fn kind_for(&self, tc: &TypedChar) -> ScriptKind {
        if self.main_size <= 0.0 || tc.font_size > self.main_size * 0.82 {
            return ScriptKind::Normal;
        }
        let shift_threshold = (self.main_size * 0.18).max(1.0);
        if tc.bbox.top as f32 > self.main_top + shift_threshold {
            return ScriptKind::Subscript;
        }
        if (tc.bbox.bottom as f32) < self.main_bottom - shift_threshold {
            return ScriptKind::Superscript;
        }
        ScriptKind::Normal
    }

    fn style_for(&self, tc: &TypedChar, script: ScriptKind) -> PdfCharStyle {
        let mut style = tc.style;
        if script != ScriptKind::Normal {
            style.text_size = Some(tc.font_size);
            style.baseline_shift = Some(tc.y - self.main_baseline);
        }
        style
    }
}

fn finish_run(
    text: &str,
    style: PdfCharStyle,
    start_x: Option<f32>,
    end_x: f32,
    line_rect: Rect,
) -> LineRun {
    let bbox = Rect {
        left: start_x.unwrap_or(line_rect.left as f32) as u32,
        top: line_rect.top,
        right: end_x.ceil() as u32,
        bottom: line_rect.bottom,
    };
    LineRun {
        text: text.to_string(),
        style,
        bbox,
    }
}

/// Strip the 6-char `XXXXXX+` subset prefix that TeX/PDF producers use
/// for embedded font subsets (`AAAAAA+CMSY9` → `CMSY9`).
fn font_stem(name: &str) -> &str {
    name.rsplit_once('+').map(|(_, s)| s).unwrap_or(name)
}

fn is_math_font(name: &str) -> bool {
    let stem = font_stem(name).to_ascii_uppercase();
    [
        "CMMI", "CMSY", "CMEX", "CMBSY", "CMMIB", "MSAM", "MSBM", "EUFM", "EUSM",
    ]
    .iter()
    .any(|prefix| stem.starts_with(prefix))
}

fn is_extensible_math_replacement(tc: &TypedChar) -> bool {
    tc.c == '\u{FFFD}'
        && font_stem(&tc.font_name)
            .to_ascii_uppercase()
            .starts_with("CMEX")
}

fn is_text_font(name: &str) -> bool {
    let stem = font_stem(name).to_ascii_uppercase();
    [
        "CMR", "CMTI", "CMSS", "CMTT", "CMBX", "CMSL", "CMU", "CMCSC", "CMITT", "CMFF", "CMFI",
        "CMFIB", "CMVTT", "CMTEX",
    ]
    .iter()
    .any(|prefix| stem.starts_with(prefix))
}

fn should_skip_pdf_line(text: &str, rect: Rect, page: PageDims) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }

    // Veto markers — these signal "this is code or prose with logic, not a
    // display equation". A `;` terminator, a dotted identifier like
    // `p.view`, or two-plus runs of 4+ alphabetic characters (real words
    // like `then`, `votes`, `view`) all rule out the symbol/letter-density
    // shortcuts below. Real display equations don't have any of these.
    if trimmed.contains(';')
        || has_dotted_identifier(trimmed)
        || count_long_alpha_runs(trimmed, 4) >= 2
    {
        return false;
    }

    let words = trimmed.split_whitespace().count();
    let letters = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    // Operators-only — no `[`, `]`, `{`, `}`, `(`, `)`. Brackets/braces are
    // routinely used in non-math contexts (citations like `[14, 18, 27]`,
    // set notation, array indexing) and were tripping false positives. Real
    // display math is caught upstream by the font-based detector
    // (`looks_like_display_math`); this heuristic is just for non-TeX
    // producers and edge cases.
    let symbols = trimmed
        .chars()
        .filter(|c| {
            matches!(
                c,
                '=' | '<' | '>' | '∑' | 'Σ' | 'σ' | '≤' | '≥' | '→' | '←' | '↔'
            )
        })
        .count();
    let underscores = trimmed.matches('_').count();
    // Truly-centered display equations sit at page center *and* have
    // roughly balanced left/right padding. Lines that are merely indented
    // (algorithm bodies, code blocks) often sit in the central 40% of the
    // page width but are anchored to the left margin — `is_centered` here
    // rejects those by demanding both small center-offset AND symmetric
    // padding. Without this, a 4-word indented body like
    // `"vote ← Alg. 6.CreateVote(...)"` would trigger the skip below and
    // disappear from the translated output.
    let centered = {
        let line_center = (rect.left + rect.right) as f32 * 0.5;
        let center_offset = (line_center - page.width_pts * 0.5).abs();
        let left_pad = rect.left as f32;
        let right_pad = (page.width_pts - rect.right as f32).max(0.0);
        let pad_imbalance = (left_pad - right_pad).abs();
        let avg_pad = (left_pad + right_pad) * 0.5;
        center_offset < page.width_pts * 0.05 && (avg_pad <= 0.0 || pad_imbalance < avg_pad * 0.3)
    };

    // Display equations/code signatures in papers should stay verbatim. They
    // are usually centered, symbol-heavy, or underscore-heavy, and contain
    // little ordinary prose.
    (centered && symbols > 0 && words <= 8)
        || (symbols >= 2 && letters < 24)
        || (underscores >= 2 && words <= 8)
}

/// True if `text` contains a `<lower>+ . <lower>+` sequence — i.e. a field
/// access like `p.view` or `local_tip.block`. Used to flag code-like lines
/// before they hit the equation-skip heuristics.
fn has_dotted_identifier(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    for i in 1..chars.len().saturating_sub(1) {
        if chars[i] != '.' {
            continue;
        }
        let prev_is_lower = chars[i - 1].is_ascii_lowercase();
        let next_is_lower = chars[i + 1].is_ascii_lowercase();
        if prev_is_lower && next_is_lower {
            return true;
        }
    }
    false
}

/// Count maximal runs of `min_len` or more consecutive alphabetic chars.
/// `count_long_alpha_runs("if |votes[proposal id]| then", 4)` returns 3
/// (`votes`, `proposal`, `then`). A real word in code or prose; equations
/// rarely have any.
fn count_long_alpha_runs(text: &str, min_len: usize) -> usize {
    let mut count = 0;
    let mut run = 0usize;
    for c in text.chars() {
        if c.is_alphabetic() {
            run += 1;
        } else {
            if run >= min_len {
                count += 1;
            }
            run = 0;
        }
    }
    if run >= min_len {
        count += 1;
    }
    count
}

/// mupdf::Rect (top-left origin, points, f32) → our Rect (top-left origin, u32).
fn rect_from_mupdf(r: mupdf::Rect) -> Rect {
    Rect {
        left: r.x0.max(0.0).floor() as u32,
        top: r.y0.max(0.0).floor() as u32,
        right: r.x1.max(0.0).ceil() as u32,
        bottom: r.y1.max(0.0).ceil() as u32,
    }
}

fn rect_from_quad(q: mupdf::Quad) -> Rect {
    let xs = [q.ul.x, q.ur.x, q.ll.x, q.lr.x];
    let ys = [q.ul.y, q.ur.y, q.ll.y, q.lr.y];
    let left = xs.iter().copied().fold(f32::INFINITY, f32::min);
    let right = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
    let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    Rect {
        left: left.max(0.0).floor() as u32,
        top: top.max(0.0).floor() as u32,
        right: right.max(0.0).ceil() as u32,
        bottom: bottom.max(0.0).ceil() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc(c: char, font_name: &str) -> TypedChar {
        TypedChar {
            c,
            x: 0.0,
            y: if font_name.contains('7') { 21.5 } else { 20.0 },
            bbox: Rect {
                left: 0,
                top: 10,
                right: 10,
                bottom: 20,
            },
            font_size: if font_name.contains('7') { 7.0 } else { 10.0 },
            style: PdfCharStyle {
                bold: false,
                italic: false,
                fill_argb: 0xFF00_0000,
                text_size: None,
                baseline_shift: None,
            },
            font_name: font_name.to_string(),
        }
    }

    fn tc_at(c: char, x: f32, top: u32, right: u32, bottom: u32, size: f32) -> TypedChar {
        TypedChar {
            c,
            x,
            y: bottom as f32,
            bbox: Rect {
                left: x as u32,
                top,
                right,
                bottom,
            },
            font_size: size,
            style: PdfCharStyle {
                bold: false,
                italic: false,
                fill_argb: 0xFF00_0000,
                text_size: None,
                baseline_shift: None,
            },
            font_name: "GENERIC".into(),
        }
    }

    fn prose_line(text: &str, top: u32, bottom: u32) -> PreLine {
        PreLine {
            block_index: 0,
            typed_chars: text.chars().map(|c| tc(c, "NUYYMY+CMR10")).collect(),
            line_rect: Rect {
                left: 0,
                top,
                right: 100,
                bottom,
            },
            math_kind: MathKind::Prose,
        }
    }

    #[test]
    fn cmex_replacement_glyph_makes_fraction_line_strong_math() {
        let chars = [
            tc('\u{FFFD}', "KJCFKR+CMEX10"),
            tc('τ', "YOUTFK+CMMI7"),
            tc('2', "XDHWRX+CMR5"),
            tc('−', "AQGLSQ+CMSY7"),
            tc('τ', "YOUTFK+CMMI7"),
            tc('1', "XDHWRX+CMR5"),
        ];

        assert_eq!(classify_math_line(&chars), MathKind::Strong);
    }

    #[test]
    fn cmex_fraction_keeps_only_formula_lines_opaque() {
        let leading_prose = prose_line("alpha beta gamma delta epsilon", 10, 20);
        let numerator = PreLine {
            block_index: 0,
            typed_chars: vec![
                tc('\u{FFFD}', "KJCFKR+CMEX10"),
                tc('τ', "YOUTFK+CMMI7"),
                tc('2', "XDHWRX+CMR5"),
            ],
            line_rect: Rect {
                left: 100,
                top: 9,
                right: 130,
                bottom: 21,
            },
            math_kind: MathKind::Strong,
        };
        let denominator = PreLine {
            block_index: 0,
            typed_chars: vec![tc('δ', "YOUTFK+CMMI7")],
            line_rect: Rect {
                left: 110,
                top: 16,
                right: 120,
                bottom: 21,
            },
            math_kind: MathKind::Weak,
        };
        let right_delimiter = PreLine {
            block_index: 0,
            typed_chars: vec![tc('\u{FFFD}', "KJCFKR+CMEX10")],
            line_rect: Rect {
                left: 130,
                top: 9,
                right: 140,
                bottom: 21,
            },
            math_kind: MathKind::Strong,
        };
        let trailing_prose = prose_line("zeta eta theta iota kappa", 11, 19);
        let next_row_prose = prose_line("lambda mu nu xi omicron", 30, 40);

        assert_eq!(
            compute_opaque_flags(&[
                leading_prose,
                numerator,
                denominator,
                right_delimiter,
                trailing_prose,
                next_row_prose
            ]),
            vec![false, true, true, true, false, false]
        );
    }

    #[test]
    fn lowered_small_glyph_gets_subscript_style() {
        let chars = vec![
            tc_at('L', 10.0, 100, 17, 110, 10.0),
            tc_at('v', 17.0, 104, 22, 112, 7.0),
        ];

        let runs = split_line_by_style(
            &chars,
            Rect {
                left: 10,
                top: 100,
                right: 22,
                bottom: 112,
            },
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "L");
        assert_eq!(runs[0].style.text_size, None);
        assert_eq!(runs[0].style.baseline_shift, None);
        assert_eq!(runs[1].text, "v");
        assert_eq!(runs[1].style.text_size, Some(7.0));
        assert_eq!(runs[1].style.baseline_shift, Some(2.0));
    }

    #[test]
    fn shifted_operator_stays_inside_subscript_group() {
        let chars = vec![
            tc_at('L', 10.0, 100, 17, 110, 10.0),
            tc_at('v', 17.0, 104, 22, 112, 7.0),
            tc_at('+', 22.0, 104, 27, 112, 7.0),
            tc_at('1', 27.0, 104, 32, 112, 7.0),
        ];

        let runs = split_line_by_style(
            &chars,
            Rect {
                left: 10,
                top: 100,
                right: 32,
                bottom: 112,
            },
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "L");
        assert_eq!(runs[1].text, "v+1");
        assert_eq!(runs[1].style.text_size, Some(7.0));
        assert_eq!(runs[1].style.baseline_shift, Some(2.0));
    }

    #[test]
    fn raised_small_glyph_gets_superscript_style() {
        let chars = vec![
            tc_at('x', 10.0, 100, 17, 110, 10.0),
            tc_at('2', 17.0, 96, 22, 104, 7.0),
        ];

        let runs = split_line_by_style(
            &chars,
            Rect {
                left: 10,
                top: 96,
                right: 22,
                bottom: 110,
            },
        );

        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "x");
        assert_eq!(runs[1].text, "2");
        assert_eq!(runs[1].style.text_size, Some(7.0));
        assert_eq!(runs[1].style.baseline_shift, Some(-6.0));
    }
}
