#!/usr/bin/env python3
"""Freeze one language-independent English short-text corpus, banded by length.

The English side is fixed once and reused for every pair, so a finetune set for
a new language is a translation job rather than a curation job, and every pair
ends up scored on translations of the same source.

Two sources, for different reasons:
  HARVESTED  real subtitle and talk English (harvest_short_en.py). Supplies the
             conversational bands cheaply and, more importantly, honestly: a
             model asked to invent dialogue writes what dialogue sounds like
             rather than what it is.
  GENERATED  signage, labels, menus, dosages, device screens (gen_short_en.py).
             No corpus contains these -- no film says "Emergency Exit" -- which
             is why every teacher we gated mistranslates them.

Output order is a seeded shuffle across bands, so ANY PREFIX IS A STRATIFIED
SAMPLE. Translation is bought in slices as quota allows, and stopping after two
slices leaves a balanced corpus rather than nothing but signs.

    build_short_corpus.py --harvest out/harvest --generated out/gen/short.en \
        --generated-done data/short.en-ka.gen.jsonl --out data/short.en.v1
"""

import argparse
import json
import random
from pathlib import Path

# Per-band targets. Weighted toward what the KD pool lacks: an en-ka draw is 96%
# crawl, so long prose needs nothing and short text needs everything.
TARGETS = {"w01": 7500, "w02_04": 30000, "w05_08": 34000,
           "w09_15": 21000, "w16_25": 7500}
BANDS = (("w01", 1, 1), ("w02_04", 2, 4), ("w05_08", 5, 8),
         ("w09_15", 9, 15), ("w16_25", 16, 25))


def band_of(s: str) -> str | None:
    n = len(s.split())
    for name, lo, hi in BANDS:
        if lo <= n <= hi:
            return name
    return None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--harvest", required=True, type=Path, help="dir of <band>.en from harvest_short_en.py")
    ap.add_argument("--generated", action="append", default=[], type=Path,
                    help="a generated one-per-line English file; repeatable")
    ap.add_argument("--generated-done", type=Path,
                    help="jsonl whose `en` field is already translated for some "
                         "language; those lines are kept and marked, so a slice "
                         "does not pay to translate them twice")
    ap.add_argument("--out", required=True, type=Path, help="output prefix")
    ap.add_argument("--seed", type=int, default=42)
    args = ap.parse_args()

    # Generated wins on collision: the same string reached us because someone
    # asked for it as signage, and that provenance is the more specific claim.
    entries: dict[str, dict] = {}
    done: set[str] = set()
    if args.generated_done is not None:
        for line in args.generated_done.read_text(encoding="utf-8").splitlines():
            en = json.loads(line)["en"]
            done.add(en.lower())
            if (b := band_of(en)) is not None:
                entries[en.lower()] = {"en": en, "band": b, "source": "generated", "translated": True}
    for path in args.generated:
        for en in path.read_text(encoding="utf-8").splitlines():
            en = en.strip()
            if en and (b := band_of(en)) is not None:
                entries.setdefault(en.lower(), {"en": en, "band": b, "source": "generated",
                                                "translated": en.lower() in done})

    rng = random.Random(args.seed)
    for name, _, _ in BANDS:
        pool = [l.strip() for l in (args.harvest / f"{name}.en").read_text(encoding="utf-8").splitlines()
                if l.strip() and l.strip().lower() not in entries]
        have = sum(1 for e in entries.values() if e["band"] == name)
        want = max(0, TARGETS[name] - have)
        rng.shuffle(pool)
        for en in pool[:want]:
            entries[en.lower()] = {"en": en, "band": name, "source": "harvested", "translated": False}

    rows = list(entries.values())
    rng.shuffle(rows)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    Path(f"{args.out}.en").write_text("\n".join(r["en"] for r in rows) + "\n", encoding="utf-8")
    with Path(f"{args.out}.meta.jsonl").open("w", encoding="utf-8") as f:
        for i, r in enumerate(rows):
            f.write(json.dumps({"i": i, **r}, ensure_ascii=False) + "\n")

    print(f"{len(rows)} lines -> {args.out}.en")
    for name, _, _ in BANDS:
        band = [r for r in rows if r["band"] == name]
        g = sum(r["source"] == "generated" for r in band)
        d = sum(r["translated"] for r in band)
        print(f"  {name:8s} {len(band):>6}  generated {g:>5}  harvested {len(band) - g:>6}  already-translated {d:>5}")
    print(f"  {'TOTAL':8s} {len(rows):>6}  already-translated {sum(r['translated'] for r in rows)}")


if __name__ == "__main__":
    main()
