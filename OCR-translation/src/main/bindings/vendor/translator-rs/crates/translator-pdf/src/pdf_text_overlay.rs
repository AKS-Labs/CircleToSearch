//! PDF-text overlay for the page-raster fallback.
//!
//! The text-extraction translator can't see anything in PDFs whose body
//! is outlined vector glyphs (CorelDRAW exports, design-tool PDFs). The
//! page-raster fallback rasterizes such pages and runs them through OCR
//! to obtain a translation plan: per-line bboxes (in raster pixel space),
//! per-line foreground/background colors, and a suggested font size in
//! pixels.
//!
//! This module turns that plan into a PDF content stream that:
//!
//! 1. paints the per-line `background_argb` over each source-text bbox
//!    (covering the original outlined glyphs that are still drawn by the
//!    page's existing content stream);
//! 2. emits the translated text in `foreground_argb` using an embedded
//!    Type-0 / CIDFontType2 font subset, sized to fit the bbox.
//!
//! The original `/Contents` stream is *kept* — non-text vector content
//! (logos, rules, photos, illustrations) survives untouched. We append
//! the overlay stream to `/Contents`, which lopdf turns into an array.
//!
//! Font handling is done with a per-pass [`OverlayFontPlan`] that mirrors
//! the structure of [`crate::pdf_write::FontPlan`]: doc-wide collection
//! of the union of translated text per `(FontRequest, FontHandle)`,
//! parse each unique font once, embed each unique font once. Pages
//! attach the embed names they actually use.

use std::collections::{HashMap, HashSet};

use log::warn;
use lopdf::{Document, ObjectId};

use crate::pdf_content::{ContentStreamBuilder, Matrix, PageGeometry, UserRect};
use crate::pdf_font_embed::{EmbeddedFont, embed_font};
use crate::pdf_resources::{append_content_stream, attach_embedded_fonts_to_page};
use translator_core::ocr::{
    OverlayLayoutMode, PreparedImageOverlay, PreparedTextBlock, PreparedTextLine,
};
use translator_render::font_metrics::FontMetrics;
use translator_render::font_provider::{FontHandle, FontProvider, FontRequest};

/// Approximate average advance for the Helvetica fallback (em fraction).
const HELVETICA_AVG_ADVANCE: f32 = 0.5;

/// Multipliers from OCR pixel-bbox height (in pt) to PDF font size,
/// chosen by the source line's typographic profile. Tesseract's bbox
/// tracks the visible glyph ink: caps ≈ 0.7 em, x-height ≈ 0.5 em,
/// cap-line to descender ≈ 0.85-1.0 em. We use the OCR-recognised text
/// of the *source* line to pick which case applies — letting headers
/// render at the right visual weight without making body text
/// overflow its row.
const FONT_SIZE_MULT_DESCENDER: f32 = 1.0;
const FONT_SIZE_MULT_LOWER_NO_DESC: f32 = 1.25;
const FONT_SIZE_MULT_CAPS_OR_OTHER: f32 = 1.43;

/// Padding (in pt) added around each mask rectangle so source vector
/// descenders / italic flourishes that exceed the OCR bbox don't peek
/// through.
const MASK_PADDING_PT: f32 = 0.6;

/// Floor for shrink-to-fit. Below this the text is unreadable; we accept
/// minor overflow instead.
const MIN_FIT_FONT_SIZE_PT: f32 = 4.0;

/// Tolerated horizontal overhang past the bbox right edge before we
/// shrink the font.
const OVERHANG_TOLERANCE: f32 = 1.05;

/// One page's complete OCR-derived translation plan, ready to be turned
/// into an overlay content stream. Workers in the page-raster pass build
/// these (no lopdf state touched) and hand them to the main thread.
pub struct OverlayPage {
    pub page_index: usize,
    pub geom: PageGeometry,
    pub dpi: f32,
    pub overlay: PreparedImageOverlay,
}

