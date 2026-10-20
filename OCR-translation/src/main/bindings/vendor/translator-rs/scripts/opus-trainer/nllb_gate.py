#!/usr/bin/env python3
"""Gate NLLB-200 as a teacher on FLORES-200 devtest (chrF++/spBLEU).

Same idea as validate_teacher.py but for the languages OPUS-MT lacks/is weak on.
NLLB uses lang-code tokens (eng_Latn, swh_Latn, urd_Arab, ...). Reuses the FLORES
tarball cache. CPU is slow for the 600M model, so keep --limit small.

    ./venv/bin/python nllb_gate.py --pairs eng_Latn-swh_Latn,swh_Latn-eng_Latn --limit 100
"""

import argparse
import os
import sys
import tarfile
import urllib.request
from pathlib import Path

import sacrebleu
import torch
from transformers import AutoModelForSeq2SeqLM, AutoTokenizer

FLORES_URL = "https://dl.fbaipublicfiles.com/nllb/flores200_dataset.tar.gz"
# Overridable because pipe mounts the scripts dir read-only on a job box, so the
# default next-to-the-script cache is unwritable there.
CACHE = Path(os.environ.get("FLORES_CACHE", Path(__file__).resolve().parent / ".cache"))


def flores_devtest(code: str) -> list[str]:
    root = CACHE / "flores200_dataset"
    if not root.exists():
        CACHE.mkdir(exist_ok=True)
        tar = CACHE / "flores200_dataset.tar.gz"
        if not tar.exists():
            urllib.request.urlretrieve(FLORES_URL, tar)
        with tarfile.open(tar) as t:
            t.extractall(CACHE)
    return (root / "devtest" / f"{code}.devtest").read_text(encoding="utf-8").splitlines()


def save_pair(out_dir: Path, src_code: str, tgt_code: str,
              src: list[str], hyps: list[str], ref: list[str]) -> None:
    """Write a decode's src/hyp/ref so chrf_score.py can score it (COMET needs src).

    All three sides are written, not just the hypothesis: --limit slices FLORES, so
    a scorer that re-derived src/ref from the tarball would silently misalign if it
    were handed a different limit.
    """
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = f"{src_code}-{tgt_code}"
    for suffix, lines in (("src", src), ("hyp", hyps), ("ref", ref)):
        (out_dir / f"{stem}.{suffix}").write_text(
            "".join(l.replace("\n", " ") + "\n" for l in lines), encoding="utf-8"
        )


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pairs", required=True, help="comma list of FLORES pairs src_code-tgt_code")
    ap.add_argument("--model", default="facebook/nllb-200-distilled-600M")
    ap.add_argument("--limit", type=int, default=100)
    ap.add_argument("--beam", type=int, default=4)
    ap.add_argument("--batch", type=int, default=8)
    ap.add_argument("--device", default="cpu", help="cpu or cuda; use cuda on a rented GPU box")
    ap.add_argument("--out-dir", type=Path,
                    help="also write src/hyp/ref per pair, for chrf_score.py (COMET)")
    args = ap.parse_args()

    tok = AutoTokenizer.from_pretrained(args.model)
    model = AutoModelForSeq2SeqLM.from_pretrained(args.model).eval().to(args.device)

    for pair in args.pairs.split(","):
        src_code, tgt_code = pair.split("-")
        src = flores_devtest(src_code)[: args.limit]
        ref = flores_devtest(tgt_code)[: args.limit]
        tok.src_lang = src_code
        bos = tok.convert_tokens_to_ids(tgt_code)
        hyps: list[str] = []
        with torch.no_grad():
            for i in range(0, len(src), args.batch):
                enc = tok(src[i : i + args.batch], return_tensors="pt", padding=True, truncation=True, max_length=256).to(args.device)
                gen = model.generate(**enc, forced_bos_token_id=bos, num_beams=args.beam, max_length=256)
                hyps.extend(tok.batch_decode(gen, skip_special_tokens=True))
                print(f"  {pair}: {min(i + args.batch, len(src))}/{len(src)}", end="\r", file=sys.stderr)
        if args.out_dir is not None:
            save_pair(args.out_dir, src_code, tgt_code, src, hyps, ref)
        chrf = sacrebleu.corpus_chrf(hyps, [ref], word_order=2)
        sp = sacrebleu.corpus_bleu(hyps, [ref], tokenize="flores200")
        print(f"{src_code}->{tgt_code}: chrF++ {chrf.score:.2f}  spBLEU {sp.score:.2f}  (n={len(hyps)})")


if __name__ == "__main__":
    main()
