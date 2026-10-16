//! Smoke tests for digital-PDF extraction and writeback.
//!
//! Skipped (with a clear message) unless every required asset is provided
//! via env vars:
//!
//!   PDF_TEST_FILE        absolute path to a sample digital PDF
//!
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use image::{ImageBuffer, Rgba, RgbaImage};

use translator::font_provider::{FontHandle, FontProvider, FontRequest, NoFontProvider};
use translator::pdf::{PageTransform, render_pages_for_debug};
use translator::pdf_text::extract_text;
use translator::pdf_translate::translate_pdf;
use translator::pdf_write::write_translated_pdf;
use translator::styled::StyledFragment;
use translator::{FsPackInstallChecker, LanguageCode, TranslatorSession};

const BITMAP_SIZE: u32 = 1024;

fn require_env(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("[pdf_smoke] skipping: {name} not set");
            None
        }
    }
}

#[test]
fn smoke_extract_text_fragments() {
    let Some(pdf_path) = require_env("PDF_TEST_FILE") else {
        return;
    };
    let dump_dir = env::var("PDF_SMOKE_DUMP_DIR").ok().map(PathBuf::from);

    let pdf_bytes = fs::read(&pdf_path).expect("read PDF_TEST_FILE");
    let extracted = extract_text(&pdf_bytes).expect("extract_text");

    let mut total_fragments = 0usize;
    let mut total_translation_groups = 0usize;

    if let Some(dir) = &dump_dir {
        fs::create_dir_all(dir).expect("create PDF_SMOKE_DUMP_DIR");
    }

    // Render the same pages so we can overlay fragment bboxes.
    let rendered_pages = render_pages_for_debug(&pdf_bytes, BITMAP_SIZE).expect("render");

    for (page, rendered) in extracted.iter().zip(rendered_pages.iter()) {
        total_fragments += page.fragments.len();
        let groups: std::collections::BTreeSet<_> =
            page.fragments.iter().map(|f| f.translation_group).collect();
        total_translation_groups += groups.len();

        eprintln!(
            "[pdf_smoke] page {} ({}x{} pts): {} fragment(s) in {} group(s)",
            page.page_index,
            page.page.width_pts as u32,
            page.page.height_pts as u32,
            page.fragments.len(),
            groups.len(),
        );

        if let Some(dir) = &dump_dir {
            let out = dir.join(format!("page-{:03}-text.png", page.page_index));
            dump_text_fragments(rendered, &page.fragments, &out);
            eprintln!("[pdf_smoke]   wrote {}", out.display());
        }
    }

    eprintln!("[pdf_smoke] total fragments: {total_fragments}");
    eprintln!("[pdf_smoke] total translation groups: {total_translation_groups}");
    assert!(total_fragments > 0, "extractor returned no fragments");
}

fn dump_text_fragments(
    rendered: &translator::pdf::RenderedPage,
    fragments: &[StyledFragment],
    out: &Path,
) {
    let size = rendered.transform.bitmap_size;
    let mut img: RgbaImage =
        ImageBuffer::from_raw(size, size, rendered.rgba.to_vec()).expect("rgba into ImageBuffer");

    // Cycle a few hues per translation_group so block boundaries are visible.
    let palette: [Rgba<u8>; 6] = [
        Rgba([0xE0, 0x10, 0x10, 0xFF]),
        Rgba([0x00, 0x80, 0xE0, 0xFF]),
        Rgba([0x00, 0xA0, 0x40, 0xFF]),
        Rgba([0xC0, 0x40, 0xC0, 0xFF]),
        Rgba([0xE0, 0x80, 0x00, 0xFF]),
        Rgba([0x40, 0x40, 0xA0, 0xFF]),
    ];

    for fragment in fragments {
        // Fragment bbox is in PDF points (top-left origin). Convert to image space.
        // MuPDF already reports fragment bboxes in top-left page points.
        let (x0, y0, x1, y1) = pdf_top_left_to_image(
            &rendered.transform,
            fragment.bounding_box.left as f32,
            fragment.bounding_box.top as f32,
            fragment.bounding_box.right as f32,
            fragment.bounding_box.bottom as f32,
        );
        let color = palette[(fragment.translation_group as usize) % palette.len()];
        draw_rect_outline(
            &mut img, x0 as i32, y0 as i32, x1 as i32, y1 as i32, color, 1,
        );
    }

    img.save(out).expect("save text-fragments PNG");
}

