//! Embed a parsed `FontMetrics::Real` font into a PDF Document as a Type-0
//! CID-keyed font with `/Identity-H` encoding, so the text we emit references
//! the same glyph metrics we used for wrapping.
//!
//! Output structure per font (4 indirect objects + 1 resource entry):
//!
//! ```text
//!   <Type0>          /DescendantFonts -> [<CIDFontType2>]   /ToUnicode -> <CMap>
//!     <CIDFontType2> /FontDescriptor  -> <FontDescriptor>
//!     <FontDescriptor>                 /FontFile2 -> <Stream>
//! ```
//!
//! Identity-H means `cid == gid`, and `/CIDToGIDMap /Identity` confirms that
//! to viewers that don't infer it. We emit `/W` only for the glyphs that
//! actually appear in the document; missing entries fall back to `/DW 1000`.
//!
//! No subsetting yet — the full TTF/OTF is embedded. That will be the main
//! follow-up.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use translator_render::font_metrics::{FontFileKind, FontMetrics, UsedGlyph};

#[derive(Debug, Clone)]
pub struct EmbeddedFont {
    /// Resource name to drop into the page's `/Font` dict (e.g. `b"Tr0"`).
    pub resource_name: Vec<u8>,
    /// Indirect-object id of the Type-0 dict.
    pub type0_id: ObjectId,
    /// Original GID → subset GID. The subsetter compacts GID space so the
    /// Tj hex strings have to be written using the new GIDs, not the ones
    /// `FontMetrics::glyph_for` returns.
    pub gid_remap: HashMap<u16, u16>,
}

/// Embed `metrics` into `doc` and return its resource handle. `slot` is
/// mixed into the resource name so multiple fonts on the same page don't
/// collide.
pub fn embed_font(doc: &mut Document, metrics: &FontMetrics, slot: usize) -> Option<EmbeddedFont> {
    let (bytes, ttc_index, descriptor) = metrics.embedding_source()?;
    let units_per_em = metrics.units_per_em()?;
    let used = metrics.used_glyphs();

    // Subset the font to just the glyphs we actually use. The remapper
    // compacts the GID space (so 0,1,2,... in the subset). We have to
    // translate every GID we hand to the PDF — the Tj hex strings, the /W
    // array, and the ToUnicode CMap — through this map.
    let mut remapper = subsetter::GlyphRemapper::new();
    for u in used {
        remapper.remap(u.gid);
    }
    let (subset_bytes, remap_gids) = match subsetter::subset(bytes.as_ref(), ttc_index, &remapper) {
        Ok(b) => (b.to_vec(), true),
        Err(e) => {
            eprintln!(
                "[pdf_font_embed] subset failed for {} (ttc_index={}): {e}; embedding full font",
                descriptor.postscript_name, ttc_index,
            );
            (bytes.as_ref().clone(), false)
        }
    };

    let mut gid_remap: HashMap<u16, u16> = HashMap::with_capacity(used.len());
    if remap_gids {
        for u in used {
            if let Some(new_gid) = remapper.get(u.gid) {
                gid_remap.insert(u.gid, new_gid);
            }
        }
    }
    // Re-key the used list by the *new* GIDs so /W and ToUnicode emit them.
    let used_remapped: Vec<UsedGlyph> = used
        .iter()
        .map(|u| UsedGlyph {
            gid: gid_remap.get(&u.gid).copied().unwrap_or(u.gid),
            advance: u.advance,
            unicode: u.unicode.clone(),
        })
        .collect();

    // 1. /FontFile2 (raw TTF) or /FontFile3 + /OpenType (CFF in OTF).
    let font_file_id = {
        let mut sd = Dictionary::new();
        sd.set("Length1", Object::Integer(subset_bytes.len() as i64));
        if descriptor.kind == FontFileKind::OpenTypeCff {
            sd.set("Subtype", Object::Name(b"OpenType".to_vec()));
        }
        let stream = Stream::new(sd, subset_bytes);
        doc.add_object(Object::Stream(stream))
    };

    // 2. /FontDescriptor.
    let font_descriptor_id = {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"FontDescriptor".to_vec()));
        d.set(
            "FontName",
            Object::Name(
                format!("{}+{}", subset_tag(slot), descriptor.postscript_name).into_bytes(),
            ),
        );
        d.set("Flags", Object::Integer(font_flags(descriptor) as i64));
        d.set(
            "FontBBox",
            Object::Array(vec![
                Object::Integer(descriptor.bbox.0 as i64),
                Object::Integer(descriptor.bbox.1 as i64),
                Object::Integer(descriptor.bbox.2 as i64),
                Object::Integer(descriptor.bbox.3 as i64),
            ]),
        );
        d.set("ItalicAngle", Object::Real(descriptor.italic_angle));
        d.set("Ascent", Object::Integer(descriptor.ascent as i64));
        d.set("Descent", Object::Integer(descriptor.descent as i64));
        d.set("CapHeight", Object::Integer(descriptor.cap_height as i64));
        // StemV isn't in TTF; PDF spec lets us guess. 80 ≈ Helvetica regular.
        d.set(
            "StemV",
            Object::Integer(if descriptor.bold { 120 } else { 80 }),
        );
        let key = match descriptor.kind {
            FontFileKind::TrueType => "FontFile2",
            FontFileKind::OpenTypeCff => "FontFile3",
        };
        d.set(key, Object::Reference(font_file_id));
        doc.add_object(Object::Dictionary(d))
    };

    // 3. CIDFontType2 (TTF) / CIDFontType0 (CFF).
    let cid_font_id = {
        let subtype: &[u8] = match descriptor.kind {
            FontFileKind::TrueType => b"CIDFontType2",
            FontFileKind::OpenTypeCff => b"CIDFontType0",
        };
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Font".to_vec()));
        d.set("Subtype", Object::Name(subtype.to_vec()));
        d.set(
            "BaseFont",
            Object::Name(
                format!("{}+{}", subset_tag(slot), descriptor.postscript_name).into_bytes(),
            ),
        );
        let mut cid_system_info = Dictionary::new();
        cid_system_info.set("Registry", Object::string_literal("Adobe"));
        cid_system_info.set("Ordering", Object::string_literal("Identity"));
        cid_system_info.set("Supplement", Object::Integer(0));
        d.set("CIDSystemInfo", Object::Dictionary(cid_system_info));
        d.set("FontDescriptor", Object::Reference(font_descriptor_id));
        if descriptor.kind == FontFileKind::TrueType {
            d.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
        }
        d.set("DW", Object::Integer(1000));
        d.set(
            "W",
            Object::Array(build_w_array(&used_remapped, units_per_em)),
        );
        doc.add_object(Object::Dictionary(d))
    };

    // 4. ToUnicode CMap (so copy/paste/search keep working).
    let to_unicode_id = {
        let cmap = build_to_unicode_cmap(&used_remapped);
        let mut sd = Dictionary::new();
        sd.set("Length", Object::Integer(cmap.len() as i64));
        let stream = Stream::new(sd, cmap);
        doc.add_object(Object::Stream(stream))
    };

    // 5. Type 0 outer dict.
    let type0_id = {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Font".to_vec()));
        d.set("Subtype", Object::Name(b"Type0".to_vec()));
        d.set(
            "BaseFont",
            Object::Name(descriptor.postscript_name.as_bytes().to_vec()),
        );
        d.set("Encoding", Object::Name(b"Identity-H".to_vec()));
        d.set(
            "DescendantFonts",
            Object::Array(vec![Object::Reference(cid_font_id)]),
        );
        d.set("ToUnicode", Object::Reference(to_unicode_id));
        doc.add_object(Object::Dictionary(d))
    };

    Some(EmbeddedFont {
        resource_name: format!("Tr{}", slot).into_bytes(),
        type0_id,
        gid_remap,
    })
}

