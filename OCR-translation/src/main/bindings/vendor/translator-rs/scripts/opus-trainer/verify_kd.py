#!/usr/bin/env python3
"""Gate a KD decode before it becomes training data.

A teacher decode fails in ways that are silent downstream: a dropped line shifts
every later pair by one, an empty target trains the student to emit nothing, and
a target left in the SOURCE script trains it to copy. None of these raise, and
all of them are cheap to find here and expensive to find after a train.

Reports per defect class rather than one summed score, for the reason
probe_check.py already documents: a summed rate ranks a model that fails one way
badly above a model that fails three ways mildly.

Fails (exit 1) only on the defects that make the corpus unusable -- a line-count
mismatch, or a defect rate above --max-rate. Everything else is reported.

    verify_kd.py --src kd_src --tgt kd_tgt --tgt-script georgian --sample 20
"""

import argparse
import random
import sys
import unicodedata
from pathlib import Path

# Ranges that count as "the target was written in the target script". Latin is
# listed so a Latin-target pair can be checked the same way; a target language
# that shares its script with the source cannot use this check at all, which is
# why --tgt-script may be omitted.
SCRIPTS = {
    "georgian": ((0x10D0, 0x10FF), (0x1C90, 0x1CBF)),
    "latin": ((0x0041, 0x005A), (0x0061, 0x007A)),
    "arabic": ((0x0600, 0x06FF), (0x0750, 0x077F)),
    "hebrew": ((0x0590, 0x05FF),),
    "cyrillic": ((0x0400, 0x04FF),),
    "devanagari": ((0x0900, 0x097F),),
}


def in_script(text: str, ranges: tuple) -> bool:
    return any(lo <= ord(c) <= hi for c in text for lo, hi in ranges)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--src", required=True, type=Path)
    ap.add_argument("--tgt", required=True, type=Path)
    ap.add_argument("--tgt-script", choices=sorted(SCRIPTS),
                    help="omit when source and target share a script, which makes "
                         "the copy-through and wrong-script checks meaningless")
    ap.add_argument("--registers", metavar="NAME:N,...",
                    help="register block sizes in PRECEDENCE order, as written by "
                         "sample_mix.py (human:30357,ui:21049,...). Without this a "
                         "flat rate cannot separate an entity register passing brand "
                         "names through, which is correct, from a model failing on "
                         "short input, which is not")
    ap.add_argument("--sample", type=int, default=10, help="aligned pairs to print for reading")
    ap.add_argument("--max-rate", type=float, default=1.0,
                    help="fail if any single defect class exceeds this percentage")
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    src = args.src.read_text(encoding="utf-8").splitlines()
    tgt = args.tgt.read_text(encoding="utf-8").splitlines()
    if len(src) != len(tgt):
        sys.exit(f"FATAL line-count mismatch: src={len(src)} tgt={len(tgt)}. "
                 "Every pair after the first divergence is mis-aligned; do not train on this.")

    ranges = SCRIPTS[args.tgt_script] if args.tgt_script else None
    counts = {k: 0 for k in ("empty", "copy_through", "wrong_script", "too_long", "too_short", "control_chars")}
    ratios = []
    for s, t in zip(src, tgt):
        st, tt = s.strip(), t.strip()
        if not tt:
            counts["empty"] += 1
            continue
        if tt.lower() == st.lower():
            counts["copy_through"] += 1
        if ranges is not None and not in_script(tt, ranges):
            counts["wrong_script"] += 1
        if any(unicodedata.category(c) == "Cc" for c in tt):
            counts["control_chars"] += 1
        if st:
            r = len(tt) / len(st)
            ratios.append(r)
            if r > 3.0:
                counts["too_long"] += 1
            elif r < 0.25:
                counts["too_short"] += 1

    n = len(src)
    print(f"pairs {n}")
    worst = 0.0
    for kind, c in sorted(counts.items()):
        rate = 100 * c / n
        worst = max(worst, rate)
        print(f"  {kind:14s} {c:>9d}  {rate:6.3f}%")
    if ratios:
        ratios.sort()
        print(f"  len ratio tgt/src   p05={ratios[len(ratios) // 20]:.2f} "
              f"median={ratios[len(ratios) // 2]:.2f} p95={ratios[19 * len(ratios) // 20]:.2f}")

    if args.registers:
        print("\n--- per register ---")
        lo = 0
        for item in args.registers.split(","):
            name, _, count = item.partition(":")
            hi = min(lo + int(count), n)
            block = list(zip(src[lo:hi], tgt[lo:hi]))
            if block:
                hits = {k: 0 for k in counts}
                for s_, t_ in block:
                    st, tt = s_.strip(), t_.strip()
                    if not tt:
                        hits["empty"] += 1
                        continue
                    if tt.lower() == st.lower():
                        hits["copy_through"] += 1
                    if ranges is not None and not in_script(tt, ranges):
                        hits["wrong_script"] += 1
                shown = " ".join(f"{k}={100 * v / len(block):.2f}%"
                                 for k, v in sorted(hits.items()) if v)
                print(f"  {name:10s} n={len(block):>8}  {shown or '-'}")
            lo = hi
        if lo < n:
            print(f"  {'(unnamed)':10s} n={n - lo:>8}  register list covers only {lo} of {n} lines")

    rng = random.Random(args.seed)
    print(f"\n--- {args.sample} aligned pairs, read them ---")
    for i in rng.sample(range(n), min(args.sample, n)):
        print(f"[{i}] EN {src[i][:110]}")
        print(f"      -> {tgt[i][:110]}")

    if worst > args.max_rate:
        sys.exit(f"\nFAIL a defect class exceeds --max-rate {args.max_rate}%")
    print(f"\nOK no defect class above {args.max_rate}%")


if __name__ == "__main__":
    main()
