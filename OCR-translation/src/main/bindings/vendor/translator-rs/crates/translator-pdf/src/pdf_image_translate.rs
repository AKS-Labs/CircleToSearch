//! Translate raster image XObjects embedded in a PDF.
//!
//! Walks every page's `/Resources /XObject` dict, finds streams whose
//! `/Subtype` is `/Image`, decodes the supported filter combinations
//! (DCTDecode → JPEG; FlateDecode → raw RGB or grayscale), runs the
//! existing OCR + image renderer on the pixels, and writes the rendered
//! result back as a `DCTDecode`-compressed JPEG — keeping the same
//! object id so existing `cm` / `Do` references in page content streams
//! resolve unchanged. JPEG is used (not flate) because XObjects are
//! typically photos / page-rasters where DCT is ~10× smaller than
//! flate-on-RGB.
//!
//! Unsupported filter combinations (JPX, JBIG2, CCITTFax, indexed
//! colorspaces) are skipped: the original image survives untranslated.
//! Likewise images smaller than [`MIN_IMAGE_AREA_PX`] are skipped — they
//! are almost certainly icons or decorative bullets, not text.
//!
//! When the caller passes `overlay_covered_pages`, XObjects whose every
//! referencing page is in that set are skipped entirely: the page-raster
//! pass will overlay PDF text on top of those pages, so rewriting their
//! XObjects would just bake redundant translated text into a bitmap.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;

use log::{debug, info, trace, warn};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::pdf_content::PageGeometry;
use crate::pdf_text::extract_text;
use crate::pdf_text_overlay::{
    OverlayPage, build_overlay_font_plan, build_page_overlay_stream, collect_used_embed_names,
    install_overlay_on_page,
};
use translator_core::api::{LanguageCode, TranslatorError};
use translator_core::ocr::{
    DetectedTextBox, OcrSourceSelection, PreparedImageOverlay, ReadingOrder,
};
use translator_core::settings::BackgroundMode;
use translator_render::font_provider::FontProvider;
use translator_render::image_render::{RenderOptions, render_overlay};

/// OCR-and-translate a still RGBA image into a renderable overlay. The PDF
/// image plumbing owns decode/placement/re-encode; this is the one capability
/// it needs from the OCR/translate stack, supplied by the facade translator so
/// this crate stays free of the OCR engine and catalog.
pub trait ImageTranslator: Send + Sync {
    #[allow(clippy::too_many_arguments)]
    fn translate_image_rgba(
        &self,
        rgba_bytes: &[u8],
        width: u32,
        height: u32,
        max_image_size: u32,
        source_selection: OcrSourceSelection,
        target_code: &str,
        min_confidence: u32,
        reading_order: Option<ReadingOrder>,
        background_mode: BackgroundMode,
        detection: Option<Vec<DetectedTextBox>>,
    ) -> Result<PreparedImageOverlay, TranslatorError>;
}

/// Minimum total pixel area for an image to be considered worth OCR'ing.
/// Roughly equivalent to "bigger than a 50×50 icon". Banner-shaped images
/// (e.g. 313×43 = 13,459 px²) clear this floor; small navigation icons
/// (32×32 = 1,024 px²) don't.
const MIN_IMAGE_AREA_PX: u64 = 2_500;

/// Default OCR confidence floor — matches what the Android caller passes
/// in normal image translation flow.
const DEFAULT_MIN_CONFIDENCE: u32 = 30;

#[derive(Debug)]
pub enum ImageTranslateError {
    Pdf(lopdf::Error),
}

impl std::fmt::Display for ImageTranslateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pdf(e) => write!(f, "pdf: {e}"),
        }
    }
}

impl std::error::Error for ImageTranslateError {}

impl From<lopdf::Error> for ImageTranslateError {
    fn from(value: lopdf::Error) -> Self {
        Self::Pdf(value)
    }
}

/// Run image-XObject translation over a PDF. Returns modified PDF bytes
/// (or the original bytes unchanged if no eligible image was found).
///
/// `source_code` is the BCP-47 of the language present *in the images*. If
/// the caller doesn't have one, pass the page-text source language — it's
/// the same document, the OCR'd images are usually in the same script.
///
/// Per-image errors (decode failure, no text detected, OCR cancel) are
/// swallowed; the image is left untranslated and the function moves on.
/// Output of a successful image-XObject translation pass.
pub struct ImageTranslateOutput {
    /// Modified PDF bytes (or a clone of the input if nothing was translated).
    pub bytes: Vec<u8>,
    /// 0-based page indices on which at least one image XObject got
    /// translated. The caller should subtract this set from the
    /// page-raster overlay's input set so it doesn't re-process pages
    /// whose visible content already received translation through the
    /// XObject path.
    pub translated_pages: HashSet<usize>,
}

/// Number of worker threads that decode + OCR + render image XObjects
/// in parallel. Encoding (G4 / JPEG) and translation (bergamot mutex)
/// overlap with pending OCR on other workers.
const XOBJECT_WORKERS: usize = 4;

