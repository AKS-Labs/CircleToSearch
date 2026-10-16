//! Translated-text overlay emission.
//!
//! Take the translated blocks plus the per-block typography & geometry
//! sampled by surgery, wrap text to fit, and emit a content stream that
//! draws the new glyphs at the original baselines through a
//! [`ContentStreamBuilder`].

use std::collections::HashMap;

use crate::pdf_content::{
    BoldItalic, ContentStreamBuilder, FontStyleFlags, Matrix, PageGeometry, UserRect,
};
use crate::pdf_font_embed::EmbeddedFont;
use crate::pdf_surgery::CapturedTextShow;
use crate::pdf_write::{BlockGeometry, BlockTypography, SampledBlockStyle};
use crate::styled::{StyleSpan, TranslatedStyledBlock};
use translator_core::ocr::Rect;
use translator_render::font_metrics::FontMetrics;
use translator_render::text_shape::{self, DirRun, segment_runs};

/// Approximate average Helvetica glyph width as a fraction of font size.
pub(crate) const HELVETICA_AVG_ADVANCE: f32 = 0.5;

/// Approximate average Courier glyph width as a fraction of font size. Courier
/// is monospaced at a known em-fraction (~0.6em), so this is tighter than the
/// Helvetica figure but still a fallback for when no real font is available.
pub(crate) const COURIER_AVG_ADVANCE: f32 = 0.6;

/// Vertical margin inside the bbox so descenders don't clip the bottom.
const TEXT_BASELINE_PAD: f32 = 0.2;

/// Leading multiplier for wrapped lines (line-height = font_size * factor).
const LINE_HEIGHT_FACTOR: f32 = 1.15;

/// Iterations of shrink-and-rewrap when fitting translated text. Six steps at
/// the chosen factors take any sampled size down to ~MIN_SHRINK_FRACTION
/// before bottoming out.
const FIT_RETRY_LIMIT: usize = 6;

/// Per-iteration shrink factor when the wrap produced more lines than the
/// original.
const MULTILINE_SHRINK_FACTOR: f32 = 0.9;

/// Per-iteration shrink factor when the unwrapped block exceeds vis_h.
const UNWRAPPED_SHRINK_FACTOR: f32 = 0.85;

/// Floor font size during shrink-to-fit. Below this the text becomes
/// unreadable so we accept overflow instead.
const MIN_FIT_FONT_SIZE_PT: f32 = 4.0;

/// Tolerated horizontal overhang past the bbox right edge before we shrink
/// the font (covers the case where our 0.5em advance is mildly pessimistic
/// against the real Helvetica metrics).
const OVERHANG_TOLERANCE: f32 = 1.05;
/// Floor for shrink-to-fit (fraction of the sampled size). Below this the
/// text becomes unreadable so we accept overflow instead.
const MIN_SHRINK_FRACTION: f32 = 0.7;

/// Gap to reserve before an inline opaque formula when wrapping the prose
/// line that precedes it.
const INLINE_FORMULA_PAD_PT: f32 = 6.0;

/// Per-block resolved fonts: one `(FontMetrics, EmbeddedFont)` entry per
/// [`BoldItalic`] variant the block actually uses (its dominant style plus
/// whatever appears in `style_spans`). Wrapping always uses the dominant
/// variant for width estimation; emit picks per segment.
pub(crate) struct BlockResources {
    pub(crate) by_flags: HashMap<BoldItalic, (FontMetrics, Option<EmbeddedFont>)>,
    pub(crate) default_flags: BoldItalic,
    pub(crate) monospace: bool,
}

impl BlockResources {
    pub(crate) fn dominant_metrics(&self) -> &FontMetrics {
        &self
            .by_flags
            .get(&self.default_flags)
            .expect("dominant variant is always inserted")
            .0
    }

    pub(crate) fn for_flags(&self, flags: BoldItalic) -> (&FontMetrics, Option<&EmbeddedFont>) {
        let entry = self
            .by_flags
            .get(&flags)
            .or_else(|| self.by_flags.get(&self.default_flags))
            .expect("at least the dominant variant exists");
        (&entry.0, entry.1.as_ref())
    }
}

pub(crate) fn build_overlay_stream(
    blocks: &[TranslatedStyledBlock],
    user_rects: &[UserRect],
    block_styles: &[SampledBlockStyle],
    block_resources: &[BlockResources],
    captured_text: &[Vec<CapturedTextShow>],
    geom: PageGeometry,
    final_ctm: Matrix,
) -> Vec<u8> {
    let mut builder = ContentStreamBuilder::new();
    builder.save_state();
    let inv_ctm = final_ctm.inverse().unwrap_or_else(Matrix::identity);
    // Cursor coordination across adjacent blocks. After each block we
    // record where its last visual line ended in user space; the next
    // block, if its first visual line is on the same row, picks up from
    // there. That's how a `<text><formula><text>` row stays coherent
    // after translation: the formula slides over to follow the actual
    // prose endpoint, the following prose slides further, and a
    // multi-line prose's later wraps fall back to their own anchors at
    // the left margin.
    let mut prev_line_end: Option<(f32, f32)> = None;
    let mut prev_block_was_opaque = false;
    let mut i = 0usize;
    while i < blocks.len() && i < user_rects.len() {
        let block = &blocks[i];
        let user_rect = user_rects[i];
        let style = block_styles.get(i).cloned().unwrap_or_default();
        let first_line_y = first_line_user_y(&style, user_rect);
        let inherit_x = inline_x_override(prev_line_end, prev_block_was_opaque, first_line_y);

        if block.opaque {
            let captured = captured_text.get(i).map(Vec::as_slice).unwrap_or(&[]);
            let _ = inherit_x; // formulas stay anchored — see emit_captured_text_block
            prev_line_end =
                emit_captured_text_block(&mut builder, captured, user_rect, &style, &inv_ctm);
            prev_block_was_opaque = true;
            i += 1;
            continue;
        }

        let Some(resources) = block_resources.get(i) else {
            prev_line_end = None;
            prev_block_was_opaque = false;
            i += 1;
            continue;
        };
        // The prose's last visual line might be on the same row as a
        // following opaque formula. Find the leftmost opaque block on
        // that row so the wrap doesn't run past it (formulas stay at
        // original x, so the prose has to leave them room).
        let last_line_obstacle =
            following_inline_obstacle_x(i, blocks, user_rects, &block_styles, geom);
        prev_line_end = emit_block(
            &mut builder,
            block,
            user_rect,
            &style,
            resources,
            inherit_x,
            last_line_obstacle,
            geom,
            &inv_ctm,
        );
        prev_block_was_opaque = false;
        i += 1;
    }
    builder.restore_state();
    builder.finish()
}

fn inline_x_override(
    prev_line_end: Option<(f32, f32)>,
    prev_block_was_opaque: bool,
    first_line_y: f32,
) -> Option<f32> {
    if !prev_block_was_opaque {
        return None;
    }
    prev_line_end.and_then(|(px, py)| {
        ((first_line_y - py).abs() <= 4.0).then_some(px + INLINE_FORMULA_PAD_PT)
    })
}

