//! Drag-to-select over the words of an OCR layer, in image-pixel space.
//!
//! Frontends own the view transform (letterboxing, zoom, pan) and unmap a pointer position into
//! image pixels before calling in; everything past that — hit-testing, which words a drag covers,
//! the merged highlight shapes, and the text that lands on the clipboard — lives here so the Qt
//! and Android surfaces cannot drift apart on the parts a user would notice.

use crate::ocr::{OrientedRect, PositionedWord, Rect};

/// Which way a run of text reads. Words of one axis never join a selection anchored on the other,
/// so a horizontal drag cannot sweep into a vertical column that happens to cross it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WritingAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Everything needed to paint and act on one selection. Produced in a single pass so a frontend
/// draws `pills`, anchors its handles at `start_handle`/`end_handle`, positions a context menu
/// against `bounds`, and copies `text` — with no geometry of its own.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SelectionView {
    /// One merged rounded-rect per selected line, already clamped so tightly-spaced lines meet
    /// halfway instead of overlapping.
    pub pills: Vec<OrientedRect>,
    /// Bottom corner at the reading-start of the first word, where a leading drag handle anchors.
    pub start_handle: Point,
    /// Bottom corner at the reading-end of the last word.
    pub end_handle: Point,
    /// Axis-aligned bounds of the selection, for anchoring a floating action bar.
    pub bounds: Rect,
    pub text: String,
}

/// Padding around a word box when hit-testing, as a fraction of its height. Touch points land
/// between lines often enough that the bare box feels unresponsive.
const HIT_PADDING: f32 = 0.25;

/// Lines further apart than this multiple of their height are separate blocks, and join with a
/// newline rather than a space.
const BLOCK_GAP: f32 = 1.5;

fn reading_pos(rect: &OrientedRect) -> f32 {
    rect.cx * rect.angle_radians.cos() + rect.cy * rect.angle_radians.sin()
}

fn cross_pos(rect: &OrientedRect) -> f32 {
    -rect.cx * rect.angle_radians.sin() + rect.cy * rect.angle_radians.cos()
}

fn axis_of(rect: &OrientedRect) -> WritingAxis {
    let pi = std::f32::consts::PI;
    let a = ((rect.angle_radians % pi) + pi) % pi;
    if a < pi / 4.0 || a > 3.0 * pi / 4.0 {
        WritingAxis::Horizontal
    } else {
        WritingAxis::Vertical
    }
}

/// The writing axis of one word, so a caller dragging a handle can keep the selection on the axis
/// it started on.
pub fn word_axis(words: &[PositionedWord], index: u32) -> Option<WritingAxis> {
    words.get(index as usize).map(|w| axis_of(&w.bounds))
}

/// The word under an image-space point, or `None` for a tap on bare image. Boxes are padded
/// vertically; the first match in reading order wins where padded boxes overlap.
pub fn hit_test_word(words: &[PositionedWord], x: f32, y: f32) -> Option<u32> {
    words
        .iter()
        .position(|w| {
            let b = &w.bounds;
            let (cos, sin) = (b.angle_radians.cos(), b.angle_radians.sin());
            let (dx, dy) = (x - b.cx, y - b.cy);
            let along = dx * cos + dy * sin;
            let across = -dx * sin + dy * cos;
            let pad = b.height * HIT_PADDING;
            along.abs() <= b.width * 0.5 + pad && across.abs() <= b.height * 0.5 + pad
        })
        .map(|i| i as u32)
}

/// The word whose centre is nearest an image-space point, optionally restricted to one writing
/// axis. Used while dragging a selection handle, where the finger is rarely inside a box.
pub fn nearest_word(
    words: &[PositionedWord],
    x: f32,
    y: f32,
    axis: Option<WritingAxis>,
) -> Option<u32> {
    words
        .iter()
        .enumerate()
        .filter(|(_, w)| axis.is_none_or(|want| axis_of(&w.bounds) == want))
        .min_by(|(_, a), (_, b)| {
            let da = (a.bounds.cx - x).powi(2) + (a.bounds.cy - y).powi(2);
            let db = (b.bounds.cx - x).powi(2) + (b.bounds.cy - y).powi(2);
            da.total_cmp(&db)
        })
        .map(|(i, _)| i as u32)
}

