#!/usr/bin/env python3
"""Phase 2: sequence-level KD data via CTranslate2 forward-translation.

Converts a teacher (OPUS-MT or NLLB-200) to CTranslate2 (int8) and translates a
source corpus, emitting synthetic targets aligned line-by-line with the source.
The (source, synthetic-target) pair is the student's KD training data. Meant to
run on a rented GPU box.

OPUS-MT teachers are self-describing (the tokenizer knows the pair). NLLB is a
single multilingual model, so pass --src-lang/--tgt-lang FLORES codes; that sets
the tokenizer source language and a target-language prefix token for the decoder.

    pip install ctranslate2 transformers sentencepiece sacremoses
    python distill_data.py --model Helsinki-NLP/opus-mt-en-tl \
        --src kd.en.gz --out kd.en2tl.tl.gz            # en->tl student targets (OPUS-MT)
    python distill_data.py --model facebook/nllb-200-distilled-600M \
        --src-lang tgl_Latn --tgt-lang eng_Latn \
        --src kd.tl.gz --out kd.tl2en.en.gz            # tl->en student targets (NLLB)

With --nbest N (requires --beam >= N) each output line is an N-column TSV of
hypotheses, best-first, at the same decode cost as 1-best for a given beam.
Feed that to extract_best.py (same box, right after) to pick the training
target per line against the human reference.

    python distill_data.py --model facebook/nllb-200-distilled-600M \
        --src-lang swh_Latn --tgt-lang eng_Latn --beam 8 --nbest 8 \
        --src shard.sw.gz --out shard.nbest.tsv.gz
"""

import argparse
import gzip
import subprocess
import sys
from pathlib import Path

import ctranslate2

from hf_offline import load_tokenizer


def opener(p: str):
    return gzip.open(p, "rt", encoding="utf-8") if p.endswith(".gz") else open(p, encoding="utf-8")


def writer(p: str):
    return gzip.open(p, "wt", encoding="utf-8") if p.endswith(".gz") else open(p, "w", encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", required=True, help="HF teacher id (OPUS-MT or NLLB-200)")
    ap.add_argument("--src", required=True, help="source text (.gz ok)")
    ap.add_argument("--out", required=True, help="synthetic target out (.gz ok)")
    ap.add_argument("--src-lang", default="", help="NLLB source FLORES code (e.g. tgl_Latn); enables NLLB mode")
    ap.add_argument("--tgt-lang", default="", help="NLLB target FLORES code (e.g. eng_Latn)")
    ap.add_argument("--ct2-dir", default="")
    ap.add_argument("--device", default="cuda")
    ap.add_argument("--compute-type", default="int8")
    ap.add_argument("--batch", type=int, default=1024, help="lines submitted per async task (CPU tokenization unit)")
    ap.add_argument("--max-batch-tokens", type=int, default=2048,
                    help="cap GPU compute batch by source tokens (batch_type=tokens); bounds VRAM on long sentences")
    ap.add_argument("--beam", type=int, default=4)
    ap.add_argument("--nbest", type=int, default=1,
                    help="emit the top N beam hypotheses as an N-column TSV (for extract_best.py)")
    ap.add_argument("--max-len", type=int, default=256)
    ap.add_argument("--inter-threads", type=int, default=2)
    ap.add_argument("--queue-depth", type=int, default=3, help="batches kept in flight to keep the GPU fed")
    args = ap.parse_args()

    nllb = bool(args.src_lang or args.tgt_lang)
    if nllb and not (args.src_lang and args.tgt_lang):
        ap.error("NLLB mode needs both --src-lang and --tgt-lang")
    if args.nbest > args.beam:
        ap.error(f"--nbest {args.nbest} needs --beam >= {args.nbest}")

    ct2_dir = args.ct2_dir or "ct2_" + args.model.split("/")[-1].replace("-", "_")
    if not Path(ct2_dir).exists():
        print(f"converting {args.model} -> {ct2_dir} ({args.compute_type})", file=sys.stderr)
        subprocess.run(
            ["ct2-transformers-converter", "--model", args.model,
             "--output_dir", ct2_dir, "--quantization", args.compute_type],
            check=True,
        )
    tok = load_tokenizer(args.model, args.src_lang if nllb else None)
    target_prefix = [args.tgt_lang] if nllb else None
    translator = ctranslate2.Translator(
        ct2_dir, device=args.device, compute_type=args.compute_type, inter_threads=args.inter_threads,
    )

    n = 0

    # translate_iterable streams the whole file through CT2's own batching
    # (bounded by max_batch_size tokens) with in-order output. Repeated
    # translate_batch calls accumulate GPU memory and OOM on a 600M teacher;
    # letting CT2 own the stream keeps VRAM flat. Cap source length to the
    # model's 512 position encodings so a long line can't crash the encoder.
    def sources():
        with opener(args.src) as fin:
            for line in fin:
                yield tok.convert_ids_to_tokens(tok.encode(line.rstrip("\n"), truncation=True, max_length=512))

    def prefixes():
        # Must be the SAME length as sources() (translate_iterable zips them and
        # pads the shorter with None), so re-read the file rather than repeat().
        with opener(args.src) as fin:
            for _ in fin:
                yield [args.tgt_lang]

    def detok(hyp) -> str:
        # NLLB decodes the target-language prefix token back out first; drop it.
        if target_prefix and hyp and hyp[0] == args.tgt_lang:
            hyp = hyp[1:]
        decoded = tok.decode(tok.convert_tokens_to_ids(hyp), skip_special_tokens=True)
        return decoded.replace("\n", " ").replace("\t", " ")

    kwargs = {"target_prefix": prefixes()} if target_prefix else {}
    with writer(args.out) as fout:
        for r in translator.translate_iterable(
            sources(), beam_size=args.beam, num_hypotheses=args.nbest,
            max_decoding_length=args.max_len,
            max_batch_size=args.max_batch_tokens, batch_type="tokens", **kwargs,
        ):
            fout.write("\t".join(detok(h) for h in r.hypotheses[:args.nbest]) + "\n")
            n += 1
            if n % 20000 == 0:
                print(f"  {n}", end="\r", file=sys.stderr)
    print(f"\nDONE {n} lines -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
