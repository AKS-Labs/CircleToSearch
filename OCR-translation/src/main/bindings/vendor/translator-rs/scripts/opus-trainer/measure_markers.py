#!/usr/bin/env python3
"""Measure Philippine-confusable marker fire-rates on a real Tagalog corpus.

Candidates are mined from FLORES (multi-parallel, so token differences are
linguistic not topical), but FLORES CANNOT bound their cost: FLORES tgl is clean
formal Tagalog with no code-switching, while real tl corpora are Taglish-heavy.
Measured 2026-07-20 on FLORES: `to` scored 0.05% on tgl and fires on 42.8% of
ENGLISH lines; `ha` scored 0.00% on tgl and is an everyday Tagalog particle
("salamat ha"). Both would look free on gold and shred the corpus.

So the cost is measured here, on the corpus that will actually be filtered —
which is the gate discipline, and the reason gold alone is not allowed to clear
a rule.

Reports per marker: share of lines it fires on. And per language: the share
firing >=2 distinct markers, which is the rule worth having — a single token can
be a loanword, a name or a particle; two independent function words rarely are.

    measure_markers.py corpus.tl.gz [--limit 5000000]
"""

import argparse
import gzip
import re
from collections import Counter
from pathlib import Path

TOKEN = re.compile(r"[^\W\d_]+", re.UNICODE)

# Mined from FLORES dev+devtest, then hand-pruned of tokens that collide with
# English (to/an/no/so) because Taglish puts English inside genuine Tagalog.
MARKERS = {
    "ceb": ["ug", "og", "kini", "adunay", "dili", "aron", "gikan", "apan",
            "mao", "niini", "bisan"],
    "ilo": ["ti", "iti", "ket", "ken", "dagiti", "kadagiti", "maysa", "idiay",
            "idi", "adda"],
    "war": ["han", "ngan", "hin", "diri", "sugad", "tikang", "ira"],
    "pag": ["ya", "ed", "tan", "saray", "diad", "nen", "pian"],
}
# EXCLUDED, with the reason, so they do not come back:
#   nga             - MEASURED 2.233% of 5M real tl lines. A common Tagalog
#                     particle ("oo nga", "talaga nga"). It was HAND-ADDED, not
#                     mined, and alone inflated ceb/ilo/war from ~0.03% to ~2.25%.
#   kaayo/gyud/hiya/waray/kas/balet/agto - also hand-added, unmeasured, dropped
#                     for the same reason: every hand-asserted rule in this
#                     project has been wrong on first write, every mined one right.
#   to/an/no/so/la  - collide with English (`to` hits 42.8% of English lines) and
#                     Taglish is authentic Tagalog, so these cut real data.
#   ha              - everyday Tagalog particle ("salamat ha"); FLORES scored it
#                     0.00% because FLORES tgl is formal and never colloquial.
#   ini/kon/say/pay - too short/ambiguous to carry a rule alone.


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("corpus", type=Path)
    ap.add_argument("--limit", type=int, default=0, help="0 = whole file")
    args = ap.parse_args()

    single: Counter = Counter()
    multi: Counter = Counter()
    total = 0
    opener = gzip.open if args.corpus.suffix == ".gz" else open
    with opener(args.corpus, "rt", encoding="utf-8", errors="replace") as f:
        for line in f:
            total += 1
            if args.limit and total > args.limit:
                total -= 1
                break
            toks = set(TOKEN.findall(line.lower()))
            if not toks:
                continue
            for lang, markers in MARKERS.items():
                hits = sum(1 for m in markers if m in toks)
                if hits:
                    single[lang] += 1
                if hits >= 2:
                    multi[lang] += 1
            for lang, markers in MARKERS.items():
                for m in markers:
                    if m in toks:
                        single[f"{lang}:{m}"] += 1

    print(f"corpus: {args.corpus}  lines: {total}\n")
    print(f"{'lang':6s} {'>=1 marker':>12s} {'>=2 markers':>12s}")
    for lang in MARKERS:
        print(f"{lang:6s} {single[lang]/total:11.2%} {multi[lang]/total:12.2%}")
    print("\nper-marker fire rate (>=1 occurrence in the line):")
    for lang, markers in MARKERS.items():
        for m in markers:
            print(f"  {lang}:{m:10s} {single[f'{lang}:{m}']/total:8.3%}")


if __name__ == "__main__":
    main()