fn following_inline_obstacles(
    index: usize,
    blocks: &[TranslatedStyledBlock],
    user_rects: &[UserRect],
    geom: PageGeometry,
) -> Vec<UserRect> {
    let Some(current) = user_rects.get(index).copied() else {
        return Vec::new();
    };
    let (_, current_top, _, current_bottom) = user_rect_visual_bounds(current, geom);
    if current_bottom - current_top > 45.0 {
        return Vec::new();
    }
    let mut obstacles = Vec::new();
    for next in (index + 1)..blocks.len().min(user_rects.len()) {
        let rect = user_rects[next];
        let (left, top, _, bottom) = user_rect_visual_bounds(rect, geom);
        if top > current_bottom + 2.0 {
            break;
        }
        if bottom < current_top - 2.0 {
            continue;
        }
        if left <= user_rect_visual_bounds(current, geom).0 {
            continue;
        }
        obstacles.push(rect);
        if blocks[next].opaque {
            continue;
        }
        if !blocks[next].opaque {
            break;
        }
    }
    obstacles
}

fn following_inline_obstacle_x(
    index: usize,
    blocks: &[TranslatedStyledBlock],
    user_rects: &[UserRect],
    _block_styles: &[SampledBlockStyle],
    geom: PageGeometry,
) -> Option<f32> {
    following_inline_obstacles(index, blocks, user_rects, geom)
        .into_iter()
        .map(|rect| user_rect_visual_bounds(rect, geom).0)
        .reduce(f32::min)
}

/// First-line baseline y in user space. Falls back to the block bbox if
/// no anchor was sampled.
fn first_line_user_y(style: &SampledBlockStyle, user_rect: UserRect) -> f32 {
    if let Some((_, y)) = style.geometry.line_anchors.first() {
        return *y;
    }
    if let Some((_, y)) = style.geometry.anchor {
        return y;
    }
    user_rect.y1
}

/// Emit one translated block. Positioning happens in PDF user space (which
/// matches what `UserRect` carries), then we inverse-transform through the
/// page's still-active CTM into the producer's local coordinate system so
/// the appended `cm`-less stream draws at the right visual spot.
///
/// `first_line_x_override` lets the caller force the first wrapped line
/// to start at a specific x — used when this block is continuing an
/// inline row that opened with a previous opaque formula. Subsequent
/// lines fall back to their own anchors (typically the left margin).
///
/// Returns the user-space `(x, y)` where the last wrapped line ended, so
/// the caller can chain a following block onto the same visual row.
fn emit_block(
    builder: &mut ContentStreamBuilder,
    block: &TranslatedStyledBlock,
    user_rect: UserRect,
    style: &SampledBlockStyle,
    resources: &BlockResources,
    first_line_x_override: Option<f32>,
    last_line_obstacle_x: Option<f32>,
    geom: PageGeometry,
    inv_ctm: &Matrix,
) -> Option<(f32, f32)> {
    let text = block.text.trim();
    if text.is_empty() {
        return None;
    }
    let user_w = user_rect.x1 - user_rect.x0;
    let user_h = user_rect.y1 - user_rect.y0;
    if user_w <= 0.0 || user_h <= 0.0 {
        return None;
    }

    let (vis_w, vis_h) = match geom.rotate {
        90 | 270 => (user_h, user_w),
        _ => (user_w, user_h),
    };
    let line_widths = line_available_widths(
        &style.geometry,
        &block.source_rects,
        user_rect,
        first_line_x_override,
        last_line_obstacle_x,
        geom,
        vis_w,
    );

    let dominant_metrics = resources.dominant_metrics();
    let source_line_boxes = source_rect_line_boxes(&block.source_rects);
    let target_line_count = style
        .geometry
        .original_line_count
        .max(source_rect_line_count(&block.source_rects));
    let (font_size, lines) = match style.typography.font_size {
        Some(size) if size.is_finite() && size > 0.0 => fit_with_sampled_size(
            text,
            &line_widths,
            vis_h,
            size,
            dominant_metrics,
            target_line_count,
        ),
        _ => {
            let initial = (vis_h * (1.0 - TEXT_BASELINE_PAD)).max(MIN_FIT_FONT_SIZE_PT);
            wrap_to_fit(text, &line_widths, vis_h, initial, dominant_metrics)
        }
    };
    let leading = font_size * LINE_HEIGHT_FACTOR;

    let (line_dx, line_dy) = match geom.rotate {
        0 => (0.0, -1.0),
        90 => (1.0, 0.0),
        180 => (0.0, 1.0),
        270 => (-1.0, 0.0),
        _ => (0.0, -1.0),
    };

    let (first_baseline_x, first_baseline_y) = match style.geometry.anchor {
        Some((ax, ay)) => (ax, ay),
        None => {
            let total_height = leading * lines.len() as f32;
            let top_pad = ((vis_h - total_height).max(0.0)) * 0.5;
            let first_baseline_offset = top_pad + font_size;
            let (top_x, top_y) = match geom.rotate {
                0 => (user_rect.x0, user_rect.y1),
                90 => (user_rect.x0, user_rect.y0),
                180 => (user_rect.x1, user_rect.y0),
                270 => (user_rect.x1, user_rect.y1),
                _ => (user_rect.x0, user_rect.y1),
            };
            (
                top_x + first_baseline_offset * line_dx,
                top_y + first_baseline_offset * line_dy,
            )
        }
    };

    builder.set_fill_rgb(
        style.typography.fill_rgb.0,
        style.typography.fill_rgb.1,
        style.typography.fill_rgb.2,
    );
    builder.begin_text();

    // Map each wrapped line back to its byte ranges in `block.text` so we
    // can intersect with `block.style_spans` and produce styled segments.
    let line_word_ranges = line_byte_ranges(&block.text, &lines);

    let advance_dx = style.geometry.text_orientation.a;
    let advance_dy = style.geometry.text_orientation.b;
    let mut last_line_end: Option<(f32, f32)> = None;

    for (i, _line) in lines.iter().enumerate() {
        let (mut line_x, mut line_y) = line_origin(
            &style.geometry,
            i,
            first_baseline_x,
            first_baseline_y,
            leading,
            line_dx,
            line_dy,
        );
        if let Some(line_box) = source_line_boxes.get(i)
            && uses_visual_top_origin(&style.geometry)
        {
            let (vx, _) = geom.to_display((line_x, line_y));
            let desired_visual_y = source_line_baseline_y(*line_box, font_size);
            (line_x, line_y) = display_to_user((vx, desired_visual_y), geom);
        }
        if i == 0
            && let Some(override_x) = first_line_x_override
        {
            line_x = override_x;
        }

        let segments = if resources.monospace {
            literal_line_segments(&lines[i], resources.default_flags)
        } else {
            segments_for_line(
                &block.text,
                &line_word_ranges[i],
                &block.style_spans,
                resources.default_flags,
            )
        };

        let mut cumulative = 0.0_f32;
        let sampled_font_size = style.typography.font_size.unwrap_or(font_size).max(0.1);
        let latin_fallback_metrics = if resources.monospace {
            FontMetrics::approx(COURIER_AVG_ADVANCE)
        } else {
            FontMetrics::approx(HELVETICA_AVG_ADVANCE)
        };

        // Lines containing a right-to-left script (Arabic, Hebrew, …) can't be
        // emitted by the logical-order, left-to-right cursor below: BiDi
        // reordering and cursive joining (shaping) are mandatory. Hand those to
        // the shaped emitter, which lays runs in visual order and right-aligns.
        // All-LTR lines keep the original cmap-direct path untouched.
        let logical: String = segments.iter().map(|s| s.text.as_str()).collect();
        if line_contains_rtl(&logical) {
            last_line_end = Some(emit_line_shaped(
                builder,
                &segments,
                resources,
                line_x,
                line_y,
                advance_dx,
                advance_dy,
                line_dx,
                line_dy,
                font_size,
                sampled_font_size,
                line_width_at(&line_widths, i),
                style.geometry.text_orientation,
                inv_ctm,
                style.typography.fill_rgb,
            ));
            continue;
        }

        for seg in segments {
            if seg.text.is_empty() {
                continue;
            }
            let (target_metrics, target_embed) = resources.for_flags(seg.style.flags);
            let seg_font_size = seg
                .style
                .text_size
                .filter(|size| size.is_finite() && *size > 0.0)
                .map(|size| font_size * (size / sampled_font_size))
                .unwrap_or(font_size);
            let baseline_shift = seg
                .style
                .baseline_shift
                .filter(|shift| shift.is_finite())
                .map(|shift| shift * (font_size / sampled_font_size))
                .unwrap_or(0.0);

            for run in segment_font_runs(&seg.text, target_metrics, target_embed) {
                let (run_metrics, run_embed) = if run.use_standard14 {
                    (&latin_fallback_metrics, None)
                } else {
                    (target_metrics, target_embed)
                };
                let run_resource_name: &[u8] = match run_embed {
                    Some(e) => &e.resource_name,
                    None => BlockTypography::font_resource_for(FontStyleFlags {
                        bold: seg.style.flags.bold,
                        italic: seg.style.flags.italic,
                        monospace: resources.monospace,
                    }),
                };
                let run_x = line_x + cumulative * advance_dx;
                let run_y = line_y + cumulative * advance_dy;
                let combined = Matrix {
                    e: run_x + baseline_shift * line_dx,
                    f: run_y + baseline_shift * line_dy,
                    ..style.geometry.text_orientation
                };
                let tm = combined.mul(*inv_ctm);
                if let Some((r, g, b)) = seg.style.fill_rgb {
                    builder.set_fill_rgb(r, g, b);
                } else {
                    builder.set_fill_rgb(
                        style.typography.fill_rgb.0,
                        style.typography.fill_rgb.1,
                        style.typography.fill_rgb.2,
                    );
                }
                builder.set_font(run_resource_name, seg_font_size);
                builder.set_text_matrix(tm);
                emit_tj_for_segment(builder, run.text, run_metrics, run_embed);

                cumulative += run_metrics.measure(run.text, seg_font_size);
            }
        }
        last_line_end = Some((
            line_x + cumulative * advance_dx,
            line_y + cumulative * advance_dy,
        ));
    }
    builder.end_text();
    last_line_end
}