pub fn translate_pdf_images_in_place(
    pdf_bytes: &[u8],
    translator: &dyn ImageTranslator,
    source_code: &str,
    target_code: &str,
    fonts: &(dyn FontProvider + Send + Sync),
    overlay_covered_pages: &HashSet<usize>,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    mut on_progress: impl FnMut(usize, usize),
) -> Result<ImageTranslateOutput, ImageTranslateError> {
    let mut doc = Document::load_mem(pdf_bytes)?;
    let image_pages = collect_image_pages(&doc);
    let orientations = collect_xobject_orientations(&doc);
    let _ = overlay_covered_pages; // pages without extractable text used to
    // skip image-XObject translation here, on the theory that the page-raster
    // overlay would handle those pages instead. That backfires on PDFs whose
    // visible content lives entirely *inside* image XObjects (e.g. FM 22-100
    // pages composed of stitched scan strips): with the skip those XObjects
    // were left untranslated, and the page-raster pass at 200 DPI couldn't
    // read the small text reliably. We now translate every XObject; pages
    // covered by translated XObjects get filtered out of the page-raster
    // pass downstream so we don't double-process.
    let image_ids: Vec<ObjectId> = image_pages.keys().copied().collect();
    info!(
        "[pdf_image_translate] {} candidate image XObject(s) — translating all ({} workers, page-raster overlay skips XObject-bearing pages)",
        image_pages.len(),
        XOBJECT_WORKERS,
    );
    if image_ids.is_empty() {
        return Ok(ImageTranslateOutput {
            bytes: pdf_bytes.to_vec(),
            translated_pages: HashSet::new(),
        });
    }

    let total = image_ids.len();
    on_progress(0, total);

    let next_idx = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<Result<WorkerXobjectOutput, (ObjectId, SkipReason)>>();

    let mut collected: Vec<WorkerXobjectOutput> = Vec::with_capacity(total);
    let mut skip_counts: HashMap<String, usize> = HashMap::new();
    let mut translated_count = 0usize;
    let mut translated_pages: HashSet<usize> = HashSet::new();
    let mut processed = 0usize;
    let cancelled_during_collect = std::cell::Cell::new(false);

    thread::scope(|scope| {
        for _ in 0..XOBJECT_WORKERS {
            let tx = tx.clone();
            let image_ids = &image_ids;
            let next_idx = &next_idx;
            let orientations = &orientations;
            scope.spawn(move || {
                // Each worker owns its own read-only lopdf::Document over
                // the same input bytes. Reparsing is cheap relative to
                // OCR and lets workers walk streams + resolve indirect
                // colorspace refs without sharing &mut Document.
                let worker_doc = match Document::load_mem(pdf_bytes) {
                    Ok(d) => d,
                    Err(err) => {
                        warn!("[pdf_image_translate] worker lopdf load failed: {err}");
                        return;
                    }
                };
                loop {
                    if is_cancelled() {
                        break;
                    }
                    let i = next_idx.fetch_add(1, Ordering::Relaxed);
                    if i >= image_ids.len() {
                        break;
                    }
                    let image_id = image_ids[i];
                    if is_cancelled() {
                        break;
                    }
                    let orient = orientations
                        .get(&image_id)
                        .copied()
                        .unwrap_or(Orientation::Identity);
                    let result = translate_one_xobject(
                        &worker_doc,
                        image_id,
                        translator,
                        source_code,
                        target_code,
                        fonts,
                        orient,
                    )
                    .map(|r| WorkerXobjectOutput {
                        image_id,
                        result: r,
                    })
                    .map_err(|reason| (image_id, reason));
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        // Drain worker results. Per-image progress increments as each
        // OCR + render completes; PDF mutation happens after the loop.
        while let Ok(received) = rx.recv() {
            if is_cancelled() {
                cancelled_during_collect.set(true);
                continue;
            }
            match received {
                Ok(out) => {
                    trace!("[pdf_image_translate] {:?} translated", out.image_id);
                    translated_count += 1;
                    if let Some(pages) = image_pages.get(&out.image_id) {
                        translated_pages.extend(pages.iter().copied());
                    }
                    collected.push(out);
                }
                Err((image_id, reason)) => {
                    trace!("[pdf_image_translate] {image_id:?} skipped: {reason}");
                    *skip_counts.entry(skip_bucket(&reason)).or_insert(0) += 1;
                }
            }
            processed += 1;
            on_progress(processed, total);
        }
    });

    if translated_count > 0 {
        info!(
            "[pdf_image_translate] translated {translated_count} image XObject(s) across {} page(s)",
            translated_pages.len(),
        );
    }
    log_skip_summary(&skip_counts);

    if cancelled_during_collect.get() {
        debug!("[pdf_image_translate] image-XObject pass cancelled mid-flight");
    }
    if collected.is_empty() {
        info!("[pdf_image_translate] no images were modified");
        return Ok(ImageTranslateOutput {
            bytes: pdf_bytes.to_vec(),
            translated_pages: HashSet::new(),
        });
    }

    for out in collected {
        if let Err(reason) = apply_xobject_result(&mut doc, out.image_id, out.result) {
            warn!(
                "[pdf_image_translate] {:?} apply failed: {reason}",
                out.image_id
            );
        }
    }

    let mut bytes_out = Vec::new();
    doc.save_to(&mut bytes_out).map_err(lopdf::Error::IO)?;
    log_size_delta("XObject pass", pdf_bytes.len(), bytes_out.len());
    Ok(ImageTranslateOutput {
        bytes: bytes_out,
        translated_pages,
    })
}

struct WorkerXobjectOutput {
    image_id: ObjectId,
    result: XobjectTranslation,
}

/// Output of OCR + render + encode for one image XObject. Produced on a
/// worker thread; consumed by the main thread when applying the new
/// stream content to the lopdf::Document.
struct XobjectTranslation {
    encoded_content: Vec<u8>,
    width: u32,
    height: u32,
    source_kind: SourceKind,
}

/// Orientation a `/cm` matrix applies to an image XObject placement.
/// Image XObjects render into the unit square; the CTM at the time of
/// the `Do` operator scales/rotates/flips that square. Most PDFs use
/// `cm w 0 0 h x y` (positive scales) which is the natural image
/// orientation. PDFs that use `d<0` or `a<0` flip the image at display
/// time; we need to apply the same flip to our decoded bitmap before
/// running OCR / rendering, then flip back before re-encoding so the
/// existing /cm renders our work right-side-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Orientation {
    /// `a > 0, d > 0, b = c = 0`: image data orientation matches display.
    Identity,
    /// `a > 0, d < 0, b = c = 0`: y-axis flipped.
    FlipY,
    /// `a < 0, d > 0, b = c = 0`: x-axis flipped.
    FlipX,
    /// `a < 0, d < 0, b = c = 0`: both flipped (= 180° rotation).
    Rotate180,
    /// `b ≠ 0` or `c ≠ 0`: non-trivial rotation/shear. We skip these.
    Other,
}

impl Orientation {
    fn from_cm(a: f32, b: f32, c: f32, d: f32) -> Self {
        const EPS: f32 = 1e-3;
        if b.abs() > EPS || c.abs() > EPS {
            return Self::Other;
        }
        match (a > 0.0, d > 0.0) {
            (true, true) => Self::Identity,
            (true, false) => Self::FlipY,
            (false, true) => Self::FlipX,
            (false, false) => Self::Rotate180,
        }
    }
}

/// Walk every page resource dict and collect each image-XObject's set
/// of referencing page indices (0-based). The same XObject can be shared
/// across multiple pages (a corporate logo on every page, etc.); the
/// returned map records every page that references it.
fn collect_image_pages(doc: &Document) -> HashMap<ObjectId, HashSet<usize>> {
    let mut by_id: HashMap<ObjectId, HashSet<usize>> = HashMap::new();
    for (page_num, page_id) in doc.get_pages() {
        let page_index = (page_num as usize).saturating_sub(1);
        let Ok((inline_dict, inherited_ids)) = doc.get_page_resources(page_id) else {
            continue;
        };
        // get_page_resources returns one optional inline /Resources dict
        // plus any /Resources references inherited via the page tree;
        // both can carry /XObject entries.
        let mut resource_dicts: Vec<&lopdf::Dictionary> = Vec::new();
        if let Some(d) = inline_dict {
            resource_dicts.push(d);
        }
        for id in &inherited_ids {
            if let Ok(d) = doc.get_object(*id).and_then(|o| o.as_dict()) {
                resource_dicts.push(d);
            }
        }
        for res_dict in resource_dicts {
            let Ok(xobjects) = res_dict.get(b"XObject") else {
                continue;
            };
            let xobj_dict = match xobjects {
                Object::Dictionary(d) => Some(d),
                Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
                _ => None,
            };
            let Some(xobj_dict) = xobj_dict else {
                continue;
            };
            for (_name, value) in xobj_dict.iter() {
                let Ok(id) = value.as_reference() else {
                    continue;
                };
                let Ok(obj) = doc.get_object(id) else {
                    continue;
                };
                let Ok(stream) = obj.as_stream() else {
                    continue;
                };
                if !is_image_stream(stream) {
                    continue;
                }
                by_id.entry(id).or_default().insert(page_index);
            }
        }
    }
    by_id
}

/// Walk every page's content stream and record the placement
/// orientation of each image XObject `/Do` reference. Resolves /XObject
/// resource names (e.g. `/im1`) to their `ObjectId`s via the page's
/// `/Resources/XObject` dict.
///
/// Returns ObjectId → orientation. If the same XObject is placed multiple
/// times with different orientations, the conflicting entry is mapped to
/// `Orientation::Other` so the caller can skip it (we'd need to translate
/// once per unique orientation, which we don't support).
fn collect_xobject_orientations(doc: &Document) -> HashMap<ObjectId, Orientation> {
    let mut by_id: HashMap<ObjectId, Orientation> = HashMap::new();
    for (_page_num, page_id) in doc.get_pages() {
        let Ok(content) = doc.get_and_decode_page_content(page_id) else {
            continue;
        };
        let resource_xobjects = page_xobject_names(doc, page_id);
        track_placements(&content, &resource_xobjects, &mut by_id);
    }
    by_id
}

/// Resolve `/Resources/XObject` for a page into a name → ObjectId map.
/// Walks the inline dict + every /Resources entry inherited via the
/// page tree.
fn page_xobject_names(doc: &Document, page_id: ObjectId) -> HashMap<Vec<u8>, ObjectId> {
    let mut out: HashMap<Vec<u8>, ObjectId> = HashMap::new();
    let Ok((inline, inherited)) = doc.get_page_resources(page_id) else {
        return out;
    };
    let mut dicts: Vec<&lopdf::Dictionary> = Vec::new();
    if let Some(d) = inline {
        dicts.push(d);
    }
    for id in &inherited {
        if let Ok(d) = doc.get_object(*id).and_then(|o| o.as_dict()) {
            dicts.push(d);
        }
    }
    for d in dicts {
        let Ok(xobjects) = d.get(b"XObject") else {
            continue;
        };
        let xobj_dict = match xobjects {
            Object::Dictionary(d) => Some(d),
            Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_dict().ok()),
            _ => None,
        };
        let Some(xobj_dict) = xobj_dict else {
            continue;
        };
        for (name, value) in xobj_dict.iter() {
            if let Ok(id) = value.as_reference() {
                out.entry(name.clone()).or_insert(id);
            }
        }
    }
    out
}

/// Track the CTM through a content stream. Each `cm` operator
/// post-multiplies the current CTM. At every `Do <name>` we look up the
/// resolved XObject id and record the *first* CTM at which it appeared;
/// later sightings with a different orientation collapse to `Other` so
/// the caller skips that XObject.
fn track_placements(
    content: &lopdf::content::Content<Vec<lopdf::content::Operation>>,
    xobjects: &HashMap<Vec<u8>, ObjectId>,
    by_id: &mut HashMap<ObjectId, Orientation>,
) {
    let mut stack: Vec<[f32; 6]> = Vec::new();
    let mut ctm: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    for op in &content.operations {
        match op.operator.as_str() {
            "q" => {
                stack.push(ctm);
            }
            "Q" => {
                if let Some(prev) = stack.pop() {
                    ctm = prev;
                }
            }
            "cm" => {
                if let Some(m) = matrix_from_operands(&op.operands) {
                    ctm = mul_matrix(&ctm, &m);
                }
            }
            "Do" => {
                let Some(name) = op.operands.first().and_then(|o| match o {
                    Object::Name(n) => Some(n),
                    _ => None,
                }) else {
                    continue;
                };
                let Some(&id) = xobjects.get(name) else {
                    continue;
                };
                let orient = Orientation::from_cm(ctm[0], ctm[1], ctm[2], ctm[3]);
                by_id
                    .entry(id)
                    .and_modify(|existing| {
                        if *existing != orient {
                            *existing = Orientation::Other;
                        }
                    })
                    .or_insert(orient);
            }
            _ => {}
        }
    }
}

fn matrix_from_operands(operands: &[Object]) -> Option<[f32; 6]> {
    if operands.len() != 6 {
        return None;
    }
    let mut out = [0.0f32; 6];
    for (i, o) in operands.iter().enumerate() {
        out[i] = match o {
            Object::Integer(n) => *n as f32,
            Object::Real(n) => *n,
            _ => return None,
        };
    }
    Some(out)
}

/// PDF CTM post-multiplication: `result = m * ctm` (PDF stores
/// row-major 3x3 with bottom row implicitly `[0 0 1]`).
fn mul_matrix(ctm: &[f32; 6], m: &[f32; 6]) -> [f32; 6] {
    [
        m[0] * ctm[0] + m[1] * ctm[2],
        m[0] * ctm[1] + m[1] * ctm[3],
        m[2] * ctm[0] + m[3] * ctm[2],
        m[2] * ctm[1] + m[3] * ctm[3],
        m[4] * ctm[0] + m[5] * ctm[2] + ctm[4],
        m[4] * ctm[1] + m[5] * ctm[3] + ctm[5],
    ]
}

/// In-place y-flip: swap rows i and (h - 1 - i). 4 bytes per pixel.
fn flip_y_bgra(bgra: &mut [u8], width: u32, height: u32) {
    let row_stride = width as usize * 4;
    let mut top = 0usize;
    let mut bottom = (height as usize - 1) * row_stride;
    while top < bottom {
        for i in 0..row_stride {
            bgra.swap(top + i, bottom + i);
        }
        top += row_stride;
        bottom -= row_stride;
    }
}

/// In-place x-flip: reverse pixels within each row.
fn flip_x_bgra(bgra: &mut [u8], width: u32, height: u32) {
    let w = width as usize;
    let row_stride = w * 4;
    for row in 0..height as usize {
        let row_start = row * row_stride;
        let mut left = 0usize;
        let mut right = w - 1;
        while left < right {
            for i in 0..4 {
                bgra.swap(row_start + left * 4 + i, row_start + right * 4 + i);
            }
            left += 1;
            right -= 1;
        }
    }
}

fn apply_orientation(bgra: &mut [u8], width: u32, height: u32, orient: Orientation) {
    match orient {
        Orientation::Identity | Orientation::Other => {}
        Orientation::FlipY => flip_y_bgra(bgra, width, height),
        Orientation::FlipX => flip_x_bgra(bgra, width, height),
        Orientation::Rotate180 => {
            flip_y_bgra(bgra, width, height);
            flip_x_bgra(bgra, width, height);
        }
    }
}

/// Compute the set of page indices that have no extractable text (no
/// `Tj`/`TJ` with Unicode-mappable glyphs). Pages in this set are
/// candidates for the page-raster overlay pass; XObjects referenced
/// only by these pages can be skipped during image-XObject translation.
///
/// On extraction failure returns an empty set — the caller falls back
/// to translating every XObject and skipping the overlay pass.
pub fn pages_without_extractable_text(pdf_bytes: &[u8]) -> HashSet<usize> {
    match extract_text(pdf_bytes) {
        Ok(extracted) => extracted
            .iter()
            .filter(|p| p.fragments.is_empty())
            .map(|p| p.page_index)
            .collect(),
        Err(err) => {
            warn!(
                "[pdf_image_translate] extract_text failed: {err}; treating no pages as overlay-covered"
            );
            HashSet::new()
        }
    }
}

/// Phase counts the document pipeline reports up-front, so the UI can
/// render three labelled progress lines (text pages / image XObjects /
/// raster pages) before any pass actually begins.
///
/// `raster_pages` is the *upper bound* before the image-XObject pass
/// runs. After XObjects translate, the image pass narrows the actual
/// raster set to `pages_without_text \ xobject_translated_pages`; the
/// raster pass then reports the refined total via its own progress
/// ticks, so the UI can update the bar's denominator from those.
#[derive(Debug, Clone, Copy)]
pub struct PdfTranslationInventory {
    pub total_pages: u32,
    pub image_xobjects: u32,
    /// Upper bound: count of pages with no extractable text. Some of
    /// these may be fully covered by translated XObjects, in which
    /// case the raster pass will skip them and report a smaller total.
    pub raster_pages: u32,
}

/// Cheap one-pass inventory used by the document orchestrator to emit a
/// "PdfPlan" progress event before any phase actually starts. Returns
/// `None` if extract_text or lopdf::load_mem fails — caller falls back
/// to per-phase progress without a plan.
pub fn pdf_translation_inventory(pdf_bytes: &[u8]) -> Option<PdfTranslationInventory> {
    let extracted = extract_text(pdf_bytes).ok()?;
    let total_pages = extracted.len() as u32;
    let raster_pages = extracted.iter().filter(|p| p.fragments.is_empty()).count() as u32;
    let doc = Document::load_mem(pdf_bytes).ok()?;
    let image_xobjects = collect_image_pages(&doc).len() as u32;
    Some(PdfTranslationInventory {
        total_pages,
        image_xobjects,
        raster_pages,
    })
}

/// Log one info-level line per page summarising what the image-translation
/// pipeline will see: text-block count, image-XObject count, and whether
/// the page is going to be picked up by the page-raster overlay pass.
/// Also logs the input PDF size so output-size deltas at each pass are
/// trivially comparable.
///
/// Cheap (one extract_text + one lopdf load); call once at the start of
/// the document pipeline.
pub fn log_page_inventory(pdf_bytes: &[u8]) {
    info!(
        "[pdf_image_translate] input PDF: {} bytes ({})",
        pdf_bytes.len(),
        format_size(pdf_bytes.len()),
    );
    let extracted = match extract_text(pdf_bytes) {
        Ok(e) => e,
        Err(err) => {
            warn!("[pdf_image_translate] log_page_inventory: extract_text failed: {err}");
            return;
        }
    };
    let doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(err) => {
            warn!("[pdf_image_translate] log_page_inventory: load_mem failed: {err}");
            return;
        }
    };
    let images_per_page = page_image_counts(&doc);
    let total = extracted.len();
    for page in &extracted {
        let n = page.page_index;
        let blocks = page.fragments.len();
        let images = images_per_page.get(&n).copied().unwrap_or(0);
        let will_overlay = page.fragments.is_empty();
        info!(
            "[pdf_image_translate] Page {}/{}: {} text blocks, {} images, will overlay={}",
            n + 1,
            total,
            blocks,
            images,
            will_overlay,
        );
    }
}

fn format_size(bytes: usize) -> String {
    const MB: f32 = 1024.0 * 1024.0;
    const KB: f32 = 1024.0;
    let b = bytes as f32;
    if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Bucket a SkipReason into a coarse label for the summary line.
/// Per-XObject reasons get the same bucket (e.g. all "unsupported
/// colorspace: X" go under "unsupported colorspace") so the end-of-pass
/// summary collapses to one line per category.
fn skip_bucket(reason: &SkipReason) -> String {
    match reason {
        SkipReason::TooSmall => "too small".to_string(),
        SkipReason::UnsupportedFilter(_) => "unsupported filter".to_string(),
        SkipReason::UnsupportedColorSpace(_) => "unsupported colorspace".to_string(),
        SkipReason::MissingDims => "missing dimensions".to_string(),
        SkipReason::Decode(_) => "decode failed".to_string(),
        SkipReason::Ocr(_) => "ocr failed".to_string(),
        SkipReason::Render(_) => "render failed".to_string(),
        SkipReason::NoTextDetected => "no text detected".to_string(),
    }
}

fn log_skip_summary(counts: &HashMap<String, usize>) {
    if counts.is_empty() {
        return;
    }
    let mut entries: Vec<(&String, &usize)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    let total: usize = counts.values().sum();
    let parts: Vec<String> = entries
        .iter()
        .map(|(reason, n)| format!("{n}× {reason}"))
        .collect();
    info!(
        "[pdf_image_translate] skipped {total} image XObject(s): {}",
        parts.join(", ")
    );
}

fn log_size_delta(label: &str, before: usize, after: usize) {
    let diff = after as i64 - before as i64;
    let sign = if diff >= 0 { "+" } else { "" };
    info!(
        "[pdf_image_translate] after {label}: {} ({}) [{sign}{} ({})]",
        after,
        format_size(after),
        diff,
        format_size(diff.unsigned_abs() as usize),
    );
}

/// Count image XObjects referenced by each page (0-based index → count).
/// Mirrors `collect_image_pages` but inverts the orientation; pages that
/// don't reference any image-XObject just don't appear.
fn page_image_counts(doc: &Document) -> HashMap<usize, usize> {
    let mut counts: HashMap<usize, usize> = HashMap::new();
    for (id, pages) in collect_image_pages(doc) {
        let _ = id;
        for p in pages {
            *counts.entry(p).or_insert(0) += 1;
        }
    }
    counts
}

fn is_image_stream(stream: &Stream) -> bool {
    let Ok(subtype) = stream.dict.get(b"Subtype") else {
        return false;
    };
    matches!(subtype.as_name().ok(), Some(name) if name == b"Image")
}

#[derive(Debug)]
enum SkipReason {
    TooSmall,
    UnsupportedFilter(String),
    UnsupportedColorSpace(String),
    MissingDims,
    Decode(String),
    Ocr(String),
    Render(String),
    NoTextDetected,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall => write!(f, "too small"),
            Self::UnsupportedFilter(s) => write!(f, "unsupported filter: {s}"),
            Self::UnsupportedColorSpace(s) => write!(f, "unsupported colorspace: {s}"),
            Self::MissingDims => write!(f, "missing /Width or /Height"),
            Self::Decode(s) => write!(f, "decode failed: {s}"),
            Self::Ocr(s) => write!(f, "ocr/translate failed: {s}"),
            Self::Render(s) => write!(f, "render failed: {s}"),
            Self::NoTextDetected => write!(f, "no text detected"),
        }
    }
}