/// Indices covered by a drag between two words. `start` and `end` may arrive in either order;
/// words off the anchor's writing axis are dropped.
pub fn selection_indices(words: &[PositionedWord], start: u32, end: u32) -> Vec<u32> {
    let (lo, hi) = (start.min(end) as usize, start.max(end) as usize);
    if lo >= words.len() {
        return Vec::new();
    }
    let hi = hi.min(words.len() - 1);
    let axis = axis_of(&words[lo].bounds);
    (lo..=hi)
        .filter(|&i| axis_of(&words[i].bounds) == axis)
        .map(|i| i as u32)
        .collect()
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF   // CJK unified ideographs
        | 0x3000..=0x303F // CJK symbols and punctuation
        | 0x3400..=0x4DBF // extension A
        | 0x3040..=0x30FF // hiragana + katakana
        | 0xAC00..=0xD7AF // hangul syllables
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xFF00..=0xFFEF // halfwidth and fullwidth forms
    )
}

/// Join one block's words. A gap closes up only when the characters on both sides of it are CJK;
/// every other gap takes a space, so an embedded Latin run keeps the spacing it was rendered with.
///
/// The decision is per gap rather than per block because a block-wide ratio miscounts: a Latin word
/// contributes one character per letter while a CJK word contributes one per word, so a single
/// "Microsoft" outvotes ten ideographs and every ideograph then gets spaced.
fn join_block(texts: &[&str]) -> String {
    let mut joined = String::new();
    for text in texts {
        let tight = match (joined.chars().last(), text.chars().next()) {
            (Some(previous), Some(next)) => is_cjk(previous) && is_cjk(next),
            _ => true,
        };
        if !joined.is_empty() && !tight {
            joined.push(' ');
        }
        joined.push_str(text);
    }
    joined.trim().to_string()
}

/// Split words into runs sharing a `line_index`. Words arrive in reading order, so a line's words
/// are already adjacent.
fn group_lines<'a>(words: &[&'a PositionedWord]) -> Vec<Vec<&'a PositionedWord>> {
    let mut lines: Vec<Vec<&PositionedWord>> = Vec::new();
    for word in words {
        match lines.last_mut() {
            Some(line) if line[0].line_index == word.line_index => line.push(word),
            _ => lines.push(vec![word]),
        }
    }
    lines
}

fn is_block_break(prev: &[&PositionedWord], cur: &[&PositionedWord]) -> bool {
    let height = prev
        .iter()
        .chain(cur.iter())
        .map(|w| w.bounds.height)
        .fold(0.0_f32, f32::max);
    (cross_pos(&cur[0].bounds) - cross_pos(&prev[0].bounds)).abs() > BLOCK_GAP * height
}

/// Merge one line's words into a single oriented rect spanning them, the inverse of
/// [`OrientedRect::subspan`].
fn merge_along_line(words: &[&PositionedWord]) -> OrientedRect {
    let reference = words[0].bounds;
    let (cos, sin) = (reference.angle_radians.cos(), reference.angle_radians.sin());
    let mut min = f32::MAX;
    let mut max = f32::MIN;
    let mut height = 0.0_f32;
    for word in words {
        let along = word.bounds.cx * cos + word.bounds.cy * sin;
        min = min.min(along - word.bounds.width * 0.5);
        max = max.max(along + word.bounds.width * 0.5);
        height = height.max(word.bounds.height);
    }
    let shift = (min + max) * 0.5 - (reference.cx * cos + reference.cy * sin);
    OrientedRect {
        cx: reference.cx + shift * cos,
        cy: reference.cy + shift * sin,
        width: max - min,
        height,
        angle_radians: reference.angle_radians,
    }
}

/// Shrink each pill to the cross-axis gap to its nearest neighbour, counting only neighbours that
/// overlap it along the reading axis — a parallel column at the same height must not shrink it.
fn clamp_pill_heights(pills: &mut [OrientedRect]) {
    let spans: Vec<(f32, f32, f32)> = pills
        .iter()
        .map(|p| {
            let read = reading_pos(p);
            (cross_pos(p), read - p.width * 0.5, read + p.width * 0.5)
        })
        .collect();
    for (i, pill) in pills.iter_mut().enumerate() {
        let (cross, start, end) = spans[i];
        let gap = spans
            .iter()
            .enumerate()
            .filter(|(j, (_, other_start, other_end))| {
                *j != i && start < *other_end && *other_start < end
            })
            .map(|(_, (other_cross, _, _))| (cross - other_cross).abs())
            .fold(f32::MAX, f32::min);
        pill.height = pill.height.min(gap);
    }
}

