//! Golden-set eval dumper: run the real PP-OCR detection + dewarp + recognition on
//! each image, save the dewarped strips (stable index = reading order) and the model's
//! recognition as `<name>.rec.json` ({ "00": text, ... }). A human/VLM then writes the
//! parallel `<name>.json` ground truth against the saved strips, and CER is computed by
//! matching index. Uses the production det+dewarp so strips match what the recognizer
//! actually sees (proper unclip), unlike a Python reimplementation.
//!
//!   cargo run --features ppocr --bin golden_eval -- \
//!       <det.mnn> <rec.mnn> <keys.txt> <script> <out_dir> <image...>

use std::fs;
use std::path::{Path, PathBuf};

use translator::PpocrScript;
use translator::ppocr::{
    PpocrEngine, PpocrProfile, PpocrRecognizerSpec, dewarp_contour_to_strip_rgb,
};

fn contour_points(c: &[f32]) -> Vec<(f32, f32)> {
    c.chunks_exact(2).map(|p| (p[0], p[1])).collect()
}

fn script_from_slug(s: &str) -> Option<PpocrScript> {
    Some(match s {
        "georgian" => PpocrScript::Georgian,
        "hebrew" => PpocrScript::Hebrew,
        "indic" => PpocrScript::Indic,
        "latin" => PpocrScript::Latin,
        "arabic" => PpocrScript::Arabic,
        "cyrillic" => PpocrScript::Cyrillic,
        "devanagari" => PpocrScript::Devanagari,
        _ => return None,
    })
}

fn jesc(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
        .replace('\r', " ")
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 6 {
        eprintln!(
            "usage: golden_eval <det.mnn> <rec.mnn> <keys.txt> <script> <out_dir> <image...>"
        );
        std::process::exit(2);
    }
    let det = PathBuf::from(&a[0]);
    let rec = PathBuf::from(&a[1]);
    let keys = PathBuf::from(&a[2]);
    let script = script_from_slug(&a[3]).unwrap_or_else(|| panic!("unknown script: {}", a[3]));
    let out = PathBuf::from(&a[4]);
    let images = &a[5..];

    let spec = PpocrRecognizerSpec {
        script,
        model_path: rec,
        keys_path: keys,
    };
    let engine = PpocrEngine::load(&det, None, None, vec![spec], 4, None)
        .unwrap_or_else(|e| panic!("load engine: {e:?}"));
    let sdir = out.join("strips");
    fs::create_dir_all(&sdir).unwrap();

    for img_path in images {
        let image = image::open(img_path).unwrap_or_else(|e| panic!("open {img_path}: {e}"));
        let rgb = image.to_rgb8();
        let mut boxes = engine
            .detect_only_image(&image, PpocrProfile::Still)
            .unwrap_or_else(|e| panic!("detect: {e:?}"));
        // reading order: top-to-bottom, then left-to-right
        boxes.sort_by(|x, y| (x.rect.top, x.rect.left).cmp(&(y.rect.top, y.rect.left)));
        let name = Path::new(img_path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();

        // Per-strip bounding box from the (unclipped) contour, in image space — used by the
        // eval to flag detector garbage (tiny ottakshara/subscript fragments swallowed by a
        // larger line box) so they don't pollute the recognizer metric.
        let mut bj = String::from("{\n");
        for (i, b) in boxes.iter().enumerate() {
            let contour = contour_points(&b.contour);
            let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for (x, y) in &contour {
                x0 = x0.min(*x);
                y0 = y0.min(*y);
                x1 = x1.max(*x);
                y1 = y1.max(*y);
            }
            let comma = if i + 1 < boxes.len() { "," } else { "" };
            bj.push_str(&format!(
                "  \"{i:02}\": [{x0:.1}, {y0:.1}, {x1:.1}, {y1:.1}]{comma}\n"
            ));
            if let Some(strip) = dewarp_contour_to_strip_rgb(&rgb, &contour, None, 0.0) {
                strip.save(sdir.join(format!("{name}-{i:02}.png"))).unwrap();
            }
        }
        bj.push_str("}\n");
        fs::write(out.join(format!("{name}.boxes.json")), bj).unwrap();

        let scripts = vec![script; boxes.len()];
        let lines = engine
            .recognize_text_in_boxes_image(&image, &boxes, &scripts, PpocrProfile::Still, None)
            .unwrap_or_else(|e| panic!("recognize: {e:?}"));

        let mut j = String::from("{\n");
        for (i, l) in lines.iter().enumerate() {
            let comma = if i + 1 < lines.len() { "," } else { "" };
            j.push_str(&format!("  \"{i:02}\": \"{}\"{comma}\n", jesc(&l.text)));
        }
        j.push_str("}\n");
        fs::write(out.join(format!("{name}.rec.json")), j).unwrap();
        println!(
            "{name}: {} boxes -> {}/strips/{name}-NN.png",
            boxes.len(),
            out.display()
        );
    }
}