/// Translate a single image XObject in place. Returns `Ok(true)` when the
/// stream was actually rewritten, `Ok(false)` when no rewrite was needed
/// (no detected text, etc.).
fn translate_one_xobject(
    doc: &Document,
    image_id: ObjectId,
    translator: &dyn ImageTranslator,
    source_code: &str,
    target_code: &str,
    fonts: &(dyn FontProvider + Send + Sync),
    orient: Orientation,
) -> Result<XobjectTranslation, SkipReason> {
    if orient == Orientation::Other {
        // Conflicting placements (same XObject used at incompatible
        // CTMs) or non-axis-aligned rotation/shear. We can't pick one
        // canonical orientation to OCR/render in, so leave the image
        // alone — mupdf will keep displaying the original correctly.
        return Err(SkipReason::UnsupportedFilter("non-trivial /cm".to_string()));
    }
    let (width, height, mut rgba, source_kind) = {
        let stream = doc
            .get_object(image_id)
            .map_err(|e| SkipReason::Decode(e.to_string()))?
            .as_stream()
            .map_err(|e| SkipReason::Decode(e.to_string()))?;
        decode_image_to_rgba(stream, doc)?
    };

    // Flip the decoded buffer into "natural display" orientation so
    // OCR + render see the image as the user does. After rendering we
    // flip back; mupdf's existing /cm will then apply the same flip
    // again at display time, presenting our work right-side-up.
    apply_orientation(&mut rgba, width, height, orient);

    let area = (width as u64) * (height as u64);
    if area < MIN_IMAGE_AREA_PX {
        return Err(SkipReason::TooSmall);
    }

    let prepared = translator
        .translate_image_rgba(
            &rgba,
            width,
            height,
            u32::MAX,
            translator_core::ocr::OcrSourceSelection::specific(LanguageCode::from(source_code)),
            target_code,
            DEFAULT_MIN_CONFIDENCE,
            Some(ReadingOrder::LeftToRight),
            BackgroundMode::AutoDetect,
            None,
        )
        .map_err(|e: TranslatorError| {
            // The OCR layer returns "No text found in image" as a normal
            // result for plain photos; treat that as a skip rather than
            // an error.
            if e.message.to_lowercase().contains("no text") {
                SkipReason::NoTextDetected
            } else {
                SkipReason::Ocr(e.message)
            }
        })?;

    if prepared.blocks.is_empty() {
        return Err(SkipReason::NoTextDetected);
    }

    let opts = RenderOptions {
        language: target_code.to_string(),
        ..RenderOptions::default()
    };
    let mut rendered = render_overlay(&prepared, fonts, &opts)
        .map_err(|e| SkipReason::Render(e.to_string()))?
        .rgba_bytes;

    // Debug hook: when XOBJECT_DUMP_DIR is set, write each translated
    // bitmap as a PNG side-by-side with the source bitmap. Lets us
    // inspect rendering anomalies (mirrored glyphs etc.) without going
    // through CCITT round-trip + mupdf rendering.
    if let Ok(dir) = std::env::var("XOBJECT_DUMP_DIR") {
        if let Err(e) = dump_bgra_pair(&dir, image_id, width, height, &rgba, &rendered) {
            warn!("[pdf_image_translate] dump failed: {e}");
        }
    }

    // Undo the orientation flip we applied before OCR. The encoded
    // bytes need to match the producer's data convention so the
    // existing /cm flips them back to right-side-up for display.
    apply_orientation(&mut rendered, width, height, orient);

    let _ = LanguageCode::from(source_code);

    // Match the output codec to the source flavor:
    // - bitonal scan in (CCITTFaxDecode) → bitonal G4 out, otherwise
    //   we'd ~30× bloat the page (JPEG-on-text is also visually
    //   inferior — DCT ringing on glyph edges).
    // - everything else → JPEG q85, where DCT is appropriate.
    let encoded_content = match source_kind {
        SourceKind::Ccitt => encode_ccitt_g4_from_bgra(&rendered, width, height),
        SourceKind::Other => {
            let rgb = bgra_to_rgb(&rendered);
            jpeg_encode_rgb(&rgb, width, height)
                .map_err(|e| SkipReason::Render(format!("jpeg: {e}")))?
        }
    };

    Ok(XobjectTranslation {
        encoded_content,
        width,
        height,
        source_kind,
    })
}

