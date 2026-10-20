#!/usr/bin/env python3
"""Draw the KD rehearsal block of a finetune corpus, over-weighting one band.

A finetune that mixes generated short material with a rehearsal sample inherits
the rehearsal's composition, and one band of it decides whether figures survive
a long sentence. The ka->en ft5 corpus was 5.7:1 short-numeric to long-numeric
where ft4 had been 3.0:1, and its regression was exactly the long numeric rows:
"the train 832 arriving at 14:05" came back without the train number
(ka_findings.md 31). The tranche cannot supply that band — it holds no rows at
all above fifteen source words — but the KD pool has 20k of them already.

So the rehearsal is drawn at the size the caller asks for and then RE-WEIGHTED
inside that size: a row that carries a figure and is long enters `--boost` times,
and enough rows from the rest are dropped to keep the total exactly N. The
rehearsal-to-finetune ratio the caller chose therefore does not move, which
matters because that ratio was measured (64% of English tokens in both ft4 and
ft5) and is not the mechanism being fixed.

Selection is a function of `--seed` alone, so a re-run reproduces the corpus.

    sample_rehearsal.py --pool train.ka2en/aligned/train.tsv --rows 331000 \\
      --out kd.rehearsal.tsv --seed ka-ft6 --long-words 8 --boost 3 \\
      --report kd.rehearsal.json
"""

from __future__ import annotations

import argparse
import json
import pathlib
import random
import re
from collections.abc import Iterable, Sequence
from dataclasses import dataclass

DIGIT = re.compile(r"\d")


@dataclass(frozen=True)
class Plan:
    """How many times each pool row enters the sample, as a sparse list."""

    counts: dict[int, int]
    long_numeric_drawn: int
    dropped_for_room: int

    @property
    def rows(self) -> int:
        return sum(self.counts.values())


@dataclass(frozen=True)
class Band:
    """Counts of the two numeric bands in some set of rows."""

    short_numeric: int
    long_numeric: int

    @property
    def ratio(self) -> float:
        return self.short_numeric / self.long_numeric if self.long_numeric else float("inf")


def is_long_numeric(source: str, long_words: int) -> bool:
    return bool(DIGIT.search(source)) and len(source.split()) >= long_words


def band_of(sources: Iterable[str], long_words: int) -> Band:
    short = long = 0
    for source in sources:
        if not DIGIT.search(source):
            continue
        if len(source.split()) >= long_words:
            long += 1
        else:
            short += 1
    return Band(short, long)


def plan_sample(flags: Sequence[bool], rows: int, boost: int, seed: str) -> Plan:
    """Pick `rows` rows out of the pool, with the flagged band entering `boost` times.

    The extra copies are paid for out of the unflagged rows, so the sample is the
    size that was asked for whatever the boost is. Raises when the flagged band
    alone would overflow the sample, because silently clipping the boost would
    make the corpus depend on the pool's composition in a way nobody reads.
    """
    if rows > len(flags):
        raise ValueError(f"asked for {rows} rows from a pool of {len(flags)}")
    rng = random.Random(seed)
    chosen = rng.sample(range(len(flags)), rows)
    flagged = [i for i in chosen if flags[i]]
    plain = [i for i in chosen if not flags[i]]
    extra = len(flagged) * (boost - 1)
    if extra > len(plain):
        raise ValueError(
            f"boosting {len(flagged)} rows {boost}x needs {extra} slots and only "
            f"{len(plain)} unflagged rows were drawn; lower --boost or raise --rows"
        )
    rng.shuffle(plain)
    kept_plain = plain[: len(plain) - extra]
    counts = {i: boost for i in flagged}
    counts.update({i: 1 for i in kept_plain})
    return Plan(counts, len(flagged), extra)


def read_pool(path: pathlib.Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--pool", type=pathlib.Path, required=True, help="the KD corpus, source in column 1")
    ap.add_argument("--rows", type=int, required=True)
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--seed", required=True)
    ap.add_argument("--long-words", type=int, default=8, help="source words at or above which a row is long")
    ap.add_argument("--boost", type=int, default=3, help="copies of each long numeric row")
    ap.add_argument("--report", type=pathlib.Path, default=None)
    args = ap.parse_args()

    pool = read_pool(args.pool)
    sources = [line.split("\t", 1)[0] for line in pool]
    flags = [is_long_numeric(s, args.long_words) for s in sources]
    plan = plan_sample(flags, args.rows, args.boost, args.seed)

    with args.out.open("w", encoding="utf-8") as out:
        for index, count in sorted(plan.counts.items()):
            out.write((pool[index] + "\n") * count)

    before = band_of((sources[i] for i in plan.counts), args.long_words)
    after = band_of(
        (sources[i] for i, count in plan.counts.items() for _ in range(count)), args.long_words
    )
    report = {
        "pool_rows": len(pool),
        "pool_long_numeric": sum(flags),
        "rows": plan.rows,
        "long_words": args.long_words,
        "boost": args.boost,
        "long_numeric_drawn": plan.long_numeric_drawn,
        "dropped_for_room": plan.dropped_for_room,
        "band_before_boost": {"short": before.short_numeric, "long": before.long_numeric,
                              "ratio": round(before.ratio, 2)},
        "band_after_boost": {"short": after.short_numeric, "long": after.long_numeric,
                             "ratio": round(after.ratio, 2)},
    }
    print(json.dumps(report, indent=2))
    if args.report:
        args.report.write_text(json.dumps(report, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
