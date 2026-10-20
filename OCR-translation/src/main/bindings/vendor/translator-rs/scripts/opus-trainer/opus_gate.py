#!/usr/bin/env python3
"""Gate OPUS-MT (Helsinki-NLP Marian) as a teacher on FLORES-200 devtest (chrF++/spBLEU).

validate_teacher.py already scores these models, but with 13a BLEU, which does not
line up against the nllb_gate.py / hy_mt2_gate.py numbers (spBLEU, flores200 tokenizer).
This reuses their FLORES cache, metrics and output line format so all three teachers
sit in one table. CPU is fine: the per-pair models are ~75M.

    ./venv/bin/python opus_gate.py --pairs eng_Latn-tgl_Latn,tgl_Latn-eng_Latn --limit 300
"""

import argparse
from pathlib import Path

import sacrebleu
import torch
from transformers import MarianMTModel, MarianTokenizer

from nllb_gate import flores_devtest, save_pair

# FLORES-200 code -> the OPUS-MT lang token used in the model name.
OPUS = {
    "eng_Latn": "en", "tgl_Latn": "tl", "swh_Latn": "sw", "urd_Arab": "ur",
    "gle_Latn": "ga", "cym_Latn": "cy",
}

# Pairs with no dedicated model; only the multilingual one covers them.
OVERRIDE = {
    ("swh_Latn", "eng_Latn"): "Helsinki-NLP/opus-mt-mul-en",
}


def model_name(src_code: str, tgt_code: str) -> str:
    override = OVERRIDE.get((src_code, tgt_code))
    if override is not None:
        return override
    return f"Helsinki-NLP/opus-mt-{OPUS[src_code]}-{OPUS[tgt_code]}"


def translate(name: str, src: list[str], beam: int, batch: int, device: str) -> list[str]:
    tok = MarianTokenizer.from_pretrained(name)
    model = MarianMTModel.from_pretrained(name).eval().to(device)
    hyps: list[str] = []
    with torch.no_grad():
        for i in range(0, len(src), batch):
            enc = tok(src[i : i + batch], return_tensors="pt", padding=True,
                      truncation=True, max_length=512).to(device)
            gen = model.generate(**enc, num_beams=beam, max_length=512)
            hyps.extend(tok.batch_decode(gen, skip_special_tokens=True))
    return hyps


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pairs", required=True, help="comma list of FLORES pairs src_code-tgt_code")
    ap.add_argument("--limit", type=int, default=300)
    ap.add_argument("--beam", type=int, default=4)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--device", default="cpu")
    ap.add_argument("--out-dir", type=Path,
                    help="also write src/hyp/ref per pair, for chrf_score.py (COMET)")
    args = ap.parse_args()

    pairs = [tuple(p.split("-")) for p in args.pairs.split(",")]
    missing = {c for pair in pairs for c in pair} - OPUS.keys()
    if missing:
        ap.error(f"no OPUS-MT lang token for FLORES code(s) {sorted(missing)}; add to OPUS")

    for src_code, tgt_code in pairs:
        src = flores_devtest(src_code)[: args.limit]
        ref = flores_devtest(tgt_code)[: args.limit]
        name = model_name(src_code, tgt_code)
        hyps = translate(name, src, args.beam, args.batch, args.device)
        if args.out_dir is not None:
            save_pair(args.out_dir, src_code, tgt_code, src, hyps, ref)
        chrf = sacrebleu.corpus_chrf(hyps, [ref], word_order=2)
        bleu = sacrebleu.corpus_bleu(hyps, [ref], tokenize="flores200")
        print(f"{src_code}->{tgt_code}: chrF++ {chrf.score:.2f}  spBLEU {bleu.score:.2f}  "
              f"(n={len(hyps)}, {name})", flush=True)


if __name__ == "__main__":
    main()