fn apply_xobject_result(
    doc: &mut Document,
    image_id: ObjectId,
    result: XobjectTranslation,
) -> Result<(), SkipReason> {
    let stream = doc
        .get_object_mut(image_id)
        .map_err(|e| SkipReason::Decode(e.to_string()))?
        .as_stream_mut()
        .map_err(|e| SkipReason::Decode(e.to_string()))?;
    stream.set_content(result.encoded_content);
    let dict = &mut stream.dict;
    match result.source_kind {
        SourceKind::Ccitt => {
            dict.set("Width", result.width as i64);
            dict.set("Height", result.height as i64);
            dict.set("BitsPerComponent", 1i64);
            dict.set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            dict.set("Filter", Object::Name(b"CCITTFaxDecode".to_vec()));
            let mut params = Dictionary::new();
            params.set("K", -1i64);
            params.set("Columns", result.width as i64);
            params.set("Rows", result.height as i64);
            params.set("BlackIs1", false);
            dict.set("DecodeParms", Object::Dictionary(params));
            dict.remove(b"SMask");
            dict.remove(b"Mask");
        }
        SourceKind::Other => {
            dict.set("Width", result.width as i64);
            dict.set("Height", result.height as i64);
            dict.set("BitsPerComponent", 8i64);
            dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
            dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
            dict.remove(b"DecodeParms");
            dict.remove(b"SMask");
            dict.remove(b"Mask");
        }
    }
    Ok(())
}

/// Hint about the *source* image format. Used to pick a matching output
/// codec so we don't bloat bitonal scans by re-encoding them as JPEG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    /// Was CCITT G3/G4 — bitonal scan. Round-trip as CCITT G4 to stay
    /// the same order of magnitude in size.
    Ccitt,
    /// Anything else (JPEG, raw RGB/Gray, Indexed, …). JPEG output is a
    /// reasonable default for natural-image content.
    Other,
}

