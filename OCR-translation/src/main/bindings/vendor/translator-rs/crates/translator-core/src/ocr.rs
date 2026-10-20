#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Rect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

/// A rotated rectangle. `cx`/`cy` are the centre in image pixels, `width` runs along the
/// reading direction (so for horizontal text it's the visible text width), `height` is
/// perpendicular to it, and `angle_radians` is the rotation of the reading direction relative
/// to the image's +x axis (image y points down, so a positive angle tilts text downward to the
/// right when read left-to-right).
///
/// We keep both `Rect` (AABB) and this type on detection structs. The AABB is fine for sorting,
/// hit-testing, and the Tesseract path which has no rotation information; the oriented rect is
/// what the erase/render steps consult so that tilted text (PPOCR detections) doesn't pick up
/// the inflated AABB height as its layout box.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OrientedRect {
    pub cx: f32,
    pub cy: f32,
    pub width: f32,
    pub height: f32,
    pub angle_radians: f32,
}

impl OrientedRect {
    /// Lift an axis-aligned rectangle into an unrotated oriented rectangle. Used wherever a
    /// pipeline (e.g. Tesseract) only produces AABBs.
    pub fn axis_aligned(rect: Rect) -> Self {
        let cx = (rect.left as f32 + rect.right as f32) * 0.5;
        let cy = (rect.top as f32 + rect.bottom as f32) * 0.5;
        Self {
            cx,
            cy,
            width: rect.width() as f32,
            height: rect.height() as f32,
            angle_radians: 0.0,
        }
    }

    /// Sub-rectangle spanning `[x0, x1]` measured from this rect's left edge along its reading
    /// direction, keeping full height and angle. `x0`/`x1` are in the same units as `width`
    /// (image pixels along the baseline). Used to carve a single word's box out of a line's
    /// oriented box given the word's start/end offset along the line.
    pub fn subspan(&self, x0: f32, x1: f32) -> OrientedRect {
        let cos = self.angle_radians.cos();
        let sin = self.angle_radians.sin();
        let half_w = self.width * 0.5;
        let mid = (x0 + x1) * 0.5;
        OrientedRect {
            cx: self.cx - half_w * cos + mid * cos,
            cy: self.cy - half_w * sin + mid * sin,
            width: (x1 - x0).max(0.0),
            height: self.height,
            angle_radians: self.angle_radians,
        }
    }

    /// Axis-aligned bounding box of the oriented rect — the smallest `Rect` that contains all
    /// four corners. Useful for coarse hit-testing, sorting, and falling back to AABB-only
    /// downstream consumers.
    pub fn to_aabb(&self) -> Rect {
        let abs_cos = self.angle_radians.cos().abs();
        let abs_sin = self.angle_radians.sin().abs();
        let hw = self.width * 0.5;
        let hh = self.height * 0.5;
        let half_aabb_w = hw * abs_cos + hh * abs_sin;
        let half_aabb_h = hw * abs_sin + hh * abs_cos;
        Rect {
            left: (self.cx - half_aabb_w).max(0.0).round() as u32,
            top: (self.cy - half_aabb_h).max(0.0).round() as u32,
            right: (self.cx + half_aabb_w).max(0.0).round() as u32,
            bottom: (self.cy + half_aabb_h).max(0.0).round() as u32,
        }
    }

    /// Four corners in image-pixel coordinates, ordered TL, TR, BR, BL relative to the line's
    /// reading direction (so "TL" is the corner the first glyph's ascent touches, even for a
    /// rotated line).
    pub fn corners(&self) -> [(f32, f32); 4] {
        let cos = self.angle_radians.cos();
        let sin = self.angle_radians.sin();
        let hw = self.width * 0.5;
        let hh = self.height * 0.5;
        // Tangent (reading direction): (cos, sin). Perpendicular pointing across the line in
        // the +y direction of image space: (-sin, cos).
        let (tx, ty) = (cos, sin);
        let (px, py) = (-sin, cos);
        [
            (self.cx - hw * tx - hh * px, self.cy - hw * ty - hh * py),
            (self.cx + hw * tx - hh * px, self.cy + hw * ty - hh * py),
            (self.cx + hw * tx + hh * px, self.cy + hw * ty + hh * py),
            (self.cx - hw * tx + hh * px, self.cy - hw * ty + hh * py),
        ]
    }

    /// True when the oriented rect is within `epsilon` of axis-aligned; the renderer uses this
    /// to skip the rotated-rasterization path when there's nothing to rotate.
    pub fn is_axis_aligned(&self, epsilon: f32) -> bool {
        self.angle_radians.abs() < epsilon
    }

    /// True when `(px, py)` lies inside the oriented rect (project into the rect's local frame
    /// and bounds-check). Used to decide whether a provisional pill is covered by a block pill.
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        let cos = self.angle_radians.cos();
        let sin = self.angle_radians.sin();
        let dx = px - self.cx;
        let dy = py - self.cy;
        let lx = dx * cos + dy * sin;
        let ly = -dx * sin + dy * cos;
        lx.abs() <= self.width * 0.5 && ly.abs() <= self.height * 0.5
    }
}

impl Default for OrientedRect {
    fn default() -> Self {
        Self::axis_aligned(Rect::default())
    }
}

impl Rect {
    pub fn width(&self) -> u32 {
        self.right.saturating_sub(self.left)
    }

    pub fn height(&self) -> u32 {
        self.bottom.saturating_sub(self.top)
    }

    pub fn center_y(&self) -> u32 {
        (self.top + self.bottom) / 2
    }

    pub fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }

    pub fn union(&mut self, other: Self) {
        if self.is_empty() {
            *self = other;
            return;
        }

        self.left = self.left.min(other.left);
        self.top = self.top.min(other.top);
        self.right = self.right.max(other.right);
        self.bottom = self.bottom.max(other.bottom);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ReadingOrder {
    #[default]
    LeftToRight,
    TopToBottomRightToLeft,
}

/// Source-language selection for OCR.
///
/// `Auto` runs PPOCR's PULC script classifier per detected strip, then derives a
/// translation source language with CLD over the recognized text. `Specific`
/// pins the recognizer to one language and skips classification + detection
/// post-processing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OcrSourceSelection {
    #[default]
    Auto,
    Specific {
        language_code: crate::api::LanguageCode,
    },
}

impl OcrSourceSelection {
    pub fn auto() -> Self {
        Self::Auto
    }

    pub fn specific(language_code: impl Into<crate::api::LanguageCode>) -> Self {
        Self::Specific {
            language_code: language_code.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum OverlayLayoutMode {
    #[default]
    PerLine,
    VerticalBlockRect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedWord {
    pub text: String,
    pub confidence: f32,
    pub bounding_box: Rect,
    pub is_at_beginning_of_para: bool,
    pub end_para: bool,
    pub end_line: bool,
}

/// The style a [`StyleRange`] applies. Bold is a weight flag; the decoration variants carry
/// an under/strike/over-line; `Color` is an emphasis foreground override (a word whose ink colour
/// is an outlier from its line's geometric core). A run can be several at once — those are
/// overlapping ranges of different kinds, resolved into per-byte [`StyleSpan`] attributes
/// downstream. Unlike the others, `Color` is semantic (follows the token through translation),
/// whereas the line's base colour is geometric and assigned at render — see [`StyleSpan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleKind {
    Bold,
    Decoration(LineDecoration),
    Color(u32),
}

/// A byte range `[start, end)` of text (line, block source, or translated block) carrying one
/// [`StyleKind`]. Empty list means "no per-word style information"; a single `[0, len)` Bold
/// means the whole string is bold. Carries per-word weight and decoration through grouping and
/// translation so the overlay can reproduce mixed-weight/decorated lines, not one flag per block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleRange {
    pub start: u32,
    pub end: u32,
    pub kind: StyleKind,
}

/// Sort and coalesce touching/overlapping ranges of the *same* kind into a minimal set. Ranges
/// of different kinds never merge (a bold range and an underline range stay distinct even when
/// they cover the same bytes), so overlap between kinds is preserved.
pub fn merge_style_ranges(mut ranges: Vec<StyleRange>) -> Vec<StyleRange> {
    ranges.sort_by(|a, b| {
        style_kind_key(a.kind)
            .cmp(&style_kind_key(b.kind))
            .then(a.start.cmp(&b.start))
    });
    let mut out: Vec<StyleRange> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if last.kind == r.kind && r.start <= last.end => {
                last.end = last.end.max(r.end)
            }
            _ => out.push(r),
        }
    }
    out
}

/// Sort key grouping equal kinds together (distinct colours stay in distinct groups so they never
/// coalesce). The tag separates the variants; the payload orders colours among themselves.
fn style_kind_key(kind: StyleKind) -> (u8, u32) {
    match kind {
        StyleKind::Bold => (0, 0),
        StyleKind::Decoration(LineDecoration::Underline) => (1, 0),
        StyleKind::Decoration(LineDecoration::Strikethrough) => (2, 0),
        StyleKind::Decoration(LineDecoration::Overline) => (3, 0),
        StyleKind::Color(c) => (4, c),
    }
}

/// Ink colour measured by the ink model's colour head (FBA-style F/B prediction):
/// `fg_argb` is the α²-pooled predicted ink colour, `bg_luma` the pooled predicted
/// background luma the overlay checks its contrast floor against. Predicted per pixel
/// by the model rather than sampled from matte-selected image pixels, so a matte that
/// grabs background does not poison the colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InkColor {
    pub fg_argb: u32,
    pub bg_luma: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub bounding_box: Rect,
    /// Oriented bounding box of this line. For Tesseract output and other axis-aligned
    /// pipelines this is just `bounding_box` lifted via `OrientedRect::axis_aligned`. For
    /// PPOCR it's the min-area rotated rectangle around the detection contour, so tilted
    /// text gets a tight box instead of the inflated AABB.
    pub oriented_box: OrientedRect,
    /// Glyph-tight oriented rect — same centre/angle as `oriented_box` but without
    /// ascender/descender padding (for PPOCR, the pre-unclip mask kernel). Used by paragraph
    /// grouping to cluster lines by x-height-like metric and to measure inter-line gaps in
    /// "tight heights" instead of inflated bounding-box heights. Falls back to `oriented_box`
    /// for engines that don't expose a tighter metric.
    pub tight_box: OrientedRect,
    pub word_rects: Vec<Rect>,
    /// Per-word style byte ranges within `text`: bold from ink-bold pooling and decoration from
    /// the ink rule channel (both paired with the line's CTC firings). Empty = plain text.
    /// Carried through grouping so the overlay renders mixed-weight/decorated lines.
    pub style_ranges: Vec<StyleRange>,
    /// Model-predicted ink colour for this line (colour-head ink models only). `None` for
    /// legacy models or too little ink — the overlay then falls back to sampling the colour
    /// from matte-selected image pixels.
    pub ink_color: Option<InkColor>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    pub lines: Vec<TextLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OverlayColors {
    pub background_argb: u32,
    pub foreground_argb: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct OverlayLayoutHints {
    pub layout_mode: OverlayLayoutMode,
    pub suggested_font_size_px: f32,
}

/// A line decoration that runs along the text rather than being part of a glyph: an
/// under/strike/over-line. Carried per-span (only the hyperlink portion of a line is
/// underlined, etc.) so the overlay reproduces it on the matching translated run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum LineDecoration {
    Underline,
    Strikethrough,
    Overline,
}

/// A byte range `[start, end)` of `translated_text` (or a source string) with constant style:
/// weight, decoration, and an optional colour *override*. Spans tile the text — every byte is in
/// exactly one. `foreground` is `None` when the run takes the line's geometric core colour (the
/// common image-overlay case — colour there is a per-line geometric property, not a per-token
/// one); `Some(argb)` is an explicit override, for the PDF's true per-run colours and (later) an
/// emphasis colour carried through alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StyleSpan {
    pub start: u32,
    pub end: u32,
    pub bold: bool,
    pub foreground: Option<u32>,
    pub decoration: Option<LineDecoration>,
}

/// Build style spans tiling `[0, text_len)` from per-word [`StyleRange`]s. Bold and decoration
/// ranges may overlap; the text is split at every range boundary and each resulting tile records
/// whether a Bold range covers it and which decoration (if any) does. Plain gaps get spans too,
/// so every byte is covered. Adjacent tiles with identical attributes are coalesced. `foreground`
/// is `None` — image-overlay colour is the line's geometric core, applied at render; a per-run
/// colour override is set separately (PDF) or carried later as an emphasis range.
pub fn style_spans_from_styles(text_len: usize, ranges: &[StyleRange]) -> Vec<StyleSpan> {
    let len = text_len as u32;
    if len == 0 {
        return Vec::new();
    }
    let clip = |r: &StyleRange| (r.start.min(len), r.end.min(len));
    let mut cuts: Vec<u32> = vec![0, len];
    for r in ranges {
        let (s, e) = clip(r);
        if s < e {
            cuts.push(s);
            cuts.push(e);
        }
    }
    cuts.sort_unstable();
    cuts.dedup();

    let covers = |s: u32, e: u32, kind: StyleKind| {
        ranges.iter().any(|r| {
            let (rs, re) = clip(r);
            r.kind == kind && rs <= s && re >= e && rs < re
        })
    };
    let decoration_at = |s: u32, e: u32| {
        [
            LineDecoration::Underline,
            LineDecoration::Strikethrough,
            LineDecoration::Overline,
        ]
        .into_iter()
        .find(|&d| covers(s, e, StyleKind::Decoration(d)))
    };
    let color_at = |s: u32, e: u32| {
        ranges.iter().find_map(|r| {
            let (rs, re) = clip(r);
            match r.kind {
                StyleKind::Color(c) if rs <= s && re >= e && rs < re => Some(c),
                _ => None,
            }
        })
    };

    let mut spans: Vec<StyleSpan> = Vec::new();
    for w in cuts.windows(2) {
        let (s, e) = (w[0], w[1]);
        if s >= e {
            continue;
        }
        let span = StyleSpan {
            start: s,
            end: e,
            bold: covers(s, e, StyleKind::Bold),
            foreground: color_at(s, e),
            decoration: decoration_at(s, e),
        };
        match spans.last_mut() {
            Some(last)
                if last.end == s
                    && last.bold == span.bold
                    && last.foreground == span.foreground
                    && last.decoration == span.decoration =>
            {
                last.end = e
            }
            _ => spans.push(span),
        }
    }
    spans
}

/// Extract per-attribute [`StyleRange`]s from a span tiling — the inverse of
/// [`style_spans_from_styles`]. A bold span yields a `Bold` range, a coloured span a `Color`
/// range, a decorated span a `Decoration` range; `merge_style_ranges` coalesces adjacent equals.
/// Used to re-remap a block's source styles onto a new translation.
pub fn style_ranges_from_spans(spans: &[StyleSpan]) -> Vec<StyleRange> {
    let mut out = Vec::new();
    for s in spans {
        if s.bold {
            out.push(StyleRange {
                start: s.start,
                end: s.end,
                kind: StyleKind::Bold,
            });
        }
        if let Some(argb) = s.foreground {
            out.push(StyleRange {
                start: s.start,
                end: s.end,
                kind: StyleKind::Color(argb),
            });
        }
        if let Some(dec) = s.decoration {
            out.push(StyleRange {
                start: s.start,
                end: s.end,
                kind: StyleKind::Decoration(dec),
            });
        }
    }
    merge_style_ranges(out)
}

/// One control point of a line's geometric core colour, at reading-axis fraction `at` in `[0, 1]`.
/// The core colour is the line's ink colour as it sits on the page — a geometric property of
/// *where* the text is drawn (illumination, region), not of the token, so it never crosses the
/// translation. One stop is a flat line colour; more stops interpolate a gradient along the line.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct LineColorStop {
    pub at: f32,
    pub argb: u32,
}

