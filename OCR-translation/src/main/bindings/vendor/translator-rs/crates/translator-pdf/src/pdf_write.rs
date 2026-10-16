//! Writeback orchestrator: take translated pages and the original PDF bytes,
//! emit a new PDF where the original text has been **surgically removed**
//! from the content stream and the translated text drawn in its place.
//!
//! The surgery walks the page's content stream operators tracking the text
//! matrix (Tm/Td/TD/T*), graphics state stack (q/Q), and CTM (cm). For each
//! text-show operator (Tj/TJ/'/"), we compute the origin point in PDF user
//! space; if it falls inside any translated block's bbox, we drop the op.
//! Once originals are gone there's no overpaint hack — we just append a
//! translated-text-only stream.
//!
//! The pipeline is split across modules:
//! - [`crate::pdf_surgery`] owns the read+filter pass.
//! - [`crate::pdf_overlay`] owns wrap/fit/emit of the translated text.
//! - [`crate::pdf_resources`] owns the page-resource bookkeeping.
//! - This file is the orchestrator and the shared style types.

use std::collections::{HashMap, HashSet};

use lopdf::{Document, ObjectId};

use crate::pdf::PageTranslationResult;
use crate::pdf::PdfError;
use crate::pdf_content::{BoldItalic, FontStyleFlags, Matrix, PageGeometry, UserRect};
use crate::pdf_overlay::{
    BlockResources, COURIER_AVG_ADVANCE, HELVETICA_AVG_ADVANCE, build_overlay_stream,
};
use crate::pdf_resources::{
    append_content_stream, attach_embedded_fonts_to_page, ensure_fonts_in_page_resources,
    prune_link_annotations, prune_unused_fonts,
};
use crate::pdf_surgery::{CapturedTextShow, rewrite_page_content};
use crate::styled::TranslatedStyledBlock;
use translator_core::ocr::Rect;

/// PDF resource names for our font variants. All eight are PDF standard-14
/// base fonts, so no embedding is needed.
pub(crate) const HELVETICA_REGULAR: &[u8] = b"TrHelv";
pub(crate) const HELVETICA_BOLD: &[u8] = b"TrHelvB";
pub(crate) const HELVETICA_OBLIQUE: &[u8] = b"TrHelvI";
pub(crate) const HELVETICA_BOLD_OBLIQUE: &[u8] = b"TrHelvBI";
pub(crate) const COURIER_REGULAR: &[u8] = b"TrCour";
pub(crate) const COURIER_BOLD: &[u8] = b"TrCourB";
pub(crate) const COURIER_OBLIQUE: &[u8] = b"TrCourI";
pub(crate) const COURIER_BOLD_OBLIQUE: &[u8] = b"TrCourBI";

const STANDARD_14_RESOURCE_NAMES: [&[u8]; 8] = [
    HELVETICA_REGULAR,
    HELVETICA_BOLD,
    HELVETICA_OBLIQUE,
    HELVETICA_BOLD_OBLIQUE,
    COURIER_REGULAR,
    COURIER_BOLD,
    COURIER_OBLIQUE,
    COURIER_BOLD_OBLIQUE,
];

/// Visual style sampled from the original Tjs in a removal rect: enough to
/// pick a Standard-14 fallback font, a fill colour, and a target font size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BlockTypography {
    pub(crate) flags: FontStyleFlags,
    /// Non-stroking fill colour as RGB in `[0, 1]`.
    pub(crate) fill_rgb: (f32, f32, f32),
    /// Median font size (in points) of the dropped Tjs, or `None` if no
    /// Tjs were dropped for this rect.
    pub(crate) font_size: Option<f32>,
}

impl Default for BlockTypography {
    fn default() -> Self {
        Self {
            flags: FontStyleFlags::default(),
            fill_rgb: (0.0, 0.0, 0.0),
            font_size: None,
        }
    }
}

impl BlockTypography {
    pub(crate) fn font_resource_for(flags: FontStyleFlags) -> &'static [u8] {
        match (flags.monospace, flags.bold, flags.italic) {
            (true, true, true) => COURIER_BOLD_OBLIQUE,
            (true, true, false) => COURIER_BOLD,
            (true, false, true) => COURIER_OBLIQUE,
            (true, false, false) => COURIER_REGULAR,
            (false, true, true) => HELVETICA_BOLD_OBLIQUE,
            (false, true, false) => HELVETICA_BOLD,
            (false, false, true) => HELVETICA_OBLIQUE,
            (false, false, false) => HELVETICA_REGULAR,
        }
    }
}