fn decode_image_to_rgba(
    stream: &Stream,
    doc: &Document,
) -> Result<(u32, u32, Vec<u8>, SourceKind), SkipReason> {
    let dict = &stream.dict;
    let width = dict
        .get(b"Width")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .ok_or(SkipReason::MissingDims)? as u32;
    let height = dict
        .get(b"Height")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .ok_or(SkipReason::MissingDims)? as u32;
    if width == 0 || height == 0 {
        return Err(SkipReason::MissingDims);
    }
    let bits_per_component = dict
        .get(b"BitsPerComponent")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(8) as u8;
    let filter_chain = read_filter_chain(stream);
    let colorspace_kind = read_colorspace(stream, doc);
    trace!(
        "[pdf_image_translate]   {}x{} bpc={} filter={:?} colorspace={:?}",
        width, height, bits_per_component, filter_chain, colorspace_kind
    );

    // DCTDecode = JPEG. lopdf doesn't decode this; feed the raw stream
    // straight into the image crate. JPEG carries its own colorspace; we
    // ignore the PDF /ColorSpace because JPEG decode produces RGB
    // directly.
    if filter_chain.iter().any(|f| f == "DCTDecode") {
        if filter_chain.iter().any(|f| f != "DCTDecode") {
            return Err(SkipReason::UnsupportedFilter(format!("{filter_chain:?}")));
        }
        let img = image::load_from_memory_with_format(&stream.content, image::ImageFormat::Jpeg)
            .map_err(|e| SkipReason::Decode(e.to_string()))?;
        return Ok((
            img.width(),
            img.height(),
            img.to_rgba8().into_raw(),
            SourceKind::Other,
        ));
    }

    // CCITTFaxDecode = G3/G4 fax compression on bitonal scans. lopdf
    // doesn't ship a decoder so we drive the `fax` crate directly off
    // the raw stream bytes. After this branch the bitmap is plain
    // grayscale (1 byte/pixel: 0x00 black, 0xFF white).
    if filter_chain.iter().any(|f| f == "CCITTFaxDecode") {
        if filter_chain.iter().any(|f| f != "CCITTFaxDecode") {
            return Err(SkipReason::UnsupportedFilter(format!("{filter_chain:?}")));
        }
        let raw = decode_ccitt_fax(stream, width, height)?;
        return Ok((width, height, gray_to_rgba(&raw), SourceKind::Ccitt));
    }

    // For everything else lopdf can do flate/lzw/ascii85; let it handle
    // the unwrap and we'll deal with the raw pixel bytes.
    let raw = stream
        .decompressed_content()
        .map_err(|e| SkipReason::Decode(e.to_string()))?;

    // Edge case: some PDF producers (non-spec) embed entire PNG/JPEG
    // files inside a FlateDecode stream. If the decompressed bytes carry
    // a PNG or JPEG signature, decode as that file format and we're done.
    if has_png_signature(&raw) || has_jpeg_signature(&raw) {
        let img = image::load_from_memory(&raw).map_err(|e| SkipReason::Decode(e.to_string()))?;
        return Ok((
            img.width(),
            img.height(),
            img.to_rgba8().into_raw(),
            SourceKind::Other,
        ));
    }

    if bits_per_component != 8 {
        return Err(SkipReason::Decode(format!(
            "unsupported BitsPerComponent: {bits_per_component}"
        )));
    }

    match colorspace_kind {
        ColorSpaceKind::Rgb => {
            let needed = (width as usize) * (height as usize) * 3;
            if raw.len() < needed {
                return Err(SkipReason::Decode(format!(
                    "rgb payload {} bytes, need {needed}",
                    raw.len()
                )));
            }
            Ok((
                width,
                height,
                rgb_to_rgba(&raw[..needed]),
                SourceKind::Other,
            ))
        }
        ColorSpaceKind::Gray => {
            let needed = (width as usize) * (height as usize);
            if raw.len() < needed {
                return Err(SkipReason::Decode(format!(
                    "gray payload {} bytes, need {needed}",
                    raw.len()
                )));
            }
            Ok((
                width,
                height,
                gray_to_rgba(&raw[..needed]),
                SourceKind::Other,
            ))
        }
        ColorSpaceKind::Indexed { base_kind, lookup } => {
            let pixels = (width as usize) * (height as usize);
            if raw.len() < pixels {
                return Err(SkipReason::Decode(format!(
                    "indexed payload {} bytes, need {pixels}",
                    raw.len()
                )));
            }
            Ok((
                width,
                height,
                indexed_to_rgba(&raw[..pixels], &lookup, base_kind),
                SourceKind::Other,
            ))
        }
        ColorSpaceKind::Unsupported(name) => Err(SkipReason::UnsupportedColorSpace(name)),
    }
}

fn read_filter_chain(stream: &Stream) -> Vec<String> {
    let Ok(filter) = stream.dict.get(b"Filter") else {
        return Vec::new();
    };
    match filter {
        Object::Name(n) => vec![String::from_utf8_lossy(n).into_owned()],
        Object::Array(items) => items
            .iter()
            .filter_map(|i| i.as_name().ok())
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Debug)]
enum ColorSpaceKind {
    Rgb,
    Gray,
    /// `[/Indexed <base> <hival> <lookup>]` — `lookup` holds the
    /// palette: `(hival+1) * base_components` bytes. `base_kind` says
    /// what each palette entry expands to (RGB or Gray today).
    Indexed {
        base_kind: IndexedBase,
        lookup: Vec<u8>,
    },
    Unsupported(String),
}

#[derive(Debug, Clone, Copy)]
enum IndexedBase {
    Rgb,
    Gray,
}

impl IndexedBase {
    fn components(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Gray => 1,
        }
    }
}

/// Resolve `/ColorSpace` to a kind we can decode. Handles names directly,
/// arrays whose first element is a colorspace family name, indirect
/// references to either, and the common `[/ICCBased <stream>]` pattern by
/// reading the embedded stream's `/N` (component count).
fn read_colorspace(stream: &Stream, doc: &Document) -> ColorSpaceKind {
    let Ok(cs) = stream.dict.get(b"ColorSpace") else {
        return ColorSpaceKind::Unsupported("<missing>".to_string());
    };
    classify_colorspace(cs, doc)
}

fn classify_colorspace(obj: &Object, doc: &Document) -> ColorSpaceKind {
    match obj {
        Object::Name(n) => match n.as_slice() {
            b"DeviceRGB" | b"CalRGB" | b"RGB" => ColorSpaceKind::Rgb,
            b"DeviceGray" | b"CalGray" | b"G" => ColorSpaceKind::Gray,
            other => ColorSpaceKind::Unsupported(String::from_utf8_lossy(other).into_owned()),
        },
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(resolved) => classify_colorspace(resolved, doc),
            Err(_) => ColorSpaceKind::Unsupported(format!("<bad-ref {id:?}>")),
        },
        Object::Array(items) => {
            let Some(first) = items.first() else {
                return ColorSpaceKind::Unsupported("<empty array>".to_string());
            };
            // Resolve first element if it's itself a reference.
            let family_obj = match first {
                Object::Reference(id) => match doc.get_object(*id) {
                    Ok(resolved) => resolved,
                    Err(_) => return ColorSpaceKind::Unsupported("<bad family ref>".to_string()),
                },
                other => other,
            };
            let Ok(family) = family_obj.as_name() else {
                return ColorSpaceKind::Unsupported("<no family name>".to_string());
            };
            match family {
                b"ICCBased" => {
                    // Look up the params stream's /N to decide RGB vs Gray.
                    let n_components = items
                        .get(1)
                        .and_then(|o| match o {
                            Object::Reference(id) => doc.get_object(*id).ok(),
                            other => Some(other),
                        })
                        .and_then(|o| match o {
                            Object::Stream(s) => {
                                s.dict.get(b"N").ok().and_then(|n| n.as_i64().ok())
                            }
                            _ => None,
                        });
                    match n_components {
                        Some(1) => ColorSpaceKind::Gray,
                        Some(3) => ColorSpaceKind::Rgb,
                        Some(other) => ColorSpaceKind::Unsupported(format!("ICCBased N={other}")),
                        None => ColorSpaceKind::Unsupported("ICCBased <no /N>".to_string()),
                    }
                }
                b"DeviceRGB" | b"CalRGB" => ColorSpaceKind::Rgb,
                b"DeviceGray" | b"CalGray" => ColorSpaceKind::Gray,
                b"Indexed" => parse_indexed_colorspace(items, doc),
                other => ColorSpaceKind::Unsupported(String::from_utf8_lossy(other).into_owned()),
            }
        }
        _ => ColorSpaceKind::Unsupported(format!("{obj:?}")),
    }
}

/// `[/Indexed <base-cs> <hival> <lookup>]`. `lookup` is a literal byte
/// string or a stream reference; either way it's `(hival+1) *
/// base_components` bytes.
fn parse_indexed_colorspace(items: &[Object], doc: &Document) -> ColorSpaceKind {
    if items.len() < 4 {
        return ColorSpaceKind::Unsupported("Indexed <short array>".to_string());
    }
    let base_kind = match classify_colorspace(&items[1], doc) {
        ColorSpaceKind::Rgb => IndexedBase::Rgb,
        ColorSpaceKind::Gray => IndexedBase::Gray,
        other => {
            return ColorSpaceKind::Unsupported(format!("Indexed base={other:?}"));
        }
    };
    let hival = items
        .get(2)
        .and_then(|o| match o {
            Object::Integer(n) => Some(*n),
            Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| o.as_i64().ok()),
            _ => None,
        })
        .unwrap_or(-1);
    if !(0..=255).contains(&hival) {
        return ColorSpaceKind::Unsupported(format!("Indexed hival={hival}"));
    }
    let palette_bytes = (hival as usize + 1) * base_kind.components();
    let lookup = match items.get(3) {
        Some(Object::String(bytes, _)) => bytes.clone(),
        Some(Object::Reference(id)) => match doc.get_object(*id).ok() {
            Some(Object::Stream(s)) => s
                .decompressed_content()
                .unwrap_or_else(|_| s.content.clone()),
            Some(Object::String(bytes, _)) => bytes.clone(),
            _ => return ColorSpaceKind::Unsupported("Indexed <bad lookup ref>".to_string()),
        },
        Some(Object::Stream(s)) => s
            .decompressed_content()
            .unwrap_or_else(|_| s.content.clone()),
        _ => return ColorSpaceKind::Unsupported("Indexed <missing lookup>".to_string()),
    };
    if lookup.len() < palette_bytes {
        return ColorSpaceKind::Unsupported(format!(
            "Indexed lookup too short: {} bytes, need {}",
            lookup.len(),
            palette_bytes
        ));
    }
    ColorSpaceKind::Indexed { base_kind, lookup }
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let pixels = rgb.len() / 3;
    let mut out = Vec::with_capacity(pixels * 4);
    for px in rgb.chunks_exact(3) {
        // The renderer expects 4-byte ARGB-as-bytes: BGRA on
        // little-endian. Swap R/B to keep parity with the existing
        // `crate::ocr` byte order.
        out.push(px[2]); // B
        out.push(px[1]); // G
        out.push(px[0]); // R
        out.push(0xFF);
    }
    out
}

