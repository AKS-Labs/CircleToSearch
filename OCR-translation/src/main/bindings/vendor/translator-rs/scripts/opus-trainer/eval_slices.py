#!/usr/bin/env python3
"""Score one system over EVERY eval slice at once: chrF++/spBLEU/COMET22 + defects.

eval_pair.py scores exactly two slices (FLORES and a check set) because that is
the shape the tl gate had. A pair whose registers are covered by several held-out
human corpora needs N of them, and scoring N slices as N invocations would reload
the 2.3GB COMET checkpoint N times -- the mistake teacher_metrics.py already made
once. So the slice list is the argument and the model is loaded once.

A slice is a directory triple {name}.src / {name}.hyp / {name}.ref, all
line-aligned. Missing .ref is allowed and means reference-free: the slice is
scored on defects only, which is what the adversarial probe set is for.

    eval_slices.py --dir decodes/nllb600m --slices flores,subtitles,ted,ui,legal,signs \
        --tgt-lang ka --label nllb-600m --out metrics.json
"""

import argparse
import json
from pathlib import Path

import sacrebleu

from chrf_score import comet22, load_comet
from probe_check import check


def lines(p: Path) -> list[str]:
    return p.read_text(encoding="utf-8").splitlines()


def defects(srcs: list[str], hyps: list[str], tgt_lang: str) -> dict[str, int]:
    found: dict[str, int] = {}
    for src, hyp in zip(srcs, hyps):
        for failure in check(src, hyp, tgt_lang=tgt_lang):
            found[failure.kind] = found.get(failure.kind, 0) + 1
    return found


def score_slice(model, d: Path, name: str, tgt_lang: str) -> dict:
    src, hyp = lines(d / f"{name}.src"), lines(d / f"{name}.hyp")
    if len(src) != len(hyp):
        raise SystemExit(f"{name}: src={len(src)} hyp={len(hyp)} not aligned")
    out = {"n": len(hyp), "defects": defects(src, hyp, tgt_lang)}

    ref_p = d / f"{name}.ref"
    if not ref_p.exists():
        return out
    ref = lines(ref_p)
    if len(ref) != len(hyp):
        raise SystemExit(f"{name}: ref={len(ref)} hyp={len(hyp)} not aligned")
    out["chrf"] = round(sacrebleu.corpus_chrf(hyp, [ref], word_order=2).score, 2)
    out["spbleu"] = round(sacrebleu.corpus_bleu(hyp, [ref], tokenize="flores200").score, 2)
    out["comet22"] = round(comet22(model, src, hyp, ref), 2)
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dir", required=True, type=Path, help="directory of {slice}.src/.hyp/.ref")
    ap.add_argument("--slices", required=True, help="comma list of slice names, in report order")
    ap.add_argument("--tgt-lang", required=True, help="language the hypotheses are IN")
    ap.add_argument("--label", required=True)
    ap.add_argument("--out", required=True, type=Path)
    args = ap.parse_args()

    names = args.slices.split(",")
    model = load_comet()
    scored = {name: score_slice(model, args.dir, name, args.tgt_lang) for name in names}

    result = {"label": args.label, "tgt_lang": args.tgt_lang, "slices": scored}
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")

    print(f"=== {args.label} -> {args.tgt_lang}")
    for name in names:
        s = scored[name]
        metrics = (f"chrF++ {s['chrf']:6.2f}  spBLEU {s['spbleu']:6.2f}  COMET {s['comet22']:6.2f}"
                   if "comet22" in s else "reference-free" + " " * 33)
        bad = " ".join(f"{k}={v}" for k, v in sorted(s["defects"].items())) or "-"
        print(f"  {name:12s} n={s['n']:<5d} {metrics}  defects: {bad}")


if __name__ == "__main__":
    main()
