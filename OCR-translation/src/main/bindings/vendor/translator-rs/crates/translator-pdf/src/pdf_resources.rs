//! Page-resource bookkeeping for the PDF writer: install the standard-14
//! fallback fonts, attach embedded fonts, append the overlay content stream,
//! and prune resource entries we own but the surviving content no longer
//! references.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::pdf_content::{UserRect, object_as_f32};
use crate::pdf_font_embed::EmbeddedFont;
use crate::pdf_write::{
    COURIER_BOLD, COURIER_BOLD_OBLIQUE, COURIER_OBLIQUE, COURIER_REGULAR, HELVETICA_BOLD,
    HELVETICA_BOLD_OBLIQUE, HELVETICA_OBLIQUE, HELVETICA_REGULAR, PdfWriteError,
};

pub(crate) fn ensure_fonts_in_page_resources(
    doc: &mut Document,
    page_id: ObjectId,
) -> Result<(), PdfWriteError> {
    // Register Helvetica + Courier variants so we can mimic bold / italic /
    // monospace styles sampled from the original. All eight are PDF
    // standard-14 base fonts; no embedding needed.
    let variants: [(&[u8], &[u8]); 8] = [
        (HELVETICA_REGULAR, b"Helvetica"),
        (HELVETICA_BOLD, b"Helvetica-Bold"),
        (HELVETICA_OBLIQUE, b"Helvetica-Oblique"),
        (HELVETICA_BOLD_OBLIQUE, b"Helvetica-BoldOblique"),
        (COURIER_REGULAR, b"Courier"),
        (COURIER_BOLD, b"Courier-Bold"),
        (COURIER_OBLIQUE, b"Courier-Oblique"),
        (COURIER_BOLD_OBLIQUE, b"Courier-BoldOblique"),
    ];
    let mut new_refs = Vec::with_capacity(variants.len());
    for (resource_name, base_font) in variants {
        let id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Font".to_vec()));
            d.set("Subtype", Object::Name(b"Type1".to_vec()));
            d.set("BaseFont", Object::Name(base_font.to_vec()));
            d.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            Object::Dictionary(d)
        });
        new_refs.push((resource_name, id));
    }

    let font_dict_id = ensure_page_font_dict(doc, page_id)?;
    let font_dict = doc
        .get_object_mut(font_dict_id)
        .and_then(Object::as_dict_mut)?;
    for (resource_name, id) in new_refs {
        font_dict.set(resource_name, Object::Reference(id));
    }
    Ok(())
}

/// Add every [`EmbeddedFont`] used on a page to its `/Resources/Font` dict
/// (deduplicated by resource name).
pub(crate) fn attach_embedded_fonts_to_page(
    doc: &mut Document,
    page_id: ObjectId,
    embeds: &[Option<EmbeddedFont>],
) -> Result<(), PdfWriteError> {
    use std::collections::HashMap;
    let unique: HashMap<Vec<u8>, ObjectId> = embeds
        .iter()
        .filter_map(|e| e.as_ref().map(|e| (e.resource_name.clone(), e.type0_id)))
        .collect();
    if unique.is_empty() {
        return Ok(());
    }
    let font_dict_id = ensure_page_font_dict(doc, page_id)?;
    let font_dict = doc
        .get_object_mut(font_dict_id)
        .and_then(Object::as_dict_mut)?;
    for (name, id) in unique {
        font_dict.set(name, Object::Reference(id));
    }
    Ok(())
}

fn ensure_page_font_dict(doc: &mut Document, page_id: ObjectId) -> Result<ObjectId, PdfWriteError> {
    let resources_id = ensure_inline_resources(doc, page_id)?;
    let existing_font = {
        let resources = doc.get_object(resources_id).and_then(Object::as_dict)?;
        resources.get(b"Font").ok().cloned()
    };

    let font_dict_id = match existing_font {
        Some(Object::Reference(id)) => {
            let dict = doc
                .get_object(id)
                .and_then(Object::as_dict)
                .cloned()
                .unwrap_or_else(|_| Dictionary::new());
            let id = doc.add_object(Object::Dictionary(dict));
            let resources = doc
                .get_object_mut(resources_id)
                .and_then(Object::as_dict_mut)?;
            resources.set("Font", Object::Reference(id));
            id
        }
        Some(Object::Dictionary(dict)) => {
            let id = doc.add_object(Object::Dictionary(dict));
            let resources = doc
                .get_object_mut(resources_id)
                .and_then(Object::as_dict_mut)?;
            resources.set("Font", Object::Reference(id));
            id
        }
        _ => {
            let id = doc.add_object(Object::Dictionary(Dictionary::new()));
            let resources = doc
                .get_object_mut(resources_id)
                .and_then(Object::as_dict_mut)?;
            resources.set("Font", Object::Reference(id));
            id
        }
    };

    Ok(font_dict_id)
}