/// Decode a `CCITTFaxDecode` stream to a 1-byte-per-pixel grayscale buffer.
///
/// PDF /DecodeParms keys honored:
/// - `K`: encoding flavor. K < 0 → G4. K == 0 → G3 1D. K > 0 → G3 mixed
///   1D/2D, treated as G3 (rare in practice).
/// - `Columns`: row width in pixels (default 1728).
/// - `BlackIs1`: if true, "1" bits are black (default false → 1=white).
/// - `Rows` / `/Height`: image height; G4 needs it explicitly.
///
/// Output buffer is `width * height` bytes; `0x00` for black, `0xFF` for
/// white. If decoding emits fewer rows than expected (truncated stream),
/// remaining rows are padded white so the buffer is always the right size.
fn decode_ccitt_fax(stream: &Stream, width: u32, height: u32) -> Result<Vec<u8>, SkipReason> {
    let params = stream
        .dict
        .get(b"DecodeParms")
        .and_then(|o| o.as_dict())
        .ok();
    let k = params
        .and_then(|d| d.get(b"K").ok())
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    let columns = params
        .and_then(|d| d.get(b"Columns").ok())
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(1728)
        .max(1) as u32;
    let black_is_1 = params
        .and_then(|d| d.get(b"BlackIs1").ok())
        .and_then(|o| o.as_bool().ok())
        .unwrap_or(false);
    if columns != width {
        // Spec lets /Columns differ from /Width but in practice it doesn't.
        // If they ever do, trust /Columns for the decoder geometry and pad
        // / clip on copy below.
    }

    let total = (width as usize) * (height as usize);
    let mut out = vec![0xFFu8; total];
    let row_stride = width as usize;
    let columns_u16 = columns.min(u16::MAX as u32) as u16;
    let height_u16 = height.min(u16::MAX as u32) as u16;
    let mut row_idx: u32 = 0;

    let mut emit = |transitions: &[u16]| {
        if row_idx >= height {
            return;
        }
        let start = row_idx as usize * row_stride;
        for (col, color) in fax::decoder::pels(transitions, columns_u16).enumerate() {
            if col >= row_stride {
                break;
            }
            // The `fax` decoder reports each pel as White or Black per
            // its run-length stream. PDF's `BlackIs1` flag just swaps
            // the polarity we serialize: with the default (false) a
            // White pel is 0xFF and Black is 0x00.
            let v = match (color, black_is_1) {
                (fax::Color::White, false) | (fax::Color::Black, true) => 0xFF,
                (fax::Color::Black, false) | (fax::Color::White, true) => 0x00,
            };
            out[start + col] = v;
        }
        row_idx += 1;
    };

    let bytes = stream.content.iter().copied();
    let result = if k < 0 {
        fax::decoder::decode_g4(bytes, columns_u16, Some(height_u16), emit)
    } else {
        fax::decoder::decode_g3(bytes, |line| emit(line))
    };
    if result.is_none() && row_idx == 0 {
        return Err(SkipReason::Decode("CCITT fax decode failed".to_string()));
    }
    Ok(out)
}

fn indexed_to_rgba(indices: &[u8], lookup: &[u8], base: IndexedBase) -> Vec<u8> {
    let n = base.components();
    let mut out = Vec::with_capacity(indices.len() * 4);
    for &idx in indices {
        let off = idx as usize * n;
        match base {
            IndexedBase::Rgb => {
                let r = lookup.get(off).copied().unwrap_or(0);
                let g = lookup.get(off + 1).copied().unwrap_or(0);
                let b = lookup.get(off + 2).copied().unwrap_or(0);
                // Renderer expects BGRA byte order on little-endian.
                out.push(b);
                out.push(g);
                out.push(r);
                out.push(0xFF);
            }
            IndexedBase::Gray => {
                let v = lookup.get(off).copied().unwrap_or(0);
                out.push(v);
                out.push(v);
                out.push(v);
                out.push(0xFF);
            }
        }
    }
    out
}

fn gray_to_rgba(gray: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(gray.len() * 4);
    for &v in gray {
        out.push(v);
        out.push(v);
        out.push(v);
        out.push(0xFF);
    }
    out
}

fn bgra_to_rgb(bgra: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bgra.len() / 4 * 3);
    for px in bgra.chunks_exact(4) {
        out.push(px[2]); // R from BGRA[2]
        out.push(px[1]); // G
        out.push(px[0]); // B
    }
    out
}

fn has_png_signature(b: &[u8]) -> bool {
    b.len() >= 8 && b[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
}

fn has_jpeg_signature(b: &[u8]) -> bool {
    b.len() >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF
}

/// Threshold a translated BGRA bitmap to bitonal and re-encode as
/// CCITT Group 4. Used when the source XObject was CCITTFaxDecode —
/// re-emitting as JPEG would balloon a typical scanned page from a
/// few hundred KB to several MB while *also* introducing DCT ringing
/// around glyph edges.
fn dump_bgra_pair(
    dir: &str,
    id: ObjectId,
    width: u32,
    height: u32,
    src_bgra: &[u8],
    rendered_bgra: &[u8],
) -> std::io::Result<()> {
    use std::fs;
    use std::path::PathBuf;
    let dir = PathBuf::from(dir);
    fs::create_dir_all(&dir)?;
    let stem = format!("xobj_{}_{}_{}x{}", id.0, id.1, width, height);
    save_bgra_as_png(
        &dir.join(format!("{stem}_src.png")),
        width,
        height,
        src_bgra,
    )?;
    save_bgra_as_png(
        &dir.join(format!("{stem}_rendered.png")),
        width,
        height,
        rendered_bgra,
    )?;
    Ok(())
}

fn save_bgra_as_png(
    path: &std::path::Path,
    width: u32,
    height: u32,
    bgra: &[u8],
) -> std::io::Result<()> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        rgba.push(px[2]);
        rgba.push(px[1]);
        rgba.push(px[0]);
        rgba.push(px[3]);
    }
    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| std::io::Error::other("bad image dims"))?;
    img.save(path)
        .map_err(|e| std::io::Error::other(format!("png save: {e}")))?;
    Ok(())
}

fn encode_ccitt_g4_from_bgra(bgra: &[u8], width: u32, height: u32) -> Vec<u8> {
    use fax::encoder::Encoder;
    use fax::{Color, VecWriter};
    let mut encoder = Encoder::new(VecWriter::new());
    let row_stride = width as usize * 4;
    let columns = width.min(u16::MAX as u32) as u16;
    for row in 0..height as usize {
        let start = row * row_stride;
        let pels = bgra[start..start + row_stride].chunks_exact(4).map(|px| {
            // BGRA: index 2 is R. Compute luma (Rec.709 weights).
            let r = px[2] as u32;
            let g = px[1] as u32;
            let b = px[0] as u32;
            let luma = (2126 * r + 7152 * g + 722 * b) / 10_000;
            if luma < 128 {
                Color::Black
            } else {
                Color::White
            }
        });
        // VecWriter's error type is Infallible, so this can't actually
        // fail in practice.
        let _ = encoder.encode_line(pels, columns);
    }
    let writer = match encoder.finish() {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };
    writer.finish()
}

