//! Stage-level benchmark for the PP-OCR detect + recognize pipeline on one
//! fixed image. The per-stage breakdown (det pre/infer/post, rec crops /
//! pre / infer / post) comes from ppocr.rs's existing info logs; this harness
//! adds wall-clock stats over N iterations.
//!
//! Usage: ppocr_bench <det.mnn> <rec.mnn> <keys.txt> <image> [iters]

use std::path::PathBuf;
use std::time::Instant;

use translator::PpocrScript;
use translator::ppocr::{PpocrEngine, PpocrProfile, PpocrRecognizerSpec};

// env_logger is not in the ppocr feature set, and the existing stage timings
// are emitted through log::info — a bare stderr sink is all the bench needs.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("{}", record.args());
        }
    }

    fn flush(&self) {}
}

fn stats(samples: &mut [f32]) -> (f32, f32, f32) {
    samples.sort_by(f32::total_cmp);
    let min = samples[0];
    let median = samples[samples.len() / 2];
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    (min, median, mean)
}

fn main() {
    log::set_logger(&StderrLogger).expect("set logger");
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("usage: ppocr_bench <det.mnn> <rec.mnn> <keys.txt> <image> [iters]");
        std::process::exit(2);
    }
    let det_path = PathBuf::from(&args[1]);
    let rec_path = PathBuf::from(&args[2]);
    let keys_path = PathBuf::from(&args[3]);
    let image_path = PathBuf::from(&args[4]);
    let iters: usize = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(10);

    let image = image::open(&image_path).expect("open image");
    eprintln!(
        "image: {} ({}x{})",
        image_path.display(),
        image.width(),
        image.height()
    );

    let t_load = Instant::now();
    let engine = PpocrEngine::load(
        &det_path,
        None,
        None,
        vec![PpocrRecognizerSpec {
            script: PpocrScript::Latin,
            model_path: rec_path,
            keys_path,
        }],
        4,
        None,
    )
    .expect("load ppocr engine");
    eprintln!(
        "engine load (det only, rec is lazy): {:.1}ms",
        t_load.elapsed().as_secs_f32() * 1000.0
    );

    // Warmup also triggers the lazy rec session loads.
    let boxes = engine
        .detect_only_image(&image, PpocrProfile::Still)
        .expect("detect");
    let scripts = vec![PpocrScript::Latin; boxes.len()];
    let lines = engine
        .recognize_text_in_boxes_image(&image, &boxes, &scripts, PpocrProfile::Still, None)
        .expect("recognize");
    eprintln!(
        "warmup: boxes={} accepted={}",
        boxes.len(),
        lines.iter().filter(|l| !l.text.is_empty()).count()
    );
    if std::env::var_os("PPOCR_BENCH_DUMP_TEXT").is_some() {
        let mut sorted: Vec<_> = lines.iter().filter(|l| !l.text.is_empty()).collect();
        sorted.sort_by_key(|l| (l.rect.top, l.rect.left));
        for l in sorted {
            println!("[{:.2}] {}", l.confidence, l.text);
        }
    }
    if std::env::var_os("PPOCR_BENCH_DUMP_BOXES").is_some() {
        let mut sorted: Vec<_> = boxes.iter().collect();
        sorted.sort_by_key(|b| (b.rect.top, b.rect.left));
        for b in sorted {
            println!(
                "box {} {} {} {} score={:.3} tight_h={:.1}",
                b.rect.left,
                b.rect.top,
                b.rect.width(),
                b.rect.height(),
                b.score,
                b.tight_box.height
            );
        }
    }

    let mut det_ms = Vec::with_capacity(iters);
    let mut rec_ms = Vec::with_capacity(iters);
    for i in 0..iters {
        let t = Instant::now();
        let boxes = engine
            .detect_only_image(&image, PpocrProfile::Still)
            .expect("detect");
        let d = t.elapsed().as_secs_f32() * 1000.0;

        let scripts = vec![PpocrScript::Latin; boxes.len()];
        let t = Instant::now();
        let _lines = engine
            .recognize_text_in_boxes_image(&image, &boxes, &scripts, PpocrProfile::Still, None)
            .expect("recognize");
        let r = t.elapsed().as_secs_f32() * 1000.0;

        eprintln!(
            "iter {i}: det={d:.1}ms rec={r:.1}ms total={:.1}ms boxes={}",
            d + r,
            boxes.len()
        );
        det_ms.push(d);
        rec_ms.push(r);
    }

    let (d_min, d_med, d_mean) = stats(&mut det_ms);
    let (r_min, r_med, r_mean) = stats(&mut rec_ms);
    eprintln!("\nsummary over {iters} iters (ms): min / median / mean");
    eprintln!("  det:   {d_min:.1} / {d_med:.1} / {d_mean:.1}");
    eprintln!("  rec:   {r_min:.1} / {r_med:.1} / {r_mean:.1}");
    eprintln!(
        "  total: {:.1} / {:.1} / {:.1}",
        d_min + r_min,
        d_med + r_med,
        d_mean + r_mean
    );
}