fn source_rect_line_count(rects: &[Rect]) -> usize {
    source_rect_line_boxes(rects).len()
}

/// Re-emit a captured run of original Tjs (a preserved formula) at
/// the original positions. We deliberately keep formulas anchored where
/// they were in the source PDF: a TeX fraction renders its bar with a
/// PDF path operator (`m`, `l`, `S`) rather than a Tj, and surgery only
/// captures text-show ops. If we shifted the captured Tjs, the
/// numerator/denominator glyphs would slide one way and the fraction
/// bar would stay behind, gutting the formula. Returns the user-space
/// `(x, y)` of the formula's right edge so the caller can place the
/// next inline block (the prose continuing on the same visual row)
/// flush against it.
fn emit_captured_text_block(
    builder: &mut ContentStreamBuilder,
    captured: &[CapturedTextShow],
    user_rect: UserRect,
    style: &SampledBlockStyle,
    inv_ctm: &Matrix,
) -> Option<(f32, f32)> {
    if captured.is_empty() {
        return None;
    }

    for show in captured {
        let tm = show.combined.mul(*inv_ctm);
        builder.set_fill_rgb(show.fill_rgb.0, show.fill_rgb.1, show.fill_rgb.2);
        builder.begin_text();
        builder.set_char_spacing(show.char_spacing);
        builder.set_word_spacing(show.word_spacing);
        builder.set_horizontal_scaling(show.horizontal_scaling);
        builder.set_font(&show.font_resource, show.font_size);
        builder.set_text_matrix(tm);
        builder.push_operation(&show.operation);
        builder.end_text();
    }

    let anchor_y = style
        .geometry
        .anchor
        .map(|(_, y)| y)
        .or_else(|| captured.first().map(|s| s.origin.1))
        .unwrap_or(user_rect.y1);
    Some((user_rect.x1, anchor_y))
}

fn line_origin(
    geometry: &BlockGeometry,
    line_index: usize,
    first_x: f32,
    first_y: f32,
    leading: f32,
    line_dx: f32,
    line_dy: f32,
) -> (f32, f32) {
    if let Some(anchor) = geometry.line_anchors.get(line_index) {
        return *anchor;
    }
    if let Some(last) = geometry.line_anchors.last() {
        let extra = (line_index + 1 - geometry.line_anchors.len()) as f32 * leading;
        return (last.0 + extra * line_dx, last.1 + extra * line_dy);
    }
    let off = line_index as f32 * leading;
    (first_x + off * line_dx, first_y + off * line_dy)
}