/// Per-pass font dedup. Mirrors `pdf_write::FontPlan` but is local to one
/// run of the page-raster overlay pass.
pub struct OverlayFontPlan {
    pub metrics: HashMap<(FontRequest, FontHandle), FontMetrics>,
    pub embeds: HashMap<(FontRequest, FontHandle), EmbeddedFont>,
    pub primary_handle: Option<FontHandle>,
    pub primary_request: FontRequest,
}

/// Pass 2-4 (doc-wide): walk every overlay's translated text, collect
/// the union per `(FontRequest, FontHandle)`, parse each unique font
/// once with the union, embed each unique font once. The overlay path
/// only needs one variant (regular) since we don't recover bold/italic.
pub fn build_overlay_font_plan(
    doc: &mut Document,
    overlays: &[OverlayPage],
    target_language: &translator_core::api::ScriptedLanguage,
    fonts: &dyn FontProvider,
) -> OverlayFontPlan {
    let script = target_language.script;
    let primary_request = FontRequest {
        script,
        language: target_language.as_str().to_string(),
        bold: false,
        italic: false,
        monospace: false,
    };

    let primary_handle = fonts.locate(&primary_request).into_iter().next();

    let mut union_text: HashMap<(FontRequest, FontHandle), String> = HashMap::new();
    if let Some(handle) = primary_handle.clone() {
        let key = (primary_request.clone(), handle);
        let buf = union_text.entry(key).or_default();
        for page in overlays {
            for block in &page.overlay.blocks {
                buf.push_str(&block.translated_text);
                buf.push('\n');
            }
        }
    }

    let mut metrics: HashMap<(FontRequest, FontHandle), FontMetrics> = HashMap::new();
    for ((req, handle), text) in &union_text {
        match FontMetrics::from_file_for_text(&handle.path, handle.ttc_index, text) {
            Ok(m) => {
                metrics.insert((req.clone(), handle.clone()), m);
            }
            Err(e) => {
                warn!(
                    "[pdf_text_overlay] could not parse {} (ttc_index={}): {e}",
                    handle.path.display(),
                    handle.ttc_index,
                );
            }
        }
    }

    let mut embeds: HashMap<(FontRequest, FontHandle), EmbeddedFont> = HashMap::new();
    let mut next_slot = 0usize;
    for (key, font_metrics) in &metrics {
        if let Some(e) = embed_font(doc, font_metrics, next_slot) {
            embeds.insert(key.clone(), e);
            next_slot += 1;
        }
    }

    OverlayFontPlan {
        metrics,
        embeds,
        primary_handle,
        primary_request,
    }
}

/// Build the overlay content stream for one page. Emits, in this order:
///
/// 1. `q ... Q` save/restore wrapper.
/// 2. For each line in each block: a filled rectangle in `background_argb`
///    covering the source bbox (plus a small pad).
/// 3. For each line: translated text in `foreground_argb` at the
///    OCR-suggested size (shrunk to fit if it overflows the bbox).
///
/// All coordinates are in PDF user space, derived from the raster pixel
/// space via `page_geom_for_overlay`.
pub fn build_page_overlay_stream(page: &OverlayPage, plan: &OverlayFontPlan) -> Vec<u8> {
    let mut builder = ContentStreamBuilder::new();
    builder.save_state();

    let metrics = plan
        .primary_handle
        .as_ref()
        .and_then(|h| plan.metrics.get(&(plan.primary_request.clone(), h.clone())));
    let embed = plan
        .primary_handle
        .as_ref()
        .and_then(|h| plan.embeds.get(&(plan.primary_request.clone(), h.clone())));
    let fallback_metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
    let active_metrics = metrics.unwrap_or(&fallback_metrics);

    // Pass A: paint mask rectangles over every source-text bbox.
    for block in &page.overlay.blocks {
        for line in &block.lines {
            let user_rect = pixel_rect_to_user(line.bounding_box, page);
            if !user_rect_is_drawable(user_rect) {
                continue;
            }
            let (r, g, b) = argb_to_rgb(line.background_argb);
            let padded = pad_user_rect(user_rect, MASK_PADDING_PT);
            emit_filled_rect(&mut builder, padded, (r, g, b));
        }
    }

    // Pass B: emit translated text per block.
    for block in &page.overlay.blocks {
        emit_block_text(&mut builder, block, page, active_metrics, embed);
    }

    builder.restore_state();
    builder.finish()
}