/// PDF `/FontDescriptor` `Flags` bitfield (PDF spec table 122).
/// Bit indices are 1-based, so bit n sets `1 << (n-1)`.
fn font_flags(d: &translator_render::font_metrics::FontDescriptorInfo) -> u32 {
    let mut f = 0u32;
    if d.monospace {
        f |= 1 << 0; // FixedPitch
    }
    f |= 1 << 5; // Nonsymbolic — we use Adobe-standard Latin glyph set
    if d.italic {
        f |= 1 << 6; // Italic
    }
    if d.bold {
        f |= 1 << 18; // ForceBold
    }
    f
}

fn subset_tag(slot: usize) -> String {
    // PDF 1.7 §9.6.4: 6 uppercase letters + '+'. Stable per slot so the
    // resource name and BaseFont stay aligned.
    let mut tag = String::with_capacity(6);
    let mut n = slot;
    for _ in 0..6 {
        tag.push((b'A' + (n % 26) as u8) as char);
        n /= 26;
    }
    tag
}

/// `/W` array using the per-CID form `cid [w]`. Compact run-encoding would
/// save bytes but the saving is dwarfed by the embedded font program.
fn build_w_array(glyphs: &[UsedGlyph], units_per_em: u16) -> Vec<Object> {
    // PDF widths are in 1/1000 em.
    let to_pdf =
        |advance: u16| -> i64 { (advance as f32 * 1000.0 / units_per_em as f32).round() as i64 };
    let mut out = Vec::with_capacity(glyphs.len() * 2);
    for u in glyphs {
        out.push(Object::Integer(u.gid as i64));
        out.push(Object::Array(vec![Object::Integer(to_pdf(u.advance))]));
    }
    out
}

/// Build a minimal Adobe Identity-UCS ToUnicode CMap.
fn build_to_unicode_cmap(glyphs: &[UsedGlyph]) -> Vec<u8> {
    let header = b"/CIDInit /ProcSet findresource begin\n\
12 dict begin\n\
begincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n\
/CMapType 2 def\n\
1 begincodespacerange\n\
<0000> <FFFF>\n\
endcodespacerange\n";
    let footer = b"endcmap\n\
CMapName currentdict /CMap defineresource pop\n\
end\nend";

    // Group up to 100 entries per `beginbfchar` (PDF spec limit).
    let mut out = Vec::with_capacity(header.len() + footer.len() + glyphs.len() * 24);
    out.extend_from_slice(header);
    for chunk in glyphs.chunks(100) {
        let _ = write_str(&mut out, &format!("{} beginbfchar\n", chunk.len()));
        for u in chunk {
            let _ = write_str(&mut out, &format!("<{:04X}> ", u.gid));
            append_utf16be(&mut out, &u.unicode);
            out.push(b'\n');
        }
        out.extend_from_slice(b"endbfchar\n");
    }
    out.extend_from_slice(footer);
    out
}

fn write_str(out: &mut Vec<u8>, s: &str) -> std::io::Result<()> {
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

fn append_utf16be(out: &mut Vec<u8>, s: &str) {
    out.push(b'<');
    for unit in s.encode_utf16() {
        let _ = write_str(out, &format!("{:04X}", unit));
    }
    out.push(b'>');
}