/// Where the translated block should be drawn: top-left baseline anchor,
/// per-line continuation anchors (preserving hanging indents), text
/// orientation reused from the producer.
#[derive(Debug, Clone)]
pub(crate) struct BlockGeometry {
    /// User-space baseline-leading-edge anchor of the visually-top-left line
    /// among the dropped Tjs. Used to place the first translated line at the
    /// same baseline as the original.
    pub(crate) anchor: Option<(f32, f32)>,
    /// Number of distinct visual baselines across the dropped Tjs — i.e.
    /// how many lines the original text actually occupied. More reliable
    /// than deriving from bbox height.
    pub(crate) original_line_count: usize,
    /// Linear (rotation/scale/skew) part of the original Tj's combined
    /// `text_matrix × CTM`, with the translation column zeroed. We reuse it
    /// for our emitted glyphs so the new text inherits the producer's
    /// orientation (essential for /Rotate-compensating producers and Y-flip
    /// top-left-origin streams). Defaults to identity when no samples are
    /// available.
    pub(crate) text_orientation: Matrix,
    /// Visually top-to-bottom baseline starts sampled from the original text.
    /// These preserve hanging indents where continuation lines start further
    /// left than the first line.
    pub(crate) line_anchors: Vec<(f32, f32)>,
}

impl Default for BlockGeometry {
    fn default() -> Self {
        Self {
            anchor: None,
            original_line_count: 0,
            text_orientation: Matrix::identity(),
            line_anchors: Vec::new(),
        }
    }
}

/// Pair of [`BlockTypography`] (presentation) and [`BlockGeometry`]
/// (positioning) sampled from the original Tjs in a removal rect.
#[derive(Debug, Clone, Default)]
pub(crate) struct SampledBlockStyle {
    pub(crate) typography: BlockTypography,
    pub(crate) geometry: BlockGeometry,
}

#[derive(Debug)]
pub enum PdfWriteError {
    Lopdf(lopdf::Error),
    Pdf(PdfError),
    Io(std::io::Error),
    PageIndexNotFound { index: usize },
    PageResourcesCycle { object: ObjectId },
}

impl From<lopdf::Error> for PdfWriteError {
    fn from(value: lopdf::Error) -> Self {
        Self::Lopdf(value)
    }
}

impl From<PdfError> for PdfWriteError {
    fn from(value: PdfError) -> Self {
        Self::Pdf(value)
    }
}

impl From<std::io::Error> for PdfWriteError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl std::fmt::Display for PdfWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lopdf(err) => write!(f, "lopdf: {err}"),
            Self::Pdf(err) => write!(f, "{err}"),
            Self::Io(err) => write!(f, "io: {err}"),
            Self::PageIndexNotFound { index } => {
                write!(
                    f,
                    "translation refers to page index {index} which does not exist"
                )
            }
            Self::PageResourcesCycle { object } => {
                write!(
                    f,
                    "cycle while resolving page resources at object {object:?}"
                )
            }
        }
    }
}

impl std::error::Error for PdfWriteError {}

/// One page's worth of surgery output: where the originals were, what they
/// looked like, the producer's still-active CTM at the end of the stream,
/// and the page's geometry. Carried forward to the overlay pass.
struct PageWork<'a> {
    page_id: ObjectId,
    translation: &'a PageTranslationResult,
    layout_rects: Vec<UserRect>,
    block_styles: Vec<SampledBlockStyle>,
    captured_text: Vec<Vec<CapturedTextShow>>,
    final_ctm: Matrix,
    geom: PageGeometry,
}

type FontKey = (
    translator_render::font_provider::FontRequest,
    translator_render::font_provider::FontHandle,
);

/// Document-wide font resolution: each unique `(FontRequest, FontHandle)`
/// has a parsed [`FontMetrics`] (covering the union of texts that need it)
/// and an embedded subset already added to the [`Document`].
struct FontPlan {
    metrics: HashMap<FontKey, translator_render::font_metrics::FontMetrics>,
    embeds: HashMap<FontKey, crate::pdf_font_embed::EmbeddedFont>,
}