fn line_available_widths(
    geometry: &BlockGeometry,
    source_rects: &[Rect],
    user_rect: UserRect,
    first_line_x_override: Option<f32>,
    last_line_obstacle_x: Option<f32>,
    geom: PageGeometry,
    fallback: f32,
) -> Vec<f32> {
    let source_widths: Vec<f32> = source_rect_line_boxes(source_rects)
        .into_iter()
        .map(|line| (line.right - line.left).max(1.0))
        .collect();

    if geometry.line_anchors.is_empty() {
        return if source_widths.is_empty() {
            vec![fallback.max(1.0)]
        } else {
            source_widths
        };
    }

    let (visual_left, _, visual_right, _) = user_rect_visual_bounds(user_rect, geom);
    let last_index = geometry.line_anchors.len().saturating_sub(1);
    let mut widths: Vec<f32> = geometry
        .line_anchors
        .iter()
        .enumerate()
        .map(|(index, origin)| {
            let (vx, _vy) = geom.to_display(*origin);
            let effective_left = if matches!(geom.rotate, 90 | 270) {
                visual_left
            } else if index == 0
                && let Some(override_x) = first_line_x_override
            {
                geom.to_display((override_x, origin.1)).0
            } else {
                vx
            };
            let effective_right = if index == last_index
                && let Some(obstacle_user_x) = last_line_obstacle_x
            {
                let (ox, _) = geom.to_display((obstacle_user_x, origin.1));
                ox.min(visual_right)
            } else {
                visual_right
            };
            (effective_right - effective_left)
                .max(fallback * 0.25)
                .min(fallback)
        })
        .collect();

    if source_widths.len() > widths.len() {
        widths.extend(source_widths.iter().skip(widths.len()).copied());
    }
    for (width, source_width) in widths.iter_mut().zip(source_widths) {
        *width = width.min(source_width.max(1.0));
    }
    widths
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct VisualLineBox {
    left: f32,
    top: f32,
    center_y: f32,
    right: f32,
    bottom: f32,
    height: f32,
}

fn source_rect_line_boxes(rects: &[Rect]) -> Vec<VisualLineBox> {
    let mut boxes: Vec<VisualLineBox> = rects
        .iter()
        .filter(|r| r.height() > 0 && r.width() > 0)
        .map(|r| VisualLineBox {
            left: r.left as f32,
            top: r.top as f32,
            center_y: (r.top + r.bottom) as f32 * 0.5,
            right: r.right as f32,
            bottom: r.bottom as f32,
            height: r.height() as f32,
        })
        .collect();
    if boxes.is_empty() {
        return boxes;
    }

    boxes.sort_by(|a, b| a.center_y.total_cmp(&b.center_y));
    let mut lines: Vec<VisualLineBox> = Vec::new();
    for b in boxes {
        if let Some(line) = lines.last_mut() {
            let threshold = (line.height.max(b.height) * 0.6).max(2.0);
            if (b.center_y - line.center_y).abs() <= threshold {
                line.left = line.left.min(b.left);
                line.top = line.top.min(b.top);
                line.right = line.right.max(b.right);
                line.bottom = line.bottom.max(b.bottom);
                line.center_y = (line.center_y + b.center_y) * 0.5;
                line.height = line.height.max(b.height);
                continue;
            }
        }
        lines.push(b);
    }
    lines
}

fn uses_visual_top_origin(geometry: &BlockGeometry) -> bool {
    geometry.text_orientation.d < -0.5
        || (geometry.text_orientation.b > 0.5 && geometry.text_orientation.c < -0.5)
}

fn source_line_baseline_y(line_box: VisualLineBox, font_size: f32) -> f32 {
    let descent = (font_size * 0.2).max(1.0);
    (line_box.bottom - descent).max(line_box.top)
}

fn display_to_user(display: (f32, f32), geom: PageGeometry) -> (f32, f32) {
    let top = geom.user_y_min + geom.user_h;
    let right = geom.user_x_min + geom.user_w;
    match geom.rotate {
        0 => (display.0 + geom.user_x_min, top - display.1),
        90 => (display.1 + geom.user_x_min, display.0 + geom.user_y_min),
        180 => (right - display.0, display.1 + geom.user_y_min),
        270 => (right - display.1, top - display.0),
        _ => (display.0 + geom.user_x_min, top - display.1),
    }
}

fn user_rect_visual_bounds(rect: UserRect, geom: PageGeometry) -> (f32, f32, f32, f32) {
    let points = [
        (rect.x0, rect.y0),
        (rect.x0, rect.y1),
        (rect.x1, rect.y0),
        (rect.x1, rect.y1),
    ];
    let mut left = f32::INFINITY;
    let mut top = f32::INFINITY;
    let mut right = f32::NEG_INFINITY;
    let mut bottom = f32::NEG_INFINITY;
    for point in points {
        let (x, y) = geom.to_display(point);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    (left, top, right, bottom)
}

/// One run of consecutive characters from a wrapped line that share the
/// same font/color style.
struct LineSegment {
    text: String,
    style: SegmentStyle,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SegmentStyle {
    flags: BoldItalic,
    fill_rgb: Option<(f32, f32, f32)>,
    text_size: Option<f32>,
    baseline_shift: Option<f32>,
}

impl SegmentStyle {
    fn is_script(self) -> bool {
        self.text_size.is_some() || self.baseline_shift.is_some()
    }
}

/// Walk the line's words (located in `block_text` via `word_ranges`),
/// snap each alphanumeric run's style to its **majority** char flag
/// (Bergamot's token alignment often spans whitespace, so going char-by-char
/// produces off-by-one bold edges; snapping to whole words makes
/// bold/italic word-aligned). Trailing/leading punctuation inside the same
/// whitespace token (`Empleado).`) is split out into its own sub-word so
/// it picks up the punctuation's actual per-byte style instead of inheriting
/// the styled word's bold.
fn segments_for_line(
    block_text: &str,
    word_ranges: &[(usize, usize)],
    style_spans: &[StyleSpan],
    default_flags: BoldItalic,
) -> Vec<LineSegment> {
    let lookup = |byte: usize| -> SegmentStyle {
        for span in style_spans {
            if byte >= span.start as usize && byte < span.end as usize {
                if let Some(s) = &span.style {
                    // OR-merge with the block's default flags so a span
                    // that lost its bold/italic during extraction (mupdf
                    // substitutes embedded fonts with host fonts whose
                    // `is_bold` returns false even when the original
                    // BaseFont was e.g. `Times-Bold`) still inherits the
                    // surgery-sampled block-level weight. Spans that do
                    // mark bold/italic explicitly stay bold/italic; they
                    // can't go *less* bold than the block signal.
                    return SegmentStyle {
                        flags: BoldItalic {
                            bold: s.bold || default_flags.bold,
                            italic: s.italic || default_flags.italic,
                        },
                        fill_rgb: s.text_color.map(argb_to_rgb),
                        text_size: s.text_size,
                        baseline_shift: s.baseline_shift,
                    };
                }
            }
        }
        SegmentStyle {
            flags: default_flags,
            fill_rgb: None,
            text_size: None,
            baseline_shift: None,
        }
    };

    // Sub-word: a (start, end, separates_words) range. Each sub-word is
    // either fully alphanumeric or fully non-alphanumeric. `separates_words`
    // is true for the FIRST sub-word of a whitespace-delimited word (i.e.
    // the ones a real word-gap precedes); subsequent sub-words inside the
    // same whitespace token never get a leading space.
    let mut sub_words: Vec<(usize, usize, bool)> = Vec::new();
    for (word_start, word_end) in word_ranges {
        let mut chunk_start = *word_start;
        let mut chunk_key: Option<(bool, bool)> = None;
        let mut byte = *word_start;
        let mut first_chunk_in_word = true;
        for c in block_text[*word_start..*word_end].chars() {
            let alpha = c.is_alphanumeric();
            let style = lookup(byte);
            let key = (alpha, style.is_script());
            if let Some(prev_key) = chunk_key
                && key != prev_key
            {
                sub_words.push((chunk_start, byte, first_chunk_in_word));
                first_chunk_in_word = false;
                chunk_start = byte;
            }
            chunk_key = Some(key);
            byte += c.len_utf8();
        }
        if chunk_key.is_some() {
            sub_words.push((chunk_start, byte, first_chunk_in_word));
        }
    }

    let style_for = |start: usize, end: usize, is_alpha: bool| -> SegmentStyle {
        if !is_alpha {
            return lookup(start);
        }
        let first_style = lookup(start);
        if first_style.is_script() {
            return first_style;
        }
        let mut counts: Vec<(SegmentStyle, usize)> = Vec::new();
        let mut byte = start;
        for c in block_text[start..end].chars() {
            let style = lookup(byte);
            if let Some((_, count)) = counts.iter_mut().find(|(s, _)| *s == style) {
                *count += 1;
            } else {
                counts.push((style, 1));
            }
            byte += c.len_utf8();
        }
        counts
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .map(|(s, _)| s)
            .unwrap_or(SegmentStyle {
                flags: default_flags,
                fill_rgb: None,
                text_size: None,
                baseline_shift: None,
            })
    };

    // Group consecutive same-flag sub-words into segments. Word-separator
    // spaces stay attached to the *previous* segment so that breaking
    // segments (bold→regular transition) doesn't drop them.
    let mut segments: Vec<LineSegment> = Vec::new();
    let mut current = String::new();
    let mut current_style = SegmentStyle {
        flags: default_flags,
        fill_rgb: None,
        text_size: None,
        baseline_shift: None,
    };
    for (i, &(start, end, separates_words)) in sub_words.iter().enumerate() {
        let chunk = &block_text[start..end];
        let is_alpha = chunk
            .chars()
            .next()
            .map(char::is_alphanumeric)
            .unwrap_or(false);
        let style = style_for(start, end, is_alpha);
        // Bergamot's SentencePiece detokenizer emits a space before
        // closing punctuation (`,`, `.`, `)`, etc.) — visually fine when
        // the surrounding text shares one font, but the bold transitions
        // we now add make the gap obvious. Suppress the separator before
        // any token that starts with a closing-punctuation glyph. Sub-words
        // that came from splitting *within* a whitespace token never get
        // a separator either.
        let hugs_previous = chunk
            .chars()
            .next()
            .is_some_and(|c| matches!(c, ',' | '.' | ')' | ']' | '}' | ':' | ';' | '?' | '!'));
        let separator = i > 0 && separates_words && !hugs_previous;
        let need_break = !current.is_empty() && style != current_style;
        if need_break {
            if separator {
                current.push(' ');
            }
            segments.push(LineSegment {
                text: std::mem::take(&mut current),
                style: current_style,
            });
            current_style = style;
        } else if separator {
            current.push(' ');
        }
        if current.is_empty() {
            current_style = style;
        }
        current.push_str(chunk);
    }
    if !current.is_empty() {
        segments.push(LineSegment {
            text: current,
            style: current_style,
        });
    }
    segments
}

fn literal_line_segments(line: &str, default_flags: BoldItalic) -> Vec<LineSegment> {
    if line.is_empty() {
        return Vec::new();
    }
    vec![LineSegment {
        text: line.to_string(),
        style: SegmentStyle {
            flags: default_flags,
            fill_rgb: None,
            text_size: None,
            baseline_shift: None,
        },
    }]
}

fn argb_to_rgb(argb: u32) -> (f32, f32, f32) {
    (
        ((argb >> 16) & 0xFF) as f32 / 255.0,
        ((argb >> 8) & 0xFF) as f32 / 255.0,
        (argb & 0xFF) as f32 / 255.0,
    )
}

/// Locate each wrapped line's word byte ranges back inside `block_text`.
/// `wrap_lines` produces `Vec<String>` whose words appear in order in the
/// source; we forward-scan, skipping whitespace, matching each word.
fn line_byte_ranges(block_text: &str, lines: &[String]) -> Vec<Vec<(usize, usize)>> {
    let mut cursor = 0usize;
    let mut all = Vec::with_capacity(lines.len());
    for line in lines {
        let mut line_ranges = Vec::new();
        for word in line.split_whitespace() {
            // Skip whitespace.
            while cursor < block_text.len() {
                let c = match block_text[cursor..].chars().next() {
                    Some(c) => c,
                    None => break,
                };
                if c.is_whitespace() {
                    cursor += c.len_utf8();
                } else {
                    break;
                }
            }
            let word_bytes = word.as_bytes();
            let end = cursor + word_bytes.len();
            if end <= block_text.len() && &block_text.as_bytes()[cursor..end] == word_bytes {
                line_ranges.push((cursor, end));
                cursor = end;
            }
            // If mismatch (shouldn't happen since wrap_lines preserves words),
            // we just skip and keep going. Style attribution may be slightly
            // off for that one word.
        }
        all.push(line_ranges);
    }
    all
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SegmentFontRun<'a> {
    text: &'a str,
    use_standard14: bool,
}

fn segment_font_runs<'a>(
    text: &'a str,
    target_metrics: &FontMetrics,
    target_embed: Option<&EmbeddedFont>,
) -> Vec<SegmentFontRun<'a>> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut run_start = 0usize;
    let mut run_kind: Option<bool> = None;
    for (byte, c) in text.char_indices() {
        let use_standard14 =
            target_embed.is_some() && c.is_ascii() && !target_metrics.covers_char(c);
        if let Some(previous) = run_kind
            && previous != use_standard14
        {
            runs.push(SegmentFontRun {
                text: &text[run_start..byte],
                use_standard14: previous,
            });
            run_start = byte;
        }
        run_kind = Some(use_standard14);
    }
    if let Some(use_standard14) = run_kind {
        runs.push(SegmentFontRun {
            text: &text[run_start..],
            use_standard14,
        });
    }
    runs
}

/// One contiguous run of a line, already resolved to a font and laid out in
/// visual order. `advance` is the run's width in text space at `font_size`.
struct ShapedPiece {
    content: PieceContent,
    resource_name: Vec<u8>,
    font_size: f32,
    baseline_shift: f32,
    fill_rgb: (f32, f32, f32),
    advance: f32,
}

enum PieceContent {
    /// Subset glyph IDs for an embedded Type-0 font, in visual order.
    Gids(Vec<u16>),
    /// WinAnsi text for a Standard-14 Latin fallback.
    Winansi(String),
}

/// Emit one line that contains right-to-left text. Reorders the line's runs
/// into visual order (BiDi), shapes each embedded-font run with rustybuzz so
/// Arabic/Hebrew letters join, and right-aligns the line when its base
/// direction is RTL. ASCII the target font can't cover still falls back to a
/// Standard-14 Latin font. Returns the user-space `(x, y)` where the line ends.
#[allow(clippy::too_many_arguments)]
fn emit_line_shaped(
    builder: &mut ContentStreamBuilder,
    segments: &[LineSegment],
    resources: &BlockResources,
    line_x: f32,
    line_y: f32,
    advance_dx: f32,
    advance_dy: f32,
    line_dx: f32,
    line_dy: f32,
    font_size: f32,
    sampled_font_size: f32,
    avail_width: f32,
    orientation: Matrix,
    inv_ctm: &Matrix,
    default_fill: (f32, f32, f32),
) -> (f32, f32) {
    // Logical line text plus, for each non-empty segment, its byte span within
    // that text — so a BiDi run (expressed in logical bytes) maps back to the
    // segment styles it overlaps.
    let mut logical = String::new();
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    for (idx, seg) in segments.iter().enumerate() {
        if seg.text.is_empty() {
            continue;
        }
        let start = logical.len();
        logical.push_str(&seg.text);
        spans.push((start, logical.len(), idx));
    }
    if logical.is_empty() {
        return (line_x, line_y);
    }

    let mut dir_runs = segment_runs(&logical);
    let base_rtl = dir_runs
        .iter()
        .min_by_key(|r| r.start)
        .map(|r| r.rtl)
        .unwrap_or(false);
    dir_runs.sort_by_key(|r| r.visual_index);

    let latin_fallback = if resources.monospace {
        FontMetrics::approx(COURIER_AVG_ADVANCE)
    } else {
        FontMetrics::approx(HELVETICA_AVG_ADVANCE)
    };

    let mut pieces: Vec<ShapedPiece> = Vec::new();
    for dr in &dir_runs {
        // Segments overlapping this run, in logical order; reversed for RTL so
        // the cursor visits them right-to-left.
        let mut overlap: Vec<(usize, usize, usize)> = spans
            .iter()
            .copied()
            .filter(|(s, e, _)| *s < dr.end && *e > dr.start)
            .collect();
        if dr.rtl {
            overlap.reverse();
        }
        for (sseg, eseg, seg_idx) in overlap {
            let a = sseg.max(dr.start);
            let b = eseg.min(dr.end);
            if a >= b {
                continue;
            }
            let seg = &segments[seg_idx];
            let (metrics, embed) = resources.for_flags(seg.style.flags);
            let seg_font_size = seg
                .style
                .text_size
                .filter(|s| s.is_finite() && *s > 0.0)
                .map(|s| font_size * (s / sampled_font_size))
                .unwrap_or(font_size);
            let baseline_shift = seg
                .style
                .baseline_shift
                .filter(|s| s.is_finite())
                .map(|s| s * (font_size / sampled_font_size))
                .unwrap_or(0.0);
            let fill = seg.style.fill_rgb.unwrap_or(default_fill);

            // Split the overlap into embedded vs Standard-14 font runs (ASCII the
            // target font lacks), keeping byte spans so we can shape exact
            // sub-slices of `logical`. Reversed for RTL.
            let sub = &logical[a..b];
            let mut frs: Vec<(usize, usize, bool)> = Vec::new();
            let mut off = a;
            for fr in segment_font_runs(sub, metrics, embed) {
                frs.push((off, off + fr.text.len(), fr.use_standard14));
                off += fr.text.len();
            }
            if dr.rtl {
                frs.reverse();
            }

            for (fr_start, fr_end, use_standard14) in frs {
                let fr_text = &logical[fr_start..fr_end];
                if fr_text.is_empty() {
                    continue;
                }
                if use_standard14 || embed.is_none() {
                    let resource_name = BlockTypography::font_resource_for(FontStyleFlags {
                        bold: seg.style.flags.bold,
                        italic: seg.style.flags.italic,
                        monospace: resources.monospace,
                    })
                    .to_vec();
                    pieces.push(ShapedPiece {
                        advance: latin_fallback.measure(fr_text, seg_font_size),
                        content: PieceContent::Winansi(fr_text.to_string()),
                        resource_name,
                        font_size: seg_font_size,
                        baseline_shift,
                        fill_rgb: fill,
                    });
                    continue;
                }

                let embed = embed.unwrap();
                let Some((bytes, ttc_index, _)) = metrics.embedding_source() else {
                    continue;
                };
                let Some(face) = rustybuzz::Face::from_slice(&bytes, ttc_index) else {
                    continue;
                };
                let run = DirRun {
                    start: fr_start,
                    end: fr_end,
                    script: dr.script,
                    rtl: dr.rtl,
                    visual_index: 0,
                };
                let shaped = text_shape::shape_run(&logical, &run, &face);
                if shaped.glyphs.is_empty() {
                    continue;
                }
                let upem = metrics.units_per_em().unwrap_or(1000).max(1) as f32;
                let advance_units: i64 = shaped.glyphs.iter().map(|g| g.advance_x as i64).sum();
                let gids: Vec<u16> = shaped
                    .glyphs
                    .iter()
                    .map(|g| embed.gid_remap.get(&g.gid).copied().unwrap_or(g.gid))
                    .collect();
                pieces.push(ShapedPiece {
                    advance: advance_units as f32 / upem * seg_font_size,
                    content: PieceContent::Gids(gids),
                    resource_name: embed.resource_name.clone(),
                    font_size: seg_font_size,
                    baseline_shift,
                    fill_rgb: fill,
                });
            }
        }
    }

    let total: f32 = pieces.iter().map(|p| p.advance).sum();
    let mut cursor = if base_rtl {
        (avail_width - total).max(0.0)
    } else {
        0.0
    };
    for p in &pieces {
        let run_x = line_x + cursor * advance_dx;
        let run_y = line_y + cursor * advance_dy;
        let combined = Matrix {
            e: run_x + p.baseline_shift * line_dx,
            f: run_y + p.baseline_shift * line_dy,
            ..orientation
        };
        builder.set_fill_rgb(p.fill_rgb.0, p.fill_rgb.1, p.fill_rgb.2);
        builder.set_font(&p.resource_name, p.font_size);
        builder.set_text_matrix(combined.mul(*inv_ctm));
        match &p.content {
            PieceContent::Gids(gids) => builder.show_hex_gids(gids.iter().copied()),
            PieceContent::Winansi(text) => builder.show_winansi(text),
        }
        cursor += p.advance;
    }

    (line_x + cursor * advance_dx, line_y + cursor * advance_dy)
}

/// Cheap codepoint test for the major RTL blocks, used to skip full BiDi
/// analysis on the all-LTR lines that dominate most documents.
pub(crate) fn has_rtl_char(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32,
            0x0590..=0x05FF // Hebrew
            | 0x0600..=0x06FF // Arabic
            | 0x0700..=0x074F // Syriac
            | 0x0750..=0x077F // Arabic Supplement
            | 0x0780..=0x07BF // Thaana
            | 0x07C0..=0x07FF // NKo
            | 0x0800..=0x083F // Samaritan
            | 0x0840..=0x085F // Mandaic
            | 0x08A0..=0x08FF // Arabic Extended-A
            | 0xFB1D..=0xFB4F // Hebrew presentation forms
            | 0xFB50..=0xFDFF // Arabic presentation forms-A
            | 0xFE70..=0xFEFF // Arabic presentation forms-B
        )
    })
}

