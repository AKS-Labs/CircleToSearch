//! Parser-agnostic core of the DOM translation pipeline.
//!
//! Both the HTML pipeline (html5ever, [`crate::html_translate`]) and the EPUB
//! pipeline (xml5ever, [`crate::epub`]) build a `markup5ever_rcdom::RcDom` and
//! then share this code: group text leaves into "scopes" by their nearest
//! non-inline ancestor ([`collect_and_index`]), and — once those scope texts
//! have been translated with token alignments — write the translations back onto
//! the same DOM leaves in place ([`apply_indexed`]). Nothing here parses or
//! serialises; the caller owns the parser, so this module depends only on the
//! shared `markup5ever` DOM types, never on html5ever or xml5ever.
//!
//! Inline elements (`<em>`, `<b>`, `<a>`, …) do not break a scope, so the model
//! gets full sentence context within a paragraph; block elements split scopes.
//! Tags this code doesn't recognise are treated as block-level, which is why the
//! same grouping degrades correctly for non-HTML XML such as the EPUB NCX (every
//! `<text>` becomes its own scope).

use markup5ever::interface::Attribute;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use crate::translate::{TokenAlignment, TranslationWithAlignment};

/// Elements whose subtree must be left verbatim — neither extracted as
/// translation source nor mutated. Two flavours of reason:
///
/// 1. Non-prose: `script`, `style`, `noscript`, `template`, `iframe`,
///    `textarea`, `svg`, `canvas`, `object`, `embed`. Translating their
///    contents is meaningless or actively breaks the page.
///
/// 2. Author-as-source-text intent: `code`, `pre`, `kbd`, `samp`, `var`,
///    `math`. The author marked these because the literal characters
///    matter (a function name, a key sequence, a sample value, an
///    equation). Running them through a translation model corrupts them.
///
/// Skip is a hard scope boundary: we flush the in-progress scope at entry,
/// don't walk into the subtree (so its text leaves never become
/// translation sources), and the next sibling starts a fresh scope. The
/// DOM tree itself stays untouched because we only mutate text leaves we
/// extracted, and we extracted none from inside.
fn is_skipped_subtree(local_name: &str) -> bool {
    matches!(
        local_name,
        "script"
            | "style"
            | "noscript"
            | "template"
            | "iframe"
            | "textarea"
            | "svg"
            | "canvas"
            | "object"
            | "embed"
            | "code"
            | "pre"
            | "kbd"
            | "samp"
            | "var"
            | "math"
    )
}

/// Author-level "do not translate" signals on an arbitrary element:
/// `translate="no"`, `class="notranslate"` (Google's de-facto convention),
/// and `contenteditable` (anything the user types into is not ours to
/// rewrite). Same skip semantics as `is_skipped_subtree`.
fn has_skip_attr(attrs: &[Attribute]) -> bool {
    for a in attrs {
        let name: &str = &a.name.local;
        let value: &str = &a.value;
        if name == "translate" && value.eq_ignore_ascii_case("no") {
            return true;
        }
        if name == "contenteditable" {
            return true;
        }
        if name == "class" && value.split_ascii_whitespace().any(|c| c == "notranslate") {
            return true;
        }
    }
    false
}

/// Inline elements do not break a translation scope. Anything not in this
/// set is treated as block-level (its boundaries split scopes). The list
/// matches slimt's historical `inline_tags` set so behaviour with the new
/// pipeline stays close to the old C++ HTML mode.
fn is_inline(local_name: &str) -> bool {
    matches!(
        local_name,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "img"
            | "ins"
            | "kbd"
            | "label"
            | "mark"
            | "math"
            | "output"
            | "q"
            | "ruby"
            | "rb"
            | "rp"
            | "rt"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "wbr"
    )
}

/// One contiguous run of text leaves whose nearest non-inline ancestor is the
/// same. The flat `text` is what we send to slimt; `leaf_char_ends` lets us
/// look up which leaf any source-text character belongs to.
pub struct Scope {
    leaves: Vec<Handle>,
    text: String,
    /// `leaf_char_ends[i]` is the cumulative char count *after* leaf `i`,
    /// so leaf `i` covers char range `leaf_char_ends[i-1]..leaf_char_ends[i]`
    /// (with an implicit 0 before index 0).
    leaf_char_ends: Vec<usize>,
}

/// Collect translation scopes from an already-parsed `RcDom` (built by either
/// the html5ever fragment parser or the xml5ever document parser) and append
/// each non-empty scope's text to `all_texts`, returning the scopes alongside
/// their index into that flat list (`None` for whitespace-only scopes left
/// untouched).
pub fn collect_and_index(
    dom: &RcDom,
    all_texts: &mut Vec<String>,
) -> (Vec<Scope>, Vec<Option<usize>>) {
    let scopes = collect_scopes(dom);
    let mut translation_idx = Vec::with_capacity(scopes.len());
    for scope in &scopes {
        if scope.text.trim().is_empty() {
            translation_idx.push(None);
        } else {
            translation_idx.push(Some(all_texts.len()));
            all_texts.push(scope.text.clone());
        }
    }
    (scopes, translation_idx)
}

