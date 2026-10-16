#!/usr/bin/env python3
"""Emit one teacher's eval numbers as a machine-readable artifact.

A teacher gate is a CHOICE, not a build failure — you cannot fix NLLB, only pick
something else — so this reports and never exits non-zero on quality. What it
must do is make the numbers comparable across teachers and durable, because the
July tl rows in NOTES were chrF-only and drifted up to 2 points when re-measured.

Reports per direction:
  flores      chrF++/spBLEU/COMET22 on FLORES devtest — the sentence-level number
  probe       the same three on the probe set — the deployment-shaped number
  delta       probe minus flores; a large negative is brittleness under
              distribution shift, which is what separated the teachers on tl
              (Hy-MT2 +0.64 chrF, OPUS-MT -11.15) when the flores numbers alone
              called them a tie
  mechanical  reference-free defect counts from probe_check (number_dropped,
              length_blowup, repetition, copy_through, empty)

The mechanical counts are reported PER KIND and never summed into a score: on tl
the summed rate ranked OPUS-MT (100% clean) above Hy-MT2 while OPUS-MT was
emitting "Reflection" for Banyo. It is a defect list, not a ranking.

Several teachers may be passed in one invocation; the COMET checkpoint is loaded
once for all of them, which is the difference between ~10 minutes and ~1.

    teacher_metrics.py PROBES_JSONL OUT_DIR \
        --teacher hy-mt2 flores_hy/ probe_hy/ --teacher nllb-600m flores_600m/ probe_600m/
"""

import argparse
import json
import subprocess
import sys
from pathlib import Path

import sacrebleu

from chrf_score import comet22, load_comet

HERE = Path(__file__).resolve().parent
PAIRS = {"en2tl": "eng_Latn-tgl_Latn", "tl2en": "tgl_Latn-eng_Latn"}


def score(model, hyp: list[str], ref: list[str], src: list[str]) -> dict:
    return {
        "chrf": round(sacrebleu.corpus_chrf(hyp, [ref], word_order=2).score, 2),
        "spbleu": round(sacrebleu.corpus_bleu(hyp, [ref], tokenize="flores200").score, 2),
        "comet22": round(comet22(model, src, hyp, ref), 2),
        "n": len(hyp),
    }


def lines(p: Path) -> list[str]:
    return p.read_text(encoding="utf-8").splitlines()


def mechanical(probes: Path, hyp: Path, direction: str) -> dict[str, int]:
    out = subprocess.run(
        [sys.executable, str(HERE / "probe_check.py"), str(probes), str(hyp), direction],
        capture_output=True, text=True, check=True,
    ).stdout
    kinds: dict[str, int] = {}
    for line in out.splitlines():
        if "!!" not in line:
            continue
        kind = line.split("!!")[1].strip().split(":")[0]
        kinds[kind] = kinds.get(kind, 0) + 1
    return kinds


def evaluate(model, name: str, flores_dir: Path, probe_dir: Path,
             probes: list[dict], probes_jsonl: Path) -> dict:
    result: dict = {"teacher": name, "directions": {}}
    for direction, pair in PAIRS.items():
        f_hyp = lines(flores_dir / f"{pair}.hyp")
        f_ref = lines(flores_dir / f"{pair}.ref")
        f_src = lines(flores_dir / f"{pair}.src")
        p_hyp_path = probe_dir / f"{direction}.hyp"
        p_hyp = lines(p_hyp_path)
        src_key, ref_key = ("en", "tl") if direction == "en2tl" else ("tl", "en")
        p_src = [p[src_key] for p in probes]
        p_ref = [p[ref_key] for p in probes]

        flores = score(model, f_hyp, f_ref, f_src)
        probe = score(model, p_hyp, p_ref, p_src)
        result["directions"][direction] = {
            "flores": flores,
            "probe": probe,
            "delta": {k: round(probe[k] - flores[k], 2)
                      for k in ("chrf", "spbleu", "comet22")},
            "mechanical": mechanical(probes_jsonl, p_hyp_path, direction),
        }
    return result


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("probes_jsonl", type=Path)
    ap.add_argument("out_dir", type=Path)
    ap.add_argument("--teacher", nargs=3, action="append", required=True,
                    metavar=("NAME", "FLORES_DIR", "PROBE_DIR"))
    args = ap.parse_args()

    probes = [json.loads(l) for l in lines(args.probes_jsonl) if l.strip()]
    args.out_dir.mkdir(parents=True, exist_ok=True)
    model = load_comet()

    for name, flores_dir, probe_dir in args.teacher:
        result = evaluate(model, name, Path(flores_dir), Path(probe_dir),
                          probes, args.probes_jsonl)
        (args.out_dir / f"{name}.json").write_text(
            json.dumps(result, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {args.out_dir / f'{name}.json'}", file=sys.stderr)


if __name__ == "__main__":
    main()
