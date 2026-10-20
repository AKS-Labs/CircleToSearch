//! On-device ink-matte benchmark: detect once, then time `ink_masks` over several runs.
//! No ort/viz deps so it cross-compiles for Android with just `--features ppocr`.
//!
//!   ink_bench <det.mnn> <ink.mnn> <image.jpg> [runs]

use std::path::PathBuf;
use std::time::Instant;

use translator::ppocr::{PpocrEngine, PpocrProfile};

struct StderrLog;
impl log::Log for StderrLog {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        eprintln!("{}", record.args());
    }
    fn flush(&self) {}
}

fn main() {
    let _ = log::set_logger(&StderrLog);
    log::set_max_level(log::LevelFilter::Info);
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: ink_bench <det.mnn> <ink.mnn> <image> [runs]");
        std::process::exit(1);
    }
    let det = PathBuf::from(&args[1]);
    let ink = PathBuf::from(&args[2]);
    let image = image::open(&args[3]).expect("open image");
    let runs: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(5);

    let engine =
        PpocrEngine::load(&det, None, None, Vec::new(), 4, Some(&ink)).expect("load ppocr engine");
    let boxes = engine
        .detect_only_image(&image, PpocrProfile::Still)
        .expect("detect");
    println!(
        "detected {} boxes, has_ink={}",
        boxes.len(),
        engine.has_ink()
    );

    for r in 0..runs {
        let t = Instant::now();
        let masks = engine.ink_masks(
            &image,
            &boxes,
            Some(translator::coords::Quadrant::R0),
            translator::ppocr::InkChannelOrder::Rgb,
        );
        let n = masks.iter().filter(|m| m.is_some()).count();
        println!(
            "run {r}: {n} masks — ink_masks wall {:.1}ms",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
}