pub(crate) fn ensure_inline_resources(
    doc: &mut Document,
    page_id: ObjectId,
) -> Result<ObjectId, PdfWriteError> {
    let inline_resources = effective_page_resources(doc, page_id)?;

    let new_id = doc.add_object(Object::Dictionary(inline_resources));
    let page_mut = doc.get_object_mut(page_id).and_then(Object::as_dict_mut)?;
    page_mut.set("Resources", Object::Reference(new_id));
    Ok(new_id)
}

fn effective_page_resources(
    doc: &Document,
    page_id: ObjectId,
) -> Result<Dictionary, PdfWriteError> {
    let mut current_id = page_id;
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(current_id) {
            return Err(PdfWriteError::PageResourcesCycle { object: current_id });
        }

        let node = doc.get_object(current_id).and_then(Object::as_dict)?;
        if let Ok(resources) = node.get(b"Resources") {
            return match resources {
                Object::Dictionary(d) => Ok(d.clone()),
                Object::Reference(id) => Ok(doc.get_dictionary(*id)?.clone()),
                _ => Ok(Dictionary::new()),
            };
        }

        let Ok(parent_id) = node.get(b"Parent").and_then(Object::as_reference) else {
            return Ok(Dictionary::new());
        };
        current_id = parent_id;
    }
}

/// Strip every `/Resources/Font` entry that no surviving `Tj`/`TJ`/`'`/`"`
/// references, then garbage-collect orphaned font dicts and their embedded
/// font streams. This is what reclaims the original PDF's font payload —
/// surgery removed the *operators* that drew the original glyphs but left
/// the font dictionaries reachable through the resources dict.
pub(crate) fn prune_unused_fonts(
    doc: &mut Document,
    modified_pages: &HashSet<ObjectId>,
    installed: &HashSet<Vec<u8>>,
) -> Result<(), PdfWriteError> {
    for page_id in modified_pages.iter().copied() {
        let used = used_font_resource_names(doc, page_id)?;
        let font_dict_id = ensure_page_font_dict(doc, page_id)?;
        let font_dict = doc.get_object(font_dict_id).and_then(Object::as_dict)?;
        let to_remove: Vec<Vec<u8>> = font_dict
            .iter()
            .filter_map(|(k, _)| (installed.contains(k) && !used.contains(k)).then(|| k.clone()))
            .collect();
        if to_remove.is_empty() {
            continue;
        }
        let font_dict = doc
            .get_object_mut(font_dict_id)
            .and_then(Object::as_dict_mut)?;
        for k in &to_remove {
            font_dict.remove(k);
        }
    }
    // Now that no /Resources/Font entry references them, the font dicts and
    // their /FontFile streams are unreachable from /Root and prune_objects()
    // collects them.
    doc.prune_objects();
    Ok(())
}

/// Remove link annotations whose clickable rectangle belongs to text we
/// surgically replaced. Without this, PDF viewers can keep drawing/storing
/// stale link rectangles at the original source-text positions.
pub(crate) fn prune_link_annotations(
    doc: &mut Document,
    page_id: ObjectId,
    removal_rects: &[UserRect],
) -> Result<(), PdfWriteError> {
    if removal_rects.is_empty() {
        return Ok(());
    }

    let annots = doc
        .get_object(page_id)
        .and_then(Object::as_dict)?
        .get(b"Annots")
        .ok()
        .cloned();
    let Some(annots) = annots else {
        return Ok(());
    };

    match annots {
        Object::Array(items) => {
            let filtered = filter_annotations(doc, items, removal_rects);
            let page = doc.get_object_mut(page_id).and_then(Object::as_dict_mut)?;
            if filtered.is_empty() {
                page.remove(b"Annots");
            } else {
                page.set("Annots", Object::Array(filtered));
            }
        }
        Object::Reference(annots_id) => {
            let Ok(items) = doc.get_object(annots_id).and_then(Object::as_array) else {
                return Ok(());
            };
            let filtered = filter_annotations(doc, items.clone(), removal_rects);
            *doc.get_object_mut(annots_id)? = Object::Array(filtered);
        }
        _ => {}
    }
    Ok(())
}