/// Does `text` contain any right-to-left script? Callers use this to decide
/// whether the logical-order, char-by-char emit path is safe (it isn't for RTL).
pub(crate) fn line_contains_rtl(text: &str) -> bool {
    has_rtl_char(text) && segment_runs(text).iter().any(|r| r.rtl)
}

/// Shape `text` end-to-end with `embed`'s font and return the subset glyph IDs
/// in visual order, concatenated across BiDi runs. For single-font lines (the
/// OCR overlay path) this is the whole-line equivalent of [`emit_line_shaped`].
/// Returns `None` if the font can't be parsed for shaping.
pub(crate) fn shape_line_to_gids(
    text: &str,
    metrics: &FontMetrics,
    embed: &EmbeddedFont,
) -> Option<Vec<u16>> {
    let (bytes, ttc_index, _) = metrics.embedding_source()?;
    let face = rustybuzz::Face::from_slice(&bytes, ttc_index)?;
    let mut runs = segment_runs(text);
    runs.sort_by_key(|r| r.visual_index);
    let mut gids = Vec::new();
    for run in &runs {
        for g in text_shape::shape_run(text, run, &face).glyphs {
            gids.push(embed.gid_remap.get(&g.gid).copied().unwrap_or(g.gid));
        }
    }
    Some(gids)
}

