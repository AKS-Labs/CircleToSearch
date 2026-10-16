use std::collections::{HashMap, HashSet};

use translator_core::api::{LanguageCode, ScriptedLanguage};
use translator_core::ocr::{OverlayColors, Rect, sample_overlay_colors};
use translator_core::settings::BackgroundMode;
use translator_translate::document_translator::DocumentTranslator;
use translator_translate::language_detect::detect_language_robust_code;
use translator_translate::routing::NothingReason;
use translator_translate::translate::TokenAlignment;

#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub text_color: Option<u32>,
    pub bg_color: Option<u32>,
    pub text_size: Option<f32>,
    pub baseline_shift: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

impl TextStyle {
    fn has_real_background(&self) -> bool {
        let Some(color) = self.bg_color else {
            return false;
        };
        if color == 0 || color == 1 || color == 0xFFFF_FFFF {
            return false;
        }
        (color >> 24) != 0
    }

    fn normalized_text_color(&self) -> Option<u32> {
        let color = self.text_color?;
        if (color >> 24) == 0 {
            None
        } else {
            Some(color)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyledFragment {
    pub text: String,
    pub bounding_box: Rect,
    pub style: Option<TextStyle>,
    pub layout_group: u32,
    pub translation_group: u32,
    pub cluster_group: u32,
    /// Treat this fragment as a black box: don't translate it into prose and
    /// don't redraw it with fallback fonts. Used for display-math lines
    /// (LaTeX formulas) where mupdf's per-char font analysis says the line
    /// is drawn predominantly in CMSY/CMMI/CMEX. The PDF writer preserves
    /// the original glyph program for these fragments.
    pub opaque: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StyleSpan {
    pub start: u32,
    pub end: u32,
    pub style: Option<TextStyle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranslationSegment {
    pub start: u32,
    pub end: u32,
    pub translation_group: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct TranslatableBlock {
    text: String,
    bounds: Rect,
    source_rects: Vec<Rect>,
    style_spans: Vec<StyleSpan>,
    segments: Vec<TranslationSegment>,
    /// True iff every fragment in this block was tagged opaque. Set on
    /// extraction for display-math blocks; carried through translation
    /// untouched so the writer can preserve the original glyph program.
    opaque: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedStyledBlock {
    pub text: String,
    pub bounding_box: Rect,
    pub source_rects: Vec<Rect>,
    pub style_spans: Vec<StyleSpan>,
    pub background_argb: u32,
    pub foreground_argb: u32,
    /// Display-math (or otherwise pass-through) block. The writer preserves
    /// the original CMSY/CMMI/etc. text-show program instead of translating
    /// and redrawing the block with fallback fonts.
    pub opaque: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StructuredTranslationResult {
    pub blocks: Vec<TranslatedStyledBlock>,
    pub nothing_reason: Option<NothingReason>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayScreenshot {
    pub rgba_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Translate many pages of structured fragments: per-page source detection and
/// block clustering happen here, then each source language's segments are
/// translated through `translator` (the aligned primitive owns the bergamot
/// call, cancellation and pair resolution), and the results are reassembled
/// into styled blocks. `Ok(None)` means the run was cancelled; progress
/// accumulates across the per-source-language batches in block units.
pub fn translate_structured_fragments_batch_ctx(
    translator: &dyn DocumentTranslator,
    pages: &[&[StyledFragment]],
    forced_source_code: Option<&str>,
    target_code: &str,
    available_languages: &[ScriptedLanguage],
    background_mode: BackgroundMode,
    on_progress: &(dyn Fn(usize, usize) + Sync),
) -> Result<Option<Vec<StructuredTranslationResult>>, String> {
    let states = pages
        .iter()
        .map(|fragments| {
            classify_page(
                fragments,
                forced_source_code,
                target_code,
                available_languages,
            )
        })
        .collect::<Vec<_>>();

    // Bundle every translate-pending page's segments into per-source-language
    // batches. One bergamot call per source code regardless of how many pages
    // it spans — that's where the worker pool gets fed.
    struct PageSegmentRef {
        page_index: usize,
        block_index: usize,
        segment: TranslationSegment,
        source_offset: u32,
        output_prefix: String,
    }
    let mut texts_by_source: HashMap<String, Vec<String>> = HashMap::new();
    let mut refs_by_source: HashMap<String, Vec<PageSegmentRef>> = HashMap::new();

    for (page_index, state) in states.iter().enumerate() {
        let PageState::Translate {
            blocks,
            source_code,
        } = state
        else {
            continue;
        };
        let key = source_code.as_str().to_string();
        let texts = texts_by_source.entry(key.clone()).or_default();
        let refs = refs_by_source.entry(key).or_default();
        for seg in collect_block_segments(blocks) {
            texts.push(seg.text);
            refs.push(PageSegmentRef {
                page_index,
                block_index: seg.block_index,
                segment: seg.segment,
                source_offset: seg.source_offset,
                output_prefix: seg.output_prefix,
            });
        }
    }

    let mut translations_by_page: HashMap<usize, Vec<BlockSegmentTranslation>> = HashMap::new();
    let mut missing_pair_pages: HashSet<usize> = HashSet::new();

    // Report progress in block units across every source-language batch, so the
    // bar reflects the whole document (not per-language) and never overshoots.
    // The worker callback counts *sentences* (each block splits into one or
    // more), so we scale each batch's sentence fraction by that batch's block
    // count — keeping numerator and denominator both in blocks. The loop runs
    // batches sequentially, so a plain accumulator is enough.
    let total_blocks: usize = texts_by_source.values().map(Vec::len).sum();
    let mut blocks_done = 0usize;

    let target_code_lc = LanguageCode::from(target_code);
    for (source_code_str, texts) in texts_by_source {
        let refs = refs_by_source.remove(&source_code_str).unwrap_or_default();
        let batch_blocks = texts.len();
        let base = blocks_done;
        let batch_progress = |bytes_done: usize, bytes_total: usize| {
            let filled = if bytes_total == 0 {
                batch_blocks
            } else {
                bytes_done * batch_blocks / bytes_total
            };
            on_progress(base + filled, total_blocks);
        };
        let translated = match translator.translate_texts_with_alignment_ctx(
            &LanguageCode::from(source_code_str.as_str()),
            &target_code_lc,
            &texts,
            &batch_progress,
        ) {
            Ok(Some(translated)) => translated,
            // No model pair for this source — report those pages as missing,
            // mirroring the old `resolve_translation_plan` None branch.
            Ok(None) => {
                for r in &refs {
                    missing_pair_pages.insert(r.page_index);
                }
                continue;
            }
            Err(e) if e.is_cancelled() => return Ok(None),
            Err(e) => return Err(e.message),
        };
        blocks_done += batch_blocks;
        for (r, t) in refs.into_iter().zip(translated) {
            translations_by_page
                .entry(r.page_index)
                .or_default()
                .push(BlockSegmentTranslation {
                    block_index: r.block_index,
                    segment: r.segment,
                    translated_text: t.translated_text,
                    alignments: t.alignments,
                    source_offset: r.source_offset,
                    output_prefix: r.output_prefix,
                });
        }
    }

    states
        .into_iter()
        .enumerate()
        .map(|(page_index, state)| match state {
            PageState::Empty(reason) => Ok(StructuredTranslationResult {
                blocks: Vec::new(),
                nothing_reason: Some(reason),
                error_message: None,
            }),
            PageState::Identity(blocks) => Ok(StructuredTranslationResult {
                blocks: identity_translated_blocks(&blocks, None, background_mode)?,
                nothing_reason: None,
                error_message: None,
            }),
            PageState::Translate {
                blocks,
                source_code,
            } => {
                if missing_pair_pages.contains(&page_index) {
                    return Ok(StructuredTranslationResult {
                        blocks: Vec::new(),
                        nothing_reason: None,
                        error_message: Some(format!(
                            "Language pair {} -> {} not installed",
                            source_code.as_str(),
                            target_code,
                        )),
                    });
                }
                let page_translations =
                    translations_by_page.remove(&page_index).unwrap_or_default();
                Ok(StructuredTranslationResult {
                    blocks: assemble_translated_blocks(
                        &blocks,
                        &page_translations,
                        None,
                        background_mode,
                    )?,
                    nothing_reason: None,
                    error_message: None,
                })
            }
        })
        .collect::<Result<Vec<_>, String>>()
        .map(Some)
}

enum PageState {
    Empty(NothingReason),
    Identity(Vec<TranslatableBlock>),
    Translate {
        blocks: Vec<TranslatableBlock>,
        source_code: LanguageCode,
    },
}

fn classify_page(
    fragments: &[StyledFragment],
    forced_source_code: Option<&str>,
    target_code: &str,
    available_languages: &[ScriptedLanguage],
) -> PageState {
    let blocks = cluster_fragments_into_blocks(fragments);
    if blocks.is_empty() {
        return PageState::Empty(NothingReason::NoTranslatableText);
    }
    let combined_text = blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let Some(source_code) = forced_source_code
        .map(LanguageCode::from)
        .or_else(|| detect_language_robust_code(&combined_text, None, available_languages))
    else {
        return PageState::Empty(NothingReason::CouldNotDetect);
    };
    if source_code.as_str() == target_code {
        return PageState::Identity(blocks);
    }
    PageState::Translate {
        blocks,
        source_code,
    }
}

struct CollectedSegment {
    block_index: usize,
    segment: TranslationSegment,
    text: String,
    source_offset: u32,
    output_prefix: String,
}

fn collect_block_segments(blocks: &[TranslatableBlock]) -> Vec<CollectedSegment> {
    let mut out = Vec::new();
    for (block_index, block) in blocks.iter().enumerate() {
        // Opaque blocks (display math) bypass bergamot — their text is the
        // original, and we want it preserved exactly. They still carry
        // through to `translated_blocks` so the writer knows to leave them
        // alone instead of erasing the area.
        if block.opaque {
            continue;
        }
        for segment in &block.segments {
            let start = segment.start as usize;
            let end = segment.end as usize;
            let raw_text = &block.text[start..end];
            let (output_prefix, source_offset, translatable_text) =
                split_protected_leading_list_marker(raw_text);
            // Marian treats '\n' as a hard sentence break, so soft line wraps
            // inside a paragraph would translate line-per-line and lose
            // cross-line context. Flatten to spaces — '\n' and ' ' are both
            // 1 byte, so alignment offsets remain valid for style mapping.
            out.push(CollectedSegment {
                block_index,
                segment: segment.clone(),
                text: translatable_text.replace('\n', " "),
                source_offset,
                output_prefix,
            });
        }
    }
    out
}

struct BlockSegmentTranslation {
    block_index: usize,
    segment: TranslationSegment,
    translated_text: String,
    alignments: Vec<TokenAlignment>,
    source_offset: u32,
    output_prefix: String,
}

fn assemble_translated_blocks(
    blocks: &[TranslatableBlock],
    translations: &[BlockSegmentTranslation],
    screenshot: Option<&OverlayScreenshot>,
    background_mode: BackgroundMode,
) -> Result<Vec<TranslatedStyledBlock>, String> {
    blocks
        .iter()
        .enumerate()
        .map(|(block_index, source_block)| {
            let colors = resolve_block_colors(
                screenshot,
                source_block.bounds,
                source_block
                    .style_spans
                    .first()
                    .and_then(|span| span.style.as_ref()),
                background_mode,
            )?;
            if source_block.opaque {
                return Ok(TranslatedStyledBlock {
                    text: source_block.text.clone(),
                    bounding_box: source_block.bounds,
                    source_rects: source_block.source_rects.clone(),
                    style_spans: source_block.style_spans.clone(),
                    background_argb: colors.background_argb,
                    foreground_argb: colors.foreground_argb,
                    opaque: true,
                });
            }
            let mut translated_text = String::new();
            let mut segment_alignments = Vec::new();
            let mut translated_segments = Vec::new();

            for entry in translations
                .iter()
                .filter(|entry| entry.block_index == block_index)
            {
                let full_translation = format!("{}{}", entry.output_prefix, entry.translated_text);
                let adjusted_alignments = adjust_alignments_for_protected_prefix(
                    &entry.alignments,
                    entry.source_offset,
                    entry.output_prefix.len() as u64,
                );
                translated_segments.push((entry.segment.clone(), full_translation.clone()));
                segment_alignments.push((entry.segment.clone(), adjusted_alignments));
                translated_text.push_str(&full_translation);
            }

            let style_spans = map_styles_to_segmented_translation(
                source_block,
                &segment_alignments,
                &translated_segments,
            );

            Ok(TranslatedStyledBlock {
                text: translated_text,
                bounding_box: source_block.bounds,
                source_rects: source_block.source_rects.clone(),
                style_spans,
                background_argb: colors.background_argb,
                foreground_argb: colors.foreground_argb,
                opaque: false,
            })
        })
        .collect()
}

fn split_protected_leading_list_marker(text: &str) -> (String, u32, &str) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    let marker_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == marker_start || i >= bytes.len() {
        return (String::new(), 0, text);
    }
    if !matches!(bytes[i], b'.' | b')') {
        return (String::new(), 0, text);
    }
    i += 1;
    let marker_end = i;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i == marker_end {
        return (String::new(), 0, text);
    }

    let mut prefix = text[..marker_end].trim().to_string();
    prefix.push(' ');
    (prefix, i as u32, &text[i..])
}

fn adjust_alignments_for_protected_prefix(
    alignments: &[TokenAlignment],
    source_offset: u32,
    target_offset: u64,
) -> Vec<TokenAlignment> {
    let source_offset = source_offset as u64;
    alignments
        .iter()
        .map(|alignment| TokenAlignment {
            src_begin: alignment.src_begin + source_offset,
            src_end: alignment.src_end + source_offset,
            tgt_begin: alignment.tgt_begin + target_offset,
            tgt_end: alignment.tgt_end + target_offset,
        })
        .collect()
}

fn identity_translated_blocks(
    blocks: &[TranslatableBlock],
    screenshot: Option<&OverlayScreenshot>,
    background_mode: BackgroundMode,
) -> Result<Vec<TranslatedStyledBlock>, String> {
    blocks
        .iter()
        .map(|source_block| {
            let colors = resolve_block_colors(
                screenshot,
                source_block.bounds,
                source_block
                    .style_spans
                    .first()
                    .and_then(|span| span.style.as_ref()),
                background_mode,
            )?;
            Ok(TranslatedStyledBlock {
                text: source_block.text.clone(),
                bounding_box: source_block.bounds,
                source_rects: source_block.source_rects.clone(),
                style_spans: source_block.style_spans.clone(),
                background_argb: colors.background_argb,
                foreground_argb: colors.foreground_argb,
                opaque: source_block.opaque,
            })
        })
        .collect()
}

fn cluster_fragments_into_blocks(fragments: &[StyledFragment]) -> Vec<TranslatableBlock> {
    if fragments.is_empty() {
        return Vec::new();
    }

    let line_height = lower_quartile_height(fragments);
    let block_gap_threshold = ((line_height as f32) * 0.75) as u32;

    let mut block_groups: Vec<Vec<StyledFragment>> = Vec::new();
    let mut block_bounds: Vec<Rect> = Vec::new();
    let mut block_layout_group_ids = Vec::new();
    let mut block_cluster_group_ids = Vec::new();
    let mut force_new_block = false;

    for fragment in fragments {
        if is_standalone_list_marker(fragment) {
            force_new_block = true;
            continue;
        }
        let mut same_line_match = None;
        let mut next_line_match = None;
        if !force_new_block {
            for i in 0..block_groups.len() {
                if block_layout_group_ids[i] != fragment.layout_group {
                    continue;
                }
                if block_cluster_group_ids[i] != fragment.cluster_group {
                    continue;
                }
                let bb: Rect = block_bounds[i];
                let vertical_overlap = bb
                    .bottom
                    .min(fragment.bounding_box.bottom)
                    .saturating_sub(bb.top.max(fragment.bounding_box.top));
                let vertical_gap = fragment.bounding_box.top.saturating_sub(bb.bottom);
                let horizontal_overlap = bb
                    .right
                    .min(fragment.bounding_box.right)
                    .saturating_sub(bb.left.max(fragment.bounding_box.left));
                let horizontal_gap = bb
                    .left
                    .max(fragment.bounding_box.left)
                    .saturating_sub(bb.right.min(fragment.bounding_box.right));
                let same_line_nearby =
                    should_merge_same_line(&block_groups[i], fragment, line_height, horizontal_gap);

                if vertical_overlap > 0 && same_line_nearby {
                    same_line_match = Some(i);
                    break;
                }
                if vertical_gap <= block_gap_threshold
                    && horizontal_overlap > 0
                    && !starts_list_item_marker(&fragment.text)
                    && should_merge_next_line(&block_groups[i], bb, fragment, line_height)
                    && next_line_match.is_none()
                {
                    next_line_match = Some(i);
                }
            }
        }
        if let Some(i) = same_line_match.or(next_line_match) {
            block_groups[i].push(fragment.clone());
            block_bounds[i].union(fragment.bounding_box);
        } else {
            block_groups.push(vec![fragment.clone()]);
            block_bounds.push(fragment.bounding_box);
            block_layout_group_ids.push(fragment.layout_group);
            block_cluster_group_ids.push(fragment.cluster_group);
        }
        force_new_block = false;
    }

    block_groups
        .into_iter()
        .flat_map(|group| build_blocks(&group))
        .collect()
}

fn same_line_gap_threshold(line_height: u32) -> u32 {
    // A real word-space is normally well below one line-height. Larger gaps
    // are commonly table label/value columns that merely share a baseline.
    ((line_height as f32) * 0.8).ceil() as u32
}

fn same_line_prose_gap_threshold(line_height: u32) -> u32 {
    line_height.saturating_mul(3)
}

fn should_merge_same_line(
    existing: &[StyledFragment],
    next: &StyledFragment,
    line_height: u32,
    horizontal_gap: u32,
) -> bool {
    if horizontal_gap <= same_line_gap_threshold(line_height) {
        return true;
    }
    if horizontal_gap > same_line_prose_gap_threshold(line_height) {
        return false;
    }

    let existing_text = joined_fragment_text(existing);
    let next_text = next.text.trim();
    if looks_like_prose(&existing_text) {
        return true;
    }
    if looks_like_enumerated_prose_start(&existing_text, next_text) {
        // Pure-digit markers ("4" or "12", no `.`/`)`/`(`) are ambiguous —
        // they could be a numbered-list bullet OR an algorithm gutter column.
        // Real list bullets sit one word-space from their content; algorithm
        // gutters sit a tab-stop+ away. So: require a tight gap for digit-only
        // markers, but keep the loose tolerance for unambiguous list markers
        // like "1." or "1)".
        let marker = existing_text.trim();
        let pure_digit_marker = !marker.is_empty() && marker.chars().all(|c| c.is_ascii_digit());
        if pure_digit_marker {
            return horizontal_gap <= line_height;
        }
        return true;
    }
    false
}

fn should_merge_next_line(
    existing: &[StyledFragment],
    existing_bounds: Rect,
    next: &StyledFragment,
    line_height: u32,
) -> bool {
    let existing_text = joined_fragment_text(existing);
    let min_paragraph_width = line_height.saturating_mul(10).max(90);
    let wide_enough = existing_bounds.width() >= min_paragraph_width
        || next.bounding_box.width() >= min_paragraph_width;

    wide_enough && looks_like_prose(&existing_text)
}

fn joined_fragment_text(fragments: &[StyledFragment]) -> String {
    let mut text = String::new();
    for fragment in fragments {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(fragment.text.trim());
    }
    text
}

fn looks_like_prose(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let words = trimmed.split_whitespace().count();
    let letters = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    words >= 4 || letters >= 32 || trimmed.contains([',', '.', ';', ':'])
}

fn looks_like_enumerated_prose_start(existing: &str, next: &str) -> bool {
    let marker = existing.trim();
    let next = next.trim();
    !next.is_empty()
        && marker.len() <= 5
        && marker
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ')' | '('))
        && next.chars().any(|c| c.is_alphabetic())
}

fn starts_list_item_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    let Some(first) = trimmed.chars().next() else {
        return false;
    };
    matches!(first, '-' | '–' | '—' | '•' | '∙' | '◦')
}

fn is_standalone_list_marker(fragment: &StyledFragment) -> bool {
    let text = fragment.text.trim();
    !text.is_empty()
        && text.chars().all(|c| matches!(c, '●' | '•' | '▪' | '◦'))
        && fragment.bounding_box.width() <= 16
}

fn build_blocks(fragments: &[StyledFragment]) -> Vec<TranslatableBlock> {
    let lines = cluster_into_lines(fragments);
    if lines.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    let mut start = 0usize;
    for i in 0..lines.len().saturating_sub(1) {
        if is_section_heading_line(&lines[i]) && starts_subsection_line(&lines[i + 1]) {
            blocks.push(build_block_from_lines(&lines[start..=i]));
            start = i + 1;
        }
    }
    if start < lines.len() {
        blocks.push(build_block_from_lines(&lines[start..]));
    }
    blocks
}

fn build_block_from_lines(lines: &[Vec<StyledFragment>]) -> TranslatableBlock {
    let mut text = String::new();
    let mut spans = Vec::new();
    let mut segments = Vec::new();
    let mut source_rects = Vec::new();
    let mut bounds = lines
        .iter()
        .flatten()
        .next()
        .map(|fragment| fragment.bounding_box)
        .unwrap_or_default();
    for fragment in lines.iter().flatten() {
        bounds.union(fragment.bounding_box);
        source_rects.push(fragment.bounding_box);
    }
    let mut current_trans_group = lines
        .iter()
        .flatten()
        .next()
        .map(|fragment| fragment.translation_group)
        .unwrap_or_default();
    let mut segment_start = 0u32;

    for (line_index, line) in lines.iter().enumerate() {
        if line_index > 0 {
            text.push('\n');
        }
        for (fragment_index, fragment) in line.iter().enumerate() {
            if fragment.translation_group != current_trans_group {
                if (text.len() as u32) > segment_start {
                    segments.push(TranslationSegment {
                        start: segment_start,
                        end: text.len() as u32,
                        translation_group: current_trans_group,
                    });
                }
                current_trans_group = fragment.translation_group;
                segment_start = text.len() as u32;
            }
            if fragment_index > 0
                && should_insert_space_between_fragments(&line[fragment_index - 1], fragment)
            {
                text.push(' ');
            }
            let start = text.len() as u32;
            text.push_str(&fragment.text);
            if fragment.style.is_some() {
                spans.extend(style_spans_for_fragment(fragment, start));
            }
        }
    }

    if (text.len() as u32) > segment_start {
        segments.push(TranslationSegment {
            start: segment_start,
            end: text.len() as u32,
            translation_group: current_trans_group,
        });
    }

    let text = normalize_pdf_math_sequences(&text);
    let opaque = !lines.is_empty() && lines.iter().flatten().all(|f| f.opaque);

    TranslatableBlock {
        text,
        bounds,
        source_rects,
        style_spans: spans,
        segments,
        opaque,
    }
}

fn normalize_pdf_math_sequences(text: &str) -> String {
    // TeX PDFs often paint a relation and its negation slash as separate
    // glyphs. MuPDF exposes the slash as U+0338; if we emit that directly,
    // shaping attaches it to the previous letter instead of the relation.
    text.replace("\u{0338}=", "≠")
        .replace("\u{0338} =", "≠")
        .replace("=\u{0338}", "≠")
}

fn line_plain_text(line: &[StyledFragment]) -> String {
    let mut text = String::new();
    for (i, fragment) in line.iter().enumerate() {
        if i > 0 && should_insert_space_between_fragments(&line[i - 1], fragment) {
            text.push(' ');
        }
        text.push_str(&fragment.text);
    }
    text
}

fn should_insert_space_between_fragments(left: &StyledFragment, right: &StyledFragment) -> bool {
    if left.text.is_empty()
        || right.text.is_empty()
        || left.text.chars().last().is_some_and(char::is_whitespace)
        || right.text.chars().next().is_some_and(char::is_whitespace)
    {
        return false;
    }
    if right
        .text
        .chars()
        .next()
        .is_some_and(|c| matches!(c, ',' | '.' | ')' | ']' | '}' | ':' | ';' | '?' | '!'))
    {
        return false;
    }

    let gap = right
        .bounding_box
        .left
        .saturating_sub(left.bounding_box.right);
    gap > 1
}

fn style_spans_for_fragment(fragment: &StyledFragment, block_start: u32) -> Vec<StyleSpan> {
    let Some(style) = &fragment.style else {
        return Vec::new();
    };
    vec![StyleSpan {
        start: block_start,
        end: block_start + fragment.text.len() as u32,
        style: Some(style.clone()),
    }]
}

fn is_section_heading_line(line: &[StyledFragment]) -> bool {
    let text = line_plain_text(line);
    let trimmed = text.trim();
    let Some((prefix, rest)) = trimmed.split_once('.') else {
        return false;
    };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_digit())
        && rest.starts_with(char::is_whitespace)
        && !matches!(trimmed.chars().last(), Some('.' | ';' | ':'))
}

fn starts_subsection_line(line: &[StyledFragment]) -> bool {
    let text = line_plain_text(line);
    let trimmed = text.trim_start();
    let Some(dot) = trimmed.find('.') else {
        return false;
    };
    if dot == 0 || !trimmed[..dot].chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let rest = &trimmed[dot + 1..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    digit_count > 0
}

fn cluster_into_lines(fragments: &[StyledFragment]) -> Vec<Vec<StyledFragment>> {
    if fragments.is_empty() {
        return Vec::new();
    }

    let median_height = median_fragment_height(fragments);
    let line_threshold = ((median_height as f32) * 0.35) as u32;
    let line_threshold = line_threshold.max(1);

    let mut lines: Vec<Vec<StyledFragment>> = Vec::new();
    let mut line_tops: Vec<u32> = Vec::new();
    let mut line_bottoms: Vec<u32> = Vec::new();
    let mut line_groups: Vec<u32> = Vec::new();

    for fragment in fragments {
        let mut best_line = None;
        for i in 0..lines.len() {
            // Multi-column pages have fragments at the same y from
            // different columns; clustering them together would make
            // the resulting block span both columns and the wrap
            // engine would use page width instead of column width.
            // Each mupdf block (column) carries a distinct
            // translation_group; gating clustering on that keeps
            // columns separate.
            if line_groups[i] != fragment.translation_group {
                continue;
            }
            let center_delta = fragment
                .bounding_box
                .center_y()
                .abs_diff((line_tops[i] + line_bottoms[i]) / 2);
            let vertical_overlap = line_bottoms[i]
                .min(fragment.bounding_box.bottom)
                .saturating_sub(line_tops[i].max(fragment.bounding_box.top));
            // Require a *substantial* vertical overlap (not just 1–2px
            // of ascender/descender bleed) before collapsing two
            // fragments onto the same visual line. Tight-leaded body
            // text (e.g. FM 22-100 page 7's heading + body packed at
            // ~12pt leading on 13pt glyphs) has consecutive lines whose
            // ink boxes touch by 1–2px; counting any overlap as
            // same-line collapses every line of a column into one
            // super-line, scrambling its reading order. The center-y
            // delta check still handles legitimate baseline-matched
            // fragments — there overlap is closer to full line height.
            if vertical_overlap > line_threshold || center_delta <= line_threshold {
                best_line = Some(i);
                break;
            }
        }

        if let Some(i) = best_line {
            lines[i].push(fragment.clone());
            line_tops[i] = line_tops[i].min(fragment.bounding_box.top);
            line_bottoms[i] = line_bottoms[i].max(fragment.bounding_box.bottom);
        } else {
            lines.push(vec![fragment.clone()]);
            line_tops.push(fragment.bounding_box.top);
            line_bottoms.push(fragment.bounding_box.bottom);
            line_groups.push(fragment.translation_group);
        }
    }

    let mut line_indices = (0..lines.len()).collect::<Vec<_>>();
    line_indices.sort_by_key(|index| line_tops[*index]);
    line_indices
        .into_iter()
        .map(|index| {
            let mut line = lines[index].clone();
            line.sort_by_key(|fragment| fragment.bounding_box.left);
            line
        })
        .collect()
}

fn map_styles_to_segmented_translation(
    source_block: &TranslatableBlock,
    segment_alignments: &[(TranslationSegment, Vec<TokenAlignment>)],
    translated_segments: &[(TranslationSegment, String)],
) -> Vec<StyleSpan> {
    let mut result = Vec::new();
    let mut target_offset = 0u32;

    for (segment, translated) in translated_segments {
        let alignments = segment_alignments
            .iter()
            .find(|(aligned_segment, _)| aligned_segment == segment)
            .map(|(_, alignments)| alignments.as_slice())
            .unwrap_or(&[]);

        // `TokenAlignment` is in CHARACTER offsets (bergamot-sys converts
        // from Marian's byte offsets), but `style_spans` are byte offsets
        // (built from `text.len()`). Build per-segment lookup tables so we
        // can convert each alignment's char offsets back to bytes; otherwise
        // every non-ASCII char before a styled run shifts the bold range
        // left by one byte (== one accent = off-by-one bold).
        let segment_start = segment.start as usize;
        let segment_end = segment.end as usize;
        let segment_text = &source_block.text[segment_start..segment_end];
        let src_byte_at_char = char_to_byte_offsets(segment_text);
        let tgt_byte_at_char = char_to_byte_offsets(translated);

        for alignment in alignments {
            // Convert char-indexed alignment to source-byte offsets. Clamp
            // out-of-range indices to the table length so we don't panic on
            // alignment edges that point past the end of the segment.
            let src_b_byte = src_byte_at_char
                .get(alignment.src_begin as usize)
                .copied()
                .unwrap_or(segment_text.len());
            let src_e_byte = src_byte_at_char
                .get(alignment.src_end as usize)
                .copied()
                .unwrap_or(segment_text.len());
            let src_mid = segment.start + ((src_b_byte + src_e_byte) / 2) as u32;
            let Some(matching_span) = source_block
                .style_spans
                .iter()
                .find(|span| src_mid >= span.start && src_mid < span.end)
            else {
                continue;
            };
            let tgt_b_byte = tgt_byte_at_char
                .get(alignment.tgt_begin as usize)
                .copied()
                .unwrap_or(translated.len());
            let tgt_e_byte = tgt_byte_at_char
                .get(alignment.tgt_end as usize)
                .copied()
                .unwrap_or(translated.len());
            result.push(StyleSpan {
                start: target_offset + tgt_b_byte as u32,
                end: target_offset + tgt_e_byte as u32,
                style: matching_span.style.clone(),
            });
        }

        target_offset += translated.len() as u32;
    }

    let translated_text = translated_segments
        .iter()
        .map(|(_, translated)| translated.as_str())
        .collect::<String>();
    merge_style_spans(expand_style_spans_to_word_boundaries(
        result,
        &translated_text,
    ))
}

/// Build `char_idx -> byte_idx` lookup. `table[n]` is the byte offset of
/// the start of the n-th char (or `s.len()` for `n == char_count`).
fn char_to_byte_offsets(s: &str) -> Vec<usize> {
    let mut table: Vec<usize> = s.char_indices().map(|(b, _)| b).collect();
    table.push(s.len());
    table
}

fn expand_style_spans_to_word_boundaries(spans: Vec<StyleSpan>, text: &str) -> Vec<StyleSpan> {
    spans
        .into_iter()
        .map(|mut span| {
            if span.style.as_ref().is_some_and(is_script_style) {
                span.start = span.start.min(text.len() as u32);
                span.end = span.end.min(text.len() as u32);
                return span;
            }
            let (start, end) =
                expand_byte_range_to_first_word(text, span.start as usize, span.end as usize);
            span.start = start as u32;
            span.end = end as u32;
            span
        })
        .filter(|span| span.start < span.end)
        .collect()
}

fn is_script_style(style: &TextStyle) -> bool {
    style.text_size.is_some() || style.baseline_shift.is_some()
}

fn expand_byte_range_to_first_word(text: &str, start: usize, end: usize) -> (usize, usize) {
    let mut word_byte = None;
    for (byte, ch) in text.char_indices() {
        let ch_end = byte + ch.len_utf8();
        if ch_end <= start {
            continue;
        }
        if byte >= end {
            break;
        }
        if is_style_word_char(ch) {
            word_byte = Some(byte);
            break;
        }
    }

    let Some(mut expanded_start) = word_byte else {
        return (start.min(text.len()), end.min(text.len()));
    };
    let mut expanded_end = expanded_start
        + text[expanded_start..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or_default();

    while let Some((prev_start, prev)) = prev_char(text, expanded_start) {
        if !is_style_word_char(prev) {
            break;
        }
        expanded_start = prev_start;
    }
    while expanded_end < text.len() {
        let Some(next) = text[expanded_end..].chars().next() else {
            break;
        };
        if !is_style_word_char(next) {
            break;
        }
        expanded_end += next.len_utf8();
    }

    (expanded_start, expanded_end)
}

fn prev_char(text: &str, index: usize) -> Option<(usize, char)> {
    text.get(..index)?.char_indices().next_back()
}

fn is_style_word_char(ch: char) -> bool {
    ch.is_alphanumeric()
}

fn merge_style_spans(mut spans: Vec<StyleSpan>) -> Vec<StyleSpan> {
    if spans.is_empty() {
        return Vec::new();
    }
    spans.sort_by_key(|span| span.start);
    let mut merged = vec![spans[0].clone()];
    for span in spans.into_iter().skip(1) {
        let last = merged.last_mut().expect("merged has at least one span");
        if span.style == last.style && span.start <= last.end {
            last.end = last.end.max(span.end);
        } else {
            merged.push(span);
        }
    }
    merged
}

fn resolve_block_colors(
    screenshot: Option<&OverlayScreenshot>,
    bounds: Rect,
    first_style: Option<&TextStyle>,
    background_mode: BackgroundMode,
) -> Result<OverlayColors, String> {
    if screenshot.is_none() {
        let fixed_colors = match background_mode {
            BackgroundMode::WhiteOnBlack => Some(OverlayColors {
                background_argb: 0xFF00_0000,
                foreground_argb: 0xFFFF_FFFF,
            }),
            BackgroundMode::BlackOnWhite => Some(OverlayColors {
                background_argb: 0xFFFF_FFFF,
                foreground_argb: 0xFF00_0000,
            }),
            BackgroundMode::AutoDetect => None,
        };

        if let Some(colors) = fixed_colors {
            return Ok(colors);
        }
    }

    let sampled_colors = match screenshot {
        Some(screenshot) => Some(sample_overlay_colors(
            &screenshot.rgba_bytes,
            screenshot.width,
            screenshot.height,
            bounds,
            background_mode,
            None,
        )?),
        None => None,
    };

    let style_fg = first_style.and_then(TextStyle::normalized_text_color);
    let style_bg = first_style
        .filter(|style| style.has_real_background())
        .and_then(|style| style.bg_color);

    if let Some(background_argb) = style_bg {
        return Ok(OverlayColors {
            background_argb,
            foreground_argb: style_fg
                .or_else(|| sampled_colors.map(|colors| colors.foreground_argb))
                .unwrap_or(0xFF00_0000),
        });
    }

    if let Some(sampled_colors) = sampled_colors {
        return Ok(OverlayColors {
            background_argb: sampled_colors.background_argb,
            foreground_argb: style_fg.unwrap_or(sampled_colors.foreground_argb),
        });
    }

    if let Some(foreground_argb) = style_fg {
        let luminance = translator_core::ocr::luminance(foreground_argb);
        let background_argb = if luminance > 0.5 {
            0xFF00_0000
        } else {
            0xFFFF_FFFF
        };
        return Ok(OverlayColors {
            background_argb,
            foreground_argb,
        });
    }

    Ok(OverlayColors {
        background_argb: 0xFFFF_FFFF,
        foreground_argb: 0xFF00_0000,
    })
}

fn median_fragment_height(fragments: &[StyledFragment]) -> u32 {
    let mut heights = fragments
        .iter()
        .map(|fragment| fragment.bounding_box.height())
        .collect::<Vec<_>>();
    heights.sort_unstable();
    heights[heights.len() / 2].max(1)
}

fn lower_quartile_height(fragments: &[StyledFragment]) -> u32 {
    let mut heights = fragments
        .iter()
        .map(|fragment| fragment.bounding_box.height())
        .collect::<Vec<_>>();
    heights.sort_unstable();
    heights[heights.len() / 4].max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fragment(text: &str, left: u32, top: u32, right: u32, bottom: u32) -> StyledFragment {
        StyledFragment {
            text: text.into(),
            bounding_box: Rect {
                left,
                top,
                right,
                bottom,
            },
            style: None,
            layout_group: 0,
            translation_group: 0,
            cluster_group: 0,
            opaque: false,
        }
    }

    fn colored_fragment(
        text: &str,
        left: u32,
        top: u32,
        right: u32,
        bottom: u32,
        color: u32,
    ) -> StyledFragment {
        let mut fragment = fragment(text, left, top, right, bottom);
        fragment.style = Some(TextStyle {
            text_color: Some(color),
            bg_color: None,
            text_size: None,
            baseline_shift: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        });
        fragment
    }

    #[test]
    fn clusters_fragments_into_two_lines_one_block() {
        let fragments = vec![
            StyledFragment {
                text: "Hello world this is a wrapped paragraph".into(),
                bounding_box: Rect {
                    left: 0,
                    top: 0,
                    right: 240,
                    bottom: 20,
                },
                style: Some(TextStyle {
                    text_color: None,
                    bg_color: None,
                    text_size: None,
                    baseline_shift: None,
                    bold: false,
                    italic: false,
                    underline: false,
                    strikethrough: false,
                }),
                layout_group: 0,
                translation_group: 0,
                cluster_group: 0,
                opaque: false,
            },
            StyledFragment {
                text: "with a styled middle run".into(),
                bounding_box: Rect {
                    left: 248,
                    top: 0,
                    right: 390,
                    bottom: 20,
                },
                style: None,
                layout_group: 0,
                translation_group: 0,
                cluster_group: 0,
                opaque: false,
            },
            StyledFragment {
                text: "again on the next line".into(),
                bounding_box: Rect {
                    left: 0,
                    top: 28,
                    right: 160,
                    bottom: 48,
                },
                style: None,
                layout_group: 0,
                translation_group: 0,
                cluster_group: 0,
                opaque: false,
            },
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text,
            "Hello world this is a wrapped paragraph with a styled middle run\nagain on the next line"
        );
    }

    #[test]
    fn normalizes_pdf_negated_equals_combining_mark() {
        assert_eq!(
            normalize_pdf_math_sequences("tc.high qc \u{0338}= ⊥"),
            "tc.high qc ≠ ⊥"
        );
        assert_eq!(
            normalize_pdf_math_sequences("tc.high qc \u{0338} = ⊥"),
            "tc.high qc ≠ ⊥"
        );
        assert_eq!(
            normalize_pdf_math_sequences("tc.high qc =\u{0338} ⊥"),
            "tc.high qc ≠ ⊥"
        );
    }

    #[test]
    fn clusters_wrapped_prose_into_one_block() {
        let fragments = vec![
            fragment(
                "This paragraph contains enough words to look like prose",
                20,
                100,
                520,
                112,
            ),
            fragment(
                "and continues naturally on the next line.",
                20,
                115,
                360,
                127,
            ),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text,
            "This paragraph contains enough words to look like prose\nand continues naturally on the next line."
        );
    }

    #[test]
    fn does_not_cluster_aligned_record_rows() {
        let fragments = vec![
            fragment("Record 1001", 22, 100, 80, 112),
            fragment("Record 1002", 22, 116, 80, 128),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Record 1001");
        assert_eq!(blocks[1].text, "Record 1002");
    }

    #[test]
    fn does_not_cluster_same_line_table_value_gap() {
        let fragments = vec![
            fragment("Metric label", 20, 100, 124, 112),
            fragment("42", 140, 100, 164, 112),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Metric label");
        assert_eq!(blocks[1].text, "42");
    }

    #[test]
    fn still_clusters_same_line_word_gap() {
        let fragments = vec![
            fragment("part 1", 140, 100, 219, 112),
            fragment("part 2", 222, 100, 245, 112),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "part 1 part 2");
    }

    #[test]
    fn clusters_same_line_prose_with_wide_style_gap() {
        let fragments = vec![
            fragment("This prose line contains enough words", 72, 100, 280, 112),
            fragment("term", 306, 100, 330, 112),
            fragment("to continue after a styled gap.", 356, 100, 520, 112),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text,
            "This prose line contains enough words term to continue after a styled gap."
        );
    }

    #[test]
    fn clusters_enumerated_marker_with_same_line_text() {
        let fragments = vec![
            fragment("1.", 72, 100, 82, 112),
            fragment("A paragraph heading starts here", 108, 100, 280, 112),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "1. A paragraph heading starts here");
    }

    #[test]
    fn leading_ordered_list_marker_is_protected_from_translation() {
        let (prefix, offset, text) = split_protected_leading_list_marker("1. Finalize report.");

        assert_eq!(prefix, "1. ");
        assert_eq!(offset, 3);
        assert_eq!(text, "Finalize report.");
    }

    #[test]
    fn assemble_reattaches_protected_list_marker_before_translation() {
        let text = "1. Finalize report.";
        let source_block = TranslatableBlock {
            text: text.into(),
            bounds: Rect::default(),
            source_rects: Vec::new(),
            style_spans: Vec::new(),
            segments: vec![TranslationSegment {
                start: 0,
                end: text.len() as u32,
                translation_group: 0,
            }],
            opaque: false,
        };
        let translated = assemble_translated_blocks(
            std::slice::from_ref(&source_block),
            &[BlockSegmentTranslation {
                block_index: 0,
                segment: source_block.segments[0].clone(),
                translated_text: "Finalizar el informe.".into(),
                alignments: Vec::new(),
                source_offset: 3,
                output_prefix: "1. ".into(),
            }],
            None,
            BackgroundMode::BlackOnWhite,
        )
        .expect("assemble");

        assert_eq!(translated[0].text, "1. Finalizar el informe.");
    }

    #[test]
    fn prose_block_can_continue_with_short_next_line_fragment() {
        let fragments = vec![
            fragment(
                "This paragraph is already wide enough to be prose",
                72,
                100,
                420,
                112,
            ),
            fragment("short", 72, 116, 105, 128),
            fragment(
                "continuation after a justified line break.",
                130,
                116,
                360,
                128,
            ),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].text,
            "This paragraph is already wide enough to be prose\nshort continuation after a justified line break."
        );
    }

    #[test]
    fn list_item_marker_starts_new_vertical_block() {
        let fragments = vec![
            fragment("Components are defined as follows:", 72, 100, 340, 112),
            fragment("- first item has prose", 90, 116, 260, 128),
            fragment("continuation of first item", 108, 132, 300, 144),
            fragment("- second item starts separately", 90, 148, 330, 160),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].text, "Components are defined as follows:");
        assert_eq!(
            blocks[1].text,
            "- first item has prose\ncontinuation of first item"
        );
        assert_eq!(blocks[2].text, "- second item starts separately");
    }

    #[test]
    fn standalone_bullet_glyph_is_not_translated_inline() {
        let fragments = vec![
            fragment("Job Description:", 72, 100, 170, 112),
            fragment("●", 90, 118, 97, 124),
            fragment("Maintain infrastructure", 108, 116, 260, 128),
            fragment("●", 90, 142, 97, 148),
            fragment("Improve security", 108, 140, 230, 152),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].text, "Job Description:");
        assert_eq!(blocks[1].text, "Maintain infrastructure");
        assert_eq!(blocks[2].text, "Improve security");
    }

    #[test]
    fn adjacent_style_split_inside_word_does_not_insert_space() {
        let fragments = vec![
            fragment("blu", 10, 10, 28, 22),
            colored_fragment("e", 28, 10, 34, 22, 0xFF00_00FF),
            fragment(")", 34, 10, 38, 22),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "blue)");
        assert_eq!(blocks[0].style_spans[0].start, 3);
        assert_eq!(blocks[0].style_spans[0].end, 4);
    }

    #[test]
    fn style_split_between_words_still_inserts_space() {
        let fragments = vec![
            fragment("thin dashed", 10, 10, 70, 22),
            colored_fragment("blue", 76, 10, 100, 22, 0xFF00_00FF),
        ];

        let blocks = cluster_fragments_into_blocks(&fragments);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "thin dashed blue");
        assert_eq!(blocks[0].style_spans[0].start, 12);
        assert_eq!(blocks[0].style_spans[0].end, 16);
    }

    #[test]
    fn mapped_style_span_expands_to_target_word_boundaries() {
        let source_style = TextStyle {
            text_color: Some(0xFF00_00FF),
            bg_color: None,
            text_size: None,
            baseline_shift: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        };
        let source_block = TranslatableBlock {
            text: "blue".into(),
            bounds: Rect::default(),
            source_rects: Vec::new(),
            style_spans: vec![StyleSpan {
                start: 0,
                end: 4,
                style: Some(source_style.clone()),
            }],
            segments: vec![TranslationSegment {
                start: 0,
                end: 4,
                translation_group: 0,
            }],
            opaque: false,
        };
        let segment = source_block.segments[0].clone();

        let spans = map_styles_to_segmented_translation(
            &source_block,
            &[(
                segment.clone(),
                vec![TokenAlignment {
                    src_begin: 3,
                    src_end: 4,
                    tgt_begin: 3,
                    tgt_end: 4,
                }],
            )],
            &[(segment, "azul)".into())],
        );

        assert_eq!(
            spans,
            vec![StyleSpan {
                start: 0,
                end: 4,
                style: Some(source_style),
            }]
        );
    }

    #[test]
    fn mapped_color_span_does_not_absorb_punctuation_or_next_word() {
        let blue_style = TextStyle {
            text_color: Some(0xFF00_00FF),
            bg_color: None,
            text_size: None,
            baseline_shift: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
        };
        let source_block = TranslatableBlock {
            text: "thin dashed blue) and thick dashed blue)".into(),
            bounds: Rect::default(),
            source_rects: Vec::new(),
            style_spans: vec![
                StyleSpan {
                    start: 12,
                    end: 16,
                    style: Some(blue_style.clone()),
                },
                StyleSpan {
                    start: 35,
                    end: 39,
                    style: Some(blue_style.clone()),
                },
            ],
            segments: vec![TranslationSegment {
                start: 0,
                end: 40,
                translation_group: 0,
            }],
            opaque: false,
        };
        let segment = source_block.segments[0].clone();

        let spans = map_styles_to_segmented_translation(
            &source_block,
            &[(
                segment.clone(),
                vec![
                    TokenAlignment {
                        src_begin: 12,
                        src_end: 16,
                        tgt_begin: 12,
                        tgt_end: 21,
                    },
                    TokenAlignment {
                        src_begin: 35,
                        src_end: 39,
                        tgt_begin: 35,
                        tgt_end: 39,
                    },
                ],
            )],
            &[(segment, "thin dashed blue) and thick dashed blue)".into())],
        );

        assert_eq!(
            spans,
            vec![
                StyleSpan {
                    start: 12,
                    end: 16,
                    style: Some(blue_style.clone()),
                },
                StyleSpan {
                    start: 35,
                    end: 39,
                    style: Some(blue_style),
                },
            ]
        );
    }
}