/// Build a translated PDF by removing the original text from each page and
/// appending the translated text in the same bbox positions.
///
/// `fonts` is consulted for non-Standard-14 scripts and for accurate wrap
/// metrics. Pass [`translator_render::font_provider::NoFontProvider`] (or `&|_| None`) to
/// keep the current Standard-14-only behavior.
pub fn write_translated_pdf(
    original_pdf_bytes: &[u8],
    translations: &[PageTranslationResult],
    fonts: &dyn translator_render::font_provider::FontProvider,
) -> Result<Vec<u8>, PdfWriteError> {
    let mut doc = Document::load_mem(original_pdf_bytes)?;

    // Resource names we installed; only these are eligible for pruning. The
    // brittle alternative — pattern-matching on `b"Tr"` prefix — could pick up
    // pre-existing fonts in the source PDF that happened to share a prefix.
    let mut installed_font_names: HashSet<Vec<u8>> = STANDARD_14_RESOURCE_NAMES
        .iter()
        .map(|n| n.to_vec())
        .collect();

    let (works, modified_pages) = run_surgery(&mut doc, translations)?;
    let plan = build_font_plan(&mut doc, &works, fonts);
    emit_pages(&mut doc, &works, &plan, fonts, &mut installed_font_names)?;
    prune_unused_fonts(&mut doc, &modified_pages, &installed_font_names)?;

    let mut out = Vec::new();
    doc.save_to(&mut out)?;
    Ok(out)
}

/// Pass 1: walk every translation, run content-stream surgery, install the
/// standard-14 fallback fonts on each touched page. Returns the per-page
/// work plus the set of touched page IDs (used by the eventual prune pass).
fn run_surgery<'a>(
    doc: &mut Document,
    translations: &'a [PageTranslationResult],
) -> Result<(Vec<PageWork<'a>>, HashSet<ObjectId>), PdfWriteError> {
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    let mut works: Vec<PageWork<'a>> = Vec::new();
    let mut modified_pages = HashSet::new();

    for translation in translations {
        let Some((_, page_id)) = pages
            .iter()
            .find(|(num, _)| (*num as usize).saturating_sub(1) == translation.page_index)
        else {
            return Err(PdfWriteError::PageIndexNotFound {
                index: translation.page_index,
            });
        };
        if translation.blocks.is_empty() {
            continue;
        }
        let geom = PageGeometry::read(doc, *page_id, Some(translation.page));
        // Tighten each block's right edge so it doesn't extend into a
        // sibling block's column. mupdf's per-block ink bbox can creep
        // a few points into the gap between columns; with longer
        // translated text those few points become real visual overlap
        // (Dutch is ~30% wider than English on average). Source rects
        // for removal stay at their original positions; only the
        // layout rect used for wrap-width is shrunk.
        let layout_bboxes = clamp_block_bounds_to_columns(&translation.blocks);
        let layout_rects: Vec<UserRect> = layout_bboxes
            .iter()
            .map(|bb| geom.user_rect_from_display(*bb))
            .collect();
        let removal_rects: Vec<Vec<UserRect>> = translation
            .blocks
            .iter()
            .map(|b| {
                let source_rects = if b.source_rects.is_empty() {
                    std::slice::from_ref(&b.bounding_box)
                } else {
                    b.source_rects.as_slice()
                };
                source_rects
                    .iter()
                    .map(|rect| geom.user_rect_from_display(*rect))
                    .collect()
            })
            .collect();
        let display_removal_rects: Vec<Vec<Rect>> = translation
            .blocks
            .iter()
            .map(|b| {
                if b.source_rects.is_empty() {
                    vec![b.bounding_box]
                } else {
                    b.source_rects.clone()
                }
            })
            .collect();
        let capture_text: Vec<bool> = translation.blocks.iter().map(|b| b.opaque).collect();
        let (final_ctm, block_styles, captured_text) = rewrite_page_content(
            doc,
            *page_id,
            &removal_rects,
            &display_removal_rects,
            &capture_text,
            geom,
        )?;
        let flat_removal_rects = removal_rects.iter().flatten().copied().collect::<Vec<_>>();
        prune_link_annotations(doc, *page_id, &flat_removal_rects)?;
        ensure_fonts_in_page_resources(doc, *page_id)?;
        modified_pages.insert(*page_id);
        works.push(PageWork {
            page_id: *page_id,
            translation,
            layout_rects,
            block_styles,
            captured_text,
            final_ctm,
            geom,
        });
    }

    Ok((works, modified_pages))
}

