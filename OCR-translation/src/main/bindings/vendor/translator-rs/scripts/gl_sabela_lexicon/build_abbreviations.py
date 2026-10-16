#!/usr/bin/env python3
"""Convert cotovia's abr.txt (ISO-8859-1 abbreviation table) into `@key\texpansion`
lines for the shipped lexicon. The runtime expands these before tokenization.
Multi-alternative expansions (`núm.,número,números`) keep the first alternative.

Usage: build_abbreviations.py <cotovia/lang/gl/abr.txt> <out.tsv>
"""
import sys

src, out = sys.argv[1], sys.argv[2]
lines = []
with open(src, encoding="iso-8859-1") as f:
    for line in f:
        line = line.rstrip("\n")
        if not line or line.startswith("#") or "," not in line:
            continue
        parts = line.split(",")
        key = parts[0].strip().lower()
        expansion = parts[1].strip()  # first alternative
        if key and expansion:
            lines.append(f"@{key}\t{expansion}")

with open(out, "w", encoding="utf-8") as f:
    f.write("\n".join(lines) + "\n")
print(f"wrote {len(lines)} abbreviations to {out}", file=sys.stderr)