fn filter_annotations(
    doc: &Document,
    items: Vec<Object>,
    removal_rects: &[UserRect],
) -> Vec<Object> {
    items
        .into_iter()
        .filter(|item| !should_prune_annotation(doc, item, removal_rects))
        .collect()
}

fn should_prune_annotation(doc: &Document, item: &Object, removal_rects: &[UserRect]) -> bool {
    let dict = match item {
        Object::Dictionary(d) => d,
        Object::Reference(id) => match doc.get_object(*id).and_then(Object::as_dict) {
            Ok(d) => d,
            Err(_) => return false,
        },
        _ => return false,
    };
    let is_link = dict
        .get(b"Subtype")
        .and_then(Object::as_name)
        .is_ok_and(|name| name == b"Link");
    if !is_link {
        return false;
    }
    let Ok(rect_obj) = dict.get(b"Rect") else {
        return false;
    };
    let Some(rect) = annotation_rect(doc, rect_obj) else {
        return false;
    };
    removal_rects
        .iter()
        .any(|removal| rects_intersect(rect, *removal))
}

fn annotation_rect(doc: &Document, obj: &Object) -> Option<UserRect> {
    let arr = match obj {
        Object::Array(arr) => arr,
        Object::Reference(id) => doc.get_object(*id).ok()?.as_array().ok()?,
        _ => return None,
    };
    if arr.len() != 4 {
        return None;
    }
    let x0 = object_as_f32(&arr[0])?;
    let y0 = object_as_f32(&arr[1])?;
    let x1 = object_as_f32(&arr[2])?;
    let y1 = object_as_f32(&arr[3])?;
    Some(UserRect {
        x0: x0.min(x1),
        y0: y0.min(y1),
        x1: x0.max(x1),
        y1: y0.max(y1),
    })
}

fn rects_intersect(a: UserRect, b: UserRect) -> bool {
    a.x0 < b.x1 && a.x1 > b.x0 && a.y0 < b.y1 && a.y1 > b.y0
}

fn used_font_resource_names(
    doc: &Document,
    page_id: ObjectId,
) -> Result<HashSet<Vec<u8>>, PdfWriteError> {
    let content = doc.get_and_decode_page_content(page_id)?;
    let mut used = HashSet::new();
    let mut current: Option<Vec<u8>> = None;
    for op in &content.operations {
        match op.operator.as_str() {
            "Tf" => {
                if let Some(Object::Name(n)) = op.operands.first() {
                    current = Some(n.clone());
                }
            }
            // Any text-show operator counts the active font as used.
            "Tj" | "TJ" | "'" | "\"" => {
                if let Some(name) = &current {
                    used.insert(name.clone());
                }
            }
            _ => {}
        }
    }
    Ok(used)
}