/// Sample a line's core colour field at reading-axis fraction `frac`. `None` for an empty field
/// (no geometric colour — the caller falls back). One stop is constant; otherwise the bracketing
/// stops are linearly interpolated per channel.
pub fn sample_line_color(stops: &[LineColorStop], frac: f32) -> Option<u32> {
    match stops {
        [] => None,
        [only] => Some(only.argb),
        _ => {
            let f = frac.clamp(0.0, 1.0);
            let hi = stops
                .iter()
                .position(|s| s.at >= f)
                .unwrap_or(stops.len() - 1);
            if hi == 0 {
                return Some(stops[0].argb);
            }
            let (a, b) = (&stops[hi - 1], &stops[hi]);
            let span = (b.at - a.at).max(f32::EPSILON);
            Some(lerp_argb(
                a.argb,
                b.argb,
                ((f - a.at) / span).clamp(0.0, 1.0),
            ))
        }
    }
}

fn lerp_argb(a: u32, b: u32, t: f32) -> u32 {
    let mix = |sh: u32| {
        let ca = ((a >> sh) & 0xFF) as f32;
        let cb = ((b >> sh) & 0xFF) as f32;
        ((ca + (cb - ca) * t).round() as u32 & 0xFF) << sh
    };
    mix(24) | mix(16) | mix(8) | mix(0)
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PreparedTextLine {
    pub text: String,
    pub bounding_box: Rect,
    /// Oriented box used by the overlay renderer to position rotated text. Mirror of
    /// `TextLine::oriented_box`.
    pub oriented_box: OrientedRect,
    pub word_rects: Vec<Rect>,
    /// Mask/erase colour for the PDF overlay, which paints a filled rect over the original text
    /// before drawing the translation. Not a text-style attribute (the image overlay erases via
    /// the ink matte, not a flat fill), so it stays off `StyleSpan`. Image path ignores it.
    pub background_argb: u32,
    /// Geometric core colour along the line (reading-axis fraction → colour). The renderer colours
    /// this output line from its source line's field, overridden per byte by a span's
    /// `foreground`. One stop today; a gradient later. Empty for paths with no sampled ink colour.
    pub foreground: Vec<LineColorStop>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PreparedTextBlock {
    pub source_text: String,
    pub translated_text: String,
    pub bounding_box: Rect,
    pub lines: Vec<PreparedTextLine>,
    pub layout_hints: OverlayLayoutHints,
    /// Style spans over `translated_text` (weight + colour + decoration per run), tiling the
    /// text. The renderer splits each line's text on these. Replaces the old `bold_ranges` +
    /// per-line/block colour fields.
    pub style_spans: Vec<StyleSpan>,
    /// The same style, but over `source_text` (the recognised source language). Carried so a
    /// language switch (`retranslate`) can re-remap the styles onto the new translation through a
    /// fresh alignment, instead of leaving `style_spans` stranded at the old language's offsets.
    /// Empty for paths that never retranslate (live/tracker).
    pub source_style_spans: Vec<StyleSpan>,
}

/// One selectable word with its position on the image, in image-pixel space. Emitted for
/// both the recognized source text (positions derived from CTC firings) and the rendered
/// translation (positions derived from the overlay layout), so the UI can hit-test a drag and
/// copy either layer. `text` is the word's own substring; `bounds` is its oriented box.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PositionedWord {
    pub text: String,
    pub bounds: OrientedRect,
    /// Index of the visual line this word belongs to, within its layer (source or translated).
    /// Lets the UI group words back into lines for per-line selection highlighting.
    pub line_index: u32,
}

/// Reorder words into visual reading order: lines (grouped by `line_index`) sorted top-to-bottom
/// by their cross-reading position, words within a line by reading position. The detection/block
/// grouping order isn't visually monotonic, so callers (drag-to-select) get this so a contiguous
/// range is contiguous on screen — keeping the ordering in the crate, not each consumer.
///
/// Lines whose `line_index` is in `rtl_lines` read right-to-left, so their words are put into
/// logical order by [`bidi_reorder_rtl_line`]: the line is reversed, but each embedded run of
/// left-to-right words (Latin, digits) keeps its internal order. A contiguous index range still
/// maps to a visually-contiguous run, so drag-select stays intuitive while list order joins to
/// logical (not screen-left-to-right) text.
pub fn order_words_visually(
    words: Vec<PositionedWord>,
    rtl_lines: &std::collections::HashSet<u32>,
) -> Vec<PositionedWord> {
    fn cross(b: &OrientedRect) -> f32 {
        -b.cx * b.angle_radians.sin() + b.cy * b.angle_radians.cos()
    }
    fn read(b: &OrientedRect) -> f32 {
        b.cx * b.angle_radians.cos() + b.cy * b.angle_radians.sin()
    }
    let mut by_line: std::collections::HashMap<u32, Vec<PositionedWord>> =
        std::collections::HashMap::new();
    for w in words {
        by_line.entry(w.line_index).or_default().push(w);
    }
    let mut lines: Vec<Vec<PositionedWord>> = by_line.into_values().collect();
    for line in &mut lines {
        line.sort_by(|a, b| {
            read(&a.bounds)
                .partial_cmp(&read(&b.bounds))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if line
            .first()
            .is_some_and(|w| rtl_lines.contains(&w.line_index))
        {
            bidi_reorder_rtl_line(line);
        }
    }
    lines.sort_by(|a, b| {
        let ca = a.iter().map(|w| cross(&w.bounds)).fold(f32::MAX, f32::min);
        let cb = b.iter().map(|w| cross(&w.bounds)).fold(f32::MAX, f32::min);
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });
    lines.into_iter().flatten().collect()
}

fn is_rtl_char(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0750..=0x077F // Arabic Supplement
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB1D..=0xFB4F // Hebrew presentation forms
        | 0xFB50..=0xFDFF // Arabic presentation forms-A
        | 0xFE70..=0xFEFF // Arabic presentation forms-B
    )
}

/// A word carries the right-to-left base direction only if it contains a strong RTL character;
/// Latin/digit/punctuation-only words are left-to-right and keep their order inside an RTL line.
fn word_is_ltr(text: &str) -> bool {
    !text.chars().any(is_rtl_char)
}

/// Put a right-to-left line's words (given in visual left-to-right order) into logical order:
/// reverse the whole line, then restore the internal order of each maximal run of LTR words. This
/// is the word-level analogue of the character-level `reverse_visual_to_logical` in the recognizer,
/// so an embedded `Play Services` reads forward while the surrounding RTL words read right-to-left.
fn bidi_reorder_rtl_line(line: &mut [PositionedWord]) {
    line.reverse();
    let mut i = 0;
    while i < line.len() {
        if !word_is_ltr(&line[i].text) {
            i += 1;
            continue;
        }
        let start = i;
        while i < line.len() && word_is_ltr(&line[i].text) {
            i += 1;
        }
        line[start..i].reverse();
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PreparedImageOverlay {
    pub rgba_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub extracted_text: String,
    pub translated_text: String,
    pub blocks: Vec<PreparedTextBlock>,
    /// Recognized source words with image-space boxes, in reading order. Drives drag-to-copy
    /// of the original text. Empty until the source-word pass populates it.
    pub source_words: Vec<PositionedWord>,
    /// Rendered translation words with image-space boxes, in reading order. Drives drag-to-copy
    /// of the translated text. Empty until the layout-word pass populates it.
    pub translated_words: Vec<PositionedWord>,
}

/// Lightweight detection result — what the PaddlePaddle detector produces before
/// the recognizer is run. Exposes only the box geometry so callers can cheaply
/// track box motion across frames (live mode) and selectively run recognition.
///
/// `contour` is the raw detection polygon flattened to alternating x,y in image
/// pixels (length is even). Empty means the detector didn't produce a usable
/// contour and the recognizer should fall back to AABB cropping. When non-empty
/// the recognizer dewarps along the contour for tilted/curved text.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct DetectedTextBox {
    pub rect: Rect,
    pub oriented_box: OrientedRect,
    pub tight_box: OrientedRect,
    pub contour: Vec<f32>,
    pub score: f32,
}

impl DetectedTextBox {
    /// Scale all geometry by a per-axis `(sx, sy)` (rect, oriented/tight boxes,
    /// contour); angle unchanged. Used to map a box between coordinate spaces of
    /// different resolution (e.g. canonical → half-res rec source for cropping).
    pub fn scaled_xy(&self, sx: f32, sy: f32) -> DetectedTextBox {
        let r = OrientedRect {
            cx: self.oriented_box.cx * sx,
            cy: self.oriented_box.cy * sy,
            width: self.oriented_box.width * sx,
            height: self.oriented_box.height * sy,
            angle_radians: self.oriented_box.angle_radians,
        };
        let t = OrientedRect {
            cx: self.tight_box.cx * sx,
            cy: self.tight_box.cy * sy,
            width: self.tight_box.width * sx,
            height: self.tight_box.height * sy,
            angle_radians: self.tight_box.angle_radians,
        };
        let contour = self
            .contour
            .iter()
            .enumerate()
            .map(|(i, v)| if i % 2 == 0 { v * sx } else { v * sy })
            .collect();
        DetectedTextBox {
            rect: Rect {
                left: ((self.rect.left as f32) * sx) as u32,
                top: ((self.rect.top as f32) * sy) as u32,
                right: ((self.rect.right as f32) * sx).ceil() as u32,
                bottom: ((self.rect.bottom as f32) * sy).ceil() as u32,
            },
            oriented_box: r,
            tight_box: t,
            contour,
            score: self.score,
        }
    }
}

/// Output of recognition over a previously-detected box. The caller can feed
/// `text` to a translation/cache layer.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RecognizedTextLine {
    pub rect: Rect,
    pub oriented_box: OrientedRect,
    pub text: String,
    pub confidence: f32,
    /// Source language selected for this recognition result when the caller used auto-source
    /// OCR. `None` in forced-source mode or when post-OCR language detection could not choose
    /// a translation source.
    pub source_code: Option<String>,
    /// Per-character CTC firings, in the final text's order. The caller pairs these with the
    /// ink bold strip to pool bold per word. Only populated for single-chunk, non-RTL lines
    /// where the firings line up 1:1 with `text`; empty otherwise (the caller then falls back
    /// to a per-line bold estimate).
    pub firings: Vec<CharFiring>,
    /// Per-character CTC firings in visual (left-to-right along the strip) order, 1:1 with the
    /// visually-ordered glyphs. Unlike `firings` (logical order, gated out for RTL so the style
    /// pooling stays aligned) this stays populated for RTL lines, and drives the selectable
    /// source-word geometry — which is inherently a visual-space quantity. Empty on dropped
    /// (empty/low-score) lines.
    pub visual_firings: Vec<CharFiring>,
}

/// One CTC firing: a recognised character (as a Unicode scalar) and where its run fired
/// along the strip's reading axis (`0..=1`). `u32` rather than `char` so the type is
/// FFI-safe as a `uniffi::Record` field.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct CharFiring {
    pub ch: u32,
    pub at: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct WordInfo {
    text: String,
    confidence: f32,
    bounding_box: Rect,
    ghost_bbox: Option<Rect>,
    is_first_in_line: bool,
    is_last_in_line: bool,
    is_last_in_para: bool,
}

enum LineJoin {
    /// Insert a single space between the two lines (default for Latin/Cyrillic/Greek text).
    Space,
    /// Concatenate with nothing between (CJK, or after stripping an end-of-line hyphen).
    Concat,
}

fn is_cjk_char(c: char) -> bool {
    let cp = c as u32;
    // Han Unified Ideographs + Ext A + Compatibility, plus Hiragana, Katakana, and Hangul
    // syllables/Jamo. Excludes the Halfwidth/Fullwidth Forms block to avoid falsely matching
    // half-width ASCII surrogates that some OCR outputs use.
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0xAC00..=0xD7AF).contains(&cp)
        || (0x1100..=0x11FF).contains(&cp)
}

/// Whether a line should be word-segmented per glyph (CJK) rather than on spaces/gaps: true
/// when at least half its non-space characters are CJK. The still path keys this off the
/// recognizer's script class; the live path has only the recognized text.
pub fn is_cjk_text(s: &str) -> bool {
    let (mut cjk, mut total) = (0usize, 0usize);
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if is_cjk_char(c) {
            cjk += 1;
        }
    }
    total > 0 && cjk * 2 >= total
}