/// For each block, return its bounding box clipped so the right edge
/// doesn't intrude into any sibling block whose vertical extent
/// overlaps. Two-column pages routinely have left-column blocks whose
/// ink bbox reaches a few points into the gap between columns
/// (mupdf includes glyph slants/extents); the english source fits
/// because lines don't actually fill that bbox, but a translated line
/// in a wider language does. By clamping at write time we keep
/// translated text from spilling past the column gap.
///
/// `source_rects` are left untouched so removal of original Tjs still
/// covers every glyph.
fn clamp_block_bounds_to_columns(
    blocks: &[TranslatedStyledBlock],
) -> Vec<translator_core::ocr::Rect> {
    /// Extra breathing room (display-space points) we shave off the
    /// clipped right edge so columns don't merely touch.
    const COLUMN_MARGIN: u32 = 2;
    let mut out: Vec<translator_core::ocr::Rect> = blocks.iter().map(|b| b.bounding_box).collect();
    for i in 0..blocks.len() {
        let me = blocks[i].bounding_box;
        let mut new_right = me.right;
        for j in 0..blocks.len() {
            if i == j {
                continue;
            }
            let other = blocks[j].bounding_box;
            // Only clip against blocks that sit at the same vertical
            // stripe (could share a row with us) AND start strictly
            // to our right.
            let y_overlap = me
                .bottom
                .min(other.bottom)
                .saturating_sub(me.top.max(other.top));
            if y_overlap == 0 {
                continue;
            }
            if other.left <= me.left {
                continue;
            }
            // The other block must also extend past our right edge
            // for this to be a sibling-column conflict. A bullet item
            // indented inside our paragraph (e.g. block at x=301..542
            // sitting under a paragraph at x=291..543) starts to our
            // right but lives entirely inside our column; clipping
            // our right edge to its left would crush our paragraph
            // down to the bullet's indent gap.
            if other.right <= me.right {
                continue;
            }
            if other.left < new_right {
                new_right = other.left;
            }
        }
        if new_right < me.right {
            out[i].right = new_right.saturating_sub(COLUMN_MARGIN).max(me.left + 1);
        }
    }
    out
}

