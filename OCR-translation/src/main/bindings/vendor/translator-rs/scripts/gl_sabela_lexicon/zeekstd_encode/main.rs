// Encode a file as seekable zstd (zeekstd format) so the runtime can read it
// with zeekstd::Decoder (and, later, seek per-frame instead of full-decoding).
// Usage: zeekstd_encode <input> <output> [frame_kib]
use std::io::Write;

use zeekstd::{EncodeOptions, Encoder, FrameSizePolicy};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: zeekstd_encode <input> <output> [frame_kib]");
        std::process::exit(2);
    }
    let frame_kib: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(256);

    let input = std::fs::read(&args[1]).expect("read input");
    let output = std::fs::File::create(&args[2]).expect("create output");

    let opts = EncodeOptions::new()
        .compression_level(19)
        .frame_size_policy(FrameSizePolicy::Uncompressed((frame_kib * 1024) as u32));
    let mut encoder = Encoder::with_opts(output, opts).expect("encoder");
    encoder.write_all(&input).expect("write");
    let compressed = encoder.finish().expect("finish");
    eprintln!("wrote {} ({} bytes) from {} bytes", args[2], compressed, input.len());
}