/// Attach the overlay's embedded fonts to the page's `/Resources/Font`
/// dict and append the overlay stream to its `/Contents` array.
pub fn install_overlay_on_page(
    doc: &mut Document,
    page_id: ObjectId,
    overlay_stream: Vec<u8>,
    embeds_used: &HashSet<Vec<u8>>,
    plan: &OverlayFontPlan,
) -> Result<(), lopdf::Error> {
    if !embeds_used.is_empty() {
        let embeds: Vec<Option<EmbeddedFont>> = plan
            .embeds
            .values()
            .filter(|e| embeds_used.contains(&e.resource_name))
            .cloned()
            .map(Some)
            .collect();
        attach_embedded_fonts_to_page(doc, page_id, &embeds).map_err(translate_pdf_write_err)?;
    }
    append_content_stream(doc, page_id, overlay_stream).map_err(translate_pdf_write_err)?;
    Ok(())
}

fn translate_pdf_write_err(err: crate::pdf_write::PdfWriteError) -> lopdf::Error {
    match err {
        crate::pdf_write::PdfWriteError::Lopdf(e) => e,
        other => lopdf::Error::Syntax(format!("{other}")),
    }
}

fn emit_block_text(
    builder: &mut ContentStreamBuilder,
    block: &PreparedTextBlock,
    page: &OverlayPage,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
) {
    let text = block.translated_text.trim();
    if text.is_empty() || block.lines.is_empty() {
        return;
    }
    match block.layout_hints.layout_mode {
        OverlayLayoutMode::PerLine => {
            emit_per_line(builder, block, page, metrics, embed);
        }
        OverlayLayoutMode::VerticalBlockRect => {
            emit_block_rect(builder, block, page, metrics, embed);
        }
    }
}

/// One translated string per source line. Used for left-to-right
/// horizontal text, which is what tesseract emits as separate
/// PreparedTextLines.
fn emit_per_line(
    builder: &mut ContentStreamBuilder,
    block: &PreparedTextBlock,
    page: &OverlayPage,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
) {
    let line_count = block.lines.len();
    let block_translated = block.translated_text.trim();
    if block_translated.is_empty() {
        return;
    }
    let translated_lines = redistribute_lines(block_translated, line_count);
    let foreground_argb = block_foreground(block);
    for (line, src_line) in translated_lines.iter().zip(block.lines.iter()) {
        if line.is_empty() {
            continue;
        }
        emit_line(
            builder,
            line,
            src_line,
            page,
            metrics,
            embed,
            foreground_argb,
        );
    }
}

/// Translated text fills the block as one wrapped horizontal paragraph.
/// The image renderer draws VerticalBlockRect blocks rotated 90° CW; this
/// PDF path keeps them horizontal because rotated text matrices aren't
/// wired up here, and the PDF image pipeline only requests LeftToRight
/// today so the arm is effectively unreachable.
fn emit_block_rect(
    builder: &mut ContentStreamBuilder,
    block: &PreparedTextBlock,
    page: &OverlayPage,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
) {
    // Take the union bbox of all lines as the wrap target.
    let mut bbox = block.bounding_box;
    for line in &block.lines {
        bbox.union(line.bounding_box);
    }
    let user_rect = pixel_rect_to_user(bbox, page);
    if !user_rect_is_drawable(user_rect) {
        return;
    }

    let mut font_size =
        font_size_from_pixel_height(block.layout_hints.suggested_font_size_px, page.dpi);
    let max_width = (user_rect.x1 - user_rect.x0).max(1.0);
    let max_height = (user_rect.y1 - user_rect.y0).max(1.0);
    let translated = block.translated_text.trim();
    let mut wrapped = wrap_to_width(translated, font_size, max_width, metrics);
    while wrapped.len() as f32 * font_size * 1.15 > max_height && font_size > MIN_FIT_FONT_SIZE_PT {
        font_size *= 0.9;
        wrapped = wrap_to_width(translated, font_size, max_width, metrics);
    }
    let leading = font_size * 1.15;
    let baseline_x = user_rect.x0;
    let mut baseline_y = user_rect.y1 - font_size;

    let (r, g, b) = argb_to_rgb(block_foreground(block));
    builder.set_fill_rgb(r, g, b);
    builder.begin_text();
    let resource_name: &[u8] = match embed {
        Some(e) => &e.resource_name,
        None => b"FFR", // Helvetica fallback (Standard-14, no embed).
    };
    builder.set_font(resource_name, font_size);
    for line in &wrapped {
        if line.is_empty() {
            baseline_y -= leading;
            continue;
        }
        builder.set_text_matrix(Matrix {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: baseline_x,
            f: baseline_y,
        });
        emit_glyphs(builder, line, metrics, embed);
        baseline_y -= leading;
    }
    builder.end_text();
}

