#!/usr/bin/env python3
"""Score one system, one direction: FLORES + check set + the gap between them.

The primitive behind the pipeline's eval steps, taking explicit paths so it works
for a teacher, a student, or an old pack without knowing which.

Emits, per run:
  flores      chrF++/spBLEU/COMET22 on FLORES devtest — the sentence-level number
  check       the same three on the check set — the deployment-shaped number
  delta       check minus flores. THE number to look at: on tl the flores scores
              called en->tl a four-way tie while the deltas ran from +0.64
              (Hy-MT2) to -11.15 (OPUS-MT), and the deltas ranked the teachers in
              the order that reading the outputs confirmed
  mechanical  reference-free defect counts, PER KIND and never summed — the
              summed rate ranked OPUS-MT top while it was emitting "Reflection"
              for *Banyo*

    eval_pair.py --flores-hyp H --flores-ref R --flores-src S \
                 --check-hyp H --check-src S --check-ref R --out metrics.json
"""

import argparse
import json
from pathlib import Path

import sacrebleu

from chrf_score import comet22, load_comet
from probe_check import check
from probe_review import render


def lines(p: Path) -> list[str]:
    return p.read_text(encoding="utf-8").splitlines()


def score(model, hyp: list[str], ref: list[str], src: list[str], label: str) -> dict:
    if not (len(hyp) == len(ref) == len(src)):
        raise SystemExit(
            f"{label}: line-count mismatch hyp={len(hyp)} ref={len(ref)} src={len(src)}")
    return {
        "chrf": round(sacrebleu.corpus_chrf(hyp, [ref], word_order=2).score, 2),
        "spbleu": round(sacrebleu.corpus_bleu(hyp, [ref], tokenize="flores200").score, 2),
        "comet22": round(comet22(model, src, hyp, ref), 2),
        "n": len(hyp),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    for name in ("flores-hyp", "flores-ref", "flores-src",
                 "check-hyp", "check-src", "check-ref"):
        ap.add_argument(f"--{name}", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--review-out", required=True, type=Path,
                    help="side-by-side review; REQUIRED so metrics-without-review "
                         "is not a runnable partial of this step")
    ap.add_argument("--label", default="system")
    ap.add_argument("--tgt-lang", default="en",
                    help="language the hypotheses are IN; selects probe_check's "
                         "spelled-out numerals, which are per-language")
    args = ap.parse_args()

    model = load_comet()
    flores = score(model, lines(args.flores_hyp), lines(args.flores_ref),
                   lines(args.flores_src), "flores")
    check_src, check_hyp = lines(args.check_src), lines(args.check_hyp)
    checked = score(model, check_hyp, lines(args.check_ref), check_src, "check")

    mechanical: dict[str, int] = {}
    for src, hyp in zip(check_src, check_hyp):
        for failure in check(src, hyp, tgt_lang=args.tgt_lang):
            mechanical[failure.kind] = mechanical.get(failure.kind, 0) + 1

    result = {
        "label": args.label,
        "flores": flores,
        "check": checked,
        "delta": {k: round(checked[k] - flores[k], 2)
                  for k in ("chrf", "spbleu", "comet22")},
        "mechanical": mechanical,
    }
    args.out.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    render(args.check_src, args.review_out, [(args.label, check_hyp)])
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
