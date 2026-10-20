//! Shared PDF content-stream helpers.
//!
//! Both style probing and writeback need to interpret enough of a page content
//! stream to know where each text-show operator lands. Keep that state machine
//! here so extraction and surgery cannot drift in matrix math, text advances,
//! font flags, or inherited page geometry.
//!
//! [`ContentStreamBuilder`] mirrors [`ContentState`] for the write side:
//! callers describe operators in PDF vocabulary (`save_state`, `set_font`,
//! `show_hex_gids`, …) instead of formatting raw bytes inline with layout
//! math.

use std::collections::{HashMap, HashSet};
use std::io::Write as _;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::pdf::PageDims;
use translator_core::ocr::Rect;

#[derive(Debug, Clone, Copy)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    /// PDF matrix multiplication: result transforms a point as
    /// `point -> other -> self`.
    pub fn mul(self, other: Matrix) -> Matrix {
        Matrix {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn inverse(&self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-9 {
            return None;
        }
        let inv_a = self.d / det;
        let inv_b = -self.b / det;
        let inv_c = -self.c / det;
        let inv_d = self.a / det;
        let inv_e = -(self.e * inv_a + self.f * inv_c);
        let inv_f = -(self.e * inv_b + self.f * inv_d);
        Some(Matrix {
            a: inv_a,
            b: inv_b,
            c: inv_c,
            d: inv_d,
            e: inv_e,
            f: inv_f,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PageGeometry {
    /// User-space width (independent of `/Rotate`; matches MediaBox x range).
    pub user_w: f32,
    /// User-space height (independent of `/Rotate`; matches MediaBox y range).
    pub user_h: f32,
    /// MediaBox lower-left x in user space. Most PDFs have 0; some (like
    /// this Cockburn book) use a non-zero offset, e.g. `[-6.69, -12.89,
    /// 504, 667.58]`.
    pub user_x_min: f32,
    /// MediaBox lower-left y in user space. Same caveat as `user_x_min`.
    pub user_y_min: f32,
    /// `/Rotate` value, normalised to 0/90/180/270.
    pub rotate: i32,
}

impl PageGeometry {
    pub fn read(doc: &Document, page_id: ObjectId, fallback_display: Option<PageDims>) -> Self {
        let rotate = inherited_object(doc, page_id, b"Rotate")
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0);
        let rotate = ((rotate % 360 + 360) % 360) as i32;

        if let Some((user_x_min, user_y_min, user_w, user_h)) =
            inherited_object(doc, page_id, b"MediaBox").and_then(|o| media_box_dims(doc, &o))
        {
            return Self {
                user_w,
                user_h,
                user_x_min,
                user_y_min,
                rotate,
            };
        }

        let (user_w, user_h) = fallback_display
            .map(|display| match rotate {
                90 | 270 => (display.height_pts, display.width_pts),
                _ => (display.width_pts, display.height_pts),
            })
            .unwrap_or((612.0, 792.0));

        Self {
            user_w,
            user_h,
            user_x_min: 0.0,
            user_y_min: 0.0,
            rotate,
        }
    }

    fn user_top(&self) -> f32 {
        self.user_y_min + self.user_h
    }

    fn user_right(&self) -> f32 {
        self.user_x_min + self.user_w
    }

    /// Convert a user-space point to display coords (top-left origin), matching
    /// MuPDF's stext line/char coordinates.
    pub fn to_display(&self, user: (f32, f32)) -> (f32, f32) {
        let top = self.user_top();
        let right = self.user_right();
        match self.rotate {
            0 => (user.0 - self.user_x_min, top - user.1),
            90 => (user.1 - self.user_y_min, user.0 - self.user_x_min),
            180 => (right - user.0, user.1 - self.user_y_min),
            270 => (top - user.1, right - user.0),
            _ => (user.0 - self.user_x_min, top - user.1),
        }
    }

    /// Convert a MuPDF stext bbox (display coords, top-left origin) to a PDF
    /// user-space rect honouring the effective `/Rotate`.
    pub fn user_rect_from_display(&self, bbox: Rect) -> UserRect {
        let (l, t, r, b) = (
            bbox.left as f32,
            bbox.top as f32,
            bbox.right as f32,
            bbox.bottom as f32,
        );
        let top = self.user_top();
        let right = self.user_right();
        let (x0, x1, y0, y1) = match self.rotate {
            0 => (l + self.user_x_min, r + self.user_x_min, top - b, top - t),
            90 => (
                t + self.user_x_min,
                b + self.user_x_min,
                l + self.user_y_min,
                r + self.user_y_min,
            ),
            180 => (
                right - r,
                right - l,
                t + self.user_y_min,
                b + self.user_y_min,
            ),
            270 => (right - b, right - t, top - r, top - l),
            _ => (l + self.user_x_min, r + self.user_x_min, top - b, top - t),
        };
        UserRect {
            x0: x0.min(x1),
            x1: x0.max(x1),
            y0: y0.min(y1),
            y1: y0.max(y1),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UserRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

fn inherited_object(doc: &Document, page_id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut current_id = page_id;
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current_id) {
            return None;
        }
        let node = doc.get_object(current_id).ok()?.as_dict().ok()?;
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
        current_id = node.get(b"Parent").ok()?.as_reference().ok()?;
    }
}

fn media_box_dims(doc: &Document, obj: &Object) -> Option<(f32, f32, f32, f32)> {
    let arr = match obj {
        Object::Array(arr) => arr,
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
        _ => return None,
    };
    if arr.len() != 4 {
        return None;
    }
    let nums: Option<Vec<f32>> = arr.iter().map(object_as_f32).collect();
    let nums = nums?;
    let lx = nums[0].min(nums[2]);
    let ly = nums[1].min(nums[3]);
    let rx = nums[0].max(nums[2]);
    let ty = nums[1].max(nums[3]);
    Some((lx, ly, rx - lx, ty - ly))
}

#[derive(Debug, Clone)]
pub struct FontAdvance {
    code_bytes: usize,
    default_width: f32,
    widths: HashMap<u16, f32>,
}

impl Default for FontAdvance {
    /// 500.0 is Adobe's de-facto default-width for simple Type-1 fonts when
    /// no `/Widths` array is present.
    fn default() -> Self {
        Self {
            code_bytes: 1,
            default_width: 500.0,
            widths: HashMap::new(),
        }
    }
}

impl FontAdvance {
    fn from_font_dict(doc: &Document, font: &Dictionary) -> Self {
        let subtype = font
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            .unwrap_or(b"");
        if subtype == b"Type0" {
            return Self::from_type0(doc, font);
        }
        Self::from_simple_font(doc, font)
    }

    fn from_simple_font(doc: &Document, font: &Dictionary) -> Self {
        let first = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| o.as_i64().ok())
            .unwrap_or(0)
            .max(0) as u16;
        let mut widths = HashMap::new();
        if let Ok(widths_obj) = font.get(b"Widths") {
            if let Some(arr) = resolve_array(doc, widths_obj) {
                for (i, width) in arr.iter().enumerate() {
                    if let Some(w) = object_as_f32(width) {
                        widths.insert(first.saturating_add(i as u16), w);
                    }
                }
            }
        }
        Self {
            code_bytes: 1,
            default_width: 500.0,
            widths,
        }
    }

    fn from_type0(doc: &Document, font: &Dictionary) -> Self {
        let descendant = font
            .get(b"DescendantFonts")
            .ok()
            .and_then(|o| o.as_array().ok())
            .and_then(|arr| arr.first())
            .and_then(|obj| match obj {
                Object::Reference(id) => doc.get_dictionary(*id).ok(),
                Object::Dictionary(d) => Some(d),
                _ => None,
            });
        // 1000.0 is the PDF spec default for `/DW` on CID fonts (PDF 1.7 §9.7.4.3).
        let Some(descendant) = descendant else {
            return Self {
                code_bytes: 2,
                default_width: 1000.0,
                widths: HashMap::new(),
            };
        };
        let default_width = descendant
            .get(b"DW")
            .ok()
            .and_then(object_as_f32)
            .unwrap_or(1000.0);
        let mut out = Self {
            code_bytes: 2,
            default_width,
            widths: HashMap::new(),
        };
        if let Ok(w_obj) = descendant.get(b"W") {
            if let Some(w_array) = resolve_array(doc, w_obj) {
                parse_cid_widths(w_array, &mut out.widths);
            }
        }
        out
    }

    fn width_for_code(&self, code: u16) -> f32 {
        self.widths
            .get(&code)
            .copied()
            .unwrap_or(self.default_width)
    }

    fn string_width_1000(&self, bytes: &[u8]) -> (f32, usize, usize) {
        let mut width = 0.0;
        let mut glyphs = 0usize;
        let mut spaces = 0usize;
        if self.code_bytes == 2 {
            for chunk in bytes.chunks(2) {
                let code = if chunk.len() == 2 {
                    u16::from_be_bytes([chunk[0], chunk[1]])
                } else {
                    chunk[0] as u16
                };
                width += self.width_for_code(code);
                glyphs += 1;
                if code == 32 {
                    spaces += 1;
                }
            }
        } else {
            for &b in bytes {
                width += self.width_for_code(b as u16);
                glyphs += 1;
                if b == b' ' {
                    spaces += 1;
                }
            }
        }
        (width, glyphs, spaces)
    }
}

#[derive(Debug, Default, Clone)]
pub struct FontAdvanceMap {
    by_resource: HashMap<Vec<u8>, FontAdvance>,
}

impl FontAdvanceMap {
    pub fn from_page(doc: &Document, page_id: ObjectId) -> Self {
        let mut by_resource = HashMap::new();
        if let Ok(fonts) = doc.get_page_fonts(page_id) {
            for (name, font) in fonts {
                by_resource.insert(name, FontAdvance::from_font_dict(doc, font));
            }
        }
        Self { by_resource }
    }

    pub fn from_resources(doc: &Document, resources: &Dictionary) -> Self {
        let mut by_resource = HashMap::new();
        collect_font_advances_from_resources(doc, resources, &mut by_resource);
        Self { by_resource }
    }

    fn get(&self, name: Option<&Vec<u8>>) -> FontAdvance {
        name.and_then(|n| self.by_resource.get(n))
            .cloned()
            .unwrap_or_default()
    }
}

fn collect_font_advances_from_resources(
    doc: &Document,
    resources: &Dictionary,
    out: &mut HashMap<Vec<u8>, FontAdvance>,
) {
    let font_dict = match resources.get(b"Font") {
        Ok(Object::Reference(id)) => doc.get_object(*id).and_then(Object::as_dict).ok(),
        Ok(Object::Dictionary(dict)) => Some(dict),
        _ => None,
    };
    let Some(font_dict) = font_dict else {
        return;
    };
    for (name, value) in font_dict.iter() {
        if out.contains_key(name) {
            continue;
        }
        let font = match value {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(dict) => Some(dict),
            _ => None,
        };
        if let Some(font) = font {
            out.insert(name.clone(), FontAdvance::from_font_dict(doc, font));
        }
    }
}

fn parse_cid_widths(w_array: &[Object], widths: &mut HashMap<u16, f32>) {
    let mut i = 0usize;
    while i < w_array.len() {
        let Some(first) = w_array
            .get(i)
            .and_then(object_as_f32)
            .map(|v| v.max(0.0) as u16)
        else {
            i += 1;
            continue;
        };
        let Some(next) = w_array.get(i + 1) else {
            break;
        };
        match next {
            Object::Array(arr) => {
                for (offset, width) in arr.iter().enumerate() {
                    if let Some(w) = object_as_f32(width) {
                        widths.insert(first.saturating_add(offset as u16), w);
                    }
                }
                i += 2;
            }
            _ => {
                if let (Some(last), Some(width)) = (
                    w_array.get(i + 1).and_then(object_as_f32),
                    w_array.get(i + 2).and_then(object_as_f32),
                ) {
                    for code in first..=(last.max(first as f32) as u16) {
                        widths.insert(code, width);
                    }
                    i += 3;
                } else {
                    i += 1;
                }
            }
        }
    }
}

/// Subset of state q/Q saves/restores. Per the PDF spec, text state (font
/// size, char/word spacing, horizontal scaling, leading) belongs here too,
/// but some real producers emit `Tf` once before any q and expect that size
/// to persist past Q. Keeping those fields flat on [`ContentState`] matches
/// that de-facto behavior.
#[derive(Debug, Clone)]
struct GraphicsState {
    ctm: Matrix,
    fill_rgb: (f32, f32, f32),
    font_resource: Option<Vec<u8>>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::identity(),
            fill_rgb: (0.0, 0.0, 0.0),
            font_resource: None,
        }
    }
}

#[derive(Debug)]
pub struct ContentState {
    stack: Vec<GraphicsState>,
    current: GraphicsState,
    in_text: bool,
    text_matrix: Matrix,
    text_line_matrix: Matrix,
    font_size: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scaling: f32,
    text_leading: f32,
}

impl ContentState {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            current: GraphicsState::default(),
            in_text: false,
            text_matrix: Matrix::identity(),
            text_line_matrix: Matrix::identity(),
            font_size: 12.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            text_leading: 0.0,
        }
    }

    pub fn with_ctm(ctm: Matrix) -> Self {
        let mut state = Self::new();
        state.current.ctm = ctm;
        state
    }

    pub fn apply_non_show_op(&mut self, op: &Operation) {
        match op.operator.as_str() {
            "q" => self.stack.push(self.current.clone()),
            "Q" => {
                if let Some(prev) = self.stack.pop() {
                    self.current = prev;
                }
            }
            "cm" => {
                if let Some(m) = matrix_from_operands(&op.operands) {
                    self.current.ctm = m.mul(self.current.ctm);
                }
            }
            "BT" => {
                self.in_text = true;
                self.text_matrix = Matrix::identity();
                self.text_line_matrix = Matrix::identity();
            }
            "ET" => self.in_text = false,
            "Tf" => {
                if let Some(Object::Name(name)) = op.operands.first() {
                    self.current.font_resource = Some(name.clone());
                }
                if let Some(size) = op.operands.get(1).and_then(object_as_f32) {
                    self.font_size = size;
                }
            }
            "Tc" => {
                if let Some(v) = op.operands.first().and_then(object_as_f32) {
                    self.char_spacing = v;
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first().and_then(object_as_f32) {
                    self.word_spacing = v;
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first().and_then(object_as_f32) {
                    self.horizontal_scaling = v / 100.0;
                }
            }
            "rg" => {
                if let (Some(r), Some(g), Some(b)) = (
                    op.operands.first().and_then(object_as_f32),
                    op.operands.get(1).and_then(object_as_f32),
                    op.operands.get(2).and_then(object_as_f32),
                ) {
                    self.current.fill_rgb = (r, g, b);
                }
            }
            "g" => {
                if let Some(v) = op.operands.first().and_then(object_as_f32) {
                    self.current.fill_rgb = (v, v, v);
                }
            }
            "k" => {
                if let (Some(c), Some(m), Some(y), Some(k)) = (
                    op.operands.first().and_then(object_as_f32),
                    op.operands.get(1).and_then(object_as_f32),
                    op.operands.get(2).and_then(object_as_f32),
                    op.operands.get(3).and_then(object_as_f32),
                ) {
                    self.current.fill_rgb = (
                        (1.0 - c) * (1.0 - k),
                        (1.0 - m) * (1.0 - k),
                        (1.0 - y) * (1.0 - k),
                    );
                }
            }
            "Tm" => {
                if let Some(m) = matrix_from_operands(&op.operands) {
                    self.text_matrix = m;
                    self.text_line_matrix = m;
                }
            }
            "Td" | "TD" => {
                if let (Some(tx), Some(ty)) = (
                    op.operands.first().and_then(object_as_f32),
                    op.operands.get(1).and_then(object_as_f32),
                ) {
                    self.move_text(tx, ty);
                    if op.operator == "TD" {
                        self.text_leading = -ty;
                    }
                }
            }
            "TL" => {
                if let Some(leading) = op.operands.first().and_then(object_as_f32) {
                    self.text_leading = leading;
                }
            }
            "T*" => self.move_text(0.0, -self.text_leading),
            _ => {}
        }
    }

    fn prepare_text_show_op(&mut self, op: &Operation) {
        if op.operator == "'" {
            self.move_text(0.0, -self.text_leading);
        } else if op.operator == "\"" {
            if let Some(word_spacing) = op.operands.first().and_then(object_as_f32) {
                self.word_spacing = word_spacing;
            }
            if let Some(char_spacing) = op.operands.get(1).and_then(object_as_f32) {
                self.char_spacing = char_spacing;
            }
            self.move_text(0.0, -self.text_leading);
        }
    }

    fn advance_text(&mut self, tx: f32) {
        self.text_matrix = Matrix::translate(tx, 0.0).mul(self.text_matrix);
    }

    /// Snapshot taken immediately before advancing the text cursor for a
    /// text-show op. `origin` is the user-space baseline-leading-edge of the
    /// glyphs about to be drawn; `combined` is the full `text_matrix × CTM`
    /// at that point.
    ///
    /// Calling this one method replaces the three-step
    /// `prepare_text_show_op` → query state → `advance_text` protocol so the
    /// sequencing can't be misordered.
    pub fn process_text_show(
        &mut self,
        op: &Operation,
        font_advances: &FontAdvanceMap,
    ) -> ShowSnapshot {
        self.prepare_text_show_op(op);
        let advance = text_show_advance(op, self, font_advances);
        let snapshot = ShowSnapshot {
            origin: self.current_text_origin(),
            combined: self.combined_text_matrix(),
            advance,
        };
        self.advance_text(advance);
        snapshot
    }

    pub fn current_text_origin(&self) -> (f32, f32) {
        let (tx, ty) = self.text_matrix.transform_point(0.0, 0.0);
        self.current.ctm.transform_point(tx, ty)
    }

    pub fn combined_text_matrix(&self) -> Matrix {
        self.text_matrix.mul(self.current.ctm)
    }

    pub fn current_ctm(&self) -> Matrix {
        self.current.ctm
    }

    pub fn font_resource(&self) -> &Option<Vec<u8>> {
        &self.current.font_resource
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn horizontal_scaling(&self) -> f32 {
        self.horizontal_scaling
    }

    pub fn char_spacing(&self) -> f32 {
        self.char_spacing
    }

    pub fn word_spacing(&self) -> f32 {
        self.word_spacing
    }

    pub fn fill_rgb(&self) -> (f32, f32, f32) {
        self.current.fill_rgb
    }

    fn move_text(&mut self, tx: f32, ty: f32) {
        let new_lm = Matrix::translate(tx, ty).mul(self.text_line_matrix);
        self.text_line_matrix = new_lm;
        self.text_matrix = new_lm;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ShowSnapshot {
    pub origin: (f32, f32),
    pub combined: Matrix,
    pub advance: f32,
}

pub fn is_text_show_operator(operator: &str) -> bool {
    matches!(operator, "Tj" | "TJ" | "'" | "\"")
}

fn text_show_advance(op: &Operation, state: &ContentState, font_advances: &FontAdvanceMap) -> f32 {
    let font = font_advances.get(state.font_resource().as_ref());
    let mut text_advance = 0.0f32;
    let add_string = |bytes: &[u8]| {
        let (width_1000, glyphs, spaces) = font.string_width_1000(bytes);
        width_1000 * state.font_size / 1000.0
            + state.char_spacing * glyphs as f32
            + state.word_spacing * spaces as f32
    };

    match op.operator.as_str() {
        "Tj" | "'" => {
            if let Some(bytes) = op.operands.last().and_then(|o| o.as_str().ok()) {
                text_advance += add_string(bytes);
            }
        }
        "\"" => {
            if let Some(bytes) = op.operands.get(2).and_then(|o| o.as_str().ok()) {
                text_advance += add_string(bytes);
            }
        }
        "TJ" => {
            if let Some(items) = op.operands.first().and_then(|o| o.as_array().ok()) {
                for item in items {
                    match item {
                        Object::String(bytes, _) => text_advance += add_string(bytes),
                        Object::Integer(_) | Object::Real(_) => {
                            if let Some(adjustment) = object_as_f32(item) {
                                text_advance -= adjustment * state.font_size / 1000.0;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }

    text_advance * state.horizontal_scaling
}

/// Follow one level of indirection if `obj` is a `Reference`, returning a
/// borrow of the resulting array. PDF font dictionaries routinely store
/// `/Widths` as an indirect array (`/Widths 123 0 R`) — match-only-on-
/// `Object::Array` callers would silently see them as missing.
fn resolve_array<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a [Object]> {
    match obj {
        Object::Array(arr) => Some(arr.as_slice()),
        Object::Reference(id) => doc
            .get_object(*id)
            .ok()?
            .as_array()
            .ok()
            .map(|v| v.as_slice()),
        _ => None,
    }
}

pub fn object_as_f32(obj: &Object) -> Option<f32> {
    match obj {
        Object::Integer(i) => Some(*i as f32),
        Object::Real(r) => Some(*r),
        _ => None,
    }
}

pub fn matrix_from_operands(operands: &[Object]) -> Option<Matrix> {
    if operands.len() < 6 {
        return None;
    }
    Some(Matrix {
        a: object_as_f32(&operands[0])?,
        b: object_as_f32(&operands[1])?,
        c: object_as_f32(&operands[2])?,
        d: object_as_f32(&operands[3])?,
        e: object_as_f32(&operands[4])?,
        f: object_as_f32(&operands[5])?,
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FontStyleFlags {
    pub bold: bool,
    pub italic: bool,
    pub monospace: bool,
}

/// Per-span subset of [`FontStyleFlags`] — what `TextStyle` actually carries
/// across translation. Used as a hashmap key when grouping by intra-block
/// style variant (monospace stays fixed for the whole block).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct BoldItalic {
    pub bold: bool,
    pub italic: bool,
}

/// Read style flags from `/FontDescriptor /Flags`, OR'd with BaseFont-name
/// pattern matching for producers that omit or under-set flags.
pub fn font_flags(doc: &Document, font_dict: &Dictionary) -> FontStyleFlags {
    let mut flags_int: Option<i64> = None;
    if let Ok(descriptor_ref) = font_dict.get(b"FontDescriptor") {
        let descriptor = match descriptor_ref {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        };
        if let Some(d) = descriptor {
            flags_int = d.get(b"Flags").ok().and_then(|o| o.as_i64().ok());
        }
    }

    let monospace_flag = flags_int.map(|f| f & (1 << 0) != 0).unwrap_or(false);
    let italic_flag = flags_int.map(|f| f & (1 << 6) != 0).unwrap_or(false);
    let bold_flag = flags_int.map(|f| f & (1 << 18) != 0).unwrap_or(false);

    let base_font = font_dict
        .get(b"BaseFont")
        .ok()
        .and_then(|o| o.as_name().ok())
        .unwrap_or(b"");
    let from_name = detect_from_name(base_font);

    FontStyleFlags {
        bold: bold_flag || from_name.bold,
        italic: italic_flag || from_name.italic,
        monospace: monospace_flag || from_name.monospace,
    }
}

pub fn detect_from_name(base_font: &[u8]) -> FontStyleFlags {
    let name = match base_font.iter().position(|&b| b == b'+') {
        Some(idx) if idx == 6 => &base_font[idx + 1..],
        _ => base_font,
    };
    let lower: Vec<u8> = name.iter().map(|b| b.to_ascii_lowercase()).collect();
    let lower_str = std::str::from_utf8(&lower).unwrap_or("");
    let bold = ["bold", "heavy", "black", "semibold", "demibold"]
        .iter()
        .any(|kw| lower_str.contains(kw));
    let italic = lower_str.contains("italic") || lower_str.contains("oblique");
    let monospace = [
        "courier",
        "mono",
        "consolas",
        "menlo",
        "monaco",
        "inconsolata",
        "sourcecodepro",
        "firacode",
        "jetbrainsmono",
        "robotomono",
        "hack",
        "fixedsys",
        "lucidaconsole",
    ]
    .iter()
    .any(|kw| lower_str.contains(kw));
    FontStyleFlags {
        bold,
        italic,
        monospace,
    }
}

/// Build a PDF content stream from typed operator calls. Mirrors
/// [`ContentState`] (the read-side interpreter): callers stay in PDF
/// vocabulary instead of formatting raw bytes inline with layout math.
#[derive(Debug, Default)]
pub struct ContentStreamBuilder {
    out: Vec<u8>,
}

impl ContentStreamBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save_state(&mut self) {
        self.out.extend_from_slice(b"q\n");
    }

    pub fn restore_state(&mut self) {
        self.out.extend_from_slice(b"Q\n");
    }

    pub fn begin_text(&mut self) {
        self.out.extend_from_slice(b"BT\n");
    }

    pub fn end_text(&mut self) {
        self.out.extend_from_slice(b"ET\n\n");
    }

    pub fn set_fill_rgb(&mut self, r: f32, g: f32, b: f32) {
        let _ = writeln!(self.out, "{r:.3} {g:.3} {b:.3} rg");
    }

    pub fn set_font(&mut self, resource_name: &[u8], size: f32) {
        self.out.push(b'/');
        self.out.extend_from_slice(resource_name);
        let _ = writeln!(self.out, " {size:.2} Tf");
    }

    pub fn set_text_matrix(&mut self, m: Matrix) {
        let _ = writeln!(
            self.out,
            "{:.4} {:.4} {:.4} {:.4} {:.2} {:.2} Tm",
            m.a, m.b, m.c, m.d, m.e, m.f
        );
    }

    pub fn set_char_spacing(&mut self, spacing: f32) {
        let _ = writeln!(self.out, "{spacing:.4} Tc");
    }

    pub fn set_word_spacing(&mut self, spacing: f32) {
        let _ = writeln!(self.out, "{spacing:.4} Tw");
    }

    pub fn set_horizontal_scaling(&mut self, scaling: f32) {
        let _ = writeln!(self.out, "{:.4} Tz", scaling * 100.0);
    }

    pub fn push_operation(&mut self, op: &Operation) {
        if let Ok(bytes) = (Content {
            operations: vec![op.clone()],
        })
        .encode()
        {
            self.out.extend_from_slice(&bytes);
            if !self.out.ends_with(b"\n") {
                self.out.push(b'\n');
            }
        }
    }

    /// `<HHHH...>` Tj — draws hex-encoded glyph IDs (used with embedded
    /// Identity-H CID fonts).
    pub fn show_hex_gids(&mut self, gids: impl IntoIterator<Item = u16>) {
        self.out.push(b'<');
        for gid in gids {
            let _ = write!(self.out, "{gid:04X}");
        }
        self.out.extend_from_slice(b"> Tj\n");
    }

    /// `(...)` Tj — draws a literal-string glyph stream encoded as WinAnsi
    /// (CP1252). Codepoints with no WinAnsi mapping become `?`.
    pub fn show_winansi(&mut self, text: &str) {
        self.out.push(b'(');
        for c in text.chars() {
            let byte = unicode_to_winansi(c);
            match byte {
                b'(' | b')' | b'\\' => {
                    self.out.push(b'\\');
                    self.out.push(byte);
                }
                b'\n' => self.out.extend_from_slice(b"\\n"),
                b'\r' => self.out.extend_from_slice(b"\\r"),
                b'\t' => self.out.extend_from_slice(b"\\t"),
                _ => self.out.push(byte),
            }
        }
        self.out.extend_from_slice(b") Tj\n");
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

/// Map a Unicode codepoint to its WinAnsi (CP1252) byte, or `b'?'` if there
/// is no WinAnsi codepoint for it.
pub fn unicode_to_winansi(c: char) -> u8 {
    match c as u32 {
        0x00..=0x7F => c as u8,
        0x20AC => 0x80, // €
        0x201A => 0x82,
        0x0192 => 0x83,
        0x201E => 0x84,
        0x2026 => 0x85,
        0x2020 => 0x86,
        0x2021 => 0x87,
        0x02C6 => 0x88,
        0x2030 => 0x89,
        0x0160 => 0x8A,
        0x2039 => 0x8B,
        0x0152 => 0x8C,
        0x017D => 0x8E,
        0x2018 => 0x91,
        0x2019 => 0x92,
        0x201C => 0x93,
        0x201D => 0x94,
        0x2022 => 0x95,
        0x2013 => 0x96,
        0x2014 => 0x97,
        0x02DC => 0x98,
        0x2122 => 0x99,
        0x0161 => 0x9A,
        0x203A => 0x9B,
        0x0153 => 0x9C,
        0x017E => 0x9E,
        0x0178 => 0x9F,
        0xA0..=0xFF => c as u8,
        _ => b'?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_inverse_of_rotation() {
        let m = Matrix {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 595.0,
            f: 0.0,
        };
        let inv = m.inverse().unwrap();
        let (ux, uy) = m.transform_point(20.0, 484.83);
        let (lx, ly) = inv.transform_point(ux, uy);
        assert!((lx - 20.0).abs() < 1e-3);
        assert!((ly - 484.83).abs() < 1e-3);
    }

    #[test]
    fn detects_style_from_basefont_name() {
        let flags = |bold, italic, monospace| FontStyleFlags {
            bold,
            italic,
            monospace,
        };
        assert_eq!(detect_from_name(b"ArialMT"), flags(false, false, false));
        assert_eq!(detect_from_name(b"Arial-BoldMT"), flags(true, false, false));
        assert_eq!(
            detect_from_name(b"Arial-ItalicMT"),
            flags(false, true, false)
        );
        assert_eq!(
            detect_from_name(b"Arial-BoldItalicMT"),
            flags(true, true, false)
        );
        assert_eq!(detect_from_name(b"Helvetica"), flags(false, false, false));
        assert_eq!(
            detect_from_name(b"Helvetica-Bold"),
            flags(true, false, false)
        );
        assert_eq!(
            detect_from_name(b"Helvetica-Oblique"),
            flags(false, true, false)
        );
        assert_eq!(
            detect_from_name(b"Helvetica-BoldOblique"),
            flags(true, true, false)
        );
        assert_eq!(
            detect_from_name(b"AAAAAA+Helvetica-Bold"),
            flags(true, false, false)
        );
        assert_eq!(detect_from_name(b"Courier"), flags(false, false, true));
        assert_eq!(detect_from_name(b"Courier-Bold"), flags(true, false, true));
        assert_eq!(
            detect_from_name(b"Courier-BoldOblique"),
            flags(true, true, true)
        );
        assert_eq!(detect_from_name(b"Consolas"), flags(false, false, true));
        assert_eq!(
            detect_from_name(b"JetBrainsMono-Regular"),
            flags(false, false, true)
        );
        assert_eq!(
            detect_from_name(b"SourceCodePro-Regular"),
            flags(false, false, true)
        );
    }

    #[test]
    fn reads_inherited_rotation_and_media_box() {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Pages".to_vec()));
            d.set("Rotate", Object::Integer(90));
            d.set(
                "MediaBox",
                Object::Array(vec![
                    Object::Integer(0),
                    Object::Integer(0),
                    Object::Integer(200),
                    Object::Integer(400),
                ]),
            );
            d.set("Kids", Object::Array(Vec::new()));
            d.set("Count", Object::Integer(1));
            Object::Dictionary(d)
        });
        let page_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            d.set("Parent", Object::Reference(pages_id));
            Object::Dictionary(d)
        });

        let geom = PageGeometry::read(&doc, page_id, None);
        assert_eq!(geom.rotate, 90);
        assert_eq!(geom.user_w, 200.0);
        assert_eq!(geom.user_h, 400.0);
    }
}
