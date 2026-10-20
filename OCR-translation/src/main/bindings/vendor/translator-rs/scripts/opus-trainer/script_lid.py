#!/usr/bin/env python3
"""Arabic-script Uyghur/Kazakh separator.

fastText lid.176 classifies Kazakh from Cyrillic only, so Arabic-script (töte)
Kazakh scores as Uyghur and passes every LID gate in prep_data.py. ~15-20% of
the uig kd_source is töte Kazakh because of this.

Orthography splits the two cleanly where it is visible at all: Uyghur spells
every vowel and has no ayn/standalone-hamza, while töte Kazakh marks front
vowels with a high hamza and keeps ع in Arabic loanwords.

Rule direction is drop-on-Kazakh, never keep-only-Uyghur: markers cover ~99.5%
of prose but only ~86% of short lines, and short pairs are the supply the
student needs to not free-run on short inputs. Lines carrying neither marker
set are RESIDUAL and are kept, not dropped.

Usage:
    script_lid.py --column 1 < train.tsv > train.uy.tsv
    script_lid.py --column 1 --dropped kk.tsv --residual res.tsv < train.tsv > out.tsv
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import TextIO

# Uyghur evidence comes in two strengths, and conflating them is a bug: a WEAK
# marker must not be allowed to overrule explicit Kazakh spelling.
#
# STRONG — effectively absent from töte Kazakh (doc-freq in certain-Kazakh vs
# certain-Uyghur): ئ 0.00%/89.1%, ې 0.00%/49.5%, ۈ 0.00%/39.1%, غ 0.03%/46.6%,
# خ 0.03%/29.3%. غ and خ are the other half of KAZAKH_MARKERS — the scripts spell
# one phoneme with different letters, so the pairs are complementary: Uyghur /ʁ/
# is غ where Kazakh writes ع, Uyghur /χ/ is خ where Kazakh writes ح.
UYGHUR_STRONG = frozenset("ئېۈغخ")

# WEAK — Uyghur-leaning but present in Kazakh: ھ 0.64%/38.8%, چ 1.01%/43.9%.
# Kazakh lines carrying ء/ع/ح alongside a ھ or چ read as Kazakh, so these only
# rescue a line from the mined-token pass, never from explicit Kazakh spelling.
# ۆ (3.95%/32.0%) is too common in Kazakh to serve even as weak evidence.
UYGHUR_WEAK = frozenset("ھچ")

UYGHUR_MARKERS = UYGHUR_STRONG | UYGHUR_WEAK

# Kazakh evidence is tiered for the same reason Uyghur is: these markers differ
# in strength by two orders of magnitude, and the weak ones must not outrank
# UYGHUR_WEAK. Rates below are doc-freq in certain-Kazakh vs certain-Uyghur.
#
# STRONG — high hamza U+0674-U+0678 (ٴ ٵ ٶ ٷ ٸ, 3.53%/0.000%) marks Kazakh front
# vowels; standalone hamza ء (0621, 51.1%/0.018%) and ع (0639, 70.1%/0.126%)
# survive in Kazakh loanwords but are respelled out of Uyghur; ح (062D,
# 9.35%/0.066%) is not in the Uyghur alphabet at all (Uyghur uses خ and ھ).
KAZAKH_STRONG = frozenset("ٴٵٶٷٸءعح")

# WEAK — ۅ (06C5) / ۉ (06C9) are Kyrgyz, picked up from the same crawls. ۉ is
# only 0.53%/0.031% (17:1) where ع is 555:1, so it is weaker evidence than ھ is
# for Uyghur (60:1) and must not overrule it. ۅ is unused by both (<0.005%).
KAZAKH_WEAK = frozenset("ۅۉ")

KAZAKH_MARKERS = KAZAKH_STRONG | KAZAKH_WEAK

# Arabic-script letter runs; tokens are delimited by construction, never matched
# as substrings: Uyghur prefixes ئ onto vowel-initial words, so ەمەس as a
# substring hits Uyghur ئەمەس (2.6% false-positive) but as a token does not.
TOKEN_RE = re.compile(r"[؀-ۿݐ-ݿ]+")


class Verdict(Enum):
    UYGHUR = "uyghur"
    KAZAKH = "kazakh"
    KAZAKH_TOKEN = "kazakh_tok"
    RESIDUAL = "residual"


def classify(text: str, kk_tokens: frozenset[str] = frozenset()) -> Verdict:
    """Uyghur markers win: the rule drops on Kazakh evidence, never keeps on Uyghur
    evidence alone, because markers cover only ~86% of short lines and short pairs
    are the supply the student needs.

    kk_tokens (mined by mine_markers.py) are reachable only for lines carrying
    neither marker set — töte writes the high hamza on front-vowel words only, so
    an all-back-vowel Kazakh sentence is marker-free by construction and lands in
    the residual. That is what the token pass is for.
    """
    chars = frozenset(text)
    if chars & UYGHUR_STRONG:
        return Verdict.UYGHUR
    if chars & KAZAKH_STRONG:
        return Verdict.KAZAKH
    if chars & UYGHUR_WEAK:
        return Verdict.UYGHUR
    if chars & KAZAKH_WEAK:
        return Verdict.KAZAKH
    if kk_tokens and kk_tokens & set(TOKEN_RE.findall(text)):
        return Verdict.KAZAKH_TOKEN
    return Verdict.RESIDUAL


@dataclass(frozen=True)
class Row:
    line: str
    field: str


def parse_row(line: str, column: int | None) -> Row:
    stripped = line.rstrip("\n")
    if column is None:
        return Row(line=stripped, field=stripped)
    fields = stripped.split("\t")
    if len(fields) < column:
        raise ValueError(f"row has {len(fields)} fields, need column {column}: {stripped[:120]!r}")
    return Row(line=stripped, field=fields[column - 1])


def run(
    src: TextIO,
    keep: TextIO,
    dropped: TextIO | None,
    residual: TextIO | None,
    column: int | None,
    kk_tokens: frozenset[str],
) -> Counter[Verdict]:
    counts: Counter[Verdict] = Counter()
    for line in src:
        row = parse_row(line, column)
        verdict = classify(row.field, kk_tokens)
        counts[verdict] += 1
        if verdict in (Verdict.KAZAKH, Verdict.KAZAKH_TOKEN):
            if dropped is not None:
                dropped.write(row.line + "\n")
            continue
        if verdict is Verdict.RESIDUAL and residual is not None:
            residual.write(row.line + "\n")
        keep.write(row.line + "\n")
    return counts


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--column", type=int, default=None,
                    help="1-based TSV field to classify; default classifies the whole line")
    ap.add_argument("--in", dest="src", type=Path, default=None, help="input file (default stdin)")
    ap.add_argument("--out", type=Path, default=None, help="kept rows (default stdout)")
    ap.add_argument("--dropped", type=Path, default=None, help="write Kazakh-marked rows here")
    ap.add_argument("--residual", type=Path, default=None,
                    help="write kept-but-unmarked rows here (they are also in --out)")
    ap.add_argument("--markers", type=Path, default=None,
                    help="mined Kazakh token list (mine_markers.py --markers-out); residual-only rule")
    args = ap.parse_args()

    if args.column is not None and args.column < 1:
        ap.error("--column is 1-based")

    kk_tokens = frozenset(args.markers.read_text(encoding="utf-8").split()) if args.markers else frozenset()

    src = args.src.open(encoding="utf-8") if args.src else sys.stdin
    keep = args.out.open("w", encoding="utf-8") if args.out else sys.stdout
    dropped = args.dropped.open("w", encoding="utf-8") if args.dropped else None
    residual = args.residual.open("w", encoding="utf-8") if args.residual else None
    try:
        counts = run(src, keep, dropped, residual, args.column, kk_tokens)
    finally:
        for handle in (src, keep, dropped, residual):
            if handle is not None and handle not in (sys.stdin, sys.stdout):
                handle.close()

    total = sum(counts.values())
    if total == 0:
        raise SystemExit("no input rows")
    for verdict in Verdict:
        n = counts[verdict]
        print(f"{verdict.value:9s} {n:>10,}  {100.0 * n / total:6.2f}%", file=sys.stderr)
    print(f"{'total':9s} {total:>10,}", file=sys.stderr)
    kept = total - counts[Verdict.KAZAKH] - counts[Verdict.KAZAKH_TOKEN]
    print(f"kept {kept:,} / {total:,} = {100.0 * kept / total:.2f}%", file=sys.stderr)


if __name__ == "__main__":
    main()