/// mupdf stext bbox is in PDF *points* with **top-left** origin (it's already
/// flipped). Convert that to letterboxed image-pixel coords.
fn pdf_top_left_to_image(
    t: &PageTransform,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> (f32, f32, f32, f32) {
    let x0 = left * t.scale + t.pad_x;
    let x1 = right * t.scale + t.pad_x;
    let y0 = top * t.scale + t.pad_y;
    let y1 = bottom * t.scale + t.pad_y;
    (x0, y0, x1, y1)
}

/// End-to-end: translate the test PDF and write the translated output to disk.
///
/// Requires:
///   PDF_TEST_FILE          source PDF
///   PDF_SMOKE_BUCKET_DIR   path to a translator-bucket directory containing
///                          `index.json` plus the relevant `translation/`
///                          model files
///   PDF_SMOKE_TARGET_LANG  target language code (e.g. "en")
/// Optional:
///   PDF_SMOKE_FORCED_SOURCE_LANG  force source detection
///   PDF_SMOKE_DUMP_DIR     where to write the output PDF
#[test]
fn smoke_translate_and_write_pdf() {
    let Some(pdf_path) = require_env("PDF_TEST_FILE") else {
        return;
    };
    let Some(bucket) = require_env("PDF_SMOKE_BUCKET_DIR") else {
        return;
    };
    let Some(target_lang) = require_env("PDF_SMOKE_TARGET_LANG") else {
        return;
    };
    let forced_source = env::var("PDF_SMOKE_FORCED_SOURCE_LANG").ok();
    let dump_dir = env::var("PDF_SMOKE_DUMP_DIR").ok().map(PathBuf::from);

    let bucket_path = PathBuf::from(&bucket);
    let catalog_path = bucket_path.join("index.json");
    let bundled_json = fs::read_to_string(&catalog_path).expect("read catalog index.json");
    let checker = FsPackInstallChecker::new(&bucket);
    let session = TranslatorSession::open(&bundled_json, None, bucket.clone(), &checker)
        .expect("open TranslatorSession");

    let available_langs = session.available_languages();
    let target = session
        .scripted_language(&LanguageCode::new(target_lang.clone()))
        .expect("target language is in the catalog");

    let pdf_bytes = fs::read(&pdf_path).expect("read PDF_TEST_FILE");
    let translations = translate_pdf(
        &session,
        &pdf_bytes,
        forced_source.as_deref(),
        &target,
        &available_langs,
    )
    .expect("translate_pdf");

    let mut total_blocks = 0usize;
    for page in &translations {
        if let Some(err) = &page.error {
            eprintln!("[pdf_smoke] page {} error: {err}", page.page_index);
        }
        total_blocks += page.blocks.len();
        eprintln!(
            "[pdf_smoke] page {}: {} translated block(s)",
            page.page_index,
            page.blocks.len()
        );
    }

    assert!(total_blocks > 0, "no blocks were translated");

    // Hard-coded host font paths for the integration test. Real consumer apps
    // (Android, native Linux) implement [`FontProvider`] via their own
    // platform shim; this test stub stands in for that.
    let noto_cjk_dir = PathBuf::from("/usr/share/fonts/opentype/noto");
    let noto_truetype_dir = PathBuf::from("/usr/share/fonts/truetype/noto");
    let dejavu_dir = PathBuf::from("/usr/share/fonts/truetype/dejavu");
    let host_fonts = |req: &FontRequest| -> Vec<FontHandle> {
        let mut chain: Vec<FontHandle> = Vec::new();
        if req.language == "zh" || req.script == translator::script::Script::Han {
            let leaf = if req.bold {
                "NotoSansCJK-Bold.ttc"
            } else {
                "NotoSansCJK-Regular.ttc"
            };
            let path = noto_cjk_dir.join(leaf);
            if path.is_file() {
                let ttc_index = if req.monospace { 7 } else { 2 };
                chain.push(FontHandle::new(path, ttc_index));
            }
        }
        if (req.language == "ar" || req.script == translator::script::Script::Arabic)
            && !req.monospace
        {
            let leaf = if req.bold {
                "NotoNaskhArabic-Bold.ttf"
            } else {
                "NotoNaskhArabic-Regular.ttf"
            };
            let path = noto_truetype_dir.join(leaf);
            if path.is_file() {
                chain.push(FontHandle::from(path));
            }
        }
        if (req.language == "bn" || req.script == translator::script::Script::Bengali)
            && !req.monospace
        {
            let leaf = if req.bold {
                "NotoSansBengali-Bold.ttf"
            } else {
                "NotoSansBengali-Regular.ttf"
            };
            let path = noto_truetype_dir.join(leaf);
            if path.is_file() {
                chain.push(FontHandle::from(path));
            }
        }

        if dejavu_dir.is_dir() {
            let leaf = match (req.monospace, req.bold, req.italic) {
                (true, true, true) => "DejaVuSansMono-BoldOblique.ttf",
                (true, true, false) => "DejaVuSansMono-Bold.ttf",
                (true, false, true) => "DejaVuSansMono-Oblique.ttf",
                (true, false, false) => "DejaVuSansMono.ttf",
                (false, true, true) => "DejaVuSans-BoldOblique.ttf",
                (false, true, false) => "DejaVuSans-Bold.ttf",
                (false, false, true) => "DejaVuSans-Oblique.ttf",
                (false, false, false) => "DejaVuSans.ttf",
            };
            chain.push(FontHandle::from(dejavu_dir.join(leaf)));
        }
        chain
    };
    // `dyn FontProvider` accepts the closure via the blanket impl in
    // `crate::font_provider`.
    let provider: &dyn FontProvider = &host_fonts;
    let _ = NoFontProvider;
    let out_pdf =
        write_translated_pdf(&pdf_bytes, &translations, provider).expect("write_translated_pdf");
    assert!(!out_pdf.is_empty());
    eprintln!("[pdf_smoke] translated pdf bytes: {}", out_pdf.len());

    if let Some(dir) = dump_dir {
        fs::create_dir_all(&dir).expect("mkdir dump dir");
        let out_path = dir.join("translated.pdf");
        fs::write(&out_path, &out_pdf).expect("write translated.pdf");
        eprintln!("[pdf_smoke] wrote {}", out_path.display());
    }
}

fn draw_rect_outline(
    img: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: Rgba<u8>,
    thickness: i32,
) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let x0 = x0.clamp(0, w - 1);
    let x1 = x1.clamp(0, w - 1);
    let y0 = y0.clamp(0, h - 1);
    let y1 = y1.clamp(0, h - 1);

    for t in 0..thickness {
        for x in x0..=x1 {
            put(img, x, y0 + t, color);
            put(img, x, y1 - t, color);
        }
        for y in y0..=y1 {
            put(img, x0 + t, y, color);
            put(img, x1 - t, y, color);
        }
    }
}

fn put(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, color);
    }
}
