//! HTML translation pipeline: parse on Rust side with html5ever, translate
//! plain text + alignment via slimt, write translated content back into the
//! same DOM nodes it came from, then serialise.
//!
//! The DOM tree is sacred: structural elements (`<p>`, `<a>`, `<button>`, …),
//! their attributes (`href`, `data-*`, `class`), and document order are never
//! mutated. Only the contents of each text leaf are replaced.
//!
//! The scope grouping and alignment reassembly are parser-agnostic and live in
//! [`crate::dom_translate`]; this module only owns the html5ever fragment parse
//! and serialise around them.

use html5ever::driver::ParseOpts;
use html5ever::serialize::{SerializeOpts, TraversalScope, serialize};
use html5ever::tendril::TendrilSink;
use html5ever::{QualName, local_name, ns, parse_fragment};
use markup5ever_rcdom::{RcDom, SerializableHandle};

use crate::dom_translate::{Scope, apply_indexed, collect_and_index};
use crate::translate::TranslationWithAlignment;

struct ParsedFragment {
    dom: RcDom,
    scopes: Vec<Scope>,
    /// For each scope, its index in the flat `scope_texts` list passed to
    /// the translator. `None` means the scope is empty/whitespace-only and
    /// should be left untouched.
    translation_idx: Vec<Option<usize>>,
}

/// Result of `prepare` — owns the parsed DOMs and the flat list of scope
/// texts that the caller must translate (with alignment) before calling
/// `finish`.
pub struct PreparedHtml {
    fragments: Vec<ParsedFragment>,
}

/// Parse fragments and extract the per-scope plain text the caller should
/// pass to the translator. Returns the prepared state plus the texts in
/// flat order; the caller batches them in one `translate_with_alignment`
/// call and feeds the responses back via `finish`.
pub fn prepare(fragments: &[String]) -> (PreparedHtml, Vec<String>) {
    let mut all_texts = Vec::new();
    let mut parsed = Vec::with_capacity(fragments.len());
    for fragment in fragments {
        let dom = parse_fragment_dom(fragment);
        let (scopes, translation_idx) = collect_and_index(&dom, &mut all_texts);
        parsed.push(ParsedFragment {
            dom,
            scopes,
            translation_idx,
        });
    }
    (PreparedHtml { fragments: parsed }, all_texts)
}

/// Convenience: parse, translate via the supplied closure, reassemble. The
/// closure receives the per-scope flat texts and must return one
/// `TranslationWithAlignment` per input in the same order.
pub fn translate_html_with<F, E>(fragments: &[String], translate: F) -> Result<Vec<String>, E>
where
    F: FnOnce(&[String]) -> Result<Vec<TranslationWithAlignment>, E>,
{
    let (prepared, scope_texts) = prepare(fragments);
    let translations = if scope_texts.is_empty() {
        Vec::new()
    } else {
        translate(&scope_texts)?
    };
    Ok(finish(prepared, &translations))
}

/// Apply translation results back onto each parsed DOM and serialise to
/// HTML strings. `translations.len()` must equal the count returned by
/// `prepare` (the second tuple element).
pub fn finish(prepared: PreparedHtml, translations: &[TranslationWithAlignment]) -> Vec<String> {
    let mut out = Vec::with_capacity(prepared.fragments.len());
    for fragment in prepared.fragments {
        apply_indexed(&fragment.scopes, &fragment.translation_idx, translations);
        out.push(serialize_fragment_dom(&fragment.dom));
    }
    out
}

fn parse_fragment_dom(input: &str) -> RcDom {
    parse_fragment(
        RcDom::default(),
        ParseOpts::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        Vec::new(),
        false,
    )
    .one(input)
}