/// Bottom corner at one end of a word, where a drag handle hangs. Image y points down, so the
/// across-axis `(-sin, cos)` points below the line.
fn handle_point(rect: &OrientedRect, leading: bool) -> Point {
    let (cos, sin) = (rect.angle_radians.cos(), rect.angle_radians.sin());
    let along = if leading { -0.5 } else { 0.5 } * rect.width;
    let across = rect.height * 0.5;
    Point {
        x: rect.cx + along * cos - across * sin,
        y: rect.cy + along * sin + across * cos,
    }
}

fn bounds_of(pills: &[OrientedRect]) -> Rect {
    let mut left = f32::MAX;
    let mut top = f32::MAX;
    let mut right = f32::MIN;
    let mut bottom = f32::MIN;
    for corner in pills.iter().flat_map(|p| p.corners()) {
        left = left.min(corner.0);
        top = top.min(corner.1);
        right = right.max(corner.0);
        bottom = bottom.max(corner.1);
    }
    Rect {
        left: left.max(0.0) as u32,
        top: top.max(0.0) as u32,
        right: right.max(0.0) as u32,
        bottom: bottom.max(0.0) as u32,
    }
}

/// Resolve a drag between two word indices into everything a frontend needs to paint and act on
/// it. `None` when the range covers no words.
pub fn resolve_selection(words: &[PositionedWord], start: u32, end: u32) -> Option<SelectionView> {
    let indices = selection_indices(words, start, end);
    if indices.is_empty() {
        return None;
    }
    let selected: Vec<&PositionedWord> = indices.iter().map(|&i| &words[i as usize]).collect();
    let lines = group_lines(&selected);

    let mut blocks: Vec<Vec<&PositionedWord>> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 || is_block_break(&lines[i - 1], line) {
            blocks.push(line.clone());
        } else {
            blocks
                .last_mut()
                .expect("first line opens a block")
                .extend(line);
        }
    }
    let text = blocks
        .iter()
        .map(|block| join_block(&block.iter().map(|w| w.text.as_str()).collect::<Vec<_>>()))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    let mut pills: Vec<OrientedRect> = lines.iter().map(|line| merge_along_line(line)).collect();
    clamp_pill_heights(&mut pills);

    let first = selected.first().expect("non-empty selection");
    let last = selected.last().expect("non-empty selection");
    Some(SelectionView {
        bounds: bounds_of(&pills),
        pills,
        start_handle: handle_point(&first.bounds, true),
        end_handle: handle_point(&last.bounds, false),
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, cx: f32, cy: f32, width: f32, line_index: u32) -> PositionedWord {
        PositionedWord {
            text: text.to_string(),
            bounds: OrientedRect {
                cx,
                cy,
                width,
                height: 10.0,
                angle_radians: 0.0,
            },
            line_index,
        }
    }

    /// Two words per line, lines 12px apart (within the block gap).
    fn paragraph() -> Vec<PositionedWord> {
        vec![
            word("hello", 20.0, 10.0, 40.0, 0),
            word("world", 70.0, 10.0, 40.0, 0),
            word("second", 20.0, 22.0, 40.0, 1),
            word("line", 70.0, 22.0, 40.0, 1),
        ]
    }

    #[test]
    fn hit_test_finds_word_under_point_and_within_padding() {
        let words = paragraph();
        assert_eq!(hit_test_word(&words, 20.0, 10.0), Some(0));
        assert_eq!(hit_test_word(&words, 70.0, 10.0), Some(1));
        // 2px below the box bottom, inside the 25%-of-height pad.
        assert_eq!(hit_test_word(&words, 20.0, 17.0), Some(0));
        assert_eq!(hit_test_word(&words, 300.0, 300.0), None);
    }

    #[test]
    fn nearest_word_ignores_the_other_axis() {
        let mut words = paragraph();
        words.push(PositionedWord {
            text: "vertical".to_string(),
            bounds: OrientedRect {
                cx: 200.0,
                cy: 10.0,
                width: 40.0,
                height: 10.0,
                angle_radians: std::f32::consts::FRAC_PI_2,
            },
            line_index: 2,
        });
        assert_eq!(nearest_word(&words, 199.0, 10.0, None), Some(4));
        assert_eq!(
            nearest_word(&words, 199.0, 10.0, Some(WritingAxis::Horizontal)),
            Some(1),
        );
    }

    #[test]
    fn selection_is_order_independent_and_axis_filtered() {
        let words = paragraph();
        assert_eq!(selection_indices(&words, 0, 2), vec![0, 1, 2]);
        assert_eq!(selection_indices(&words, 2, 0), vec![0, 1, 2]);
        assert_eq!(selection_indices(&words, 9, 9), Vec::<u32>::new());
    }

    #[test]
    fn close_lines_join_with_a_space_and_far_lines_with_a_newline() {
        let words = paragraph();
        let joined = resolve_selection(&words, 0, 3).expect("selection");
        assert_eq!(joined.text, "hello world second line");

        let mut spread = paragraph();
        // Push the second line past 1.5x the line height.
        spread[2].bounds.cy = 60.0;
        spread[3].bounds.cy = 60.0;
        let split = resolve_selection(&spread, 0, 3).expect("selection");
        assert_eq!(split.text, "hello world\nsecond line");
    }

    #[test]
    fn latin_inside_cjk_does_not_space_the_ideographs() {
        // One unit per selection unit, as WordCarver carves them: ideographs individually, Latin
        // words whole. A block-wide CJK ratio scores "Microsoft" as nine characters against ten
        // ideographs and spaces the lot.
        let units = [
            "\u{81ea}",
            "\u{5b9a}",
            "\u{4e49}",
            "Microsoft",
            "Edge",
            "\u{4ee5}",
            "\u{5339}",
            "\u{914d}",
            "\u{60a8}",
            "\u{7684}",
            "\u{98ce}",
            "\u{683c}",
        ];
        let words: Vec<PositionedWord> = units
            .iter()
            .enumerate()
            .map(|(i, t)| word(t, 20.0 + (i as f32) * 30.0, 10.0, 28.0, 0))
            .collect();
        let view = resolve_selection(&words, 0, units.len() as u32 - 1).expect("selection");
        assert_eq!(
            view.text,
            "\u{81ea}\u{5b9a}\u{4e49} Microsoft Edge \u{4ee5}\u{5339}\u{914d}\u{60a8}\u{7684}\u{98ce}\u{683c}"
        );
    }

    #[test]
    fn cjk_punctuation_stays_tight() {
        let words = vec![
            word("\u{4f60}", 20.0, 10.0, 20.0, 0),
            word("\u{597d}", 45.0, 10.0, 20.0, 0),
            word("\u{3002}", 70.0, 10.0, 20.0, 0),
        ];
        let view = resolve_selection(&words, 0, 2).expect("selection");
        assert_eq!(view.text, "\u{4f60}\u{597d}\u{3002}");
    }

    #[test]
    fn cjk_words_join_without_spaces() {
        let words = vec![
            word("日本", 20.0, 10.0, 40.0, 0),
            word("語", 70.0, 10.0, 20.0, 0),
        ];
        let view = resolve_selection(&words, 0, 1).expect("selection");
        assert_eq!(view.text, "日本語");
    }

    #[test]
    fn one_pill_per_line_spans_its_words() {
        let words = paragraph();
        let view = resolve_selection(&words, 0, 3).expect("selection");
        assert_eq!(view.pills.len(), 2);
        // 40-wide words centred at 20 and 70 span 0..90.
        assert_eq!(view.pills[0].width, 90.0);
        assert_eq!(view.pills[0].cx, 45.0);
    }

    #[test]
    fn pill_height_is_clamped_to_the_neighbouring_line_gap() {
        let words = paragraph();
        let view = resolve_selection(&words, 0, 3).expect("selection");
        // Lines sit 12px apart, so a 10px-tall pill keeps its height.
        assert_eq!(view.pills[0].height, 10.0);

        let mut tight = paragraph();
        tight[2].bounds.cy = 16.0;
        tight[3].bounds.cy = 16.0;
        let clamped = resolve_selection(&tight, 0, 3).expect("selection");
        assert_eq!(clamped.pills[0].height, 6.0);
    }

    #[test]
    fn a_parallel_column_does_not_clamp_the_pill() {
        // Same cross position as line 0 but far along the reading axis: no overlap, no clamping.
        let words = vec![
            word("left", 20.0, 10.0, 40.0, 0),
            word("right", 500.0, 12.0, 40.0, 1),
        ];
        let view = resolve_selection(&words, 0, 1).expect("selection");
        assert_eq!(view.pills[0].height, 10.0);
    }

    #[test]
    fn handles_sit_at_the_outer_bottom_corners() {
        let words = paragraph();
        let view = resolve_selection(&words, 0, 3).expect("selection");
        // First word spans x 0..40, last spans 50..90; both boxes bottom out at y=15.
        assert_eq!(view.start_handle, Point { x: 0.0, y: 15.0 });
        assert_eq!(view.end_handle, Point { x: 90.0, y: 27.0 });
    }
}