fn emit_tj_for_segment(
    builder: &mut ContentStreamBuilder,
    text: &str,
    metrics: &FontMetrics,
    embed: Option<&EmbeddedFont>,
) {
    if let Some(embedded) = embed {
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

/// Fit `text` inside (`vis_w`, `vis_h`) starting from the original sampled
/// `font_size`. Distinguishes single-line from multi-line originals and
/// chooses between width-shrink (single-line) and wrap-then-shrink
/// (multi-line). Refuses to shrink below `MIN_SHRINK_FRACTION` of the
/// sampled size — beyond that we'd produce unreadable output.
fn fit_with_sampled_size(
    text: &str,
    line_widths: &[f32],
    vis_h: f32,
    sampled: f32,
    metrics: &FontMetrics,
    original_lines: usize,
) -> (f32, Vec<String>) {
    let min_size = (sampled * MIN_SHRINK_FRACTION).max(MIN_FIT_FONT_SIZE_PT);

    if original_lines <= 1 {
        // Originally one line. Prefer keeping one line and shrink the font if
        // it'd overflow the bbox by more than tolerance. If it still overflows
        // at the readability floor, wrap unbreakable text by character rather
        // than letting CJK/URLs run across the page.
        let vis_w = line_width_at(line_widths, 0);
        let width_at_sampled = metrics.measure(text, sampled);
        let allowed = vis_w * OVERHANG_TOLERANCE;
        let final_size = if width_at_sampled <= allowed || width_at_sampled == 0.0 {
            sampled
        } else {
            (sampled * vis_w / width_at_sampled).max(min_size)
        };
        (final_size, vec![text.to_string()])
    } else {
        // Originally multi-line. Wrap at the sampled size, and if the
        // wrap produces more lines than the original used, shrink and
        // re-wrap. Targeting the original line count is what actually
        // matters: the original bbox height is just the union of glyph
        // ink. Keep iterating until we fit or hit the absolute readability
        // floor — a soft per-block fraction floor is too restrictive for
        // dense pages where the substitute font (DejaVu) is wider than
        // the producer's font (CMR/CMTI) and the sampled size already
        // packs the column tight.
        let mut size = sampled;
        let mut lines = wrap_lines_to_widths(text, line_widths, size, metrics);
        let absolute_min = MIN_FIT_FONT_SIZE_PT;
        while lines.len() > original_lines && size > absolute_min {
            size = (size * MULTILINE_SHRINK_FACTOR).max(absolute_min);
            lines = wrap_lines_to_widths(text, line_widths, size, metrics);
        }
        let _ = (vis_h, min_size);
        (size, lines)
    }
}

fn wrap_to_fit(
    text: &str,
    line_widths: &[f32],
    max_height: f32,
    mut font_size: f32,
    metrics: &FontMetrics,
) -> (f32, Vec<String>) {
    for _ in 0..FIT_RETRY_LIMIT {
        let lines = wrap_lines_to_widths(text, line_widths, font_size, metrics);
        let total_height = font_size * LINE_HEIGHT_FACTOR * lines.len() as f32;
        if total_height <= max_height || font_size <= MIN_FIT_FONT_SIZE_PT {
            return (font_size, lines);
        }
        font_size *= UNWRAPPED_SHRINK_FACTOR;
    }
    let final_size = font_size.max(MIN_FIT_FONT_SIZE_PT);
    (
        final_size,
        wrap_lines_to_widths(text, line_widths, final_size, metrics),
    )
}

fn wrap_lines_to_widths(
    text: &str,
    line_widths: &[f32],
    font_size: f32,
    metrics: &FontMetrics,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for hard_line in text.split('\n') {
        let mut current = String::new();
        for word in hard_line.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            let max_width = line_width_at(line_widths, lines.len());
            if metrics.measure(&candidate, font_size) <= max_width {
                current = candidate;
            } else if current.is_empty() {
                current = push_wrapped_word(word, line_widths, font_size, metrics, &mut lines);
            } else {
                lines.push(current);
                current = push_wrapped_word(word, line_widths, font_size, metrics, &mut lines);
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn push_wrapped_word(
    word: &str,
    line_widths: &[f32],
    font_size: f32,
    metrics: &FontMetrics,
    lines: &mut Vec<String>,
) -> String {
    let max_width = line_width_at(line_widths, lines.len());
    if metrics.measure(word, font_size) <= max_width || word.chars().count() <= 1 {
        return word.to_string();
    }

    let mut current = String::new();
    for ch in word.chars() {
        let mut candidate = current.clone();
        candidate.push(ch);
        let max_width = line_width_at(line_widths, lines.len());
        if !current.is_empty() && metrics.measure(&candidate, font_size) > max_width {
            lines.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    current
}

fn line_width_at(line_widths: &[f32], index: usize) -> f32 {
    line_widths
        .get(index)
        .copied()
        .or_else(|| line_widths.last().copied())
        .unwrap_or(1.0)
        .max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use translator_render::font_metrics::{FontDescriptorInfo, FontFileKind, GlyphInfo};

    /// Body bytes between `(` and `) Tj\n` from a single show_winansi call.
    fn encode_helper(text: &str) -> Vec<u8> {
        let mut builder = ContentStreamBuilder::new();
        builder.show_winansi(text);
        let bytes = builder.finish();
        let stripped = bytes
            .strip_prefix(b"(")
            .and_then(|b| b.strip_suffix(b") Tj\n"))
            .expect("show_winansi wraps body in (...) Tj\\n");
        stripped.to_vec()
    }

    #[test]
    fn encodes_basic_latin() {
        assert_eq!(encode_helper("Hola"), b"Hola");
        assert_eq!(
            encode_helper("á é í ó ú ñ"),
            b"\xE1 \xE9 \xED \xF3 \xFA \xF1"
        );
        assert_eq!(encode_helper("(parens)"), b"\\(parens\\)");
        assert_eq!(encode_helper("back\\slash"), b"back\\\\slash");
    }

    #[test]
    fn encodes_euro_as_single_byte() {
        assert_eq!(encode_helper("€100"), b"\x80100");
    }

    #[test]
    fn replaces_unmappable_codepoints() {
        assert_eq!(encode_helper("日本"), b"??");
    }

    fn fake_real_metrics(chars: &[char]) -> FontMetrics {
        let glyphs = chars
            .iter()
            .enumerate()
            .map(|(i, ch)| {
                (
                    *ch,
                    GlyphInfo {
                        gid: (i + 1) as u16,
                        advance: 500,
                    },
                )
            })
            .collect();
        FontMetrics::Real {
            units_per_em: 1000,
            glyphs,
            fallback: GlyphInfo {
                gid: 0,
                advance: 500,
            },
            used: Vec::new(),
            bytes: Arc::new(Vec::new()),
            ttc_index: 0,
            descriptor: FontDescriptorInfo {
                postscript_name: "Fake".into(),
                italic: false,
                bold: false,
                monospace: false,
                italic_angle: 0.0,
                ascent: 800,
                descent: -200,
                cap_height: 700,
                bbox: (0, -200, 1000, 800),
                kind: FontFileKind::TrueType,
            },
        }
    }

    fn arabic_font_path() -> Option<&'static str> {
        [
            "/usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf",
            "/home/david/Android/Sdk/platforms/android-28/data/fonts/NotoNaskhArabic-Regular.ttf",
            "/usr/share/fonts/truetype/noto/NotoSansArabicUI-Regular.ttf",
        ]
        .into_iter()
        .find(|p| std::path::Path::new(p).exists())
    }

    /// Parse the glyph ids out of the single `<HHHH...> Tj` the shaped emitter
    /// writes for a one-run line.
    fn parse_hex_tj_gids(stream: &str) -> Vec<u16> {
        let open = stream.find('<').expect("hex Tj present");
        let close = stream[open..].find("> Tj").expect("hex Tj terminator") + open;
        stream[open + 1..close]
            .as_bytes()
            .chunks(4)
            .map(|c| u16::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn arabic_line_emits_joined_visual_order_glyphs() {
        let Some(path) = arabic_font_path() else {
            eprintln!("no arabic font; skipping");
            return;
        };
        let text = "مرحبا بالعالم";
        let metrics = FontMetrics::from_file_for_text(path, 0, text).expect("parse arabic font");
        // Identity remap: emitted gids == shaped gids, so we can compare directly.
        let embed = EmbeddedFont {
            resource_name: b"Tr0".to_vec(),
            type0_id: (1, 0),
            gid_remap: HashMap::new(),
        };
        let mut by_flags = HashMap::new();
        by_flags.insert(BoldItalic::default(), (metrics.clone(), Some(embed)));
        let resources = BlockResources {
            by_flags,
            default_flags: BoldItalic::default(),
            monospace: false,
        };
        let segments = vec![LineSegment {
            text: text.to_string(),
            style: SegmentStyle {
                flags: BoldItalic::default(),
                fill_rgb: None,
                text_size: None,
                baseline_shift: None,
            },
        }];

        let mut builder = ContentStreamBuilder::new();
        builder.begin_text();
        emit_line_shaped(
            &mut builder,
            &segments,
            &resources,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            -1.0,
            20.0,
            20.0,
            500.0,
            Matrix::identity(),
            &Matrix::identity(),
            (0.0, 0.0, 0.0),
        );
        builder.end_text();
        let stream = String::from_utf8(builder.finish()).unwrap();
        let emitted = parse_hex_tj_gids(&stream);

        // Expected: rustybuzz shaped gids in visual order across the whole line.
        let font_bytes = std::fs::read(path).unwrap();
        let face = rustybuzz::Face::from_slice(&font_bytes, 0).unwrap();
        let mut expected: Vec<u16> = Vec::new();
        let mut runs = segment_runs(text);
        runs.sort_by_key(|r| r.visual_index);
        for run in &runs {
            for g in text_shape::shape_run(text, run, &face).glyphs {
                expected.push(g.gid);
            }
        }
        assert_eq!(
            emitted, expected,
            "emitted gids must be shaped + visual order"
        );

        // The naive cmap-per-codepoint path (the old bug) would emit the
        // isolated glyph of each char in logical order. The shaped output must
        // differ — proving joining + reordering actually happened.
        let naive: Vec<u16> = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| face.glyph_index(c).map(|g| g.0).unwrap_or(0))
            .collect();
        let emitted_nonspace: Vec<u16> = emitted.iter().copied().filter(|g| *g != 0).collect();
        assert_ne!(
            emitted_nonspace, naive,
            "shaped output must differ from naive isolated-glyph emission"
        );
    }

    #[test]
    fn ascii_segment_uses_standard14_when_target_font_lacks_latin() {
        let metrics = fake_real_metrics(&['আ']);
        let embed = EmbeddedFont {
            resource_name: b"Tr0".to_vec(),
            type0_id: (1, 0),
            gid_remap: HashMap::new(),
        };

        assert_eq!(
            segment_font_runs("https://sample-files.com", &metrics, Some(&embed)),
            vec![SegmentFontRun {
                text: "https://sample-files.com",
                use_standard14: true,
            }]
        );
        assert_eq!(
            segment_font_runs("আ", &metrics, Some(&embed)),
            vec![SegmentFontRun {
                text: "আ",
                use_standard14: false,
            }]
        );
    }

    #[test]
    fn mixed_segment_splits_unsupported_ascii_to_standard14() {
        let metrics = fake_real_metrics(&['আ', 'ম', 'ি']);
        let embed = EmbeddedFont {
            resource_name: b"Tr0".to_vec(),
            type0_id: (1, 0),
            gid_remap: HashMap::new(),
        };

        let runs = segment_font_runs("আমি 123 OK", &metrics, Some(&embed));

        assert_eq!(
            runs,
            vec![
                SegmentFontRun {
                    text: "আমি",
                    use_standard14: false,
                },
                SegmentFontRun {
                    text: " 123 OK",
                    use_standard14: true,
                },
            ]
        );
    }

    #[test]
    fn wraps_long_text_into_multiple_lines() {
        let text = "the quick brown fox jumps over the lazy dog repeatedly";
        let metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
        let lines = wrap_lines_to_widths(text, &[60.0], 10.0, &metrics);
        assert!(lines.len() > 1);
        for line in &lines {
            if line.contains(' ') {
                let w = metrics.measure(line, 10.0);
                assert!(w <= 60.0, "line too wide: {line:?} width {w}");
            }
        }
    }

    #[test]
    fn wraps_unspaced_cjk_text_into_multiple_lines() {
        let text = "这是一个没有空格的中文句子";
        let metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
        let lines = wrap_lines_to_widths(text, &[30.0], 10.0, &metrics);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(""), text);
        for line in &lines {
            let w = metrics.measure(line, 10.0);
            assert!(w <= 30.0, "line too wide: {line:?} width {w}");
        }
    }

    #[test]
    fn preserves_unspaced_single_line_when_shrink_floor_still_overflows() {
        let text = "这是一个没有空格的中文句子";
        let metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
        let (_size, lines) = fit_with_sampled_size(text, &[30.0], 20.0, 10.0, &metrics, 1);
        assert_eq!(lines, vec![text]);
    }

    #[test]
    fn preserves_spaced_single_line_when_shrink_floor_still_overflows() {
        let text = "Lead in prose";
        let metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
        let (_size, lines) = fit_with_sampled_size(text, &[20.0], 12.0, 10.0, &metrics, 1);
        assert_eq!(lines, vec![text]);
    }

    #[test]
    fn literal_line_segments_preserve_monospace_spacing() {
        let segments = literal_line_segments(
            "VIA  <1184,1080>       8:36 ICE143",
            BoldItalic {
                bold: false,
                italic: false,
            },
        );

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "VIA  <1184,1080>       8:36 ICE143");
    }

    #[test]
    fn counts_source_rect_visual_lines() {
        let rects = vec![
            Rect {
                left: 20,
                top: 509,
                right: 797,
                bottom: 520,
            },
            Rect {
                left: 20,
                top: 521,
                right: 808,
                bottom: 533,
            },
            Rect {
                left: 20,
                top: 534,
                right: 794,
                bottom: 546,
            },
        ];

        assert_eq!(source_rect_line_count(&rects), 3);
    }

    #[test]
    fn line_widths_follow_source_rect_line_boxes() {
        let geom = PageGeometry {
            user_w: 842.0,
            user_h: 595.0,
            user_x_min: 0.0,
            user_y_min: 0.0,
            rotate: 0,
        };
        let user_rect = geom.user_rect_from_display(Rect {
            left: 20,
            top: 100,
            right: 320,
            bottom: 136,
        });
        let source_rects = vec![
            Rect {
                left: 20,
                top: 100,
                right: 320,
                bottom: 112,
            },
            Rect {
                left: 40,
                top: 124,
                right: 220,
                bottom: 136,
            },
        ];
        let geometry = BlockGeometry {
            line_anchors: vec![(20.0, 490.0)],
            ..BlockGeometry::default()
        };

        let widths =
            line_available_widths(&geometry, &source_rects, user_rect, None, None, geom, 300.0);

        assert_eq!(widths, vec![300.0, 180.0]);
    }

    #[test]
    fn detects_rotated_visual_top_origin() {
        let geometry = BlockGeometry {
            text_orientation: Matrix {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                e: 0.0,
                f: 0.0,
            },
            ..BlockGeometry::default()
        };

        assert!(uses_visual_top_origin(&geometry));
    }

    #[test]
    fn display_to_user_round_trips_rotated_point() {
        let geom = PageGeometry {
            user_w: 595.0,
            user_h: 842.0,
            user_x_min: 0.0,
            user_y_min: 0.0,
            rotate: 90,
        };
        let display = (22.0, 216.0);

        let user = display_to_user(display, geom);

        assert_eq!(geom.to_display(user), display);
    }

    #[test]
    fn rotated_line_width_uses_visual_bbox_left_edge() {
        let geom = PageGeometry {
            user_w: 595.0,
            user_h: 842.0,
            user_x_min: 0.0,
            user_y_min: 0.0,
            rotate: 90,
        };
        let user_rect = geom.user_rect_from_display(translator_core::ocr::Rect {
            left: 20,
            top: 121,
            right: 53,
            bottom: 132,
        });
        let geometry = BlockGeometry {
            line_anchors: vec![(126.0, 52.0)],
            ..BlockGeometry::default()
        };

        let widths = line_available_widths(&geometry, &[], user_rect, None, None, geom, 33.0);

        assert_eq!(widths, vec![33.0]);
    }

    #[test]
    fn preserves_hard_line_breaks_when_wrapping() {
        let metrics = FontMetrics::approx(HELVETICA_AVG_ADVANCE);
        let lines =
            wrap_lines_to_widths("alpha beta\ngamma delta", &[200.0, 200.0], 10.0, &metrics);
        assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
    }

    #[test]
    fn same_row_plain_text_blocks_do_not_inherit_x() {
        assert_eq!(inline_x_override(Some((120.0, 300.0)), false, 300.0), None);
    }

    #[test]
    fn prose_after_opaque_inline_block_inherits_x() {
        assert_eq!(
            inline_x_override(Some((120.0, 300.0)), true, 302.0),
            Some(126.0)
        );
    }

    #[test]
    fn prose_after_opaque_block_on_different_row_keeps_anchor() {
        assert_eq!(inline_x_override(Some((120.0, 300.0)), true, 308.5), None);
    }
}