/// Encode interleaved 8-bit RGB to a JPEG byte stream. Quality 85 is the
/// usual "indistinguishable from lossless for photos" knob; higher costs
/// size for no perceptible gain. For Guru-class page rasters (~800×1100
/// flat-color pages) the resulting JPEG is comparable in size to the
/// original DCT-encoded XObject, instead of the ~10× blowup we got
/// with flate-on-RGB.
fn jpeg_encode_rgb(rgb: &[u8], width: u32, height: u32) -> Result<Vec<u8>, image::ImageError> {
    let mut out: Vec<u8> = Vec::with_capacity(rgb.len() / 4);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    encoder.encode(rgb, width, height, image::ExtendedColorType::Rgb8)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Page-as-raster fallback for PDFs whose body content is outlined vector
// glyphs (CorelDRAW, design-tool exports). Neither text extraction nor
// image-XObject translation finds anything; the only path is to rasterize
// the page, OCR it, and replace the page's content stream.

/// DPI to render PDF pages at when running them through the OCR pipeline.
/// 200 is a balance between OCR accuracy (~300 is ideal) and PDF
/// payload size (200 DPI A4 ≈ 1700×2200 = 3.7M pixels).
const RASTER_PAGE_DPI: f32 = 200.0;

/// Number of worker threads that run mupdf render + OCR + image_render +
/// flate compress in parallel.
const RASTER_WORKERS: usize = 4;

/// Run a PDF-text overlay pass on every page that has no extractable
/// text. Pages that already have extractable text are left alone for the
/// existing text-translation pipeline.
///
/// For each affected page: rasterize → OCR + translate → emit a content
/// stream that masks each detected line's bbox with its sampled
/// background color and draws the translated text on top, using
/// document-wide embedded font subsets. The original page `/Contents`
/// is preserved (lopdf turns it into a `[orig, overlay]` array), so
/// non-text vector content (logos, rules, photos) survives unchanged.
///
/// Worker output: just the OCR plan + raster size. Workers don't touch
/// the lopdf::Document — they OCR in parallel and the main thread
/// performs all PDF mutation once they're done.
struct PageOcrResult {
    page_index: usize,
    overlay: PreparedImageOverlay,
}

pub fn translate_pdf_pages_as_raster_in_place(
    pdf_bytes: &[u8],
    translator: &dyn ImageTranslator,
    source_code: &str,
    target: &translator_core::api::ScriptedLanguage,
    fonts: &(dyn FontProvider + Send + Sync),
    pages_without_text: &HashSet<usize>,
    is_cancelled: &(dyn Fn() -> bool + Send + Sync),
    mut on_progress: impl FnMut(usize, usize),
) -> Result<Vec<u8>, ImageTranslateError> {
    if pages_without_text.is_empty() {
        return Ok(pdf_bytes.to_vec());
    }
    info!(
        "[pdf_image_translate] {} page(s) have no extractable text; rasterizing for OCR ({} workers)",
        pages_without_text.len(),
        RASTER_WORKERS
    );

    let mut doc = Document::load_mem(pdf_bytes)?;
    let pages: Vec<(u32, ObjectId)> = doc.get_pages().into_iter().collect();
    // Build a Vec to hand to workers (need indexable, deterministic order).
    // Sorted so the round-robin progress feels left-to-right through the doc.
    let mut pages_to_do: Vec<usize> = pages_without_text.iter().copied().collect();
    pages_to_do.sort();
    let total = pages_to_do.len();
    on_progress(0, total);

    // Worker dispatch via shared atomic counter. Each worker fetches the
    // next index, OCRs the rasterized page, and pushes the OCR plan on
    // the mpsc channel. Workers do not touch the lopdf::Document — all
    // mutation happens after collection so we can build a doc-wide
    // font subset.
    //
    // Cancellation: workers check is_cancelled before each fetch_add
    // and before send; in-flight OCR can't be interrupted (Tesseract is
    // a blocking C call), so worst case 4 pages finish after cancel.
    let next_page = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<Result<PageOcrResult, (usize, SkipReason)>>();

    let mut collected: Vec<PageOcrResult> = Vec::with_capacity(pages_to_do.len());
    let mut processed = 0usize;
    let mut page_skip_counts: HashMap<String, usize> = HashMap::new();
    let mut page_translated_count = 0usize;
    let cancelled_during_collect = std::cell::Cell::new(false);

    thread::scope(|scope| {
        for _ in 0..RASTER_WORKERS {
            let tx = tx.clone();
            let pages_to_do = &pages_to_do;
            let next_page = &next_page;
            scope.spawn(move || {
                // Each worker owns its own mupdf::Document — it isn't Sync
                // and a fresh open over the same bytes is cheap.
                let mupdf_doc = match mupdf::Document::from_bytes(pdf_bytes, "application/pdf") {
                    Ok(d) => d,
                    Err(err) => {
                        warn!("[pdf_image_translate] worker mupdf load failed: {err}");
                        return;
                    }
                };
                loop {
                    if is_cancelled() {
                        break;
                    }
                    let slot = next_page.fetch_add(1, Ordering::Relaxed);
                    if slot >= pages_to_do.len() {
                        break;
                    }
                    let page_index = pages_to_do[slot];
                    if is_cancelled() {
                        break;
                    }
                    let result = ocr_page(
                        &mupdf_doc,
                        translator,
                        source_code,
                        target.as_str(),
                        page_index,
                    )
                    .map_err(|reason| (page_index, reason));
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);

        // Drain results. Per-page progress increments as OCR completes,
        // matching the previous installation cadence even though all
        // PDF mutation now happens after this loop.
        while let Ok(result) = rx.recv() {
            if is_cancelled() {
                cancelled_during_collect.set(true);
                continue;
            }
            match result {
                Ok(r) => {
                    trace!(
                        "[pdf_image_translate] page {} OCR'd + translated ({} block(s))",
                        r.page_index,
                        r.overlay.blocks.len()
                    );
                    page_translated_count += 1;
                    collected.push(r);
                }
                Err((page_index, reason)) => {
                    trace!("[pdf_image_translate] page {page_index} skipped: {reason}");
                    *page_skip_counts.entry(skip_bucket(&reason)).or_insert(0) += 1;
                }
            }
            processed += 1;
            on_progress(processed, total);
        }
    });

    if page_translated_count > 0 {
        info!(
            "[pdf_image_translate] page-raster: OCR'd + translated {page_translated_count} page(s)"
        );
    }
    if !page_skip_counts.is_empty() {
        let total_skipped: usize = page_skip_counts.values().sum();
        let mut entries: Vec<(&String, &usize)> = page_skip_counts.iter().collect();
        entries.sort_by(|a, b| b.1.cmp(a.1));
        let parts: Vec<String> = entries
            .iter()
            .map(|(reason, n)| format!("{n}× {reason}"))
            .collect();
        info!(
            "[pdf_image_translate] page-raster: skipped {total_skipped} page(s): {}",
            parts.join(", ")
        );
    }

    if cancelled_during_collect.get() {
        debug!("[pdf_image_translate] page-raster pass cancelled mid-flight");
    }
    if collected.is_empty() {
        return Ok(pdf_bytes.to_vec());
    }
    if is_cancelled() {
        return Ok(pdf_bytes.to_vec());
    }

    // Doc-wide font plan (one subset per (script, language, style),
    // covering the union of all translated text).
    let overlay_pages: Vec<OverlayPage> = collected
        .into_iter()
        .filter_map(|r| {
            let page_id = pages.iter().find_map(|(num, id)| {
                if (*num as usize).saturating_sub(1) == r.page_index {
                    Some(*id)
                } else {
                    None
                }
            })?;
            let geom = PageGeometry::read(&doc, page_id, None);
            Some(OverlayPage {
                page_index: r.page_index,
                geom,
                dpi: RASTER_PAGE_DPI,
                overlay: r.overlay,
            })
        })
        .collect();
    if overlay_pages.is_empty() {
        return Ok(pdf_bytes.to_vec());
    }

    let plan = build_overlay_font_plan(&mut doc, &overlay_pages, target, fonts);
    let used_embeds = collect_used_embed_names(&plan);

    let mut any_installed = false;
    for page in &overlay_pages {
        let Some(page_id) = pages.iter().find_map(|(num, id)| {
            if (*num as usize).saturating_sub(1) == page.page_index {
                Some(*id)
            } else {
                None
            }
        }) else {
            continue;
        };
        let stream = build_page_overlay_stream(page, &plan);
        if let Err(err) = install_overlay_on_page(&mut doc, page_id, stream, &used_embeds, &plan) {
            debug!(
                "[pdf_image_translate] page {} install failed: {err}",
                page.page_index
            );
            continue;
        }
        any_installed = true;
    }

    let _ = LanguageCode::from(source_code);
    if !any_installed {
        return Ok(pdf_bytes.to_vec());
    }

    let mut out = Vec::new();
    doc.save_to(&mut out).map_err(lopdf::Error::IO)?;
    log_size_delta("page-raster pass", pdf_bytes.len(), out.len());
    Ok(out)
}

/// Worker step: rasterize the page, OCR + translate it, return the OCR
/// plan + raster size. No lopdf state is touched — the main thread
/// performs all PDF mutation after worker collection.
fn ocr_page(
    mupdf_doc: &mupdf::Document,
    translator: &dyn ImageTranslator,
    source_code: &str,
    target_code: &str,
    page_index: usize,
) -> Result<PageOcrResult, SkipReason> {
    let page = mupdf_doc
        .load_page(page_index as i32)
        .map_err(|e| SkipReason::Decode(format!("mupdf load_page: {e}")))?;
    let bounds = page
        .bounds()
        .map_err(|e| SkipReason::Decode(format!("page bounds: {e}")))?;
    let page_w_pts = bounds.x1 - bounds.x0;
    let page_h_pts = bounds.y1 - bounds.y0;
    if page_w_pts <= 0.0 || page_h_pts <= 0.0 {
        return Err(SkipReason::MissingDims);
    }

    let scale = RASTER_PAGE_DPI / 72.0;
    let ctm = mupdf::Matrix::new_scale(scale, scale);
    let pixmap = page
        .to_pixmap(&ctm, &mupdf::Colorspace::device_rgb(), true, false)
        .map_err(|e| SkipReason::Decode(format!("to_pixmap: {e}")))?;
    if pixmap.n() != 4 {
        return Err(SkipReason::Decode(format!(
            "expected RGBA pixmap, got n={}",
            pixmap.n()
        )));
    }
    let width = pixmap.width();
    let height = pixmap.height();
    // mupdf hands us RGBA; the OCR pipeline operates on BGRA byte order
    // (u32 ARGB on little-endian). Swap R/B going in.
    let mut bgra = pixmap.samples().to_vec();
    swap_r_b(&mut bgra);

    let prepared = translator
        .translate_image_rgba(
            &bgra,
            width,
            height,
            u32::MAX,
            translator_core::ocr::OcrSourceSelection::specific(LanguageCode::from(source_code)),
            target_code,
            DEFAULT_MIN_CONFIDENCE,
            Some(ReadingOrder::LeftToRight),
            BackgroundMode::AutoDetect,
            None,
        )
        .map_err(|e: TranslatorError| {
            if e.message.to_lowercase().contains("no text") {
                SkipReason::NoTextDetected
            } else {
                SkipReason::Ocr(e.message)
            }
        })?;
    if prepared.blocks.is_empty() {
        return Err(SkipReason::NoTextDetected);
    }

    let _ = (width, height);
    Ok(PageOcrResult {
        page_index,
        overlay: prepared,
    })
}

fn swap_r_b(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_to_rgba_swaps_to_bgra_byte_order() {
        let rgb = vec![10, 20, 30, 40, 50, 60];
        let bgra = rgb_to_rgba(&rgb);
        // Pixel 0: R=10, G=20, B=30 -> bytes [30, 20, 10, 255].
        assert_eq!(&bgra[..4], &[30, 20, 10, 255]);
        assert_eq!(&bgra[4..], &[60, 50, 40, 255]);
    }

    #[test]
    fn bgra_to_rgb_round_trips() {
        let rgb_in = vec![10, 20, 30, 99, 0, 0];
        let bgra = rgb_to_rgba(&rgb_in);
        let rgb_out = bgra_to_rgb(&bgra);
        assert_eq!(rgb_out, rgb_in);
    }

    /// Round-trip a pattern where ONLY row 0 is black: if the encoder
    /// or decoder reverses the y-axis, we'd see the black band at row 3
    /// after the trip.
    #[test]
    fn ccitt_g4_top_row_marker_survives_round_trip() {
        let w = 16u32;
        let h = 4u32;
        let row_stride = w as usize * 4;
        let mut bgra = vec![0xFFu8; row_stride * h as usize];
        // Row 0: all black.
        for col in 0..w as usize {
            let off = col * 4;
            bgra[off] = 0x00;
            bgra[off + 1] = 0x00;
            bgra[off + 2] = 0x00;
            bgra[off + 3] = 0xFF;
        }

        let g4 = encode_ccitt_g4_from_bgra(&bgra, w, h);
        let mut dict = Dictionary::new();
        let mut params = Dictionary::new();
        params.set("K", -1i64);
        params.set("Columns", w as i64);
        params.set("Rows", h as i64);
        params.set("BlackIs1", false);
        dict.set("DecodeParms", Object::Dictionary(params));
        let stream = Stream::new(dict, g4);
        let gray = decode_ccitt_fax(&stream, w, h).expect("decode ccitt");

        let row0_black = gray[..w as usize].iter().all(|&v| v == 0x00);
        let row3_black = gray[3 * w as usize..].iter().all(|&v| v == 0x00);
        assert!(
            row0_black,
            "row 0 should be black, got {:?}",
            &gray[..w as usize]
        );
        assert!(
            !row3_black,
            "row 3 should NOT be black (would mean y was flipped)"
        );
    }

    /// Encode a known top-down BGRA bitmap as G4, then decode it via the
    /// same path used at read-time, and verify the orientation survives
    /// the round-trip. If this fails the FM 22-100 page-8 vertical flip
    /// is reproducible at unit-test speed.
    #[test]
    fn ccitt_g4_round_trip_preserves_orientation() {
        // 8x4 image: row 0 = all white, row 1 = all black, row 2 = all
        // white, row 3 = all black. Stripes parallel to x.
        let w = 8u32;
        let h = 4u32;
        let row_stride = w as usize * 4;
        let mut bgra = vec![0u8; row_stride * h as usize];
        for row in 0..h as usize {
            let v = if row % 2 == 0 { 0xFF } else { 0x00 };
            for col in 0..w as usize {
                let off = row * row_stride + col * 4;
                bgra[off] = v;
                bgra[off + 1] = v;
                bgra[off + 2] = v;
                bgra[off + 3] = 0xFF;
            }
        }

        let g4 = encode_ccitt_g4_from_bgra(&bgra, w, h);
        assert!(!g4.is_empty(), "empty CCITT output");

        // Build a synthetic Stream so decode_ccitt_fax sees the right
        // /DecodeParms/Width/Height/BlackIs1.
        let mut dict = Dictionary::new();
        let mut params = Dictionary::new();
        params.set("K", -1i64);
        params.set("Columns", w as i64);
        params.set("Rows", h as i64);
        params.set("BlackIs1", false);
        dict.set("DecodeParms", Object::Dictionary(params));
        let stream = Stream::new(dict, g4);

        let gray = decode_ccitt_fax(&stream, w, h).expect("decode ccitt");
        assert_eq!(gray.len(), (w * h) as usize);
        let row0: Vec<u8> = gray[0..w as usize].to_vec();
        let row1: Vec<u8> = gray[w as usize..2 * w as usize].to_vec();
        let row2: Vec<u8> = gray[2 * w as usize..3 * w as usize].to_vec();
        let row3: Vec<u8> = gray[3 * w as usize..4 * w as usize].to_vec();
        assert!(row0.iter().all(|&v| v == 0xFF), "row 0 not white: {row0:?}");
        assert!(row1.iter().all(|&v| v == 0x00), "row 1 not black: {row1:?}");
        assert!(row2.iter().all(|&v| v == 0xFF), "row 2 not white: {row2:?}");
        assert!(row3.iter().all(|&v| v == 0x00), "row 3 not black: {row3:?}");
    }

    #[test]
    fn finds_jpeg_image_xobject_in_naur_pdf() {
        let path = "files/1985-naur.pdf";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skipping: {path} not present");
            return;
        };
        let doc = Document::load_mem(&bytes).expect("load pdf");
        let ids: Vec<ObjectId> = collect_image_pages(&doc).keys().copied().collect();
        assert!(
            !ids.is_empty(),
            "expected at least one image XObject in {path}"
        );
        // Decode the first one — naur is a scanned-page JPEG.
        let stream = doc
            .get_object(ids[0])
            .expect("get xobject")
            .as_stream()
            .expect("stream");
        let (w, h, rgba, _kind) = decode_image_to_rgba(stream, &doc).expect("decode jpeg");
        assert!(w >= 100 && h >= 100, "naur scan should be big");
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
    }

    #[test]
    fn decodes_iccbased_jpeg_in_guru_menu() {
        let path = "files/Guru-menu.pdf";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("skipping: {path} not present");
            return;
        };
        let doc = Document::load_mem(&bytes).expect("load pdf");
        let ids: Vec<ObjectId> = collect_image_pages(&doc).keys().copied().collect();
        assert!(
            ids.len() >= 1,
            "expected ICCBased JPEG XObjects in {path}, got {}",
            ids.len()
        );
        // Each page is one big JPEG. Decode the first to confirm
        // ICCBased(3) doesn't trip the colorspace check.
        let stream = doc
            .get_object(ids[0])
            .expect("get xobject")
            .as_stream()
            .expect("stream");
        let (w, h, rgba, _kind) =
            decode_image_to_rgba(stream, &doc).expect("decode iccbased jpeg in guru menu");
        assert!(w >= 100 && h >= 100, "guru page should be big: {w}x{h}");
        assert_eq!(rgba.len(), (w as usize) * (h as usize) * 4);
    }
}
