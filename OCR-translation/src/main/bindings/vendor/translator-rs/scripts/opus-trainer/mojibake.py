#!/usr/bin/env python3
"""Fraction of Georgian-script lines that are actually Cyrillic mojibake.

The archaic Mkhedruli letters U+10F1-10FA are the images of common Cyrillic
letters under a legacy 8-bit Georgian font table, so modern Georgian text
essentially never contains them while transliterated Bulgarian is dense in
them. Counting them separates the two without needing a language model.
"""
import sys

ARCHAIC = set(range(0x10F1, 0x10FB))
MKHEDRULI = set(range(0x10D0, 0x10F1))


def rate(path: str) -> tuple[int, int, float]:
    hit = tot = 0
    for line in open(path, encoding="utf-8"):
        cps = [ord(c) for c in line]
        if not any(c in MKHEDRULI or c in ARCHAIC for c in cps):
            continue
        tot += 1
        hit += any(c in ARCHAIC for c in cps)
    return hit, tot, 100 * hit / tot if tot else 0.0


for p in sys.argv[1:]:
    h, t, r = rate(p)
    print(f"{p:52s} {h:5d}/{t:<5d} {r:5.1f}% mojibake")
