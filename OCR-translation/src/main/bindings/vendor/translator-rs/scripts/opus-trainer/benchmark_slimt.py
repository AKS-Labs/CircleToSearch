#!/usr/bin/env python3
"""Benchmark a quantized slimt pack on FLORES-200 devtest (chrF++/BLEU).

Runs the actual on-device engine via the `slimt_load_test` bin (build it first:
`cargo build --bin slimt_load_test`), translating FLORES devtest through the int8
model + optional shortlist, and scores against the reference. This is the number
that matters — it measures slimt output, not marian's.

A wrong shortlist tanks the score (34 vs 58); benchmark with `--shortlist none`
first to check the model, then with the real shortlist to check the pack.

    ./venv/bin/python benchmark_slimt.py --lang tl \
        --model model.ft.intgemm.alphas.bin --vocab vocab.entl.spm \
        --shortlist lex.50.50.entl.s2t.bin           # or: none
"""

import argparse
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

import sacrebleu

# lang -> FLORES-200 code (non-English side)
FLORES = {"tl": "tgl_Latn", "sw": "swh_Latn", "ur": "urd_Arab"}
FLORES_URL = "https://dl.fbaipublicfiles.com/nllb/flores200_dataset.tar.gz"
CACHE = Path(__file__).resolve().parent / ".cache"
REPO = Path(__file__).resolve().parents[2]


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


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--lang", required=True, choices=FLORES)
    ap.add_argument("--src", default="en", help="'en' for en->X, else X->en")
    ap.add_argument("--model", required=True)
    ap.add_argument("--vocab", required=True)
    ap.add_argument("--shortlist", default="none")
    ap.add_argument("--bin", default=str(REPO / "target" / "debug" / "slimt_load_test"))
    ap.add_argument("--no-comet", action="store_true", help="skip COMET22 (first run downloads a ~2.3GB model)")
    ap.add_argument("--probes", required=True, type=Path,
                    help="adversarial/probe SOURCE lines for this direction "
                         "(probes/adversarial.en for en->X)")
    ap.add_argument("--probe-out", required=True, type=Path,
                    help="where to write probe hypotheses")
    ap.add_argument("--review-out", required=True, type=Path,
                    help="side-by-side review; REQUIRED so bench-without-review is "
                         "not a runnable partial of this step")
    args = ap.parse_args()

    code = FLORES[args.lang]
    eng, tgt = flores_devtest("eng_Latn"), flores_devtest(code)
    src, ref = (eng, tgt) if args.src == "en" else (tgt, eng)

    def translate(lines: list[str]) -> list[str]:
        proc = subprocess.run(
            [args.bin, args.model, args.vocab, args.shortlist, "-"],
            input="\n".join(lines), capture_output=True, text=True,
        )
        if proc.returncode != 0:
            sys.exit(f"slimt_load_test failed: {proc.stderr[-500:]}")
        return proc.stdout.splitlines()

    # Decoded unconditionally, and its output is never scored here: FLORES cannot
    # see the failure class that decides shippability, and no metric replaces
    # reading the pairs. The bench's job is to always produce them.
    probe_src = [l for l in args.probes.read_text(encoding="utf-8").splitlines() if l.strip()]
    probe_hyps = translate(probe_src)
    if len(probe_hyps) != len(probe_src):
        sys.exit(f"probe decode returned {len(probe_hyps)} lines for {len(probe_src)} inputs")
    args.probe_out.write_text("\n".join(probe_hyps) + "\n", encoding="utf-8")

    hyps = translate(src)
    n = min(len(hyps), len(ref))
    chrf = sacrebleu.corpus_chrf(hyps[:n], [ref[:n]], word_order=2)
    bleu = sacrebleu.corpus_bleu(hyps[:n], [ref[:n]])
    comet = ""
    if not args.no_comet:
        from chrf_score import comet22, load_comet

        comet = f"  COMET22 {comet22(load_comet(), src[:n], hyps[:n], ref[:n]):.2f}"
    direction = f"{args.src}->{args.lang}" if args.src == "en" else f"{args.lang}->en"
    print(f"{direction}: chrF++ {chrf.score:.2f}  BLEU {bleu.score:.2f}{comet}  (n={n}, shortlist={args.shortlist})")
    from probe_review import render

    render(args.probes, args.review_out, [("student", probe_hyps)])
    print(f"{direction}: READ {args.review_out} -- FLORES does not cover signs, "
          f"imperatives or short informal input, and no metric substitutes")


if __name__ == "__main__":
    main()