/// Write the translated text + alignments back onto each scope's DOM leaves in
/// place. The DOM is mutated; the caller serialises it afterwards with the
/// matching serialiser.
pub fn apply_indexed(
    scopes: &[Scope],
    translation_idx: &[Option<usize>],
    translations: &[TranslationWithAlignment],
) {
    for (scope, idx) in scopes.iter().zip(translation_idx.iter()) {
        let Some(idx) = idx else { continue };
        let translation = &translations[*idx];
        apply_scope(scope, &translation.translated_text, &translation.alignments);
    }
}

/// Collect the text of every element whose local name is in `locals` as its own
/// scope, ignoring all other text in the tree. Used for the EPUB OPF package
/// metadata, where only specific Dublin Core fields (e.g. `dc:title`) should be
/// translated while the identifier, language code, dates and manifest are left
/// untouched — the opposite default from [`collect_and_index`], which takes all
/// prose text.
pub fn collect_named_elements(
    dom: &RcDom,
    locals: &[&str],
    all_texts: &mut Vec<String>,
) -> (Vec<Scope>, Vec<Option<usize>>) {
    let mut scopes = Vec::new();
    collect_named(&dom.document, locals, &mut scopes);
    let mut translation_idx = Vec::with_capacity(scopes.len());
    for scope in &scopes {
        if scope.text.trim().is_empty() {
            translation_idx.push(None);
        } else {
            translation_idx.push(Some(all_texts.len()));
            all_texts.push(scope.text.clone());
        }
    }
    (scopes, translation_idx)
}

fn collect_named(node: &Handle, locals: &[&str], scopes: &mut Vec<Scope>) {
    if let NodeData::Element { name, .. } = &node.data {
        let local: &str = &name.local;
        if locals.contains(&local) {
            // Group this element's own text leaves into a scope, then stop —
            // we don't translate nested elements of a matched field.
            let mut collector = ScopeCollector {
                scopes: Vec::new(),
                current: Vec::new(),
            };
            for child in node.children.borrow().iter() {
                collector.walk(child);
            }
            collector.flush();
            scopes.append(&mut collector.scopes);
            return;
        }
    }
    for child in node.children.borrow().iter() {
        collect_named(child, locals, scopes);
    }
}

fn collect_scopes(dom: &RcDom) -> Vec<Scope> {
    let mut state = ScopeCollector {
        scopes: Vec::new(),
        current: Vec::new(),
    };
    // A fragment parse wraps the input in an implicit context element under
    // `dom.document`; a document parse puts the root element (and doctype)
    // there. Either way we walk `dom.document`'s children and treat the
    // document root as a scope-breaking boundary, so a leading text leaf still
    // lives in its own scope cleanly.
    let document_children = dom.document.children.borrow();
    for child in document_children.iter() {
        state.walk(child);
    }
    state.flush();
    state.scopes
}

struct ScopeCollector {
    scopes: Vec<Scope>,
    current: Vec<Handle>,
}

impl ScopeCollector {
    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let leaves = std::mem::take(&mut self.current);
        let mut text = String::new();
        let mut leaf_char_ends = Vec::with_capacity(leaves.len());
        let mut total = 0usize;
        let mut prev_text_end_char: Option<char> = None;
        for leaf in &leaves {
            if let NodeData::Text { contents } = &leaf.data {
                let s = contents.borrow();
                let s_str: &str = &s;
                // Inline tags between leaves (e.g. `cat<br>eats`) leave no
                // whitespace in either neighbouring leaf, so naive concat
                // would feed `cateats` to the model. If neither side already
                // has whitespace at the boundary, inject a single space —
                // and grow this leaf's char range to include it so the
                // synthetic char is owned by the trailing leaf.
                let starts_ws = s_str
                    .chars()
                    .next()
                    .map(char::is_whitespace)
                    .unwrap_or(true);
                let prev_ends_ws = prev_text_end_char.map(char::is_whitespace).unwrap_or(true);
                if !prev_ends_ws && !starts_ws {
                    text.push(' ');
                    total += 1;
                }
                text.push_str(s_str);
                total += s_str.chars().count();
                prev_text_end_char = s_str.chars().next_back().or(prev_text_end_char);
            }
            leaf_char_ends.push(total);
        }
        self.scopes.push(Scope {
            leaves,
            text,
            leaf_char_ends,
        });
    }

    fn walk(&mut self, node: &Handle) {
        match &node.data {
            NodeData::Text { .. } => {
                self.current.push(node.clone());
            }
            NodeData::Element { name, attrs, .. } => {
                let local: &str = &name.local;
                let inline = is_inline(local);
                let skipped = is_skipped_subtree(local) || has_skip_attr(&attrs.borrow());
                if !inline {
                    self.flush();
                }
                if !skipped {
                    let children = node.children.borrow().clone();
                    for child in &children {
                        self.walk(child);
                    }
                }
                if !inline {
                    self.flush();
                }
            }
            // Document, Doctype, Comment, ProcessingInstruction: walk children
            // (Document only) but otherwise contribute no text.
            NodeData::Document => {
                let children = node.children.borrow().clone();
                for child in &children {
                    self.walk(child);
                }
            }
            _ => {}
        }
    }
}

