#!/usr/bin/env python3
"""Sweep CT2 max_batch_size (token budget) for the NLLB teacher on one GPU.

max-batch-tokens is a SOURCE-token budget, not a VRAM target, so the right value
is hardware-specific and worth measuring once per teacher/beam rather than
inheriting sw's conservative 3072. The model loads ONCE and every batch size is
timed against it, so each lines/s is clean of load cost. OOM at a setting is
caught and reported as the ceiling rather than crashing the sweep.

Replicates distill_data.py's exact CT2 call (translate_iterable, tokens batch
type, same-length target_prefix generator) so the numbers transfer directly.

Usage: bench_batch.py SRC SRC_LANG TGT_LANG [BEAM=4] [NBEST=4] [BATCHES=csv]
  model + ct2 dir come from the image env (NLLB_MODEL / NLLB_CT2_DIR).
"""

from __future__ import annotations

import os
import sys
import time

import ctranslate2

from hf_offline import load_tokenizer


def main() -> None:
    src, src_lang, tgt_lang = sys.argv[1], sys.argv[2], sys.argv[3]
    beam = int(sys.argv[4]) if len(sys.argv) > 4 else 4
    nbest = int(sys.argv[5]) if len(sys.argv) > 5 else 4
    batches = [int(x) for x in (sys.argv[6] if len(sys.argv) > 6 else
               "3072,4096,6144,8192,12288,16384").split(",")]

    model, ct2_dir = os.environ["NLLB_MODEL"], os.environ["NLLB_CT2_DIR"]
    tok = load_tokenizer(model, src_lang)
    translator = ctranslate2.Translator(ct2_dir, device="cuda", compute_type="int8")

    lines = [l.rstrip("\n") for l in open(src, encoding="utf-8")]
    print(f"model={model} beam={beam} nbest={nbest} sample={len(lines)} lines", file=sys.stderr)

    def sources():
        for line in lines:
            yield tok.convert_ids_to_tokens(tok.encode(line, truncation=True, max_length=512))

    def prefixes():
        for _ in lines:
            yield [tgt_lang]

    print(f"{'max_batch_tokens':>16}  {'lines/s':>8}  {'wall_s':>7}  status")
    prev = None
    for mbt in batches:
        t0 = time.time()
        try:
            n = 0
            for _ in translator.translate_iterable(
                sources(), beam_size=beam, num_hypotheses=nbest,
                max_decoding_length=256,
                max_batch_size=mbt, batch_type="tokens", target_prefix=prefixes(),
            ):
                n += 1
            wall = time.time() - t0
            lps = n / wall
            gain = f"{lps / prev:.2f}x" if prev else "—"
            print(f"{mbt:>16}  {lps:>8.1f}  {wall:>7.1f}  ok  {gain} vs prev")
            prev = lps
        except RuntimeError as e:
            msg = "OOM" if "out of memory" in str(e).lower() else f"ERR {str(e)[:40]}"
            print(f"{mbt:>16}  {'—':>8}  {'—':>7}  {msg} — ceiling reached")
            break


if __name__ == "__main__":
    main()