fn emit_line(
    builder: &mut ContentStreamBuilder,
    text: &str,
    src_line: &PreparedTextLine,
    page: &OverlayPage,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
    foreground_argb: u32,
) {
    let user_rect = pixel_rect_to_user(src_line.bounding_box, page);
    if !user_rect_is_drawable(user_rect) {
        return;
    }
    let target_height = (user_rect.y1 - user_rect.y0).max(1.0);
    let multiplier = font_size_multiplier_for(&src_line.text);
    let mut font_size = (target_height * multiplier).max(MIN_FIT_FONT_SIZE_PT);
    let max_width = (user_rect.x1 - user_rect.x0).max(1.0);

    let measured = metrics.measure(text, font_size);
    if measured > max_width * OVERHANG_TOLERANCE && measured > 0.0 {
        let shrunk = font_size * (max_width / measured);
        font_size = shrunk.max(MIN_FIT_FONT_SIZE_PT);
    }

    // Baseline: align the cap-line with the bbox top so the visible
    // glyph height matches the source ink we just masked out. Cap-height
    // is conventionally ~0.7 of the em-square in Latin sans-serif, so
    // baseline = bbox_top - 0.7 × font_size.
    let baseline_y = user_rect.y1 - font_size * 0.7;

    let (r, g, b) = argb_to_rgb(foreground_argb);
    builder.set_fill_rgb(r, g, b);
    builder.begin_text();
    let resource_name: &[u8] = match embed {
        Some(e) => &e.resource_name,
        None => b"FFR",
    };
    builder.set_font(resource_name, font_size);
    builder.set_text_matrix(Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: user_rect.x0,
        f: baseline_y,
    });
    emit_glyphs(builder, text, metrics, embed);
    builder.end_text();
}

fn emit_glyphs(
    builder: &mut ContentStreamBuilder,
    text: &str,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
) {
    if let Some(embedded) = embed {
        // RTL scripts (Arabic, Hebrew, …) need BiDi reordering + cursive
        // joining; the char-by-char cmap path below would emit isolated,
        // left-to-right glyphs. Latin/CJK keep the original fast path.
        if crate::pdf_overlay::line_contains_rtl(text)
            && let Some(gids) = crate::pdf_overlay::shape_line_to_gids(text, metrics, embedded)
        {
            builder.show_hex_gids(gids);
            return;
        }
        builder.show_hex_gids(text.chars().map(|c| {
            let original = metrics.glyph_for(c).map(|g| g.gid).unwrap_or(0);
            embedded
                .gid_remap
                .get(&original)
                .copied()
                .unwrap_or(original)
        }));
    } else {
        builder.show_winansi(text);
    }
}

fn emit_filled_rect(builder: &mut ContentStreamBuilder, rect: UserRect, rgb: (f32, f32, f32)) {
    builder.set_fill_rgb(rgb.0, rgb.1, rgb.2);
    builder.push_operation(&lopdf::content::Operation::new(
        "re",
        vec![
            lopdf::Object::Real(rect.x0),
            lopdf::Object::Real(rect.y0),
            lopdf::Object::Real(rect.x1 - rect.x0),
            lopdf::Object::Real(rect.y1 - rect.y0),
        ],
    ));
    builder.push_operation(&lopdf::content::Operation::new("f", vec![]));
}