/// Passes 2-4: walk every block, build the document-wide union of texts per
/// `(FontRequest, FontHandle)`, parse each unique font once, embed each
/// unique font once. Returns the cached metrics + embeds.
fn build_font_plan(
    doc: &mut Document,
    works: &[PageWork<'_>],
    fonts: &dyn translator_render::font_provider::FontProvider,
) -> FontPlan {
    use translator_render::font_provider::FontRequest;

    // Pass 2: union of texts per (req, handle).
    let mut union_text: HashMap<FontKey, String> = HashMap::new();
    for work in works {
        let script = work.translation.target_language.script;
        for (block, style) in work.translation.blocks.iter().zip(work.block_styles.iter()) {
            for variant in block_variants(block, style) {
                let req = FontRequest {
                    script,
                    language: work.translation.target_language.as_str().to_string(),
                    bold: variant.bold,
                    italic: variant.italic,
                    monospace: style.typography.flags.monospace,
                };
                if let Some(handle) = fonts.locate(&req).into_iter().next() {
                    union_text
                        .entry((req, handle))
                        .or_default()
                        .push_str(&block.text);
                }
            }
        }
    }

    // Pass 3: parse each unique font once with its union text.
    let mut metrics: HashMap<FontKey, translator_render::font_metrics::FontMetrics> =
        HashMap::new();
    for (key, text) in &union_text {
        let (_req, handle) = key;
        match translator_render::font_metrics::FontMetrics::from_file_for_text(
            &handle.path,
            handle.ttc_index,
            text,
        ) {
            Ok(m) => {
                metrics.insert(key.clone(), m);
            }
            Err(e) => {
                eprintln!(
                    "[pdf_write] could not parse {} (ttc_index={}): {e}",
                    handle.path.display(),
                    handle.ttc_index,
                );
            }
        }
    }

    // Pass 4: embed each unique font once.
    let mut embeds: HashMap<FontKey, crate::pdf_font_embed::EmbeddedFont> = HashMap::new();
    let mut next_slot = 0usize;
    for (key, font_metrics) in &metrics {
        if let Some(e) = crate::pdf_font_embed::embed_font(doc, font_metrics, next_slot) {
            embeds.insert(key.clone(), e);
            next_slot += 1;
        }
    }

    FontPlan { metrics, embeds }
}

/// Pass 5: per-page resolve `BlockResources` from the [`FontPlan`], attach
/// embeds to the page, build the overlay content stream, and append it.
/// Adds every embed resource name to `installed_font_names` so the final
/// prune pass knows it owns them.
fn emit_pages(
    doc: &mut Document,
    works: &[PageWork<'_>],
    plan: &FontPlan,
    fonts: &dyn translator_render::font_provider::FontProvider,
    installed_font_names: &mut HashSet<Vec<u8>>,
) -> Result<(), PdfWriteError> {
    use translator_render::font_provider::FontRequest;

    let helvetica_fallback =
        translator_render::font_metrics::FontMetrics::approx(HELVETICA_AVG_ADVANCE);
    let courier_fallback =
        translator_render::font_metrics::FontMetrics::approx(COURIER_AVG_ADVANCE);

    for work in works {
        let mut block_resources: Vec<BlockResources> =
            Vec::with_capacity(work.translation.blocks.len());
        let script = work.translation.target_language.script;
        for (block, style) in work.translation.blocks.iter().zip(work.block_styles.iter()) {
            let mut by_flags: HashMap<
                BoldItalic,
                (
                    translator_render::font_metrics::FontMetrics,
                    Option<crate::pdf_font_embed::EmbeddedFont>,
                ),
            > = HashMap::new();
            for variant in block_variants(block, style) {
                let req = FontRequest {
                    script,
                    language: work.translation.target_language.as_str().to_string(),
                    bold: variant.bold,
                    italic: variant.italic,
                    monospace: style.typography.flags.monospace,
                };
                let key = fonts
                    .locate(&req)
                    .into_iter()
                    .next()
                    .map(|handle| (req, handle));
                let metrics_for_seg = key
                    .as_ref()
                    .and_then(|k| plan.metrics.get(k).cloned())
                    .unwrap_or_else(|| {
                        if style.typography.flags.monospace {
                            courier_fallback.clone()
                        } else {
                            helvetica_fallback.clone()
                        }
                    });
                let embed = key.as_ref().and_then(|k| plan.embeds.get(k).cloned());
                by_flags.insert(variant, (metrics_for_seg, embed));
            }
            block_resources.push(BlockResources {
                by_flags,
                default_flags: BoldItalic {
                    bold: style.typography.flags.bold,
                    italic: style.typography.flags.italic,
                },
                monospace: style.typography.flags.monospace,
            });
        }
        let block_embeds: Vec<Option<crate::pdf_font_embed::EmbeddedFont>> = block_resources
            .iter()
            .flat_map(|r| r.by_flags.values().map(|(_, e)| e.clone()))
            .collect();
        for embed in block_embeds.iter().flatten() {
            installed_font_names.insert(embed.resource_name.clone());
        }
        attach_embedded_fonts_to_page(doc, work.page_id, &block_embeds)?;
        let overlay_stream = build_overlay_stream(
            &work.translation.blocks,
            &work.layout_rects,
            &work.block_styles,
            &block_resources,
            &work.captured_text,
            work.geom,
            work.final_ctm,
        );
        append_content_stream(doc, work.page_id, overlay_stream)?;
    }

    Ok(())
}

/// Bold/italic variants needed for a block: its dominant style plus every
/// distinct variant present in its `style_spans`.
fn block_variants(block: &TranslatedStyledBlock, style: &SampledBlockStyle) -> HashSet<BoldItalic> {
    let mut variants = HashSet::new();
    variants.insert(BoldItalic {
        bold: style.typography.flags.bold,
        italic: style.typography.flags.italic,
    });
    for span in &block.style_spans {
        if let Some(s) = &span.style {
            variants.insert(BoldItalic {
                bold: s.bold,
                italic: s.italic,
            });
        }
    }
    variants
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_translations_does_not_touch_pdf() {
        let result =
            write_translated_pdf(b"", &[], &translator_render::font_provider::NoFontProvider);
        assert!(result.is_err());
    }
}
