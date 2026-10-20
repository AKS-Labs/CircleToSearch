#!/usr/bin/env python3
"""Gate check for OPUS-MT teacher quality before committing to distillation.

Translates FLORES-200 devtest with the Helsinki-NLP OPUS-MT models for a
language and reports chrF++/BLEU against the reference, plus a few sample
translations to eyeball. Runs on CPU; no GPU needed. A student can never
beat its teacher, so if the scores/samples here are bad, stop before renting
anything.

FLORES is multi-parallel, so both directions come from the single pair
`eng_Latn-<tgt>`: english + target sentences are fetched once and each
direction is scored against the other side.

Setup:
    python3 -m venv venv && venv/bin/pip install -r requirements.txt
    venv/bin/python validate_teacher.py --langs sw,tl
"""

import argparse
import sys
import tarfile
import urllib.request
from pathlib import Path

import sacrebleu
import torch
from transformers import MarianMTModel, MarianTokenizer

# lang code -> (FLORES-200 target code, opus-mt en->X model, opus-mt X->en model)
# No dedicated opus-mt-sw-en exists; sw->en only via the multilingual mul->en model.
LANGS = {
    "sw": ("swh_Latn", "Helsinki-NLP/opus-mt-en-sw", "Helsinki-NLP/opus-mt-mul-en"),
    "tl": ("tgl_Latn", "Helsinki-NLP/opus-mt-en-tl", "Helsinki-NLP/opus-mt-tl-en"),
    "ur": ("urd_Arab", "Helsinki-NLP/opus-mt-en-ur", "Helsinki-NLP/opus-mt-ur-en"),
}

FLORES_URL = "https://dl.fbaipublicfiles.com/nllb/flores200_dataset.tar.gz"
CACHE = Path(__file__).resolve().parent / ".cache"


def flores_devtest(code: str) -> list[str]:
    root = CACHE / "flores200_dataset"
    if not root.exists():
        CACHE.mkdir(exist_ok=True)
        tar = CACHE / "flores200_dataset.tar.gz"
        if not tar.exists():
            print(f"downloading FLORES-200 -> {tar}", file=sys.stderr)
            urllib.request.urlretrieve(FLORES_URL, tar)
        with tarfile.open(tar) as t:
            t.extractall(CACHE)
    devtest = root / "devtest" / f"{code}.devtest"
    if not devtest.exists():
        sys.exit(f"missing FLORES devtest for {code}: {devtest}")
    return devtest.read_text(encoding="utf-8").splitlines()


def translate(model_name: str, src: list[str], beams: int, batch: int) -> list[str]:
    tok = MarianTokenizer.from_pretrained(model_name)
    model = MarianMTModel.from_pretrained(model_name).eval()
    hyps: list[str] = []
    with torch.no_grad():
        for i in range(0, len(src), batch):
            chunk = src[i : i + batch]
            enc = tok(chunk, return_tensors="pt", padding=True, truncation=True, max_length=512)
            gen = model.generate(**enc, num_beams=beams, max_length=512)
            hyps.extend(tok.batch_decode(gen, skip_special_tokens=True))
            print(f"  {model_name}: {min(i + batch, len(src))}/{len(src)}", end="\r", file=sys.stderr)
    print(file=sys.stderr)
    return hyps


def score(direction: str, model_name: str, src: list[str], ref: list[str], beams: int, batch: int, samples: int) -> None:
    try:
        hyps = translate(model_name, src, beams, batch)
    except OSError as e:
        print(f"\n=== {direction}  ({model_name}) === SKIPPED: {str(e).splitlines()[0]}")
        return
    chrf = sacrebleu.corpus_chrf(hyps, [ref], word_order=2)
    bleu = sacrebleu.corpus_bleu(hyps, [ref])
    print(f"\n=== {direction}  ({model_name}) ===")
    print(f"chrF++ {chrf.score:.2f}   BLEU {bleu.score:.2f}   (n={len(hyps)})")
    for s, h, r in list(zip(src, hyps, ref))[:samples]:
        print(f"  SRC {s}\n  HYP {h}\n  REF {r}\n")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--langs", default="sw,tl", help="comma list from " + ",".join(LANGS))
    ap.add_argument("--limit", type=int, default=300, help="devtest sentences to use (0 = all 1012)")
    ap.add_argument("--beams", type=int, default=4)
    ap.add_argument("--batch", type=int, default=16)
    ap.add_argument("--samples", type=int, default=8, help="translations to print per direction")
    args = ap.parse_args()

    torch.set_num_threads(torch.get_num_threads())

    for lang in [l.strip() for l in args.langs.split(",") if l.strip()]:
        if lang not in LANGS:
            sys.exit(f"unknown lang {lang!r}; known: {','.join(LANGS)}")
        tgt_code, en_x, x_en = LANGS[lang]
        eng = flores_devtest("eng_Latn")
        tgt = flores_devtest(tgt_code)
        if args.limit:
            eng, tgt = eng[: args.limit], tgt[: args.limit]

        print(f"\n########## en <-> {lang}  (FLORES {tgt_code}, n={len(eng)}) ##########")
        score(f"en->{lang}", en_x, eng, tgt, args.beams, args.batch, args.samples)
        score(f"{lang}->en", x_en, tgt, eng, args.beams, args.batch, args.samples)


if __name__ == "__main__":
    main()