fn apply_scope(scope: &Scope, translated: &str, alignments: &[TokenAlignment]) {
    if scope.leaves.is_empty() {
        return;
    }
    let tgt_chars: Vec<char> = translated.chars().collect();
    let tgt_len = tgt_chars.len();
    let src_len = scope.text.chars().count();

    let mut per_char_leaf: Vec<Option<usize>> = vec![None; tgt_len];

    // Walk alignments in target-text order, alternating between gap regions
    // (inter-token whitespace, untranslated runs) and aligned regions. Each
    // gap maps a `[prev_tgt_end, this_tgt_begin)` slice of target text to a
    // `[prev_src_end, this_src_begin)` slice of source text — we use the
    // *source* slice's leaf membership to assign whitespace correctly. That
    // matters for inline tags like `<b>my name</b>`: the trailing space
    // belongs to whichever leaf held the corresponding space in the source.
    let mut sorted_aligns: Vec<&TokenAlignment> = alignments.iter().collect();
    sorted_aligns.sort_by_key(|a| (a.tgt_begin, a.tgt_end));

    let mut prev_tgt_end = 0usize;
    let mut prev_src_end = 0usize;
    for align in &sorted_aligns {
        let tgt_b = (align.tgt_begin as usize).min(tgt_len);
        let tgt_e = (align.tgt_end as usize).min(tgt_len);
        let src_b = (align.src_begin as usize).min(src_len);
        let src_e = (align.src_end as usize).min(src_len);

        if tgt_b > prev_tgt_end {
            fill_gap(
                &mut per_char_leaf,
                scope,
                prev_tgt_end,
                tgt_b,
                prev_src_end,
                src_b,
            );
        }
        let mid = if src_e > src_b {
            (src_b + src_e) / 2
        } else {
            src_b
        };
        let leaf_idx = leaf_for_src_char(scope, mid);
        for slot in &mut per_char_leaf[tgt_b..tgt_e] {
            *slot = Some(leaf_idx);
        }
        prev_tgt_end = tgt_e.max(prev_tgt_end);
        prev_src_end = src_e.max(prev_src_end);
    }
    if prev_tgt_end < tgt_len {
        fill_gap(
            &mut per_char_leaf,
            scope,
            prev_tgt_end,
            tgt_len,
            prev_src_end,
            src_len,
        );
    }

    // Group target chars into per-leaf strings, in target text order. A leaf
    // with no chars assigned ends up empty — its element stays in the DOM
    // but its text content is cleared (e.g. a word the model dropped).
    let mut per_leaf_text: Vec<String> = vec![String::new(); scope.leaves.len()];
    for (c_idx, ch) in tgt_chars.iter().enumerate() {
        let leaf_idx = per_char_leaf[c_idx].unwrap_or(0);
        per_leaf_text[leaf_idx].push(*ch);
    }

    // Write back into the DOM. Each leaf's `contents` is a `RefCell<StrTendril>`
    // we can mutate in place — the DOM tree itself is unchanged.
    for (leaf, text) in scope.leaves.iter().zip(per_leaf_text) {
        if let NodeData::Text { contents } = &leaf.data {
            *contents.borrow_mut() = text.into();
        }
    }
}

fn fill_gap(
    per_char_leaf: &mut [Option<usize>],
    scope: &Scope,
    tgt_begin: usize,
    tgt_end: usize,
    src_begin: usize,
    src_end: usize,
) {
    let tgt_len = tgt_end - tgt_begin;
    if tgt_len == 0 {
        return;
    }
    let src_len = src_end.saturating_sub(src_begin);
    if src_len == 0 {
        // Empty source gap: collapse to whichever leaf the source position
        // sits in (or the previous leaf if we're at the very end).
        let probe = src_begin.min(scope.text.chars().count().saturating_sub(1));
        let leaf = leaf_for_src_char(scope, probe);
        for slot in &mut per_char_leaf[tgt_begin..tgt_end] {
            *slot = Some(leaf);
        }
        return;
    }
    // Distribute target gap chars proportionally across the source gap so a
    // wider source gap (multiple leaves) splits naturally between them.
    for c in 0..tgt_len {
        let frac = (c as f64 + 0.5) / tgt_len as f64;
        let src_pos = src_begin + ((src_len as f64) * frac).floor() as usize;
        let src_pos = src_pos.min(src_end.saturating_sub(1));
        per_char_leaf[tgt_begin + c] = Some(leaf_for_src_char(scope, src_pos));
    }
}

fn leaf_for_src_char(scope: &Scope, src_char: usize) -> usize {
    // leaf_char_ends is non-decreasing; return the first leaf whose end is
    // strictly greater than `src_char`. If src_char is past the end (e.g. an
    // alignment edge), pin to the last leaf.
    for (i, &end) in scope.leaf_char_ends.iter().enumerate() {
        if src_char < end {
            return i;
        }
    }
    scope.leaves.len().saturating_sub(1)
}
