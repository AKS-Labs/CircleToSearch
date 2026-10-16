#!/usr/bin/env python3
"""Decode a source file with MADLAD-400, into a target language.

The MADLAD half of the probe/gate eval, mirroring nllb_decode.py so every
teacher's hypotheses land in the same shape for side-by-side reading.

MADLAD selects the target language with a `<2xx>` token prepended to the SOURCE,
not with a decoder prefix the way NLLB does, so there is no target_prefix here
and the tag is part of what the encoder reads.

    madlad_decode.py google/madlad400-3b-mt probes.en ka out.ka cuda
"""

import sys

import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer


def main() -> None:
    model_id, src_p, tgt_code, out_p, device = sys.argv[1:6]
    lines = [l for l in open(src_p, encoding="utf-8").read().splitlines() if l.strip()]

    tok = AutoTokenizer.from_pretrained(model_id)
    dtype = torch.bfloat16 if device.startswith("cuda") else torch.float32
    model = AutoModelForSeq2SeqLM.from_pretrained(model_id, dtype=dtype).eval().to(device)

    tagged = [f"<2{tgt_code}> {l}" for l in lines]
    hyps: list[str] = []
    batch = 16
    with torch.no_grad():
        for i in range(0, len(tagged), batch):
            enc = tok(tagged[i : i + batch], return_tensors="pt", padding=True,
                      truncation=True, max_length=256).to(device)
            gen = model.generate(**enc, num_beams=4, max_length=256)
            hyps.extend(tok.batch_decode(gen, skip_special_tokens=True))
            print(f"  {min(i + batch, len(tagged))}/{len(tagged)}", end="\r", file=sys.stderr)

    with open(out_p, "w", encoding="utf-8") as f:
        for h in hyps:
            f.write(h.replace("\n", " ") + "\n")


if __name__ == "__main__":
    main()