/// Convert a Tesseract pixel-space rect to a PDF user-space rect.
/// Pixels are top-left origin; user space is bottom-left.
fn pixel_rect_to_user(rect: translator_core::ocr::Rect, page: &OverlayPage) -> UserRect {
    let scale = 72.0 / page.dpi;
    let display_left = rect.left as f32 * scale;
    let display_top = rect.top as f32 * scale;
    let display_right = rect.right as f32 * scale;
    let display_bottom = rect.bottom as f32 * scale;

    let display_rect = translator_core::ocr::Rect {
        left: display_left.floor().max(0.0) as u32,
        top: display_top.floor().max(0.0) as u32,
        right: display_right.ceil().max(0.0) as u32,
        bottom: display_bottom.ceil().max(0.0) as u32,
    };
    // The geom-aware helper expects integer Rect; for sub-pt precision
    // we manually replicate the rotate-0 transform here using floats.
    if page.geom.rotate == 0 {
        let top = page.geom.user_y_min + page.geom.user_h;
        return UserRect {
            x0: display_left + page.geom.user_x_min,
            x1: display_right + page.geom.user_x_min,
            y0: top - display_bottom,
            y1: top - display_top,
        };
    }
    page.geom.user_rect_from_display(display_rect)
}

fn user_rect_is_drawable(r: UserRect) -> bool {
    (r.x1 - r.x0) > 0.0 && (r.y1 - r.y0) > 0.0
}

fn pad_user_rect(r: UserRect, pad: f32) -> UserRect {
    UserRect {
        x0: r.x0 - pad,
        y0: r.y0 - pad,
        x1: r.x1 + pad,
        y1: r.y1 + pad,
    }
}

fn block_foreground(block: &PreparedTextBlock) -> u32 {
    block
        .style_spans
        .iter()
        .find_map(|s| s.foreground)
        .or_else(|| {
            block
                .lines
                .first()
                .and_then(|l| translator_core::ocr::sample_line_color(&l.foreground, 0.5))
        })
        .unwrap_or(0xFF00_0000)
}

fn argb_to_rgb(argb: u32) -> (f32, f32, f32) {
    (
        ((argb >> 16) & 0xFF) as f32 / 255.0,
        ((argb >> 8) & 0xFF) as f32 / 255.0,
        (argb & 0xFF) as f32 / 255.0,
    )
}

fn font_size_from_pixel_height(px: f32, dpi: f32) -> f32 {
    let pt = px * 72.0 / dpi;
    // No source line text to inspect here — assume mixed case with
    // descenders, the safe default that won't overflow row gaps.
    (pt * FONT_SIZE_MULT_DESCENDER).max(MIN_FIT_FONT_SIZE_PT)
}

/// Pick a px-bbox-to-font-size multiplier from the source line's
/// recognised text. Lines with descender glyphs (`gjpqy`) get the
/// tightest factor; lines with lowercase ascenders but no descenders
/// (`bdfhklt`) sit between; lines that are all-caps or have no Latin
/// lowercase at all use the loosest factor (cap-height = ~0.7 em).
fn font_size_multiplier_for(text: &str) -> f32 {
    let mut has_descender = false;
    let mut has_lowercase = false;
    for c in text.chars() {
        if matches!(c, 'g' | 'j' | 'p' | 'q' | 'y') {
            has_descender = true;
        }
        if c.is_ascii_lowercase() {
            has_lowercase = true;
        }
    }
    if has_descender {
        FONT_SIZE_MULT_DESCENDER
    } else if has_lowercase {
        FONT_SIZE_MULT_LOWER_NO_DESC
    } else {
        FONT_SIZE_MULT_CAPS_OR_OTHER
    }
}

