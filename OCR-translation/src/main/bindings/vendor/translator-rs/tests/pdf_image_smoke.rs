//! End-to-end test: run image-XObject translation against `Guru-menu.pdf`
//! (or any PDF with embedded JPEGs) using a real translator session.
//!
//! Skipped unless every required asset is provided via env vars:
//!   PDF_IMAGE_TEST_FILE       path to a PDF with raster image XObjects
//!   PDF_IMAGE_BUCKET_DIR      bucket dir with index.json + bin/ + ppocr/
//!   PDF_IMAGE_TARGET_LANG     target BCP-47, e.g. "es"
//!   PDF_IMAGE_SOURCE_LANG     source BCP-47, e.g. "en"
//! Optional:
//!   PDF_IMAGE_DUMP_DIR        where to write the modified PDF for inspection
//!
//! On Linux dev hosts, also expects DejaVuSans at /usr/share/fonts/truetype/dejavu.

use std::env;
use std::fs;
use std::path::PathBuf;

use translator::font_provider::{FontHandle, FontProvider, FontRequest};
use translator::pdf_image_translate::{
    log_page_inventory, pages_without_extractable_text, translate_pdf_images_in_place,
    translate_pdf_pages_as_raster_in_place,
};
use translator::script::Script;
use translator::{FsPackInstallChecker, TranslatorSession};

fn require_env(name: &str) -> Option<String> {
    match env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => {
            eprintln!("[pdf_image_smoke] skipping: {name} not set");
            None
        }
    }
}

#[test]
fn translates_images_in_pdf() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some(pdf_path) = require_env("PDF_IMAGE_TEST_FILE") else {
        return;
    };
    let Some(bucket) = require_env("PDF_IMAGE_BUCKET_DIR") else {
        return;
    };
    let Some(target_lang) = require_env("PDF_IMAGE_TARGET_LANG") else {
        return;
    };
    let Some(source_lang) = require_env("PDF_IMAGE_SOURCE_LANG") else {
        return;
    };
    let dump_dir = env::var("PDF_IMAGE_DUMP_DIR").ok().map(PathBuf::from);

    let bucket_path = PathBuf::from(&bucket);
    let bundled_json =
        fs::read_to_string(bucket_path.join("index.json")).expect("read catalog index.json");
    let checker = FsPackInstallChecker::new(&bucket);
    let session = TranslatorSession::open(&bundled_json, None, bucket.clone(), &checker)
        .expect("open TranslatorSession");

    let pdf_bytes = fs::read(&pdf_path).expect("read PDF_IMAGE_TEST_FILE");
    let original_len = pdf_bytes.len();

    let provider = HostFonts;
    log_page_inventory(&pdf_bytes);
    let overlay_pages = pages_without_extractable_text(&pdf_bytes);
    eprintln!(
        "[pdf_image_smoke] {} page(s) flagged for overlay (no extractable text)",
        overlay_pages.len()
    );
    let no_cancel = || false;
    // Pass 1: image-XObject translation.
    let xobject_output = translate_pdf_images_in_place(
        &pdf_bytes,
        &session,
        &source_lang,
        &target_lang,
        &provider,
        &overlay_pages,
        &no_cancel,
        |c, t| eprintln!("[pdf_image_smoke] xobject {c}/{t}"),
    )
    .expect("translate_pdf_images_in_place");
    eprintln!(
        "[pdf_image_smoke] after XObject pass: {} bytes (input {original_len})",
        xobject_output.bytes.len()
    );

    // Pages already translated through XObjects shouldn't be
    // re-processed by page-raster overlay.
    let raster_pages: std::collections::HashSet<usize> = overlay_pages
        .difference(&xobject_output.translated_pages)
        .copied()
        .collect();
    let after_pages = translate_pdf_pages_as_raster_in_place(
        &xobject_output.bytes,
        &session,
        &source_lang,
        &target_lang,
        &provider,
        &raster_pages,
        &no_cancel,
        |c, t| eprintln!("[pdf_image_smoke] page {c}/{t}"),
    )
    .expect("translate_pdf_pages_as_raster_in_place");
    eprintln!(
        "[pdf_image_smoke] after page-raster pass: {} bytes",
        after_pages.len()
    );

    let any_change = xobject_output.bytes != pdf_bytes || after_pages != xobject_output.bytes;
    assert!(
        any_change,
        "neither pass modified the PDF — nothing was translated"
    );

    if let Some(dir) = dump_dir {
        fs::create_dir_all(&dir).expect("create dump dir");
        let out = dir.join("translated.pdf");
        fs::write(&out, &after_pages).expect("write translated pdf");
        eprintln!("[pdf_image_smoke] wrote {}", out.display());
    }
}

#[derive(Default)]
struct HostFonts;

impl FontProvider for HostFonts {
    fn locate(&self, req: &FontRequest) -> Vec<FontHandle> {
        let mut chain: Vec<FontHandle> = Vec::new();
        let dejavu_dir = PathBuf::from("/usr/share/fonts/truetype/dejavu");
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
        let _ = Script::Latin;
        chain
    }
}
