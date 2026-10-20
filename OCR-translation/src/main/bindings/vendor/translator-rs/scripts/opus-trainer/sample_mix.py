#!/usr/bin/env python3
"""Draw a KD source from the per-register pools, to absolute per-register targets.

Replaces the `dedup | shuf | head` draw over one concatenated pool. That draw is
proportional by construction, and the proportions are catastrophic: en-tl has
63.5M NLLB lines against 23k translatewiki, so a 10M sample took ~4k UI lines and
the model never saw the register the camera path actually photographs.

Emits, into OUT_DIR:
    kd_src    the source side, one line per KD decode
    kd_ref    the SAME line's bitext target, pairing preserved (extract-best/ce-filter)
    mix.json  target vs available vs realized, per register

mix.json is the point of the exercise as much as the corpus is: "Swahili only had
3k UI lines" should be a recorded number, not something rediscovered months later
by reading model output.

Usage:
    sample_mix.py --out DIR --kd-col N --total N --mix SPEC [--seed 42] \
                  --pool human=PATH --pool ui=PATH ... (all five)

Every register is passed explicitly rather than globbed out of a directory, so a
flow that forgets one fails here instead of quietly training without it — which
is the failure this whole mechanism exists to prevent, and it would be perverse
to reintroduce it in the tool that fixes it.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

from registers import PRECEDENCE, Mix, Register

# The KD source column is deduplicated, not the whole line: two bitext pairs with
# the same source and different targets are one decode, and keeping both would
# feed the teacher the same input twice and pay for it twice. The pool is already
# unique on (src, tgt); this is the stronger key.
DEDUP_AND_SHUFFLE = """
set -euo pipefail
awk -F'\\t' -v k="$2" 'NF == 2 && $1 ~ /[^[:space:]]/ && $2 ~ /[^[:space:]]/ && !seen[$k]++' "$1" \
  | shuf --random-source=<(yes "$3")
"""


def available_lines(pool: Path, kd_col: int, seed: int, scratch: Path) -> tuple[Path, int]:
    """Dedup + shuffle one register's pool, returning the file and its length.

    Shuffled BEFORE the count so the later `head` is a uniform sample of the
    register rather than whatever order the corpus arrived in — the old draw
    shuffled a concatenated pool, which is uniform overall and therefore not
    uniform within any register that is small.
    """
    out = scratch / f"shuf.{pool.stem}"
    with out.open("w") as w:
        subprocess.run(["bash", "-c", DEDUP_AND_SHUFFLE, "sample_mix",
                        str(pool), str(kd_col), str(seed)],
                       check=True, stdout=w)
    return out, sum(1 for _ in out.open("rb"))


def parse_pools(items: list[str]) -> dict[Register, Path]:
    pools: dict[Register, Path] = {}
    for item in items:
        key, _, value = item.partition("=")
        try:
            register = Register(key.strip())
        except ValueError:
            raise SystemExit(f"unknown register {key!r} in --pool {item!r}") from None
        if register in pools:
            raise SystemExit(f"--pool {register} given twice")
        path = Path(value)
        if not path.exists():
            raise SystemExit(
                f"--pool {register}={path} does not exist. Empty pools must still be "
                "passed as empty FILES — an absent file is indistinguishable from a "
                "register nobody assigned."
            )
        pools[register] = path
    missing = set(Register) - set(pools)
    if missing:
        raise SystemExit(f"no --pool for {sorted(r.value for r in missing)}")
    return pools


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--kd-col", required=True, type=int, choices=(1, 2))
    ap.add_argument("--total", required=True, type=int)
    ap.add_argument("--mix", required=True)
    ap.add_argument("--seed", type=int, default=42)
    ap.add_argument("--pool", action="append", default=[], metavar="REGISTER=PATH")
    args = ap.parse_args()

    kd_col, out_dir, total = args.kd_col, args.out, args.total
    ref_col = 2 if kd_col == 1 else 1
    pools = parse_pools(args.pool)

    # Mix.parse validates that every register is named — a missing one raises
    # here, before any GPU is rented, rather than silently contributing zero.
    mix = Mix.parse(args.mix, total)

    out_dir.mkdir(parents=True, exist_ok=True)
    scratch = out_dir / "scratch"
    scratch.mkdir(exist_ok=True)

    shuffled: dict[Register, Path] = {}
    available: dict[Register, int] = {}
    for register in PRECEDENCE:
        path, n = available_lines(pools[register], kd_col, args.seed, scratch)
        shuffled[register], available[register] = path, n
        print(f"[{register.value}] {n} available", file=sys.stderr)

    taken = mix.draw(available)

    src_path, ref_path = out_dir / "kd_src", out_dir / "kd_ref"
    with src_path.open("w", encoding="utf-8") as fs, ref_path.open("w", encoding="utf-8") as fr:
        for register in PRECEDENCE:
            n = taken[register]
            if n <= 0:
                continue
            with shuffled[register].open(encoding="utf-8") as f:
                for i, line in enumerate(f):
                    if i >= n:
                        break
                    parts = line.rstrip("\n").split("\t")
                    fs.write(parts[kd_col - 1] + "\n")
                    fr.write(parts[ref_col - 1] + "\n")

    report = {
        r.value: {
            "target": "fill" if r == mix.fill else mix.caps[r],
            "available": available[r],
            "realized": taken[r],
        }
        for r in PRECEDENCE
    }
    realized_total = sum(taken.values())
    report["_total"] = {"target": total, "realized": realized_total}
    (out_dir / "mix.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    for register in PRECEDENCE:
        row = report[register.value]
        short = " SHORT" if row["realized"] < (row["target"] if isinstance(row["target"], int) else 0) else ""
        print(f"[{register.value}] target={row['target']} available={row['available']} "
              f"realized={row['realized']}{short}", file=sys.stderr)
    if realized_total < total:
        print(f"NOTE: {realized_total} of a {total} target — the pair does not have "
              f"enough data to fill it, which mix.json records.", file=sys.stderr)

    for path in shuffled.values():
        path.unlink()
    scratch.rmdir()
    print(f"kd_src/kd_ref: {realized_total} lines", file=sys.stderr)


if __name__ == "__main__":
    main()