/// Re-distribute a paragraph of translated text across `target_lines`
/// line slots, splitting roughly evenly by word count. This is a coarse
/// heuristic — better than dumping the whole translation on line 0,
/// worse than a real wrap. Used for PerLine layouts where each
/// `PreparedTextLine` carries its own bbox (which is the actual fit
/// constraint, not character count).
fn redistribute_lines(text: &str, target_lines: usize) -> Vec<String> {
    if target_lines == 0 {
        return Vec::new();
    }
    if target_lines == 1 {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![String::new(); target_lines];
    }
    if words.len() <= target_lines {
        // Spread one word per line so at least the leading lines get
        // something visible. Trailing slots stay empty.
        let mut out: Vec<String> = words.iter().map(|w| (*w).to_string()).collect();
        while out.len() < target_lines {
            out.push(String::new());
        }
        return out;
    }
    let mut out: Vec<String> = Vec::with_capacity(target_lines);
    let n = words.len();
    let mut start = 0usize;
    for line_idx in 0..target_lines {
        // Words remaining = n - start. Lines remaining = target_lines - line_idx.
        let remaining_lines = target_lines - line_idx;
        let take = ((n - start) + remaining_lines - 1) / remaining_lines;
        let end = (start + take).min(n);
        out.push(words[start..end].join(" "));
        start = end;
    }
    out
}

fn wrap_to_width(text: &str, font_size: f32, max_width: f32, metrics: &FontMetrics) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if metrics.measure(&candidate, font_size) <= max_width || current.is_empty() {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Names of the embeds actually referenced by the overlay stream — used
/// when attaching the page's `/Font` resources. We attach every embed
/// regardless (fallback to one font), but this list lets the caller
/// avoid attaching fonts that no page uses.
pub fn collect_used_embed_names(plan: &OverlayFontPlan) -> HashSet<Vec<u8>> {
    plan.embeds
        .values()
        .map(|e| e.resource_name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_geom() -> PageGeometry {
        PageGeometry {
            user_w: 612.0,
            user_h: 792.0,
            user_x_min: 0.0,
            user_y_min: 0.0,
            rotate: 0,
        }
    }

    fn dummy_page(_rect: translator_core::ocr::Rect) -> OverlayPage {
        OverlayPage {
            page_index: 0,
            geom: dummy_geom(),
            dpi: 200.0,
            overlay: PreparedImageOverlay {
                rgba_bytes: Vec::new(),
                width: 1700,
                height: 2200,
                extracted_text: String::new(),
                translated_text: String::new(),
                blocks: Vec::new(),
                source_words: Vec::new(),
                translated_words: Vec::new(),
            },
        }
    }

    #[test]
    fn pixel_to_user_unrotated_origin_topleft_to_bottomleft() {
        let page = dummy_page(translator_core::ocr::Rect::default());
        // A pixel rect at (0,0) → (200,100) at 200 dpi is 1in × 0.5in.
        let user = pixel_rect_to_user(
            translator_core::ocr::Rect {
                left: 0,
                top: 0,
                right: 200,
                bottom: 100,
            },
            &page,
        );
        // 200 px / 200 dpi = 1 in = 72 pt.
        assert!((user.x0 - 0.0).abs() < 1e-3);
        assert!((user.x1 - 72.0).abs() < 1e-3);
        // y: top (792) - 36 = 756; top - 0 = 792.
        assert!((user.y1 - 792.0).abs() < 1e-3);
        assert!((user.y0 - 756.0).abs() < 1e-3);
    }

    #[test]
    fn argb_to_rgb_white() {
        let (r, g, b) = argb_to_rgb(0xFFFFFFFF);
        assert!((r - 1.0).abs() < 1e-6);
        assert!((g - 1.0).abs() < 1e-6);
        assert!((b - 1.0).abs() < 1e-6);
    }

    #[test]
    fn redistribute_splits_evenly() {
        let out = redistribute_lines("the quick brown fox jumps over", 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out.iter().filter(|s| !s.is_empty()).count(), 3);
    }

    #[test]
    fn redistribute_pads_when_few_words() {
        let out = redistribute_lines("hi", 3);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], "hi");
        assert_eq!(out[1], "");
        assert_eq!(out[2], "");
    }
}
