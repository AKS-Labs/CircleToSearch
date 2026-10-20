#!/usr/bin/env python3
"""Decode a probe file with NLLB-200, into a FLORES-coded target language.

The NLLB half of the probe eval, mirroring probe_decode.py / opus_decode.py so all
three teachers' hypotheses land in the same shape for side-by-side reading.

    nllb_decode.py facebook/nllb-200-distilled-600M probes.en eng_Latn tgl_Latn out.tl cuda
"""

import sys

import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer


def main() -> None:
    model_id, src_p, src_code, tgt_code, out_p, device = sys.argv[1:7]
    lines = [l for l in open(src_p, encoding="utf-8").read().splitlines() if l.strip()]

    tok = AutoTokenizer.from_pretrained(model_id, src_lang=src_code)
    model = AutoModelForSeq2SeqLM.from_pretrained(model_id).eval().to(device)
    bos = tok.convert_tokens_to_ids(tgt_code)

    hyps: list[str] = []
    batch = 16
    with torch.no_grad():
        for i in range(0, len(lines), batch):
            enc = tok(lines[i : i + batch], return_tensors="pt", padding=True,
                      truncation=True, max_length=256).to(device)
            gen = model.generate(**enc, forced_bos_token_id=bos, num_beams=4, max_length=256)
            hyps.extend(tok.batch_decode(gen, skip_special_tokens=True))

    with open(out_p, "w", encoding="utf-8") as f:
        for h in hyps:
            f.write(h.replace("\n", " ") + "\n")


if __name__ == "__main__":
    main()
