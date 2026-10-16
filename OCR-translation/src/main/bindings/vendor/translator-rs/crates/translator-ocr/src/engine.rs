use std::sync::{Arc, Mutex};

use translator_core::api::{TranslatorError, TranslatorErrorKind};
use translator_core::catalog::{
    CatalogSnapshot, FileRole, OcrPack, PackKind, PackRecord, PpocrScript,
};
use translator_core::coords::Quadrant;
use translator_core::ocr::{DetectedTextBox, RecognizedTextLine};
use translator_raster::live_frame::OrientedImage;

use crate::ocr_runtime::route_ppocr_predictions;
use crate::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpocrEngineKey {
    detector_path: String,
    classifier_path: Option<String>,
    textline_orientation_path: Option<String>,
    ink_path: Option<String>,
    recognizers: Vec<(PpocrScript, String, String)>,
}

pub struct OcrEngine {
    warm: Mutex<Option<(PpocrEngineKey, Arc<PpocrEngine>)>>,
}

impl Default for OcrEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrEngine {
    pub fn new() -> Self {
        Self {
            warm: Mutex::new(None),
        }
    }

    pub fn engine(&self, snap: &CatalogSnapshot) -> Result<Arc<PpocrEngine>, TranslatorError> {
        let catalog = &snap.catalog;
        let pack_installed = |pack_id: &str| {
            snap.pack_statuses
                .get(pack_id)
                .map(|s| s.installed)
                .unwrap_or(false)
        };

        let (det_pack_id, det_pack) = catalog
            .packs
            .iter()
            .find(|(_, pack)| matches!(&pack.kind, PackKind::Ocr(OcrPack::PpocrDetector)))
            .ok_or_else(|| {
                TranslatorError::new(
                    TranslatorErrorKind::MissingAsset,
                    "ppocr detector pack missing from catalog",
                )
            })?;
        if !pack_installed(det_pack_id) {
            return Err(TranslatorError::new(
                TranslatorErrorKind::MissingAsset,
                "ppocr detector pack is not installed",
            ));
        }

        let base = std::path::Path::new(&snap.base_dir);
        let best_present = |pack: &PackRecord, role: &FileRole| {
            pack.role_alternatives(role)
                .into_iter()
                .map(|f| base.join(&f.install_path))
                .find(|path| path.exists())
        };
        let det_path = best_present(det_pack, &FileRole::Detector).ok_or_else(|| {
            TranslatorError::missing_asset("ppocr detector pack has no detector model on disk")
        })?;
        let classifier_path = best_present(det_pack, &FileRole::ScriptClassifier);
        let textline_orientation_path = best_present(det_pack, &FileRole::TextlineOrientation);

        let mut specs = Vec::new();
        for (pack_id, pack) in &catalog.packs {
            if !pack_installed(pack_id) {
                continue;
            }
            let PackKind::Ocr(OcrPack::PpocrRecognizer { script }) = &pack.kind else {
                continue;
            };
            // A recognizer's charset must match its model, so alternates are
            // paired by priority: take the best recognizer whose same-priority
            // keys file is also on disk. A half-downloaded upgrade then falls
            // back to the older complete pair instead of mixing generations.
            let pair = pack
                .role_alternatives(&FileRole::Recognizer)
                .into_iter()
                .find_map(|rec| {
                    let model_path = base.join(&rec.install_path);
                    if !model_path.exists() {
                        return None;
                    }
                    let keys_path = pack
                        .role_alternatives(&FileRole::Keys)
                        .into_iter()
                        .find(|keys| keys.priority == rec.priority)
                        .map(|keys| base.join(&keys.install_path))
                        .filter(|path| path.exists())?;
                    Some((model_path, keys_path))
                });
            let Some((model_path, keys_path)) = pair else {
                continue;
            };
            specs.push(PpocrRecognizerSpec {
                script: *script,
                model_path,
                keys_path,
            });
        }
        specs.sort_by_key(|s| s.script.as_slug());

        // Optional ink-matte model. Resolved via the detector pack's declared install
        // path (not det_path.parent() — det may be a different OCR version dir).
        let ink_path = best_present(det_pack, &FileRole::Ink);

        let key = PpocrEngineKey {
            detector_path: det_path.to_string_lossy().into_owned(),
            classifier_path: classifier_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            textline_orientation_path: textline_orientation_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            ink_path: ink_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            recognizers: specs
                .iter()
                .map(|s| {
                    (
                        s.script,
                        s.model_path.to_string_lossy().into_owned(),
                        s.keys_path.to_string_lossy().into_owned(),
                    )
                })
                .collect(),
        };

        let mut cache = self.warm.lock().expect("ppocr cache poisoned");
        if let Some((cached_key, engine)) = cache.as_ref() {
            if cached_key == &key {
                return Ok(Arc::clone(engine));
            }
        }
        let engine = PpocrEngine::load(
            &det_path,
            classifier_path.as_deref(),
            textline_orientation_path.as_deref(),
            specs,
            4,
            ink_path.as_deref(),
        )?;
        let engine = Arc::new(engine);
        *cache = Some((key, Arc::clone(&engine)));
        Ok(engine)
    }
}

pub fn recognize_oriented(
    ppocr: &PpocrEngine,
    oriented: &OrientedImage,
    boxes: &[DetectedTextBox],
    script: Option<PpocrScript>,
    canonical_quadrant: Option<Quadrant>,
) -> Result<Vec<RecognizedTextLine>, TranslatorError> {
    let rgb = oriented
        .rgb
        .as_ref()
        .expect("recognize path requires build_with_rgb");
    let scripts = match script {
        None => {
            let predictions = ppocr.classify_text_boxes_image(rgb, boxes, canonical_quadrant)?;
            route_ppocr_predictions(ppocr, &predictions, boxes)?
        }
        Some(script) => vec![script; boxes.len()],
    };
    ppocr.recognize_text_in_boxes_image(
        rgb,
        boxes,
        &scripts,
        PpocrProfile::Live,
        canonical_quadrant,
    )
}
