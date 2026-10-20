#!/usr/bin/env python3
"""Sentence-segment document-level Uyghur mono (HPLT v2, MADLAD-400) into lines.

Both sources ship DOCUMENT records; the rest of the pipeline is line-based, so
this runs before anything else. No Uyghur-specific segmenter is needed: split on
sentence punctuation (. ؟ ! ؛ …) plus newlines, then length-filter.

Reads .jsonl.zst / .jsonl.gz / .jsonl and writes one sentence per line. The text
field differs by source, so it is named explicitly rather than guessed — a
renamed field must fail loudly, not silently yield zero lines.

Usage:
    segment_mono.py --in hplt/1.jsonl.zst   --text-field text --out hplt.lines
    segment_mono.py --in madlad/*.jsonl.gz  --text-field text --out madlad.lines
"""

from __future__ import annotations

import argparse
import gzip
import json
import re
import subprocess
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator

# Split AFTER sentence-final punctuation. Arabic question mark / semicolon and
# the Latin set both occur in Uyghur web text.
SPLIT_RE = re.compile(r"(?<=[.!?؟؛…])\s+|\n+")
# Uyghur text must be majority Arabic-script; menus and nav bars are mostly not.
ARABIC_RE = re.compile(r"[؀-ۿݐ-ݿ]")

# Web junk that survives dedup and would otherwise be decoded. Each fires on 0 of
# the 1012 FLORES devtest lines; counting punctuation does NOT (>=4 ascii punct
# hits 11.9% of HPLT but also 7.7% of FLORES), so only these precise forms drop.
JUNK_RES = (
    # Discuz! forum anti-scrape watermark: "$ u% d* d& f& ]; R( ^$ F' ~; |* d".
    # The random prefix makes each copy of a boilerplate post unique, which is
    # exactly why dedup does not catch the footer it is glued to.
    re.compile(r"[A-Za-z] ?[%&$#*;,)(]+ ?[A-Za-z0-9]"),
    # Forum footer, repeated across posts: "this content is entirely from ...".
    re.compile(r"مەزمۇنلار پۈتۈنلەي"),
    # Chinese-mixed rows (UI strings, product catalogues) — the teacher would
    # translate the Chinese too.
    re.compile(r"[一-鿿]"),
)


@dataclass(frozen=True)
class Limits:
    min_chars: int
    max_chars: int
    min_arabic_ratio: float


def arabic_ratio(text: str) -> float:
    # Deliberately per-character despite costing ~80% of segmentation runtime: the
    # ratio is over LETTERS, and the Arabic block [؀-ۿ] also holds punctuation (، ؟)
    # and Arabic-Indic digits, so a whole-string findall/sub counts those as letters
    # and disagrees with this on 53% of real lines. A faster version needs a
    # letter-only Arabic class, not a cheaper way to run the wrong one.
    letters = [c for c in text if c.isalpha()]
    if not letters:
        return 0.0
    return sum(1 for c in letters if ARABIC_RE.match(c)) / len(letters)


def sentences(doc: str, limits: Limits) -> Iterator[str]:
    for piece in SPLIT_RE.split(doc):
        text = " ".join(piece.split())
        if not text:
            continue
        if not limits.min_chars <= len(text) <= limits.max_chars:
            continue
        if arabic_ratio(text) < limits.min_arabic_ratio:
            continue
        if any(rx.search(text) for rx in JUNK_RES):
            continue
        yield text


def open_jsonl(path: Path) -> Iterator[str]:
    if path.suffix == ".zst":
        proc = subprocess.Popen(["zstdcat", str(path)], stdout=subprocess.PIPE, text=True, encoding="utf-8")
        assert proc.stdout is not None
        yield from proc.stdout
        if proc.wait() != 0:
            raise RuntimeError(f"zstdcat failed on {path}")
        return
    if path.suffix == ".gz":
        with gzip.open(path, "rt", encoding="utf-8") as f:
            yield from f
        return
    with path.open(encoding="utf-8") as f:
        yield from f


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="src", type=Path, nargs="+", required=True)
    ap.add_argument("--text-field", required=True, help="JSON field holding the document text")
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--min-chars", type=int, default=20)
    ap.add_argument("--max-chars", type=int, default=500)
    ap.add_argument("--min-arabic-ratio", type=float, default=0.5)
    args = ap.parse_args()

    limits = Limits(args.min_chars, args.max_chars, args.min_arabic_ratio)
    stats: Counter[str] = Counter()
    with args.out.open("w", encoding="utf-8") as out:
        for path in args.src:
            for raw in open_jsonl(path):
                raw = raw.strip()
                if not raw:
                    continue
                stats["docs"] += 1
                rec = json.loads(raw)
                if args.text_field not in rec:
                    raise KeyError(
                        f"{path}: record has no {args.text_field!r}; fields are {sorted(rec)[:12]}"
                    )
                for sent in sentences(rec[args.text_field], limits):
                    stats["lines"] += 1
                    out.write(sent + "\n")
            print(f"  {path.name}: docs={stats['docs']:,} lines={stats['lines']:,}", file=sys.stderr)

    if stats["docs"] == 0:
        raise SystemExit("no documents read")
    print(f"docs {stats['docs']:,} -> lines {stats['lines']:,} "
          f"({stats['lines'] / stats['docs']:.1f} lines/doc)", file=sys.stderr)


if __name__ == "__main__":
    main()