impl TextBlock {
    pub fn source_text(&self) -> String {
        self.lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn translation_text(&self) -> String {
        let mut out = String::new();
        for line in self
            .lines
            .iter()
            .map(|l| l.text.trim())
            .filter(|l| !l.is_empty())
        {
            if out.is_empty() {
                out.push_str(line);
                continue;
            }
            let prev_last = out.chars().next_back();
            let next_first = line.chars().next();
            let separator = match (prev_last, next_first) {
                // OCR-style soft hyphen at end of line preceding a lowercase letter: the source
                // word was broken at the hyphen, so glue the halves together and drop the
                // dangling hyphen. We use a permissive lowercase test to catch German/Spanish
                // diacritics, not just ASCII. Mirrors the Tesseract path's behaviour in
                // `merge_hyphenated_words`.
                (Some(prev), Some(next))
                    if matches!(prev, '-' | '\u{2010}' | '\u{00AD}') && next.is_lowercase() =>
                {
                    out.pop();
                    LineJoin::Concat
                }
                // CJK on both sides of the line break: no space.
                (Some(prev), Some(next)) if is_cjk_char(prev) && is_cjk_char(next) => {
                    LineJoin::Concat
                }
                _ => LineJoin::Space,
            };
            if matches!(separator, LineJoin::Space) {
                out.push(' ');
            }
            out.push_str(line);
        }
        out
    }

    /// Like [`Self::translation_text`] but also returns the block's style byte ranges, each
    /// line's per-word [`TextLine::style_ranges`] re-based onto the joined string (accounting
    /// for per-line trim, the dropped soft hyphen, and the space/concat separator). Drives
    /// per-word bold and decoration through translation; empty when no line carried style ranges.
    pub fn translation_text_with_styles(&self) -> (String, Vec<StyleRange>) {
        let mut out = String::new();
        let mut styles: Vec<StyleRange> = Vec::new();
        for line in &self.lines {
            let trimmed = line.text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let lead = line.text.len() - line.text.trim_start().len();
            let trail_end = line.text.trim_end().len();
            if !out.is_empty() {
                let prev_last = out.chars().next_back();
                let next_first = trimmed.chars().next();
                match (prev_last, next_first) {
                    (Some(prev), Some(next))
                        if matches!(prev, '-' | '\u{2010}' | '\u{00AD}') && next.is_lowercase() =>
                    {
                        out.pop();
                    }
                    (Some(prev), Some(next)) if is_cjk_char(prev) && is_cjk_char(next) => {}
                    _ => out.push(' '),
                }
            }
            let base = out.len();
            out.push_str(trimmed);
            for r in &line.style_ranges {
                let s = (r.start as usize).max(lead);
                let e = (r.end as usize).min(trail_end);
                if s < e {
                    styles.push(StyleRange {
                        start: (base + s - lead) as u32,
                        end: (base + e - lead) as u32,
                        kind: r.kind,
                    });
                }
            }
        }
        let len = out.len() as u32;
        for r in styles.iter_mut() {
            r.end = r.end.min(len);
        }
        styles.retain(|r| r.start < r.end);
        (out, merge_style_ranges(styles))
    }

    pub fn bounds(&self) -> Rect {
        let Some(first) = self.lines.first().map(|line| line.bounding_box) else {
            return Rect::default();
        };
        let mut combined = first;
        for line in self.lines.iter().skip(1) {
            combined.union(line.bounding_box);
        }
        combined
    }
}

/// Tunables for `group_lines_into_paragraphs`. All distance tolerances are expressed in units of
/// the paragraph's running median tight-height, so the same values work across font sizes.
#[derive(Debug, Clone, Copy)]
pub struct ParagraphGroupingOptions {
    /// Max permitted relative tight-height jitter between a new line and the paragraph's
    /// running median tight-height. Sized to cover normal body-text variation: the DB mask is
    /// tight to the inked glyphs and skips ascenders/descenders, so an all-caps acronym like
    /// `TMJ` or an ascender-heavy line like `"At first I thought"` can be 20–40% taller than
    /// an adjacent lowercase-only line in the same paragraph. We accept up to ~40% so those
    /// false-breaks go away; real headings remain a clear step above (typically 70%+ taller
    /// than body) and continue to break correctly.
    pub height_tolerance: f32,
    /// Maximum gap_ratio for the FIRST intra-paragraph join (i.e. line 1 → line 2 of a
    /// freshly opened paragraph), before any leading baseline has been established. Sized to
    /// admit loose-leading blog/article body text (gap_ratio ≈ 2–3) but stay below typical
    /// blank-line inter-paragraph gaps (≥ 3.3 in observed real-world docs).
    pub initial_max_gap_ratio: f32,
    /// Max permitted deviation from the paragraph's running leading baseline for subsequent
    /// joins, in median-tight-height units. Once line 1 → line 2 succeeds, that gap becomes
    /// the paragraph's "leading"; later lines must stay within `baseline + leading_jitter`,
    /// catching a blank-line break as soon as the gap inflates above the running rhythm.
    pub leading_jitter: f32,
    /// Hard ceiling on gap_ratio regardless of leading baseline — a sanity stop for documents
    /// where the EMA baseline could drift upward through a sequence of accepted joins.
    pub max_gap_ratio: f32,
    /// Max negative gap (overlap) in median-tight-height units. Allows a sliver of overlap so
    /// rasterisation jitter or `sort_lines_reading_order` bucket aliasing doesn't split
    /// paragraphs. A more-negative gap (the next "line" sits well above the previous line's
    /// bottom) is the signature of column interleaving, and should break.
    pub max_overlap_ratio: f32,
    /// Maximum left-edge jitter in median-tight-height units for two lines to count as
    /// belonging to the same column. Used both for justified text (left edges align tightly)
    /// and ragged-right (same column edge, varying right edges).
    pub edge_alignment_tolerance: f32,
    /// "List-item start" gate. When the next line starts with an uppercase letter or a digit
    /// AND the inter-line gap (in median-tight-height units) exceeds this value, treat the new
    /// line as a fresh item and break. Catches menus / lists where each row starts with a
    /// capital (or a price digit) and is set with looser leading than body text. Body
    /// paragraphs with tight leading (typical gap_ratio ≤ 0.7) pass under the gate even when a
    /// sentence happens to begin at a line head; only sentences split off in body text with
    /// generous leading, which is acceptable — each sentence still translates correctly.
    pub list_item_break_gap_ratio: f32,
    /// Word-count ceiling for the list-item gate. Body prose lines that happen to start with a
    /// capitalised pronoun or article ("I", "The", "After") are 5+ words long; menu/list items
    /// are typically 1–4 words. Lines exceeding this count are not treated as list-item starts
    /// even when the other conditions are met.
    pub max_list_item_word_count: usize,
}

impl Default for ParagraphGroupingOptions {
    fn default() -> Self {
        Self {
            height_tolerance: 0.40,
            initial_max_gap_ratio: 3.5,
            leading_jitter: 0.5,
            max_gap_ratio: 4.0,
            max_overlap_ratio: 0.5,
            edge_alignment_tolerance: 0.6,
            list_item_break_gap_ratio: 0.8,
            max_list_item_word_count: 4,
        }
    }
}

/// Column-tolerance for `assign_column_ids` and for the grouper's first-line-indent gate, in
/// units of median tight-height. Anything left-edge-shift smaller than this is "same column";
/// larger is a column hop. Sized so that a typical first-line indent (~2 em ≈ 3-4 × x-height)
/// stays within one column while a multi-column page's column gap (typically ≥ 8 × x-height)
/// triggers a new column.
const COLUMN_TOLERANCE_HEIGHTS: f32 = 4.0;

/// Half-window for clustering detections into the same visual row. Two detections whose
/// tight-box centres are within this many tight-heights of each other are treated as the same
/// row by `merge_same_row_detections`.
const SAME_ROW_CY_HEIGHTS: f32 = 0.5;

/// Maximum horizontal gap (in tight-height units) between two consecutive same-row detections
/// before the pre-merge pass refuses to combine them. Tuned to reconnect typical wide-kerning
/// word spacing (≤ 5 × x-height) plus a small safety margin, while staying below the gutter
/// of a real two-column layout where line tops happen to align (typical column gap ≥ 8–10 ×
/// x-height between text-end on the left and text-start on the right). When rows are
/// vertically *offset* across columns — the much more common multi-column case — the cy-based
/// row clustering already keeps them apart and this gate doesn't even run.
const SAME_ROW_MAX_GAP_HEIGHTS: f32 = 6.0;

/// Max relative tight-height difference for two same-row detections to count as the same font
/// run. Stops two stacked-but-different-size lines (e.g. a heading sitting half on the row of
/// a small body line above) from being collapsed together.
const SAME_ROW_HEIGHT_TOLERANCE: f32 = 0.35;

/// PPOCR's `sort_lines_reading_order` interleaves columns by binning on `top`, which means a
/// two-column page comes in as A0, B0, A1, B1, … — breaking the gap and alignment heuristics
/// that paragraph grouping relies on. We cluster lines into "columns" by left edge first, then
/// sort within each column top-to-bottom. The grouper then walks each column's lines in order
/// and can rely on vertical-gap / left-alignment without column-induced jitter.
///
/// Bucket-based bucketing (e.g. `left / bucket`) doesn't work here because an indented
/// first-line at `left=50` and body lines at `left=20` straddle any fixed bucket boundary near
/// 40-50 px. So we sort by left edge and grow column IDs greedily based on inter-line gaps.
/// Tight (pre-inflate) top/bottom/left edges, used for all grouping math. The AABB in
/// `TextLine::bounding_box` is unclip-and-border-inflated for PPOCR detections, so adjacent
/// lines' AABBs often overlap vertically even when the actual ink is well-separated.
/// `tight_box.cy ± height/2` and `cx - width/2` give us the actual ink extents.
fn tight_top(line: &TextLine) -> f32 {
    line.tight_box.cy - line.tight_box.height * 0.5
}
fn tight_bottom(line: &TextLine) -> f32 {
    line.tight_box.cy + line.tight_box.height * 0.5
}
fn tight_left(line: &TextLine) -> f32 {
    line.tight_box.cx - line.tight_box.width * 0.5
}

fn reorder_lines_for_grouping(lines: Vec<TextLine>) -> Vec<TextLine> {
    if lines.len() < 2 {
        return lines;
    }
    let mut heights: Vec<f32> = lines.iter().map(|l| l.tight_box.height.max(1.0)).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_h = heights[heights.len() / 2].max(1.0);
    let tolerance = COLUMN_TOLERANCE_HEIGHTS * median_h;
    let column_ids = assign_column_ids(&lines, tolerance);
    let mut indexed: Vec<(usize, TextLine)> = column_ids.into_iter().zip(lines).collect();
    indexed.sort_by(|(id_a, a), (id_b, b)| {
        id_a.cmp(id_b).then_with(|| {
            tight_top(a)
                .partial_cmp(&tight_top(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    indexed.into_iter().map(|(_, line)| line).collect()
}

/// Assigns each line a column ID by sorting by tight-left and growing a new ID whenever the
/// gap between two consecutive tight-left edges exceeds `tolerance` pixels. The walk is in
/// sort order, so a staircase of left edges within tolerance steps still ends up as a single
/// column — this is what lets an indented first-line stay in the same column as its flush
/// body lines.
fn assign_column_ids(lines: &[TextLine], tolerance: f32) -> Vec<usize> {
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| {
        tight_left(&lines[a])
            .partial_cmp(&tight_left(&lines[b]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut ids = vec![0usize; lines.len()];
    let mut current_id = 0usize;
    let mut prev_left: Option<f32> = None;
    for &i in &order {
        let left = tight_left(&lines[i]);
        if let Some(p) = prev_left
            && (left - p) > tolerance
        {
            current_id += 1;
        }
        ids[i] = current_id;
        prev_left = Some(left);
    }
    ids
}

struct ParaState {
    lines: Vec<TextLine>,
    /// Running `min(left)` across the paragraph's lines. For both justified and ragged-right
    /// blocks, every line's left edge sits at this value (modulo first-line indent which
    /// drives the min downward when the body line arrives). This single value lets us reject
    /// new-column lines (large positive delta) while still admitting indented first lines
    /// (any negative delta, which shifts the column edge to follow the flush body).
    column_left: f32,
    /// Running estimate of the paragraph's tight-height. Updated as a simple EMA so the gate
    /// drifts with the paragraph but isn't pinned to the first line.
    median_h: f32,
    /// Paragraph-local leading baseline (intra-paragraph gap_ratio). `None` until the first
    /// join records a gap; afterwards an EMA over accepted-join gaps. Used as the centre of
    /// the gap-acceptance band so a paragraph's running line rhythm sets its own threshold,
    /// independent of document-wide leading.
    leading_gap_ratio: Option<f32>,
}

fn tight_right(line: &TextLine) -> f32 {
    line.tight_box.cx + line.tight_box.width * 0.5
}

/// Reconnect detections that PaddleOCR split into per-word boxes within a single visual row.
/// DB-based detectors sometimes break a line on wide inter-word kerning (typical for pixel /
/// retro fonts), which fragments a single sentence into many "lines" — each with the same
/// `top` but a different `left`. The grouper's column-ID stage then puts each fragment in its
/// own column, and paragraph merging never gets a chance to run.
///
/// The merge keys on the glyph-tight box: detections cluster into a row when their `cy`
/// values are within `SAME_ROW_CY_HEIGHTS` of each other (so a row tolerance shrinks/grows
/// with font size), and within a row consecutive boxes are joined when their horizontal gap is
/// at most `SAME_ROW_MAX_GAP_HEIGHTS × tight_h` and their heights agree within
/// `SAME_ROW_HEIGHT_TOLERANCE`. The gap ceiling deliberately sits above typical wide-kerning
/// gaps but well below typical menu name-vs-price column gaps, so this pass restores rows
/// without collapsing 2-column tabular layouts.
///
/// Output rows are returned in input order (the caller hasn't yet sorted by reading order);
/// row order is established later by `reorder_lines_for_grouping`.
fn merge_same_row_detections(lines: Vec<TextLine>) -> Vec<TextLine> {
    if lines.len() < 2 {
        return lines;
    }

    let mut heights: Vec<f32> = lines.iter().map(|l| l.tight_box.height.max(1.0)).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_h = heights[heights.len() / 2].max(1.0);
    let row_window = SAME_ROW_CY_HEIGHTS * median_h;

    // Assign row IDs by clustering on cy. Sort indices by cy, grow the ID whenever the cy
    // delta exceeds the row window. This is the same shape as `assign_column_ids` — it works
    // for the same reason (cluster-by-gap, not modular bucket).
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&a, &b| {
        lines[a]
            .tight_box
            .cy
            .partial_cmp(&lines[b].tight_box.cy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut row_ids = vec![0usize; lines.len()];
    let mut current_row = 0usize;
    let mut prev_cy: Option<f32> = None;
    for &i in &order {
        let cy = lines[i].tight_box.cy;
        if let Some(p) = prev_cy
            && (cy - p).abs() > row_window
        {
            current_row += 1;
        }
        row_ids[i] = current_row;
        prev_cy = Some(cy);
    }

    // Bucket lines by row.
    let row_count = current_row + 1;
    let mut rows: Vec<Vec<TextLine>> = (0..row_count).map(|_| Vec::new()).collect();
    for (line, &rid) in lines.into_iter().zip(row_ids.iter()) {
        rows[rid].push(line);
    }

    // Per row, sort by tight-left and merge adjacent boxes whose horizontal gap is small
    // enough to plausibly be intra-line word spacing.
    let mut out: Vec<TextLine> = Vec::new();
    for mut row in rows {
        row.sort_by(|a, b| {
            tight_left(a)
                .partial_cmp(&tight_left(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut accumulator: Option<TextLine> = None;
        for line in row {
            let Some(prev) = accumulator.as_ref() else {
                accumulator = Some(line);
                continue;
            };
            let prev_h = prev.tight_box.height.max(1.0);
            let unit = prev_h.max(line.tight_box.height).max(1.0);
            let big = prev_h.max(line.tight_box.height);
            let small = prev_h.min(line.tight_box.height).max(1.0);
            let height_ok = (big / small - 1.0) <= SAME_ROW_HEIGHT_TOLERANCE;
            let gap = tight_left(&line) - tight_right(prev);
            let gap_ok = gap <= SAME_ROW_MAX_GAP_HEIGHTS * unit;
            if height_ok && gap_ok {
                let merged = merge_two_lines(prev.clone(), line);
                accumulator = Some(merged);
            } else {
                out.push(accumulator.take().expect("had a value"));
                accumulator = Some(line);
            }
        }
        if let Some(line) = accumulator.take() {
            out.push(line);
        }
    }
    out
}

/// Combine two same-row TextLines into one. AABB and tight-box are unioned; angle is reset to
/// axis-aligned (the merged span is by construction left-to-right); text is concatenated with
/// a space, except when both adjacent characters are CJK (in which case nothing).
fn merge_two_lines(a: TextLine, b: TextLine) -> TextLine {
    let mut bb = a.bounding_box;
    bb.union(b.bounding_box);
    let separator = match (a.text.chars().last(), b.text.chars().next()) {
        (Some(p), Some(n)) if is_cjk_char(p) && is_cjk_char(n) => "",
        _ => " ",
    };
    let a_left = tight_left(&a);
    let a_right = tight_right(&a);
    let b_left = tight_left(&b);
    let b_right = tight_right(&b);
    let a_text_len = a.text.len();
    let mut text = a.text;
    text.push_str(separator);
    text.push_str(&b.text);
    let b_offset = (a_text_len + separator.len()) as u32;
    let mut style_ranges = a.style_ranges;
    style_ranges.extend(b.style_ranges.iter().map(|r| StyleRange {
        start: r.start + b_offset,
        end: r.end + b_offset,
        kind: r.kind,
    }));
    let new_left = a_left.min(b_left);
    let new_right = a_right.max(b_right);
    let new_cx = (new_left + new_right) * 0.5;
    let new_width = (new_right - new_left).max(1.0);
    let new_cy = (a.tight_box.cy + b.tight_box.cy) * 0.5;
    let new_height = a.tight_box.height.max(b.tight_box.height);
    let tight_box = OrientedRect {
        cx: new_cx,
        cy: new_cy,
        width: new_width,
        height: new_height,
        angle_radians: 0.0,
    };

    let mut word_rects = a.word_rects;
    word_rects.extend(b.word_rects);

    TextLine {
        text,
        bounding_box: bb,
        oriented_box: OrientedRect::axis_aligned(bb),
        tight_box,
        word_rects,
        style_ranges,
        // Merged CJK segments share one visual line; either segment's colour serves.
        ink_color: a.ink_color.or(b.ink_color),
    }
}

/// Group already-line-detected text into paragraphs.
///
/// Input is expected to be in reading order (top-to-bottom, left-to-right within row) — for the
/// PPOCR pipeline that's what `sort_lines_reading_order` produces. Each `TextLine` should carry
/// its glyph-tight `tight_box` (PPOCR's pre-unclip mask rect); engines that don't expose one
/// can pass `oriented_box` as the tight box at the cost of slightly looser gap/height checks
/// because of ascender/descender padding.
///
/// V1 grouping criteria (all must pass to merge a new line into the running paragraph):
///   * **Height match.** New line's tight-height within `±height_tolerance` of running median.
///   * **Vertical gap.** Baseline-to-baseline gap within `[-max_overlap_ratio, max_gap_ratio]`
///     median-tight-height units. Stops column-interleaved sort artifacts (large negative gap)
///     and blank-line paragraph breaks (large positive gap).
///   * **Column alignment.** New line's left edge within `edge_alignment_tolerance` to the right
///     of running `column_left`, or anywhere to the left of it (in which case `column_left`
///     shifts to follow the new line — handles first-line indent of any depth).
///
/// We don't gate on the oriented-rect angle in V1 — perspective shift and DB-mask jitter make
/// the per-line angle estimate too unstable to trust as a paragraph-break signal.
///
/// Right-justified and centered blocks aren't detected as multi-line paragraphs here; their
/// lines will fall out as one-line paragraphs (same as the pre-change behaviour). That's a
/// deliberate scope cut.
///
/// All gap/alignment math runs in the page's *reading frame*: tight-box centres are rotated
/// by the lines' (width-weighted) median angle before grouping and rotated back after, the
/// same move the vertical grouper makes with its transpose. In raw image space a tilted
/// column's left edges drift by `leading·sin(θ)` per line (~3.6 px/line at 4.5°), which blows
/// through `edge_alignment_tolerance` after a few lines and splits paragraphs spuriously —
/// and a long tilted row's `cy` drifts by `width·sin(θ)`, defeating the same-row pre-merge.
pub fn group_lines_into_paragraphs(
    lines: Vec<TextLine>,
    opts: ParagraphGroupingOptions,
) -> Vec<TextBlock> {
    let theta = frame_reading_angle(&lines);
    if theta.abs() < FRAME_ROTATION_MIN_RAD || theta.abs() > FRAME_ROTATION_MAX_RAD {
        return group_lines_in_reading_frame(lines, opts);
    }
    log::debug!(
        "ppocr paragraph grouping: rotating reading frame by {:.2}°",
        theta.to_degrees(),
    );
    let rotated = lines
        .into_iter()
        .map(|l| rotate_line_tight(l, -theta))
        .collect();
    group_lines_in_reading_frame(rotated, opts)
        .into_iter()
        .map(|block| TextBlock {
            lines: block
                .lines
                .into_iter()
                .map(|l| rotate_line_tight(l, theta))
                .collect(),
        })
        .collect()
}

/// Skip the frame rotation below this angle: the per-line left-edge drift it would correct
/// (`leading·sin(θ)`) is a fraction of a pixel, far inside every grouping tolerance.
const FRAME_ROTATION_MIN_RAD: f32 = 0.005;
/// Skip the frame rotation above this angle: past ~45° the reading axis is ambiguous (a
/// sideways page belongs to the orientation-normalization path, not a grouping rotation),
/// and a garbage median from a chaotic scene must not scramble the grouping coordinates.
const FRAME_ROTATION_MAX_RAD: f32 = std::f32::consts::FRAC_PI_4;

/// Width-weighted median of the lines' tight-box angles: long lines measure their angle far
/// more precisely than short words, and a median ignores rotated outliers (a skewed label on
/// an otherwise straight page).
fn frame_reading_angle(lines: &[TextLine]) -> f32 {
    let mut samples: Vec<(f32, f32)> = lines
        .iter()
        .map(|l| (l.tight_box.angle_radians, l.tight_box.width.max(1.0)))
        .collect();
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let total: f32 = samples.iter().map(|s| s.1).sum();
    let mut acc = 0.0;
    for (angle, weight) in &samples {
        acc += weight;
        if acc >= total * 0.5 {
            return *angle;
        }
    }
    samples.last().expect("samples is non-empty").0
}

/// Rigidly rotate a line's tight box about the image origin. Only the tight box moves: the
/// grouper's decisions read nothing else, and `merge_two_lines` unions of `bounding_box` /
/// `word_rects` stay in image space throughout, so they need no inverse transform.
fn rotate_line_tight(mut line: TextLine, angle: f32) -> TextLine {
    let (s, c) = angle.sin_cos();
    let x = line.tight_box.cx;
    let y = line.tight_box.cy;
    line.tight_box.cx = c * x - s * y;
    line.tight_box.cy = s * x + c * y;
    line.tight_box.angle_radians += angle;
    line
}

fn group_lines_in_reading_frame(
    lines: Vec<TextLine>,
    opts: ParagraphGroupingOptions,
) -> Vec<TextBlock> {
    let input_count = lines.len();
    let lines = merge_same_row_detections(lines);
    let post_merge_count = lines.len();
    if post_merge_count != input_count {
        log::info!(
            "ppocr same-row pre-merge: {} → {} lines",
            input_count,
            post_merge_count,
        );
    }
    let lines = reorder_lines_for_grouping(lines);
    let mut paragraphs: Vec<TextBlock> = Vec::new();
    let mut current: Option<ParaState> = None;

    for line in lines {
        let h = line.tight_box.height.max(1.0);
        let top = tight_top(&line);
        let left = tight_left(&line);

        let decision = match current.as_ref() {
            None => JoinDecision::OpenFirst,
            Some(state) => {
                let unit = state.median_h.max(1.0);
                let last = state
                    .lines
                    .last()
                    .expect("paragraph state always has at least one line");
                let last_bottom = tight_bottom(last);

                let big_h = h.max(state.median_h);
                let small_h = h.min(state.median_h).max(1.0);
                let height_ratio_excess = big_h / small_h - 1.0;
                let height_ok = height_ratio_excess <= opts.height_tolerance;

                let gap = top - last_bottom;
                let gap_ratio = gap / unit;
                // Dynamic upper bound: until we've seen any intra-paragraph gap we use
                // `initial_max_gap_ratio` (generous, has to admit loose blog/article body);
                // afterwards we stay within `leading_jitter` of the running leading baseline,
                // capped at `max_gap_ratio` as a global sanity ceiling.
                let gap_upper = match state.leading_gap_ratio {
                    Some(baseline) => (baseline + opts.leading_jitter).min(opts.max_gap_ratio),
                    None => opts.initial_max_gap_ratio.min(opts.max_gap_ratio),
                };
                let gap_ok = gap_ratio >= -opts.max_overlap_ratio && gap_ratio <= gap_upper;

                // Column alignment: a line at or left of the running column edge
                // confirms/establishes the flush-left column (the previous line was
                // a first-line indent sitting to the right); a line to the right
                // must fall within the edge tolerance. The old height-normalized
                // indent cap rejected legitimately deep indents, so it's gone.
                let delta = left - state.column_left;
                let align_ok = delta <= opts.edge_alignment_tolerance * unit;

                // List-item check: a new short line that starts with an uppercase letter or
                // digit, separated from the previous line by more than
                // `list_item_break_gap_ratio`, is treated as a fresh list/menu item.
                //   * Sub-details like "-espresso and filter" start with punctuation and slip
                //     past the first-character gate so they stay glued to the parent item.
                //   * Prose body lines that happen to begin with a capitalised pronoun or
                //     article ("I", "The", "After ...") are filtered out by the word-count
                //     ceiling — menu items are 1–4 words, prose lines 5+.
                let word_count = line.text.split_whitespace().count();
                let list_item_start = starts_with_break_signal(&line.text)
                    && word_count <= opts.max_list_item_word_count
                    && gap_ratio > opts.list_item_break_gap_ratio;

                if height_ok && gap_ok && align_ok && !list_item_start {
                    JoinDecision::Join
                } else {
                    JoinDecision::Break {
                        height_excess: height_ratio_excess,
                        height_limit: opts.height_tolerance,
                        gap_ratio,
                        gap_upper,
                        leading_baseline: state.leading_gap_ratio,
                        delta,
                        unit,
                        list_item_start,
                    }
                }
            }
        };

        match decision {
            JoinDecision::Join => {
                let state = current
                    .as_mut()
                    .expect("join implies a current paragraph exists");
                let unit = state.median_h.max(1.0);
                let accepted_gap_ratio =
                    (tight_top(&line) - tight_bottom(state.lines.last().unwrap())) / unit;
                state.column_left = state.column_left.min(left);
                // EMA towards each new line's tight-height. Reacts to drift over a few lines
                // without letting a single outlier dominate.
                state.median_h = state.median_h * 0.7 + h * 0.3;
                state.leading_gap_ratio = Some(match state.leading_gap_ratio {
                    Some(prev) => prev * 0.7 + accepted_gap_ratio * 0.3,
                    None => accepted_gap_ratio,
                });
                state.lines.push(line);
            }
            JoinDecision::OpenFirst | JoinDecision::Break { .. } => {
                if let JoinDecision::Break {
                    height_excess,
                    height_limit,
                    gap_ratio,
                    gap_upper,
                    leading_baseline,
                    delta,
                    unit,
                    list_item_start,
                } = decision
                {
                    log::debug!(
                        "ppocr group break: \"{}\" h={:.1} top={:.1} left={:.1} \
                         vs prev unit={:.1} → height_excess={:.2} (limit {:.2}) \
                         gap_ratio={:.2} (overlap {:.2}, upper {:.2}, baseline {:?}, \
                         list-item {:.2}) delta_left={:.1} (align {:.1}) \
                         list_item={}",
                        truncate_for_log(&line.text),
                        h,
                        top,
                        left,
                        unit,
                        height_excess,
                        height_limit,
                        gap_ratio,
                        -opts.max_overlap_ratio,
                        gap_upper,
                        leading_baseline.map(|b| (b * 100.0).round() / 100.0),
                        opts.list_item_break_gap_ratio,
                        delta,
                        opts.edge_alignment_tolerance * unit,
                        list_item_start,
                    );
                }
                if let Some(state) = current.take() {
                    paragraphs.push(TextBlock { lines: state.lines });
                }
                current = Some(ParaState {
                    lines: vec![line],
                    column_left: left,
                    median_h: h,
                    leading_gap_ratio: None,
                });
            }
        }
    }

    if let Some(state) = current.take() {
        paragraphs.push(TextBlock { lines: state.lines });
    }

    log::info!(
        "ppocr paragraph grouping: {} lines → {} paragraph(s)",
        input_count,
        paragraphs.len(),
    );

    paragraphs
}

/// Vertical (CJK top-to-bottom, columns right-to-left) counterpart of
/// [`group_lines_into_paragraphs`]. Rather than duplicating the grouping
/// heuristics with swapped axes, lines are mapped into the vertical reading
/// frame — reading direction becomes +x, column progression becomes +y — via
/// the rigid transform `(x, y) → (y, X − x)` (with `X` the lines' max right
/// edge, keeping the integer rects non-negative), grouped with the horizontal
/// grouper, and mapped back. Right-to-left column order falls out of the
/// grouper's top-to-bottom sort: the rightmost column has the smallest
/// transposed top. The same-row pre-merge also transfers: two detections that
/// split one visual column rejoin exactly like a kerning-split horizontal row.
pub fn group_vertical_lines_into_paragraphs(
    lines: Vec<TextLine>,
    opts: ParagraphGroupingOptions,
) -> Vec<TextBlock> {
    let Some(frame_x) = lines.iter().map(|l| l.bounding_box.right).max() else {
        return Vec::new();
    };
    let transposed = lines
        .into_iter()
        .map(|l| transpose_line(l, frame_x))
        .collect();
    group_lines_into_paragraphs(transposed, opts)
        .into_iter()
        .map(|block| TextBlock {
            lines: block
                .lines
                .into_iter()
                .map(|l| untranspose_line(l, frame_x))
                .collect(),
        })
        .collect()
}

fn transpose_line(line: TextLine, frame_x: u32) -> TextLine {
    TextLine {
        text: line.text,
        bounding_box: transpose_rect(line.bounding_box, frame_x),
        oriented_box: transpose_oriented(line.oriented_box, frame_x as f32),
        tight_box: transpose_oriented(line.tight_box, frame_x as f32),
        word_rects: line
            .word_rects
            .into_iter()
            .map(|r| transpose_rect(r, frame_x))
            .collect(),
        style_ranges: line.style_ranges,
        ink_color: line.ink_color,
    }
}

fn untranspose_line(line: TextLine, frame_x: u32) -> TextLine {
    TextLine {
        text: line.text,
        bounding_box: untranspose_rect(line.bounding_box, frame_x),
        oriented_box: untranspose_oriented(line.oriented_box, frame_x as f32),
        tight_box: untranspose_oriented(line.tight_box, frame_x as f32),
        word_rects: line
            .word_rects
            .into_iter()
            .map(|r| untranspose_rect(r, frame_x))
            .collect(),
        style_ranges: line.style_ranges,
        ink_color: line.ink_color,
    }
}

fn transpose_rect(rect: Rect, frame_x: u32) -> Rect {
    Rect {
        left: rect.top,
        top: frame_x.saturating_sub(rect.right),
        right: rect.bottom,
        bottom: frame_x.saturating_sub(rect.left),
    }
}

fn untranspose_rect(rect: Rect, frame_x: u32) -> Rect {
    Rect {
        left: frame_x.saturating_sub(rect.bottom),
        top: rect.left,
        right: frame_x.saturating_sub(rect.top),
        bottom: rect.right,
    }
}

/// `width`/`height` stay put — they live along/across the box's own axis,
/// which the angle rotates with the frame.
fn transpose_oriented(o: OrientedRect, frame_x: f32) -> OrientedRect {
    OrientedRect {
        cx: o.cy,
        cy: frame_x - o.cx,
        width: o.width,
        height: o.height,
        angle_radians: o.angle_radians - std::f32::consts::FRAC_PI_2,
    }
}

fn untranspose_oriented(o: OrientedRect, frame_x: f32) -> OrientedRect {
    OrientedRect {
        cx: frame_x - o.cy,
        cy: o.cx,
        width: o.width,
        height: o.height,
        angle_radians: o.angle_radians + std::f32::consts::FRAC_PI_2,
    }
}

/// Minimum long/short aspect for a detection to vote on the page's reading
/// orientation. Near-square boxes (single characters, square logos) have an
/// unstable PCA axis, so their orientation is noise — they abstain.
const READING_ORIENTATION_MIN_ASPECT: f32 = 1.5;

/// Length-weighted vote over the detections' long-axis orientations: do the
/// detected lines read vertically? Used to auto-resolve the reading order for
/// CJK pages, where the detector fuses a vertical column into one tall box
/// (see `group_vertical_lines_into_paragraphs`) and a horizontal line into a
/// wide one. Weighting by long-axis length keeps one long body column from
/// being outvoted by a few short horizontal scraps (page numbers, headers).
pub fn detected_lines_read_vertically(boxes: &[DetectedTextBox]) -> bool {
    let mut vertical = 0.0f32;
    let mut horizontal = 0.0f32;
    for b in boxes {
        let t = &b.tight_box;
        let long = t.width.max(t.height);
        let short = t.width.min(t.height).max(1.0);
        if long / short < READING_ORIENTATION_MIN_ASPECT {
            continue;
        }
        // `width` lies along the box's own angle; the long axis is vertical
        // when that axis points vertically (for the usual width ≥ height
        // case) or when the box is taller than wide along a horizontal axis.
        let axis_vertical = t.angle_radians.sin().abs() > t.angle_radians.cos().abs();
        let long_axis_vertical = if t.width >= t.height {
            axis_vertical
        } else {
            !axis_vertical
        };
        if long_axis_vertical {
            vertical += long;
        } else {
            horizontal += long;
        }
    }
    vertical > horizontal
}

/// Live camera overlays see more centered packaging/signage text than document paragraphs.
/// This keeps the conservative document grouper intact and adds a simpler visual grouping pass
/// for stacked, center-aligned labels such as product names and compact package claims.
///
/// R0-axis variant — equivalent to
/// `group_live_lines_into_blocks_in_quadrant(lines, Quadrant::R0)`. Kept
/// as a no-arg shim for callers that don't yet know about the scene
/// canonical quadrant (still-image OCR, legacy tests, doc-style pages).
pub fn group_live_lines_into_blocks(lines: Vec<TextLine>) -> Vec<TextBlock> {
    group_live_lines_into_blocks_in_quadrant(lines, crate::coords::Quadrant::R0)
}

/// Same as `group_live_lines_into_blocks` but expresses the sort key and
/// merge predicate in the **canonical reading frame**. Necessary for
/// scenes captured with the camera rotated 90° / 180° / 270°: in image
/// coords the lines stack along a non-`+y` axis, so a sort by image-y and
/// a merge predicate based on `cy ± height/2` gap give the wrong block
/// order (180° reverses, 270° re-orders) and the wrong merge decision
/// (90° puts each line in its own block because their AABB heights look
/// like wide separators rather than tight stacked rows).
///
/// Implementation: project each line's center onto the paragraph axis
/// (perpendicular to reading direction in image coords) and onto the
/// reading axis. Lines in canonical reading order have increasing
/// paragraph-axis projection.
pub fn group_live_lines_into_blocks_in_quadrant(
    lines: Vec<TextLine>,
    canonical_quadrant: crate::coords::Quadrant,
) -> Vec<TextBlock> {
    if lines.is_empty() {
        return Vec::new();
    }

    let theta = canonical_quadrant.radians();
    let sort_key = |l: &TextLine| (canonical_v(l, theta), canonical_u(l, theta));

    let mut ordered = lines;
    ordered.sort_by(|a, b| {
        let (va, ua) = sort_key(a);
        let (vb, ub) = sort_key(b);
        va.partial_cmp(&vb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| ua.partial_cmp(&ub).unwrap_or(std::cmp::Ordering::Equal))
    });

    // Union-find over all-pairs mergeability. Same connected-components
    // strategy as the R0 grouper; only the per-pair merge predicate
    // changes for non-R0 scenes.
    let n = ordered.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    // Heading-row barriers: any line ending in `!`/`?` marks a row that
    // shouldn't merge with the body below it. The threshold sits just
    // below the barrier line (v + h/2) so sibling strips on the same
    // reading line (e.g. "WORD" + "!" as separate detections) stay with
    // the heading, while strips on the next visual line are blocked.
    // Blocking direct pairs is enough: any transitive path between a
    // heading-row and a body strip must cross some pair that itself
    // straddles a barrier.
    let line_vs: Vec<f32> = ordered.iter().map(|l| canonical_v(l, theta)).collect();
    let barrier_thresholds: Vec<f32> = ordered
        .iter()
        .enumerate()
        .filter(|(_, l)| ends_with_heading_punct(&l.text))
        .map(|(i, l)| line_vs[i] + l.tight_box.height.max(1.0) * 0.5)
        .collect();
    let straddles_heading_barrier = |i: usize, j: usize| -> bool {
        let lo = line_vs[i].min(line_vs[j]);
        let hi = line_vs[i].max(line_vs[j]);
        barrier_thresholds.iter().any(|&t| lo <= t && hi > t)
    };

    for i in 0..n {
        for j in (i + 1)..n {
            if straddles_heading_barrier(i, j) {
                continue;
            }
            if live_lines_should_merge_in_quadrant(&ordered[i], &ordered[j], canonical_quadrant) {
                let ri = find(&mut parent, i);
                let rj = find(&mut parent, j);
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    let roots: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let mut ordered_opt: Vec<Option<TextLine>> = ordered.into_iter().map(Some).collect();
    let mut by_root: std::collections::HashMap<usize, Vec<TextLine>> =
        std::collections::HashMap::new();
    for i in 0..n {
        let line = ordered_opt[i].take().unwrap();
        by_root.entry(roots[i]).or_default().push(line);
    }

    let mut blocks: Vec<TextBlock> = by_root
        .into_values()
        .map(|lines| TextBlock { lines })
        .collect();
    // Order blocks by their min paragraph-axis projection, then by min
    // reading-axis projection. Same intent as the R0 sort but in
    // canonical coords.
    blocks.sort_by(|a, b| {
        let va = a
            .lines
            .iter()
            .map(|l| canonical_v(l, theta))
            .fold(f32::INFINITY, f32::min);
        let vb = b
            .lines
            .iter()
            .map(|l| canonical_v(l, theta))
            .fold(f32::INFINITY, f32::min);
        va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal)
    });
    blocks
}

/// Reading-axis projection of a line's center: `cx*cos(θ) + cy*sin(θ)`.
fn canonical_u(line: &TextLine, theta: f32) -> f32 {
    line.tight_box.cx * theta.cos() + line.tight_box.cy * theta.sin()
}

/// Paragraph-axis projection of a line's center: `-cx*sin(θ) + cy*cos(θ)`.
/// In all four quadrants, lines later in reading order have larger `v`.
fn canonical_v(line: &TextLine, theta: f32) -> f32 {
    -line.tight_box.cx * theta.sin() + line.tight_box.cy * theta.cos()
}

pub fn live_lines_should_merge(prev: &TextLine, next: &TextLine) -> bool {
    live_lines_should_merge_in_quadrant(prev, next, crate::coords::Quadrant::R0)
}

/// Same as `live_lines_should_merge` but performs all geometric checks
/// in the canonical reading frame. Gap is measured along the paragraph
/// axis (`v`); column alignment along the reading axis (`u`). Heights
/// (cross-line) and widths (reading-axis extents) come straight from
/// `tight_box` since those are already in reading-axis-aligned coords.
pub fn live_lines_should_merge_in_quadrant(
    prev: &TextLine,
    next: &TextLine,
    canonical_quadrant: crate::coords::Quadrant,
) -> bool {
    if is_live_measurement_token(prev.text.trim()) || is_live_measurement_token(next.text.trim()) {
        return false;
    }

    let prev_h = prev.tight_box.height.max(1.0);
    let next_h = next.tight_box.height.max(1.0);
    let big_h = prev_h.max(next_h);
    let small_h = prev_h.min(next_h);
    let height_ratio = big_h / small_h;

    let theta = canonical_quadrant.radians();
    let prev_u = canonical_u(prev, theta);
    let next_u = canonical_u(next, theta);
    let prev_v = canonical_v(prev, theta);
    let next_v = canonical_v(next, theta);

    // Gap along the paragraph axis. `next_v - prev_v` is signed; a
    // negative gap means the lines overlap on the paragraph axis (or
    // were passed in non-canonical order). Both directions are valid
    // for merging within tolerance.
    let canonical_gap = (next_v - prev_v).abs() - (prev_h + next_h) * 0.5;
    if canonical_gap < -big_h * 0.75 || canonical_gap > big_h * 4.25 {
        return false;
    }

    let max_w = prev.tight_box.width.max(next.tight_box.width).max(1.0);
    let min_w = prev.tight_box.width.min(next.tight_box.width).max(1.0);

    // Column alignment is measured along the *reading* axis: center
    // alignment is about how much the line midpoints differ in `u`;
    // left/right alignment is about how the (u - width/2) and
    // (u + width/2) edges line up.
    let center_aligned = (prev_u - next_u).abs() <= max_w * 0.25;
    let edge_tol = big_h * 2.0;
    let similar_width = max_w / min_w <= 1.8;
    let prev_left = prev_u - prev.tight_box.width * 0.5;
    let next_left = next_u - next.tight_box.width * 0.5;
    let prev_right = prev_u + prev.tight_box.width * 0.5;
    let next_right = next_u + next.tight_box.width * 0.5;
    let left_aligned = similar_width && (prev_left - next_left).abs() <= edge_tol;
    let right_aligned = similar_width && (prev_right - next_right).abs() <= edge_tol;
    let strongly_centered = (prev_u - next_u).abs() <= max_w * 0.12;
    let very_close = canonical_gap <= big_h * 1.25;

    let height_compatible =
        height_ratio <= 1.8 || (height_ratio <= 2.2 && strongly_centered && very_close);
    if !height_compatible {
        return false;
    }

    center_aligned || left_aligned || right_aligned
}

/// Does this line's trimmed text end in heading-style punctuation (`!` or `?`)?
/// Used to refuse merging a heading/callout with the body that follows. `.` is
/// excluded — body paragraphs end in periods, so it would split mid-paragraph.
fn ends_with_heading_punct(text: &str) -> bool {
    matches!(text.trim_end().chars().last(), Some('!') | Some('?'))
}

fn is_live_measurement_token(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }
    let normalized = text
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if normalized.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    matches!(
        normalized.as_str(),
        "mg" | "g" | "kg" | "ml" | "l" | "mcg" | "ug" | "iu" | "%"
    )
}

enum JoinDecision {
    OpenFirst,
    Join,
    Break {
        height_excess: f32,
        height_limit: f32,
        gap_ratio: f32,
        gap_upper: f32,
        leading_baseline: Option<f32>,
        delta: f32,
        unit: f32,
        list_item_start: bool,
    },
}

/// Does this line *open* what looks like a fresh list / menu item? A leading uppercase letter
/// or digit qualifies; lines starting with punctuation (e.g. "-espresso and filter") do not,
/// so sub-details still join their parent item under the gate.
fn starts_with_break_signal(text: &str) -> bool {
    let Some(first) = text.trim_start().chars().next() else {
        return false;
    };
    if first.is_ascii_digit() {
        return true;
    }
    if !first.is_alphabetic() {
        return false;
    }
    first.is_uppercase()
}

fn truncate_for_log(text: &str) -> String {
    const MAX_CHARS: usize = 40;
    let mut out = String::with_capacity(MAX_CHARS + 3);
    for (i, c) in text.chars().enumerate() {
        if i >= MAX_CHARS {
            out.push('…');
            break;
        }
        out.push(c);
    }
    out
}

pub struct RasterImage<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

pub struct RasterImageMut {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl<'a> RasterImage<'a> {
    pub fn new(rgba: &'a [u8], width: u32, height: u32) -> Result<Self, String> {
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "image dimensions overflow".to_string())?
            as usize;
        if rgba.len() != expected_len {
            return Err(format!(
                "invalid rgba size: expected {expected_len}, got {}",
                rgba.len()
            ));
        }
        Ok(Self {
            width,
            height,
            rgba,
        })
    }

    pub fn pixel_argb(&self, x: u32, y: u32) -> u32 {
        let index = ((y * self.width + x) * 4) as usize;
        u32::from_ne_bytes([
            self.rgba[index],
            self.rgba[index + 1],
            self.rgba[index + 2],
            self.rgba[index + 3],
        ])
    }
}

impl RasterImageMut {
    pub fn new(rgba: &[u8], width: u32, height: u32) -> Result<Self, String> {
        let image = RasterImage::new(rgba, width, height)?;
        Ok(Self {
            width: image.width,
            height: image.height,
            rgba: rgba.to_vec(),
        })
    }

    pub fn as_image(&self) -> RasterImage<'_> {
        RasterImage {
            width: self.width,
            height: self.height,
            rgba: &self.rgba,
        }
    }

    pub fn fill_rect(&mut self, rect: Rect, argb: u32) {
        let Some(rect) = clamp_rect(rect, self.width, self.height) else {
            return;
        };
        let bytes = argb.to_ne_bytes();
        for y in rect.top..rect.bottom {
            for x in rect.left..rect.right {
                let index = ((y * self.width + x) * 4) as usize;
                self.rgba[index..index + 4].copy_from_slice(&bytes);
            }
        }
    }

    /// Fill a rotated rectangle with a flat color. Iterates over the rect's AABB and accepts
    /// pixels whose line-local coordinates fall inside the half-extents. Falls back to
    /// `fill_rect` when the rect is effectively axis-aligned so the unrotated path stays
    /// pixel-exact.
    pub fn fill_oriented_rect(&mut self, rect: OrientedRect, argb: u32) {
        if rect.is_axis_aligned(AXIS_ALIGNED_EPSILON_RAD) {
            self.fill_rect(rect.to_aabb(), argb);
            return;
        }
        let cos = rect.angle_radians.cos();
        let sin = rect.angle_radians.sin();
        let Some(aabb) = clamp_rect(rect.to_aabb(), self.width, self.height) else {
            return;
        };
        let hw = rect.width * 0.5;
        let hh = rect.height * 0.5;
        let bytes = argb.to_ne_bytes();
        for y in aabb.top..aabb.bottom {
            for x in aabb.left..aabb.right {
                let dx = x as f32 + 0.5 - rect.cx;
                let dy = y as f32 + 0.5 - rect.cy;
                let lx = dx * cos + dy * sin;
                let ly = -dx * sin + dy * cos;
                if lx.abs() <= hw && ly.abs() <= hh {
                    let index = ((y * self.width + x) * 4) as usize;
                    self.rgba[index..index + 4].copy_from_slice(&bytes);
                }
            }
        }
    }
}

/// Within ~0.057° of horizontal we treat the rect as axis-aligned and use the pixel-exact
/// `fill_rect` path; below this threshold the rotated rasterization's sub-pixel error is
/// larger than the rotation itself.
const AXIS_ALIGNED_EPSILON_RAD: f32 = 0.001;

pub fn channel_r(color: u32) -> u8 {
    ((color >> 16) & 0xFF) as u8
}

pub fn channel_g(color: u32) -> u8 {
    ((color >> 8) & 0xFF) as u8
}

pub fn channel_b(color: u32) -> u8 {
    (color & 0xFF) as u8
}

pub fn argb(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

pub fn clamp_rect(rect: Rect, width: u32, height: u32) -> Option<Rect> {
    if width == 0 || height == 0 {
        return None;
    }
    let left = rect.left.min(width - 1);
    let top = rect.top.min(height - 1);
    let right = rect.right.clamp(left + 1, width);
    let bottom = rect.bottom.clamp(top + 1, height);
    let clamped = Rect {
        left,
        top,
        right,
        bottom,
    };
    if clamped.is_empty() {
        None
    } else {
        Some(clamped)
    }
}

pub fn luminance(color: u32) -> f32 {
    let r = channel_r(color) as f32 / 255.0;
    let g = channel_g(color) as f32 / 255.0;
    let b = channel_b(color) as f32 / 255.0;
    0.299 * r + 0.587 * g + 0.114 * b
}

fn luminance_u8(color: u32) -> u8 {
    let r = channel_r(color) as u32;
    let g = channel_g(color) as u32;
    let b = channel_b(color) as u32;
    ((77 * r + 150 * g + 29 * b) >> 8).min(255) as u8
}

fn get_surrounding_average_color(image: &RasterImage<'_>, text_bounds: Rect) -> u32 {
    let margin = 4;
    let sample_regions = [
        Rect {
            left: text_bounds.left.saturating_sub(margin),
            top: text_bounds.top,
            right: text_bounds.left,
            bottom: text_bounds.bottom,
        },
        Rect {
            left: text_bounds.right,
            top: text_bounds.top,
            right: (text_bounds.right + margin).min(image.width),
            bottom: text_bounds.bottom,
        },
        Rect {
            left: text_bounds.left,
            top: text_bounds.top.saturating_sub(margin),
            right: text_bounds.right,
            bottom: text_bounds.top,
        },
        Rect {
            left: text_bounds.left,
            top: text_bounds.bottom,
            right: text_bounds.right,
            bottom: (text_bounds.bottom + margin).min(image.height),
        },
    ];

    let mut total_r = 0u64;
    let mut total_g = 0u64;
    let mut total_b = 0u64;
    let mut count = 0u64;

    for region in sample_regions {
        let Some(region) = clamp_rect(region, image.width, image.height) else {
            continue;
        };
        for y in region.top..region.bottom {
            for x in region.left..region.right {
                let pixel = image.pixel_argb(x, y);
                total_r += channel_r(pixel) as u64;
                total_g += channel_g(pixel) as u64;
                total_b += channel_b(pixel) as u64;
                count += 1;
            }
        }
    }

    if count == 0 {
        argb(255, 255, 255)
    } else {
        argb(
            (total_r / count) as u8,
            (total_g / count) as u8,
            (total_b / count) as u8,
        )
    }
}

fn otsu_threshold(histogram: &[u64; 256]) -> u8 {
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return 127;
    }
    let total_sum: f64 = histogram
        .iter()
        .enumerate()
        .map(|(i, &c)| i as f64 * c as f64)
        .sum();

    let mut w_bg = 0u64;
    let mut sum_bg = 0f64;
    let mut best_variance = -1f64;
    let mut best_threshold = 127u8;
    for t in 0..256 {
        w_bg += histogram[t];
        if w_bg == 0 {
            continue;
        }
        let w_fg = total - w_bg;
        if w_fg == 0 {
            break;
        }
        sum_bg += t as f64 * histogram[t] as f64;
        let mean_bg = sum_bg / w_bg as f64;
        let mean_fg = (total_sum - sum_bg) / w_fg as f64;
        let variance = w_bg as f64 * w_fg as f64 * (mean_bg - mean_fg).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = t as u8;
        }
    }
    best_threshold
}

fn mean_color(r: u64, g: u64, b: u64, n: u64) -> Option<u32> {
    if n == 0 {
        None
    } else {
        Some(argb((r / n) as u8, (g / n) as u8, (b / n) as u8))
    }
}

fn average_corner_color(corners: [u32; 4]) -> u32 {
    let mut r = 0u32;
    let mut g = 0u32;
    let mut b = 0u32;
    for c in corners {
        r += channel_r(c) as u32;
        g += channel_g(c) as u32;
        b += channel_b(c) as u32;
    }
    argb((r / 4) as u8, (g / 4) as u8, (b / 4) as u8)
}

fn peak_luminance(histogram: &[u64; 256], low: usize, high_exclusive: usize) -> u8 {
    let mut best_count = 0u64;
    let mut best = ((low + high_exclusive.saturating_sub(1)) / 2) as u8;
    for i in low..high_exclusive {
        if histogram[i] > best_count {
            best_count = histogram[i];
            best = i as u8;
        }
    }
    best
}

fn autodetect_paint(image: &RasterImage<'_>, bounds: Rect) -> OverlayColors {
    let fallback = |bg: u32| {
        let fg = if luminance(bg) > 0.5 {
            argb(0, 0, 0)
        } else {
            argb(255, 255, 255)
        };
        OverlayColors {
            background_argb: bg,
            foreground_argb: fg,
        }
    };

    let Some(bounds) = clamp_rect(bounds, image.width, image.height) else {
        return fallback(argb(255, 255, 255));
    };
    if bounds.width() < 2 || bounds.height() < 2 {
        return fallback(get_surrounding_average_color(image, bounds));
    }

    let mut histogram = [0u64; 256];
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            let pixel = image.pixel_argb(x, y);
            histogram[luminance_u8(pixel) as usize] += 1;
        }
    }
    let threshold = otsu_threshold(&histogram);

    let surround = get_surrounding_average_color(image, bounds);
    let bg_is_bright = luminance_u8(surround) > threshold;

    // BG = pixels within a narrow band around the paper's modal luminance.
    // Rejects specular highlights (too bright) and anti-aliased stroke
    // transitions (near threshold).
    let (bg_lo, bg_hi) = if bg_is_bright {
        (threshold as usize + 1, 256)
    } else {
        (0, threshold as usize + 1)
    };
    let paper_peak = peak_luminance(&histogram, bg_lo, bg_hi);
    const BG_BAND: u8 = 12;

    // FG = extreme 5% of fg-cluster by luminance. This captures core-ink
    // even on low-contrast lines where most fg pixels are anti-aliased
    // greys (the peak of the fg cluster can sit in the anti-aliased band).
    let fg_total_count: u64 = if bg_is_bright {
        histogram[0..=threshold as usize].iter().sum()
    } else {
        histogram[threshold as usize + 1..256].iter().sum()
    };
    let fg_percentile_target = (fg_total_count / 20).max(1);
    let fg_cutoff: u8 = if bg_is_bright {
        let mut cumulative = 0u64;
        let mut cutoff = threshold;
        for i in 0..=threshold as usize {
            cumulative += histogram[i];
            if cumulative >= fg_percentile_target {
                cutoff = i as u8;
                break;
            }
        }
        cutoff
    } else {
        let mut cumulative = 0u64;
        let mut cutoff = threshold.saturating_add(1);
        for i in (threshold as usize + 1..256).rev() {
            cumulative += histogram[i];
            if cumulative >= fg_percentile_target {
                cutoff = i as u8;
                break;
            }
        }
        cutoff
    };

    #[derive(Default, Clone, Copy)]
    struct Accum {
        r: u64,
        g: u64,
        b: u64,
        n: u64,
    }
    let mut bg_quad = [Accum::default(); 4];
    let mut fg_total = Accum::default();

    // FG is sampled from the interior — that's where the ink lives.
    for y in bounds.top..bounds.bottom {
        for x in bounds.left..bounds.right {
            let pixel = image.pixel_argb(x, y);
            let lum = luminance_u8(pixel);
            let is_core_fg = if bg_is_bright {
                lum <= fg_cutoff
            } else {
                lum >= fg_cutoff
            };
            if is_core_fg {
                fg_total.r += channel_r(pixel) as u64;
                fg_total.g += channel_g(pixel) as u64;
                fg_total.b += channel_b(pixel) as u64;
                fg_total.n += 1;
            }
        }
    }

    // BG = per-corner sample from a small L-shaped window of paper immediately
    // around each corner. Keeping each sample corner-local means the fill
    // matches the paper at that exact edge — important when the page has
    // horizontal gradients or lighting bands that vary across the text column.
    const SURROUND_MARGIN: u32 = 6;
    const CORNER_EXTENT: u32 = 8;
    let add_bg = |bg_quad: &mut [Accum; 4], pixel: u32, qi: usize| {
        let lum = luminance_u8(pixel);
        if lum.abs_diff(paper_peak) > BG_BAND {
            return;
        }
        let a = &mut bg_quad[qi];
        a.r += channel_r(pixel) as u64;
        a.g += channel_g(pixel) as u64;
        a.b += channel_b(pixel) as u64;
        a.n += 1;
    };
    let windows: [(u32, u32, u32, u32); 4] = [
        // tl: x in [left-margin, left+extent], y in [top-margin, top+extent]
        (
            bounds.left.saturating_sub(SURROUND_MARGIN),
            (bounds.left + CORNER_EXTENT).min(bounds.right),
            bounds.top.saturating_sub(SURROUND_MARGIN),
            (bounds.top + CORNER_EXTENT).min(bounds.bottom),
        ),
        // tr
        (
            bounds.right.saturating_sub(CORNER_EXTENT).max(bounds.left),
            (bounds.right + SURROUND_MARGIN).min(image.width),
            bounds.top.saturating_sub(SURROUND_MARGIN),
            (bounds.top + CORNER_EXTENT).min(bounds.bottom),
        ),
        // bl
        (
            bounds.left.saturating_sub(SURROUND_MARGIN),
            (bounds.left + CORNER_EXTENT).min(bounds.right),
            bounds.bottom.saturating_sub(CORNER_EXTENT).max(bounds.top),
            (bounds.bottom + SURROUND_MARGIN).min(image.height),
        ),
        // br
        (
            bounds.right.saturating_sub(CORNER_EXTENT).max(bounds.left),
            (bounds.right + SURROUND_MARGIN).min(image.width),
            bounds.bottom.saturating_sub(CORNER_EXTENT).max(bounds.top),
            (bounds.bottom + SURROUND_MARGIN).min(image.height),
        ),
    ];

    for (qi, (x0, x1, y0, y1)) in windows.iter().copied().enumerate() {
        for y in y0..y1 {
            for x in x0..x1 {
                // Skip the text-rect interior — those pixels carry ink.
                if x >= bounds.left && x < bounds.right && y >= bounds.top && y < bounds.bottom {
                    continue;
                }
                add_bg(&mut bg_quad, image.pixel_argb(x, y), qi);
            }
        }
    }

    let quad_color = |q: &Accum| mean_color(q.r, q.g, q.b, q.n).unwrap_or(surround);
    let corners = [
        quad_color(&bg_quad[0]),
        quad_color(&bg_quad[1]),
        quad_color(&bg_quad[2]),
        quad_color(&bg_quad[3]),
    ];
    let foreground_argb = mean_color(fg_total.r, fg_total.g, fg_total.b, fg_total.n).unwrap_or({
        if bg_is_bright {
            argb(0, 0, 0)
        } else {
            argb(255, 255, 255)
        }
    });

    OverlayColors {
        background_argb: average_corner_color(corners),
        foreground_argb,
    }
}

fn get_overlay_colors(
    image: &RasterImage<'_>,
    bounds: Rect,
    background_mode: crate::BackgroundMode,
) -> OverlayColors {
    match background_mode {
        crate::BackgroundMode::WhiteOnBlack => OverlayColors {
            background_argb: argb(0, 0, 0),
            foreground_argb: argb(255, 255, 255),
        },
        crate::BackgroundMode::BlackOnWhite => OverlayColors {
            background_argb: argb(255, 255, 255),
            foreground_argb: argb(0, 0, 0),
        },
        crate::BackgroundMode::AutoDetect => autodetect_paint(image, bounds),
    }
}

pub fn sample_overlay_colors(
    rgba_bytes: &[u8],
    width: u32,
    height: u32,
    bounds: Rect,
    background_mode: crate::BackgroundMode,
    _word_rects: Option<&[Rect]>,
) -> Result<OverlayColors, String> {
    let image = RasterImage::new(rgba_bytes, width, height)?;
    Ok(get_overlay_colors(&image, bounds, background_mode))
}

fn merge_hyphenated_words(words: Vec<WordInfo>) -> Vec<WordInfo> {
    if words.is_empty() {
        return words;
    }

    let mut result = Vec::new();
    let mut index = 0;

    while index < words.len() {
        let current_word = &words[index];
        if index == words.len() - 1 {
            result.push(current_word.clone());
            break;
        }

        if !current_word.is_last_in_line || !current_word.text.ends_with('-') {
            result.push(current_word.clone());
            index += 1;
            continue;
        }

        let next_word = &words[index + 1];
        let poor_mans_first_in_line = next_word.bounding_box.left < current_word.bounding_box.left
            && next_word.bounding_box.top > current_word.bounding_box.top;
        if !next_word.is_first_in_line && !poor_mans_first_in_line {
            result.push(current_word.clone());
            index += 1;
            continue;
        }

        let merged_text = format!(
            "{}{}",
            current_word.text.trim_end_matches('-'),
            next_word.text
        );
        let mut ghost_bbox = current_word.bounding_box;
        ghost_bbox.right += next_word.bounding_box.width();

        result.push(WordInfo {
            text: merged_text,
            confidence: current_word.confidence.min(next_word.confidence),
            bounding_box: current_word.bounding_box,
            ghost_bbox: Some(ghost_bbox),
            is_first_in_line: current_word.is_first_in_line,
            is_last_in_line: true,
            is_last_in_para: next_word.is_last_in_para,
        });

        if index + 2 >= words.len() {
            index += 2;
            continue;
        }

        let next_after_merged = &words[index + 2];
        let mut expanded_bbox = next_word.bounding_box;
        expanded_bbox.union(next_after_merged.bounding_box);
        result.push(WordInfo {
            bounding_box: expanded_bbox,
            is_first_in_line: true,
            ..next_after_merged.clone()
        });
        index += 3;
    }

    result
}

pub fn build_text_blocks(
    detected_words: &[DetectedWord],
    min_confidence: u32,
    join_without_spaces: bool,
    relax_single_char_confidence: bool,
) -> Vec<TextBlock> {
    let effective_min_confidence = if relax_single_char_confidence {
        (min_confidence.min(60)) as f32
    } else {
        min_confidence as f32
    };

    let all_words = detected_words
        .iter()
        .map(|word| WordInfo {
            text: word.text.clone(),
            confidence: word.confidence,
            bounding_box: word.bounding_box,
            ghost_bbox: None,
            is_first_in_line: word.is_at_beginning_of_para,
            is_last_in_line: word.end_line,
            is_last_in_para: word.end_para,
        })
        .collect::<Vec<_>>();

    let mut filtered_words = Vec::new();
    let mut pending_first_in_line = false;
    for (index, word) in all_words.iter().enumerate() {
        let should_include = word.confidence >= effective_min_confidence
            && (relax_single_char_confidence
                || !(word.text.chars().count() == 1
                    && word.confidence < (effective_min_confidence + 5.0).min(100.0)));

        if should_include {
            filtered_words.push(WordInfo {
                is_first_in_line: word.is_first_in_line || pending_first_in_line,
                ..word.clone()
            });
            pending_first_in_line = false;
        } else {
            if word.is_first_in_line {
                pending_first_in_line = true;
            }
            if word.is_last_in_line && index > 0 {
                if let Some(previous) = filtered_words.last_mut() {
                    previous.is_last_in_line = true;
                }
            }
            if word.is_last_in_para && index > 0 {
                if let Some(previous) = filtered_words.last_mut() {
                    previous.is_last_in_para = true;
                }
            }
        }
    }

    let filtered_words = merge_hyphenated_words(filtered_words);
    let mut blocks = Vec::new();
    let mut lines = Vec::new();
    let mut current_line: Option<TextLine> = None;
    let mut last_right = 0u32;

    for word in filtered_words {
        if word.text.trim().is_empty() {
            continue;
        }

        let real_bbox = word.ghost_bbox.unwrap_or(word.bounding_box);
        let skipped_first_word = current_line
            .as_ref()
            .is_some_and(|line| word.bounding_box.right < line.bounding_box.left);
        let first_word_in_line = word.is_first_in_line || skipped_first_word;
        let last_word_in_line = word.is_last_in_line;
        let last_word_in_para = word.is_last_in_para;

        if first_word_in_line || current_line.is_none() {
            current_line = Some(TextLine {
                text: word.text.clone(),
                bounding_box: word.bounding_box,
                oriented_box: OrientedRect::axis_aligned(word.bounding_box),
                tight_box: OrientedRect::axis_aligned(word.bounding_box),
                word_rects: vec![word.bounding_box],
                style_ranges: Vec::new(),
                ink_color: None,
            });
        } else if let Some(line) = current_line.as_mut() {
            let delta = word.bounding_box.left.saturating_sub(last_right);
            let char_width = real_bbox.width() as f32 / word.text.chars().count().max(1) as f32;
            let delta_in_chars = if char_width > 0.0 {
                delta as f32 / char_width
            } else {
                0.0
            };

            if delta_in_chars >= 3.0 {
                lines.push(line.clone());
                *line = TextLine {
                    text: word.text.clone(),
                    bounding_box: word.bounding_box,
                    oriented_box: OrientedRect::axis_aligned(word.bounding_box),
                    tight_box: OrientedRect::axis_aligned(word.bounding_box),
                    word_rects: vec![word.bounding_box],
                    style_ranges: Vec::new(),
                    ink_color: None,
                };
                if !lines.is_empty() {
                    blocks.push(TextBlock {
                        lines: std::mem::take(&mut lines),
                    });
                }
            } else {
                if join_without_spaces || line.text.is_empty() {
                    line.text.push_str(&word.text);
                } else {
                    line.text.push(' ');
                    line.text.push_str(&word.text);
                }
                line.word_rects.push(word.bounding_box);
                line.bounding_box.union(word.bounding_box);
                // Tesseract path: words are axis-aligned, so the line's oriented box is just
                // the AABB lifted. PPOCR detects whole lines as one "word" and never reaches
                // this merge branch.
                line.oriented_box = OrientedRect::axis_aligned(line.bounding_box);
                line.tight_box = OrientedRect::axis_aligned(line.bounding_box);
            }
        }

        if last_word_in_line {
            if let Some(line) = current_line.take() {
                if !line.text.trim().is_empty() {
                    lines.push(line);
                }
            }
        }

        if last_word_in_para && !lines.is_empty() {
            blocks.push(TextBlock {
                lines: std::mem::take(&mut lines),
            });
        }

        last_right = word.bounding_box.right;
    }

    if let Some(line) = current_line.take() {
        if !line.text.trim().is_empty() {
            lines.push(line);
        }
    }
    if !lines.is_empty() {
        blocks.push(TextBlock { lines });
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::{
        DetectedWord, LineDecoration, OrientedRect, ParagraphGroupingOptions, Rect, StyleKind,
        StyleRange, TextBlock, TextLine, build_text_blocks, group_lines_into_paragraphs,
        group_live_lines_into_blocks, group_vertical_lines_into_paragraphs,
        style_spans_from_styles,
    };

    #[test]
    fn order_words_visually_sorts_top_down_then_reading_order() {
        use super::{OrientedRect, PositionedWord, order_words_visually};
        let mk = |text: &str, cx: f32, cy: f32, line: u32| PositionedWord {
            text: text.to_string(),
            bounds: OrientedRect {
                cx,
                cy,
                width: 10.0,
                height: 10.0,
                angle_radians: 0.0,
            },
            line_index: line,
        };
        // Jumbled: bottom line's second word, top line's word, bottom line's first word.
        let input = vec![
            mk("b", 30.0, 100.0, 1),
            mk("top", 10.0, 10.0, 0),
            mk("a", 10.0, 100.0, 1),
        ];
        let out = order_words_visually(input, &std::collections::HashSet::new());
        let texts: Vec<&str> = out.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["top", "a", "b"]);
    }

    #[test]
    fn order_words_visually_reverses_rtl_lines() {
        use super::{OrientedRect, PositionedWord, order_words_visually};
        let mk = |text: &str, cx: f32, line: u32| PositionedWord {
            text: text.to_string(),
            bounds: OrientedRect {
                cx,
                cy: 10.0,
                width: 10.0,
                height: 10.0,
                angle_radians: 0.0,
            },
            line_index: line,
        };
        // Line 0 is RTL: logical-first word sits rightmost on screen. Screen order (l→r) is
        // "من نظام هل"; logical order (the join order we want) is "هل نظام من".
        let input = vec![mk("من", 10.0, 0), mk("نظام", 30.0, 0), mk("هل", 50.0, 0)];
        let rtl: std::collections::HashSet<u32> = [0u32].into_iter().collect();
        let out = order_words_visually(input, &rtl);
        let texts: Vec<&str> = out.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["هل", "نظام", "من"]);
    }

    #[test]
    fn order_words_visually_keeps_embedded_ltr_run_forward() {
        use super::{OrientedRect, PositionedWord, order_words_visually};
        let mk = |text: &str, cx: f32, line: u32| PositionedWord {
            text: text.to_string(),
            bounds: OrientedRect {
                cx,
                cy: 10.0,
                width: 10.0,
                height: 10.0,
                angle_radians: 0.0,
            },
            line_index: line,
        };
        // RTL line "متجر أو Play Services دون" — on screen (l→r): متجر أو Play Services دون.
        // Logical order reverses the RTL words but keeps "Play Services" forward.
        let input = vec![
            mk("متجر", 10.0, 0),
            mk("أو", 30.0, 0),
            mk("Play", 50.0, 0),
            mk("Services", 70.0, 0),
            mk("دون", 90.0, 0),
        ];
        let rtl: std::collections::HashSet<u32> = [0u32].into_iter().collect();
        let out = order_words_visually(input, &rtl);
        let texts: Vec<&str> = out.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, vec!["دون", "Play", "Services", "أو", "متجر"]);
    }

    #[test]
    fn subspan_horizontal_carves_a_sub_rect() {
        // 100-wide box centred at x=50 (left edge x=0), height 10 at y=20.
        let line = OrientedRect {
            cx: 50.0,
            cy: 20.0,
            width: 100.0,
            height: 10.0,
            angle_radians: 0.0,
        };
        let w = line.subspan(10.0, 30.0);
        assert!((w.cx - 20.0).abs() < 1e-3); // centre of [10,30]
        assert!((w.cy - 20.0).abs() < 1e-3);
        assert!((w.width - 20.0).abs() < 1e-3);
        assert!((w.height - 10.0).abs() < 1e-3);
        assert_eq!(w.angle_radians, 0.0);
    }

    #[test]
    fn subspan_vertical_carves_along_reading_axis() {
        // 90°-rotated line: reading axis runs down +y. Left edge at the box top.
        let line = OrientedRect {
            cx: 50.0,
            cy: 50.0,
            width: 100.0,
            height: 8.0,
            angle_radians: std::f32::consts::FRAC_PI_2,
        };
        // A unit at reading-offset [0,20] sits near the top; centre at offset 10.
        let w = line.subspan(0.0, 20.0);
        // Reading axis is +y, so the sub-rect advances in y, not x.
        assert!((w.cx - 50.0).abs() < 1e-3);
        assert!((w.cy - (0.0 + 10.0)).abs() < 1e-3); // top edge (cy-half_w=0) + 10
        assert!((w.width - 20.0).abs() < 1e-3);
    }

    /// A vertical text column the way the ppocr still path delivers it: AABB
    /// around the column, tight box with `width` along the (vertical) reading
    /// axis, `height` the cross-axis ink thickness, angle 90°.
    fn vline(text: &str, left: u32, top: u32, right: u32, bottom: u32, tight_w: f32) -> TextLine {
        let rect = Rect {
            left,
            top,
            right,
            bottom,
        };
        let tight = OrientedRect {
            cx: (left + right) as f32 * 0.5,
            cy: (top + bottom) as f32 * 0.5,
            width: (bottom - top) as f32,
            height: tight_w,
            angle_radians: std::f32::consts::FRAC_PI_2,
        };
        TextLine {
            text: text.to_string(),
            bounding_box: rect,
            oriented_box: OrientedRect {
                width: (bottom - top) as f32,
                height: (right - left) as f32,
                ..tight
            },
            tight_box: tight,
            word_rects: vec![rect],
            style_ranges: Vec::new(),
            ink_color: None,
        }
    }

    fn line(text: &str, left: u32, top: u32, right: u32, bottom: u32, tight_h: f32) -> TextLine {
        let rect = Rect {
            left,
            top,
            right,
            bottom,
        };
        let cx = (left + right) as f32 * 0.5;
        let cy = (top + bottom) as f32 * 0.5;
        let width = (right - left) as f32;
        let tight = OrientedRect {
            cx,
            cy,
            width,
            height: tight_h,
            angle_radians: 0.0,
        };
        TextLine {
            text: text.to_string(),
            bounding_box: rect,
            oriented_box: OrientedRect::axis_aligned(rect),
            tight_box: tight,
            word_rects: vec![rect],
            style_ranges: Vec::new(),
            ink_color: None,
        }
    }

    fn word(
        text: &str,
        left: u32,
        top: u32,
        right: u32,
        bottom: u32,
        is_first_in_line: bool,
        is_last_in_line: bool,
        is_last_in_para: bool,
    ) -> DetectedWord {
        DetectedWord {
            text: text.to_string(),
            confidence: 95.0,
            bounding_box: Rect {
                left,
                top,
                right,
                bottom,
            },
            is_at_beginning_of_para: is_first_in_line,
            end_line: is_last_in_line,
            end_para: is_last_in_para,
        }
    }

    #[test]
    fn translation_text_flattens_wrapped_lines_into_one_paragraph() {
        let detected_words = vec![
            word("relax", 52, 129, 103, 145, true, false, false),
            word("slightly", 115, 129, 192, 150, false, false, false),
            word("as", 202, 134, 224, 144, false, false, false),
            word("the", 235, 129, 267, 145, false, false, false),
            word("reprieve", 279, 133, 365, 150, false, false, false),
            word("of", 376, 128, 395, 144, false, false, false),
            word("warmth", 404, 127, 486, 144, false, false, false),
            word("began", 498, 127, 560, 148, false, false, false),
            word("to", 571, 129, 590, 143, false, false, false),
            word("press", 601, 131, 657, 148, false, false, false),
            word("against", 668, 128, 744, 147, false, true, false),
            word("his", 51, 158, 80, 174, true, false, false),
            word("frozen", 90, 159, 155, 174, false, false, false),
            word("cheeks.", 164, 158, 243, 174, false, true, true),
        ];

        let blocks = build_text_blocks(&detected_words, 75, false, false);
        assert_eq!(blocks.len(), 1);
        assert_eq!(
            blocks[0].source_text(),
            "relax slightly as the reprieve of warmth began to press against\nhis frozen cheeks."
        );
        assert_eq!(
            blocks[0].translation_text(),
            "relax slightly as the reprieve of warmth began to press against his frozen cheeks."
        );
    }

    #[test]
    fn grouping_single_paragraph_with_similar_heights_and_normal_gap() {
        // Three lines, x-height 10, baseline-gap 4 (line height ~14 ⇒ gap_ratio ~ 0.4).
        // Same column. Should collapse to one paragraph.
        let lines = vec![
            line("first body line", 20, 10, 200, 24, 10.0),
            line("second body line", 20, 28, 200, 42, 10.0),
            line("third body line", 20, 46, 180, 60, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_breaks_on_blank_line() {
        // Big vertical gap between line 1 and line 2 — gap is ~3.5 × tight_h, well past 1.8.
        let lines = vec![
            line("paragraph one", 20, 10, 200, 24, 10.0),
            line("paragraph two", 20, 70, 200, 84, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[1].lines.len(), 1);
    }

    #[test]
    fn vertical_grouping_merges_columns_right_to_left() {
        // Mirrors files/japanese-vertical.png: three columns of one sentence,
        // detector order is arbitrary, reading order is rightmost-first.
        let lines = vec![
            vline("複数の列に", 34, 0, 78, 169, 14.0),
            vline("この日本語の文章は", 70, 0, 113, 272, 14.0),
            vline("分かれています", 0, 1, 43, 216, 14.0),
        ];
        let blocks = group_vertical_lines_into_paragraphs(lines, Default::default());
        assert_eq!(blocks.len(), 1);
        let texts: Vec<&str> = blocks[0].lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(
            texts,
            ["この日本語の文章は", "複数の列に", "分かれています"]
        );
        assert_eq!(
            blocks[0].translation_text(),
            "この日本語の文章は複数の列に分かれています"
        );
        // Geometry survives the reading-frame round trip.
        assert_eq!(
            blocks[0].lines[0].bounding_box,
            Rect {
                left: 70,
                top: 0,
                right: 113,
                bottom: 272,
            }
        );
        assert_eq!(blocks[0].lines[0].tight_box.cx, 91.5);
        assert_eq!(blocks[0].lines[0].tight_box.width, 272.0);
    }

    fn det_box(
        cx: f32,
        cy: f32,
        along: f32,
        across: f32,
        angle_deg: f32,
    ) -> super::DetectedTextBox {
        let tight = OrientedRect {
            cx,
            cy,
            width: along,
            height: across,
            angle_radians: angle_deg.to_radians(),
        };
        super::DetectedTextBox {
            rect: tight.to_aabb(),
            oriented_box: tight,
            tight_box: tight,
            contour: Vec::new(),
            score: 0.9,
        }
    }

    #[test]
    fn reading_orientation_vote_detects_vertical_columns() {
        let boxes = vec![
            det_box(91.5, 136.0, 243.0, 14.0, 90.0),
            det_box(56.0, 84.5, 140.0, 15.0, 90.0),
            det_box(21.5, 108.5, 186.0, 14.0, 90.0),
        ];
        assert!(super::detected_lines_read_vertically(&boxes));
    }

    #[test]
    fn reading_orientation_vote_detects_horizontal_lines() {
        let boxes = vec![
            det_box(100.0, 20.0, 180.0, 12.0, 0.0),
            det_box(100.0, 40.0, 180.0, 12.0, -1.5),
            det_box(100.0, 60.0, 90.0, 12.0, 1.0),
        ];
        assert!(!super::detected_lines_read_vertically(&boxes));
    }

    #[test]
    fn reading_orientation_vote_long_column_outweighs_short_scraps() {
        // One long vertical body column vs two short horizontal scraps
        // (header + page number): the length weighting keeps the page
        // vertical even though horizontal boxes are the majority.
        let boxes = vec![
            det_box(50.0, 150.0, 280.0, 14.0, 90.0),
            det_box(40.0, 10.0, 60.0, 12.0, 0.0),
            det_box(90.0, 290.0, 30.0, 12.0, 0.0),
        ];
        assert!(super::detected_lines_read_vertically(&boxes));
    }

    #[test]
    fn reading_orientation_vote_square_boxes_abstain() {
        // Near-square single-character detections have a noise PCA axis and
        // must not flip the page vertical.
        let boxes = vec![
            det_box(20.0, 20.0, 16.0, 14.0, 90.0),
            det_box(60.0, 20.0, 15.0, 14.0, 88.0),
            det_box(100.0, 20.0, 120.0, 12.0, 0.0),
        ];
        assert!(!super::detected_lines_read_vertically(&boxes));
    }

    #[test]
    fn vertical_grouping_breaks_on_wide_column_gap() {
        // Two adjacent columns plus a third far to the left (gap ≈ 9 ×
        // column thickness) — the far column is a separate block.
        let lines = vec![
            vline("右の段", 200, 0, 230, 200, 14.0),
            vline("続きの段", 160, 0, 190, 200, 14.0),
            vline("別の段", 20, 0, 50, 200, 14.0),
        ];
        let blocks = group_vertical_lines_into_paragraphs(lines, Default::default());
        assert_eq!(blocks.len(), 2);
        let texts: Vec<&str> = blocks[0].lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["右の段", "続きの段"]);
        assert_eq!(blocks[1].lines[0].text, "別の段");
    }

    #[test]
    fn grouping_breaks_on_heading_size_change() {
        // The big-font line should not glue to the body line beneath it.
        let lines = vec![
            line("Big Heading", 20, 10, 220, 40, 26.0),
            line("body line one", 20, 48, 200, 62, 10.0),
            line("body line two", 20, 66, 200, 80, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[1].lines.len(), 2);
    }

    #[test]
    fn grouping_keeps_first_line_indent_in_paragraph() {
        // First line indented by ~3× tight_h; body lines flush at left=20.
        let lines = vec![
            line("Indented opener", 50, 10, 200, 24, 10.0),
            line("flush body line", 20, 28, 200, 42, 10.0),
            line("more flush body", 20, 46, 180, 60, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_breaks_at_column_change() {
        // Reading-order sort interleaves two columns, alternating: A0, B0, A1, B1.
        // The grouper should split them by left-edge mismatch / large overlap.
        // Column gap is sized at ~20 × tight_h to stay above the same-row pre-merge gate
        // (real two-column page gutters comfortably exceed that).
        let lines = vec![
            line("col A line 1", 20, 10, 180, 24, 10.0),
            line("col B line 1", 400, 10, 560, 24, 10.0),
            line("col A line 2", 20, 28, 180, 42, 10.0),
            line("col B line 2", 400, 28, 560, 42, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lines.len(), 2);
        assert_eq!(blocks[1].lines.len(), 2);
        assert_eq!(blocks[0].lines[0].text, "col A line 1");
        assert_eq!(blocks[0].lines[1].text, "col A line 2");
        assert_eq!(blocks[1].lines[0].text, "col B line 1");
        assert_eq!(blocks[1].lines[1].text, "col B line 2");
    }

    #[test]
    fn grouping_does_not_split_paragraph_just_because_last_line_is_short() {
        let lines = vec![
            line("long full-width body line", 20, 10, 280, 24, 10.0),
            line("also long body line", 20, 28, 270, 42, 10.0),
            line("short tail.", 20, 46, 90, 60, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_separates_heading_only_when_size_difference_is_clearly_larger_than_body_jitter() {
        // The DB mask is content-tracking, so consecutive body lines can differ in tight_h by
        // 20–40% just from cap/acronym/ascender mix. We deliberately accept that range so
        // those don't false-break, which means a *modestly* larger heading (≤ 40% over body)
        // gets glued to its body. Real headings tend to be 70%+ taller and still separate.
        // First scenario: heading h=14 vs body h=11 → excess 0.27 → merges with body.
        let merged = group_lines_into_paragraphs(
            vec![
                line("Modest heading", 20, 10, 200, 24, 14.0),
                line("body line one", 20, 28, 200, 42, 11.0),
                line("body line two", 20, 46, 200, 60, 11.0),
            ],
            ParagraphGroupingOptions::default(),
        );
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].lines.len(), 3);

        // Second scenario: heading h=20 vs body h=11 → excess 0.82 → still breaks.
        let split = group_lines_into_paragraphs(
            vec![
                line("Real heading", 20, 10, 200, 34, 20.0),
                line("body line one", 20, 38, 200, 56, 11.0),
                line("body line two", 20, 60, 200, 78, 11.0),
            ],
            ParagraphGroupingOptions::default(),
        );
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].lines.len(), 1);
        assert_eq!(split[0].lines[0].text, "Real heading");
        assert_eq!(split[1].lines.len(), 2);
    }

    #[test]
    fn grouping_merges_body_paragraph_with_acronym_height_jitter() {
        // The TMJ case from the zackoverflow blog: three body lines in one paragraph whose
        // tight_h's vary because of mid-line acronyms (TMJ) and cap-heavy openings ("At
        // first I"). Old 0.15 opening tolerance fired on excess 0.19 and 0.26; the new 0.40
        // single tolerance lets them all stay together.
        let lines = vec![
            line(
                "I used to get random headaches while writing code. At first I",
                36,
                558,
                540,
                570,
                11.5,
            ),
            line(
                "thought it was sleep, or hydration, or TMJ pain from clenching my",
                36,
                595,
                540,
                609,
                13.7,
            ),
            line(
                "jaw. It turned out to be eye strain.",
                36,
                633,
                280,
                644,
                10.9,
            ),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_does_not_treat_long_prose_lines_starting_with_capital_as_list_items() {
        // Two body paragraphs in the same blog page. Each paragraph's first line happens to
        // start with a capitalised pronoun/article ("There", "I"). Without the word-count
        // gate the list-item rule fires on any gap > 0.8, splitting *every* such line off.
        // The word-count ceiling (≤ 4) excludes these 10+ word prose lines, so they stay in
        // their own paragraphs joined by the gap-baseline logic.
        let lines = vec![
            line(
                "There are strategies like the 20/20/20 rule to avoid eye-strain, but",
                36,
                692,
                540,
                704,
                11.1,
            ),
            line(
                "I never could actually stick to it. It's very flow breaking to stare off",
                36,
                729,
                540,
                740,
                11.0,
            ),
            line(
                "into the distance every 20 minutes while working.",
                36,
                765,
                380,
                777,
                11.6,
            ),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_splits_menu_items_starting_with_capitals() {
        // Coffee-menu shape: each item starts with a capital and sits with looser leading than
        // body text (gap ≈ 1.2 × tight_h). The list-item-start gate should split each into its
        // own paragraph. A sub-detail starting with punctuation (`-espresso and filter`) stays
        // glued to its parent.
        let lines = vec![
            line("Espresso", 20, 10, 80, 22, 10.0),
            line("Americano", 20, 34, 100, 46, 10.0),
            line("Iced Black", 20, 58, 105, 70, 10.0),
            line("Double Parked", 20, 82, 120, 94, 10.0),
            line("-espresso and filter", 20, 106, 150, 118, 10.0),
            line("Off The Hook +75c", 20, 134, 140, 146, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        let texts: Vec<Vec<&str>> = blocks
            .iter()
            .map(|b| b.lines.iter().map(|l| l.text.as_str()).collect())
            .collect();
        assert_eq!(
            texts,
            vec![
                vec!["Espresso"],
                vec!["Americano"],
                vec!["Iced Black"],
                vec!["Double Parked", "-espresso and filter"],
                vec!["Off The Hook +75c"],
            ]
        );
    }

    #[test]
    fn grouping_merges_per_word_detections_from_a_pixel_font_row() {
        // PaddleOCR's DB detector splits wide-kerning pixel fonts into per-word boxes. All
        // five "words" share a row (top ≈ 748); pre-merge should restore them into a single
        // line, and the grouper should then return one paragraph with one line of text.
        let lines = vec![
            line("How", 169, 748, 220, 758, 9.5),
            line("did", 226, 747, 275, 757, 9.5),
            line("other", 282, 748, 360, 758, 9.5),
            line("people", 366, 749, 455, 759, 9.5),
            line("do?", 463, 748, 510, 758, 9.5),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[0].lines[0].text, "How did other people do?");
    }

    #[test]
    fn grouping_does_not_merge_menu_columns_into_one_row() {
        // Item name on the left and price on the right at the same vertical position. Real
        // menus place these in distinct columns with a generous gutter (here ~14 × tight_h),
        // and pre-merge must leave them as two separate rows so column grouping can keep them
        // apart.
        let lines = vec![
            line("Espresso", 130, 200, 200, 212, 10.0),
            line("3.00", 360, 200, 410, 212, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lines.len(), 1);
        assert_eq!(blocks[1].lines.len(), 1);
    }

    #[test]
    fn grouping_keeps_body_lines_starting_uppercase_after_sentence_at_line_head_when_leading_is_tight()
     {
        // A justified body paragraph where a new sentence happens to begin at a line head.
        // Tight leading (gap_ratio ≈ 0.4) puts the join under `list_item_break_gap_ratio`, so
        // the paragraph stays whole even though line 3 starts uppercase.
        let lines = vec![
            line("first sentence ends here.", 20, 10, 240, 24, 10.0),
            line("Second sentence begins on this", 20, 28, 240, 42, 10.0),
            line("line and wraps to here.", 20, 46, 220, 60, 10.0),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    #[test]
    fn grouping_merges_loose_leading_blog_body() {
        // Mirrors the zackoverflow.dev blog body in the bug report: each line is fully
        // detected (one box per visual row), heights vary 9–12 from regular vs cap-heavy
        // content, line tops are ~36–38 px apart with tight_h ≈ 10 (gap_ratio ≈ 2.5). Before
        // the adaptive gate this exceeded the fixed `max_gap_ratio = 1.8` and every line
        // ended up its own paragraph. Three intra-paragraph lines plus a fourth across a
        // bigger gap should now collapse to two paragraphs.
        let lines = vec![
            line(
                "low-refresh B/W screen that's easier to look at seems to reduce",
                35,
                82,
                540,
                92,
                9.3,
            ),
            line(
                "visual noise / stimulation and help me read text more intently and",
                35,
                117,
                540,
                128,
                10.3,
            ),
            line("with less distractions.", 37, 153, 200, 164, 10.4),
            line(
                "I also just find the screen more comfortable to look at, which",
                36,
                213,
                540,
                225,
                11.7,
            ),
            line(
                "seems to result in me getting more hours of productivity.",
                36,
                251,
                540,
                262,
                11.0,
            ),
        ];
        let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].lines.len(), 3);
        assert_eq!(blocks[1].lines.len(), 2);
    }

    fn tilted_line(i: usize, theta: f32) -> TextLine {
        let (w, h, leading) = (1000.0f32, 20.0f32, 46.0f32);
        let (s, c) = theta.sin_cos();
        // Column origin, stepping along the column normal v = (−sinθ, cosθ); the line centre
        // sits half a width along the reading direction u = (cosθ, sinθ).
        let sx = 200.0 - (i as f32) * leading * s;
        let sy = 300.0 + (i as f32) * leading * c;
        let cx = sx + (w * 0.5) * c;
        let cy = sy + (w * 0.5) * s;
        let rect = Rect {
            left: (cx - w * 0.5) as u32,
            top: (cy - w * 0.5 * s.abs() - h * 0.5) as u32,
            right: (cx + w * 0.5) as u32,
            bottom: (cy + w * 0.5 * s.abs() + h * 0.5) as u32,
        };
        TextLine {
            text: "lorem ipsum dolor sit amet consectetur adipiscing".to_string(),
            bounding_box: rect,
            oriented_box: OrientedRect::axis_aligned(rect),
            tight_box: OrientedRect {
                cx,
                cy,
                width: w,
                height: h,
                angle_radians: theta,
            },
            word_rects: vec![rect],
            style_ranges: Vec::new(),
            ink_color: None,
        }
    }

    #[test]
    fn tilted_column_groups_as_one_paragraph() {
        // At −4° the left edges drift rightward ~3.2 px/line in image space, blowing through
        // edge_alignment_tolerance (0.6 × 20 px) after four lines; the reading-frame rotation
        // removes the drift. +4° drifts leftward (misread as indents without the rotation).
        for theta in [-4.0f32.to_radians(), 4.0f32.to_radians()] {
            let lines: Vec<TextLine> = (0..6).map(|i| tilted_line(i, theta)).collect();
            let blocks = group_lines_into_paragraphs(lines, ParagraphGroupingOptions::default());
            assert_eq!(blocks.len(), 1, "theta {:.1}°", theta.to_degrees());
            assert_eq!(blocks[0].lines.len(), 6);
            // Output geometry must be back in image space, not the rotated frame.
            let first = &blocks[0].lines[0].tight_box;
            assert!((first.cx - (200.0 + 500.0 * theta.cos())).abs() < 0.5);
            assert!((first.angle_radians - theta).abs() < 1e-4);
        }
    }

    #[test]
    fn translation_text_drops_hyphen_for_broken_word() {
        let block = TextBlock {
            lines: vec![
                line("self-", 20, 10, 90, 24, 10.0),
                line("awareness builds", 20, 28, 200, 42, 10.0),
            ],
        };
        assert_eq!(block.translation_text(), "selfawareness builds");
    }

    #[test]
    fn translation_text_joins_cjk_without_space() {
        let block = TextBlock {
            lines: vec![
                line("今日は晴れ", 20, 10, 200, 28, 16.0),
                line("ています。", 20, 32, 200, 50, 16.0),
            ],
        };
        assert_eq!(block.translation_text(), "今日は晴れています。");
    }

    #[test]
    fn translation_text_joins_latin_with_space() {
        let block = TextBlock {
            lines: vec![
                line("hello world", 20, 10, 200, 24, 10.0),
                line("from rust", 20, 28, 200, 42, 10.0),
            ],
        };
        assert_eq!(block.translation_text(), "hello world from rust");
    }

    fn block_texts(blocks: &[TextBlock]) -> Vec<Vec<String>> {
        let mut out: Vec<Vec<String>> = blocks
            .iter()
            .map(|b| b.lines.iter().map(|l| l.text.clone()).collect())
            .collect();
        for texts in &mut out {
            texts.sort();
        }
        out.sort();
        out
    }

    #[test]
    fn live_grouping_splits_heading_with_embedded_exclamation() {
        let lines = vec![
            line("DANGER!", 20, 10, 120, 26, 14.0),
            line("Keep out", 20, 32, 120, 46, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        assert_eq!(
            block_texts(&blocks),
            vec![vec!["DANGER!".to_string()], vec!["Keep out".to_string()]]
        );
    }

    #[test]
    fn live_grouping_splits_heading_with_embedded_question_mark() {
        let lines = vec![
            line("What should you do?", 20, 10, 220, 26, 12.0),
            line("Stay calm and wait.", 20, 30, 220, 44, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        assert_eq!(
            block_texts(&blocks),
            vec![
                vec!["Stay calm and wait.".to_string()],
                vec!["What should you do?".to_string()],
            ]
        );
    }

    #[test]
    fn live_grouping_splits_heading_when_punctuation_is_its_own_strip() {
        // Reproduces the sign_raw.jpg case: OCR detects the heading word
        // and its trailing `!` as two separate strips on the same reading
        // line, with the body on the line below. The barrier rule must
        // prevent the heading row from merging into the body row even
        // though the pair (heading-word, body) doesn't itself involve a
        // `!`-terminated strip.
        let lines = vec![
            line("ATTENTION", 20, 10, 140, 26, 14.0),
            line("!", 150, 10, 160, 26, 14.0),
            line("HIGH TENSION", 20, 34, 220, 48, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        let leaked = blocks.iter().any(|b| {
            let has_heading = b
                .lines
                .iter()
                .any(|l| l.text == "ATTENTION" || l.text == "!");
            let has_body = b.lines.iter().any(|l| l.text == "HIGH TENSION");
            has_heading && has_body
        });
        assert!(
            !leaked,
            "heading row leaked into body block: {:?}",
            block_texts(&blocks)
        );
    }

    #[test]
    fn live_grouping_keeps_body_paragraphs_terminated_by_period() {
        let lines = vec![
            line("This is a sentence.", 20, 10, 220, 24, 12.0),
            line("And another one.", 20, 28, 220, 42, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 2);
    }

    #[test]
    fn live_grouping_ignores_lower_lines_terminal_question() {
        let lines = vec![
            line("Some prose continues", 20, 10, 220, 24, 12.0),
            line("with a question?", 20, 28, 220, 42, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 2);
    }

    #[test]
    fn live_grouping_heading_check_ignores_trailing_whitespace() {
        let lines = vec![
            line("WARNING!   ", 20, 10, 220, 26, 12.0),
            line("Read carefully.", 20, 30, 220, 44, 12.0),
        ];
        let blocks = group_live_lines_into_blocks(lines);
        assert_eq!(
            block_texts(&blocks),
            vec![
                vec!["Read carefully.".to_string()],
                vec!["WARNING!   ".to_string()],
            ]
        );
    }

    #[test]
    fn style_spans_tile_overlapping_bold_and_underline() {
        // "ab cd ef" (len 8): bold over [0,5), underline over [3,8). The two ranges overlap on
        // "cd" (bytes 3..5), which must come out as a single span that is both bold and underlined.
        let ranges = vec![
            StyleRange {
                start: 0,
                end: 5,
                kind: StyleKind::Bold,
            },
            StyleRange {
                start: 3,
                end: 8,
                kind: StyleKind::Decoration(LineDecoration::Underline),
            },
        ];
        let spans = style_spans_from_styles(8, &ranges);
        let summary: Vec<(u32, u32, bool, Option<LineDecoration>)> = spans
            .iter()
            .map(|s| (s.start, s.end, s.bold, s.decoration))
            .collect();
        assert_eq!(
            summary,
            vec![
                (0, 3, true, None),
                (3, 5, true, Some(LineDecoration::Underline)),
                (5, 8, false, Some(LineDecoration::Underline)),
            ]
        );
    }

    #[test]
    fn style_spans_coalesce_identical_neighbours() {
        // Two touching bold ranges tile into one continuous bold span, not two.
        let ranges = vec![
            StyleRange {
                start: 0,
                end: 2,
                kind: StyleKind::Bold,
            },
            StyleRange {
                start: 2,
                end: 4,
                kind: StyleKind::Bold,
            },
        ];
        let spans = style_spans_from_styles(6, &ranges);
        assert_eq!(spans.len(), 2);
        assert_eq!((spans[0].start, spans[0].end, spans[0].bold), (0, 4, true));
        assert_eq!((spans[1].start, spans[1].end, spans[1].bold), (4, 6, false));
    }

    #[test]
    fn style_ranges_round_trip_through_spans() {
        // Tiling then extracting must recover the original ranges — this is what `retranslate`
        // relies on to recover a block's source styles from its cached `source_style_spans`.
        let ranges = vec![
            StyleRange {
                start: 0,
                end: 5,
                kind: StyleKind::Bold,
            },
            StyleRange {
                start: 3,
                end: 8,
                kind: StyleKind::Color(0xFFCC2020),
            },
            StyleRange {
                start: 10,
                end: 14,
                kind: StyleKind::Decoration(LineDecoration::Underline),
            },
        ];
        let spans = style_spans_from_styles(16, &ranges);
        let mut recovered = super::style_ranges_from_spans(&spans);
        recovered.sort_by_key(|r| (super::style_kind_key(r.kind), r.start));
        let mut expected = super::merge_style_ranges(ranges);
        expected.sort_by_key(|r| (super::style_kind_key(r.kind), r.start));
        assert_eq!(recovered, expected);
    }
}