pub(crate) fn append_content_stream(
    doc: &mut Document,
    page_id: ObjectId,
    stream_bytes: Vec<u8>,
) -> Result<(), PdfWriteError> {
    let new_stream_id =
        doc.add_object(Object::Stream(Stream::new(Dictionary::new(), stream_bytes)));

    let page_mut = doc.get_object_mut(page_id).and_then(Object::as_dict_mut)?;
    let new_contents = match page_mut.get(b"Contents") {
        Ok(Object::Reference(existing_id)) => Object::Array(vec![
            Object::Reference(*existing_id),
            Object::Reference(new_stream_id),
        ]),
        Ok(Object::Array(existing)) => {
            let mut arr = existing.clone();
            arr.push(Object::Reference(new_stream_id));
            Object::Array(arr)
        }
        _ => Object::Reference(new_stream_id),
    };
    page_mut.set("Contents", new_contents);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(subtype: &[u8], rect: [f32; 4]) -> Object {
        let mut d = Dictionary::new();
        d.set("Subtype", Object::Name(subtype.to_vec()));
        d.set(
            "Rect",
            Object::Array(rect.into_iter().map(Object::Real).collect()),
        );
        Object::Dictionary(d)
    }

    #[test]
    fn helvetica_font_dict_shape() {
        let mut doc = Document::with_version("1.5");
        let page_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            Object::Dictionary(d)
        });
        ensure_fonts_in_page_resources(&mut doc, page_id).unwrap();

        let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let resources_ref = page.get(b"Resources").unwrap();
        let resources_id = resources_ref.as_reference().unwrap();
        let resources = doc.get_object(resources_id).unwrap().as_dict().unwrap();
        let fonts_id = resources.get(b"Font").unwrap().as_reference().unwrap();
        let fonts = doc.get_object(fonts_id).unwrap().as_dict().unwrap();
        let helv_ref = fonts.get(HELVETICA_REGULAR).unwrap();
        let helv_id = helv_ref.as_reference().unwrap();
        let helv = doc.get_object(helv_id).unwrap().as_dict().unwrap();
        assert_eq!(
            helv.get(b"BaseFont").unwrap().as_name().unwrap(),
            b"Helvetica"
        );
    }

    #[test]
    fn ensure_fonts_preserves_existing_referenced_font_dict() {
        let mut doc = Document::with_version("1.5");
        let original_font_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Font".to_vec()));
            d.set("Subtype", Object::Name(b"Type1".to_vec()));
            d.set("BaseFont", Object::Name(b"Courier".to_vec()));
            Object::Dictionary(d)
        });
        let font_dict_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("F1", Object::Reference(original_font_id));
            Object::Dictionary(d)
        });
        let resources_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Font", Object::Reference(font_dict_id));
            Object::Dictionary(d)
        });
        let page_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            d.set("Resources", Object::Reference(resources_id));
            Object::Dictionary(d)
        });

        ensure_fonts_in_page_resources(&mut doc, page_id).unwrap();

        let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let resources_id = page.get(b"Resources").unwrap().as_reference().unwrap();
        let resources = doc.get_object(resources_id).unwrap().as_dict().unwrap();
        let fonts_id = resources.get(b"Font").unwrap().as_reference().unwrap();
        assert_ne!(fonts_id, font_dict_id);
        let fonts = doc.get_object(fonts_id).unwrap().as_dict().unwrap();
        assert_eq!(
            fonts.get(b"F1").unwrap().as_reference().unwrap(),
            original_font_id
        );
        assert!(fonts.get(HELVETICA_REGULAR).is_ok());
    }

    #[test]
    fn ensure_inline_resources_clones_inherited_resources() {
        let mut doc = Document::with_version("1.5");
        let resources_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("XObject", Object::Dictionary(Dictionary::new()));
            Object::Dictionary(d)
        });
        let pages_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Pages".to_vec()));
            d.set("Resources", Object::Reference(resources_id));
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

        let local_resources_id = ensure_inline_resources(&mut doc, page_id).unwrap();
        assert_ne!(local_resources_id, resources_id);
        let local = doc
            .get_object(local_resources_id)
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(matches!(local.get(b"XObject"), Ok(Object::Dictionary(_))));
    }

    #[test]
    fn prune_link_annotations_removes_intersecting_links_only() {
        let mut doc = Document::with_version("1.5");
        let page_id = doc.add_object({
            let mut d = Dictionary::new();
            d.set("Type", Object::Name(b"Page".to_vec()));
            d.set(
                "Annots",
                Object::Array(vec![
                    annotation(b"Link", [10.0, 10.0, 30.0, 30.0]),
                    annotation(b"Link", [80.0, 80.0, 90.0, 90.0]),
                    annotation(b"Text", [10.0, 10.0, 30.0, 30.0]),
                ]),
            );
            Object::Dictionary(d)
        });

        prune_link_annotations(
            &mut doc,
            page_id,
            &[UserRect {
                x0: 20.0,
                y0: 20.0,
                x1: 40.0,
                y1: 40.0,
            }],
        )
        .unwrap();

        let page = doc.get_object(page_id).unwrap().as_dict().unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 2);
        let kept_link = annots[0].as_dict().unwrap();
        assert_eq!(
            kept_link.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Link"
        );
        let kept_rect = annotation_rect(&doc, kept_link.get(b"Rect").unwrap()).unwrap();
        assert_eq!(kept_rect.x0, 80.0);
        assert_eq!(
            annots[1]
                .as_dict()
                .unwrap()
                .get(b"Subtype")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Text"
        );
    }
}