fn serialize_fragment_dom(dom: &RcDom) -> String {
    // `parse_fragment` puts everything under one synthetic context element
    // (a `<html>`-named wrapper) sitting directly under `dom.document`. We
    // serialise that wrapper with `ChildrenOnly` so the wrapper itself is
    // not emitted — only the user's original markup, with translated text.
    let document_children = dom.document.children.borrow();
    let Some(root) = document_children.first().cloned() else {
        return String::new();
    };
    drop(document_children);

    let serializable = SerializableHandle::from(root);
    let mut buf: Vec<u8> = Vec::new();
    serialize(
        &mut buf,
        &serializable,
        SerializeOpts {
            traversal_scope: TraversalScope::ChildrenOnly(None),
            ..Default::default()
        },
    )
    .expect("html serialize must succeed for an in-memory tree");
    String::from_utf8(buf).expect("html5ever emits UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::{TokenAlignment, TranslationWithAlignment};

    /// Build an alignment that says "source char range [sb,se) maps to
    /// target char range [tb,te)". Used by the synthetic-translation tests
    /// below to exercise the reassembly logic without loading a model.
    fn align(sb: u64, se: u64, tb: u64, te: u64) -> TokenAlignment {
        TokenAlignment {
            src_begin: sb,
            src_end: se,
            tgt_begin: tb,
            tgt_end: te,
        }
    }

    #[test]
    fn flat_text_no_tags_round_trips() {
        let (prepared, texts) = prepare(&["Hello world".to_string()]);
        assert_eq!(texts, vec!["Hello world".to_string()]);
        let translations = vec![TranslationWithAlignment {
            source_text: "Hello world".into(),
            translated_text: "Hola mundo".into(),
            alignments: vec![align(0, 5, 0, 4), align(6, 11, 5, 10)],
        }];
        let out = finish(prepared, &translations);
        assert_eq!(out, vec!["Hola mundo".to_string()]);
    }

    #[test]
    fn inline_tag_keeps_translation_in_one_scope() {
        // <p>hi <b>my name</b> is david</p> — three text leaves but one scope
        // because <b> is inline. The model receives "hi my name is david" as
        // one string and the alignment routes each translated word back to
        // the leaf its source word belonged to.
        let input = "<p>hi <b>my name</b> is david</p>".to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["hi my name is david".to_string()],
            "inline <b> must not break the scope"
        );

        // Synthetic translation: "hola mi nombre es david". Alignments:
        //   "hola"   ← src "hi"     (leaf 0, chars 0..2)    → tgt 0..4
        //   "mi"     ← src "my"     (leaf 1, chars 3..5)    → tgt 5..7
        //   "nombre" ← src "name"   (leaf 1, chars 6..10)   → tgt 8..14
        //   "es"     ← src "is"     (leaf 2, chars 11..13)  → tgt 15..17
        //   "david"  ← src "david"  (leaf 2, chars 14..19)  → tgt 18..23
        let translations = vec![TranslationWithAlignment {
            source_text: "hi my name is david".into(),
            translated_text: "hola mi nombre es david".into(),
            alignments: vec![
                align(0, 2, 0, 4),
                align(3, 5, 5, 7),
                align(6, 10, 8, 14),
                align(11, 13, 15, 17),
                align(14, 19, 18, 23),
            ],
        }];
        let out = finish(prepared, &translations);
        assert_eq!(out.len(), 1);
        // `<b>` survives, attribute-free elements survive, and the words
        // for "my name" land inside the <b>...</b> the same way "my name"
        // did in the source. (Inter-word whitespace inherits the previous
        // leaf, so the leading "hola " trailing space goes with leaf 0.)
        assert!(out[0].starts_with("<p>hola "), "got: {}", out[0]);
        assert!(
            out[0].contains("<b>mi nombre</b>"),
            "<b> must wrap the translation of 'my name': {}",
            out[0]
        );
        assert!(out[0].ends_with(" es david</p>"), "got: {}", out[0]);
    }

    #[test]
    fn attributes_and_void_tags_are_preserved_verbatim() {
        // href, data-*, class must round-trip exactly. <br> has no text node,
        // so it just stays in place — we never touch structure, only text.
        let input = "<a href=\"https://example.com\" data-id=\"42\" class=\"link\">Click <em>here</em><br>to continue</a>".to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(texts.len(), 1);
        // <a> is inline, <em> is inline, <br> is inline+void → one scope
        // with three text leaves: "Click ", "here", "to continue". The
        // collector injects a synthetic space between "here" and "to" so
        // the model sees a properly tokenisable sentence; the synthetic
        // char is owned by the trailing leaf for alignment purposes.
        assert_eq!(texts[0], "Click here to continue");

        let translations = vec![TranslationWithAlignment {
            source_text: texts[0].clone(),
            translated_text: "Haz clic aquí para continuar".into(),
            alignments: vec![
                align(0, 5, 0, 8),     // "Click" → "Haz clic" → leaf 0
                align(6, 10, 9, 13),   // "here"  → "aquí"     → leaf 1
                align(11, 22, 14, 28), // "to continue" → "para continuar" → leaf 2
            ],
        }];
        let out = finish(prepared, &translations);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].contains("href=\"https://example.com\""),
            "href must be preserved verbatim: {}",
            out[0]
        );
        assert!(
            out[0].contains("data-id=\"42\""),
            "data-id must be preserved verbatim: {}",
            out[0]
        );
        assert!(
            out[0].contains("class=\"link\""),
            "class must be preserved verbatim: {}",
            out[0]
        );
        assert!(
            out[0].contains("<br>"),
            "void <br> tag must be preserved: {}",
            out[0]
        );
        assert!(
            out[0].contains("<em>aquí</em>"),
            "<em> wrapping must be preserved with translated content: {}",
            out[0]
        );
    }

    #[test]
    fn block_level_elements_form_separate_scopes() {
        // Two <p> blocks → two scopes → two translation calls (the model
        // never sees them as one sentence — block boundaries split context,
        // matching how slimt's old HTML mode also inserted \n\n at block
        // boundaries).
        let input = "<div><p>The cat</p><p>eats fish</p></div>".to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(texts, vec!["The cat".to_string(), "eats fish".to_string()]);
    }

    #[test]
    fn malformed_html_does_not_panic() {
        // html5ever recovers from mismatched tags rather than aborting.
        // The XHScanner-based slimt HTML mode used to crash on this; our
        // pipeline must always produce *some* translatable output.
        let input = "<p>open <b>but never closed</p>".to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0], "open but never closed");
        // Empty alignments + identity translation: the parser tolerates the
        // mismatch and the round-trip should at minimum keep the text content.
        let translations = vec![TranslationWithAlignment {
            source_text: texts[0].clone(),
            translated_text: texts[0].clone(),
            alignments: vec![align(0, 21, 0, 21)],
        }];
        let out = finish(prepared, &translations);
        assert!(out[0].contains("open"), "got: {}", out[0]);
        assert!(out[0].contains("but never closed"), "got: {}", out[0]);
    }

    #[test]
    fn empty_and_whitespace_fragments_skip_translation() {
        let (prepared, texts) = prepare(&[
            "".to_string(),
            "   ".to_string(),
            "<p>real content</p>".to_string(),
        ]);
        assert_eq!(
            texts,
            vec!["real content".to_string()],
            "only non-empty scopes should be sent to the translator"
        );
        let translations = vec![TranslationWithAlignment {
            source_text: "real content".into(),
            translated_text: "contenido real".into(),
            alignments: vec![align(0, 12, 0, 14)],
        }];
        let out = finish(prepared, &translations);
        assert_eq!(out.len(), 3);
        assert!(out[2].contains("contenido real"), "got: {}", out[2]);
    }

    #[test]
    fn inline_code_kbd_samp_var_subtrees_are_not_translated() {
        // Inline code-ish elements inside a paragraph used to leak their
        // text into the surrounding scope, so a sentence like
        // "the console.log call" would get its function name machine-
        // translated. The skip keeps those subtrees opaque while the
        // surrounding sentence stays one scope so the model sees the
        // intended phrasing.
        let input =
            r#"<p>Call <code>console.log</code> then press <kbd>Enter</kbd>.</p>"#.to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["Call  then press .".to_string()],
            "inline skipped subtrees keep the surrounding scope intact"
        );
        let translations = vec![TranslationWithAlignment {
            source_text: "Call  then press .".into(),
            translated_text: "Llama luego pulsa.".into(),
            alignments: vec![align(0, 18, 0, 18)],
        }];
        let out = finish(prepared, &translations);
        assert!(
            out[0].contains("<code>console.log</code>"),
            "code text must round-trip verbatim: {}",
            out[0]
        );
        assert!(
            out[0].contains("<kbd>Enter</kbd>"),
            "kbd text must round-trip verbatim: {}",
            out[0]
        );
    }

    #[test]
    fn pre_block_inside_structural_keeps_text_verbatim() {
        // A <pre> preserves whitespace and is by author intent literal.
        // Even when wrapped in a structural element that JS picks as a
        // unit, Rust must not extract the pre's text leaves.
        let input = "<div>before<pre>x = 1\ny = 2</pre>after</div>".to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["before".to_string(), "after".to_string()],
            "pre subtree must not be a translation source"
        );
        let translations = vec![
            TranslationWithAlignment {
                source_text: "before".into(),
                translated_text: "antes".into(),
                alignments: vec![align(0, 6, 0, 5)],
            },
            TranslationWithAlignment {
                source_text: "after".into(),
                translated_text: "después".into(),
                alignments: vec![align(0, 5, 0, 7)],
            },
        ];
        let out = finish(prepared, &translations);
        assert!(
            out[0].contains("<pre>x = 1\ny = 2</pre>"),
            "pre content must round-trip verbatim: {}",
            out[0]
        );
    }

    #[test]
    fn translate_no_attribute_is_honored() {
        // <span translate="no"> is the standards-track way to mark a
        // brand name or term as untranslatable. The span is inline so
        // the surrounding sentence stays one scope.
        let input = r#"<p>Buy a <span translate="no">Acme Widget</span> today</p>"#.to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["Buy a  today".to_string()],
            "translate=no on an inline element keeps scope continuity"
        );
    }

    #[test]
    fn notranslate_class_is_honored() {
        // Google's de-facto convention: class="notranslate" on any
        // element opts that subtree out of translation.
        let input =
            r##"<p>Send to <a class="notranslate" href="#">support@example.com</a> now</p>"##
                .to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["Send to  now".to_string()],
            "class=notranslate on an inline element keeps scope continuity"
        );
    }

    #[test]
    fn contenteditable_subtree_is_skipped() {
        // User-editable regions are not ours to rewrite — translating
        // them would clobber whatever the user typed.
        let input =
            r#"<div>label <span contenteditable="true">user typed text</span></div>"#.to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["label ".to_string()],
            "contenteditable subtree must be skipped"
        );
    }

    #[test]
    fn svg_and_canvas_subtrees_are_skipped() {
        // SVG <text> children would otherwise be extracted; we leave them
        // alone because translating chart labels is rarely what the user
        // wants and we have no way of knowing. SVG is block-level so it
        // does break the surrounding scope.
        let input = r#"<div>chart: <svg><text>2024</text></svg></div>"#.to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["chart: ".to_string()],
            "svg subtree must be skipped"
        );
    }

    #[test]
    fn user_reported_create_extension_stays_literal() {
        // Reproduces the user-reported bug: SQL-like content inside an
        // inline <code> got translated as "CREAR EXTENSIÓN <extension>".
        // The whole <code> subtree must be opaque.
        let input =
            "<p>When a user runs <code>CREATE EXTENSION &lt;extension&gt;</code>, the server will:</p>"
                .to_string();
        let (prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["When a user runs , the server will:".to_string()],
            "code subtree must not contribute to scope text"
        );
        let translations = vec![TranslationWithAlignment {
            source_text: "When a user runs , the server will:".into(),
            translated_text: "Cuando un usuario ejecuta , el servidor:".into(),
            alignments: vec![align(0, 35, 0, 41)],
        }];
        let out = finish(prepared, &translations);
        assert!(
            out[0].contains("<code>CREATE EXTENSION &lt;extension&gt;</code>"),
            "code content must round-trip verbatim: {}",
            out[0]
        );
    }

    #[test]
    fn script_and_style_subtrees_are_skipped() {
        // Defensive: even if a <p> somehow gets a <script> child shipped
        // to us, we must not feed its source code to the model. <script>
        // is block-level (parser hoists it anyway) so it splits scopes.
        let input = r#"<p>hello <script>alert(1)</script> world</p>"#.to_string();
        let (_prepared, texts) = prepare(&[input]);
        assert_eq!(
            texts,
            vec!["hello ".to_string(), " world".to_string()],
            "script subtree must be skipped"
        );
    }
}
