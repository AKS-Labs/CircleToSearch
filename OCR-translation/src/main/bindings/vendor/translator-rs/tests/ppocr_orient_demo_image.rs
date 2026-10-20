//! End-to-end sanity check on the PaddleX-shipped demo image
//! `img_textline180_demo_res.jpg` (which Paddle classifies as
//! `180_degree @ 0.992`). We feed the exact same file straight to our
//! `textline_orientation_classify`, with no detect / dewarp / windowing,
//! and report what we get for the raw image AND its 180° rotation.
//!
//! If our pipeline matches Paddle (Flipped180 high conf for the raw,
//! Up high conf for the 180° rotation), the model is being called
//! correctly. If not, something between Paddle's preprocessing and
//! ours is misaligned (scale/mean/std/channel order/resize semantics).
//!
//! Override paths via `OCR_DEMO_*` env vars; defaults assume the
//! upstream demo bundle is sitting at /tmp/textline-ori-demo/.

#![cfg(all(feature = "ppocr", feature = "planar-tracker"))]

use std::path::{Path, PathBuf};

use image::DynamicImage;

use translator::PpocrScript;
use translator::ppocr::{PpocrEngine, PpocrRecognizerSpec, TextlineOriCandidate};

const MODEL_DIR: &str = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5";
const DEMO_DIR: &str = "/tmp/textline-ori-demo/PP-LCNet_x1_0_textline_ori_infer";

#[test]
fn demo_image_matches_paddle_reference() {
    let _ = env_logger::builder().is_test(true).try_init();

    let Some(textline_ori) = textline_ori_path() else {
        eprintln!("textline-ori model missing under {MODEL_DIR}; skipping");
        return;
    };
    let demo_path = Path::new(DEMO_DIR).join("img_textline180_demo_res.jpg");
    if !demo_path.exists() {
        eprintln!("demo image {} missing; skipping", demo_path.display());
        return;
    }

    eprintln!("loading model: {}", textline_ori.display());
    eprintln!("loading demo:  {}", demo_path.display());

    let detector_path = env_path("OCR_DEMO_DET")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("PP-OCRv5_mobile_det.mnn"));
    let rec_path = env_path("OCR_DEMO_REC")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_mobile_rec_infer.mnn"));
    let keys_path = env_path("OCR_DEMO_KEYS")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("latin_PP-OCRv5_keys.txt"));

    let engine = PpocrEngine::load(
        &detector_path,
        None,
        Some(&textline_ori),
        vec![PpocrRecognizerSpec {
            script: PpocrScript::Latin,
            model_path: rec_path,
            keys_path,
        }],
        1,
        None,
    )
    .expect("load ppocr");

    let raw = image::open(&demo_path).expect("open demo");
    let raw180 = raw.rotate180();

    // Feed both to the classifier in a single batch. No detect, no
    // dewarp, no windowing — just the raw image like Paddle does.
    let inputs: Vec<DynamicImage> = vec![raw.clone(), raw180.clone()];
    let labels = engine
        .textline_orientation_classify(&inputs)
        .expect("classify");

    eprintln!("raw    → {}", fmt(&labels[0]));
    eprintln!("raw180 → {}", fmt(&labels[1]));
    eprintln!();
    eprintln!("Paddle reference for raw: 180_degree @ 0.9926");
    eprintln!("If our `raw` answer is Flipped180 @ ~0.99, preprocessing matches.");
    eprintln!(
        "If our `raw180` answer is Up @ ~0.99, the model also discriminates correctly on this content."
    );

    // Diagnostic only — don't assert; the user wants to read the numbers.
}

fn fmt(c: &Option<TextlineOriCandidate>) -> String {
    match c {
        Some(c) => format!("{:?} @ {:.4}", c.label, c.score),
        None => "None".to_string(),
    }
}

fn textline_ori_path() -> Option<PathBuf> {
    let raw = env_path("OCR_DEMO_TEXTLINE")
        .unwrap_or_else(|| Path::new(MODEL_DIR).join("textline_ori_x1_0_wq8.mnn"));
    if raw.exists() { Some(raw) } else { None }
}

fn env_path(key: &str) -> Option<PathBuf> {
    let raw = std::env::var(key).ok()?;
    if let Some(rest) = raw.strip_prefix("~/") {
        let home = std::env::var("HOME").ok()?;
        Some(PathBuf::from(home).join(rest))
    } else {
        Some(PathBuf::from(raw))
    }
}
