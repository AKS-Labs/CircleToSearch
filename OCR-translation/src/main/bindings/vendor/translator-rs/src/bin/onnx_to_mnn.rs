use std::path::PathBuf;

use translator_convert::{WeightQuant, convert};

// Stage a `.mnn` blob into the bucket for a new model. The public index rewrites
// each model `.onnx` to its sibling `.mnn` and requires the `.mnn` to already
// exist, so new models must be converted before `generate_index.py --mode
// public`. Uses the same `translator_convert` path (mnn-sys) the app runs
// on-device, so the output is byte-equivalent to a fresh install's conversion.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: onnx_to_mnn IN.onnx OUT.mnn [weight_quant_bits|none]  (default: 8)");
        std::process::exit(2);
    }
    let onnx = PathBuf::from(&args[1]);
    let mnn = PathBuf::from(&args[2]);
    let quant = match args.get(3).map(String::as_str) {
        None | Some("8") => WeightQuant::Bits(8),
        Some("none") => WeightQuant::None,
        Some(bits) => match bits.parse() {
            Ok(bits) => WeightQuant::Bits(bits),
            Err(_) => {
                eprintln!("bad weight_quant_bits `{bits}`");
                std::process::exit(2);
            }
        },
    };

    if let Err(err) = convert(&onnx, &mnn, quant) {
        eprintln!("{err}");
        std::process::exit(1);
    }
    println!("wrote {}", mnn.display());
}
