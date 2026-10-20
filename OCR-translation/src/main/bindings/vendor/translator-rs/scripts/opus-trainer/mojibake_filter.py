#!/usr/bin/env python3
"""Drop Bulgarian/Macedonian/Serbian text that a legacy Georgian font table turned
into Mkhedruli, from the ka side of an en-ka corpus.

The OPUS OpenSubtitles ka side is ~80% South-Slavic subtitles rendered through an
8-bit Georgian font: CP1251 Cyrillic bytes were painted with Georgian glyphs and
then stored as the Georgian codepoints. The result sits inside the Mkhedruli
block, so codepoint-range checks, "contains Georgian script" tests and fastText
lid.176 all pass it. Undoing the table recovers readable Slavic:

    გთზ რჲგა.     -> виж това.      (Look at that.)
    ჲბთფამ ოთუა!  -> обичам пица!   (I love pizza!)

The table is the 32 CP1251 lowercase Cyrillic letters laid position-by-position
onto the first 32 letters of the TRADITIONAL (38-letter) Georgian alphabet, which
is why the archaic letters ჱ ჲ ჳ carry з и х: they sit at traditional positions
7, 14 and 21 while modern Georgian skips them. Aligning onto the 30-letter
Bulgarian alphabet instead is wrong past щ - it mis-maps ჩ ც ძ წ, and წ=я is one
of the most frequent letters in the corrupt text.

Two tiers, both DROP-ONLY - a line is dropped on positive evidence of Slavic, never
kept on positive evidence of Georgian:

  TIER 1  any archaic Mkhedruli letter U+10F1-10FA. Those are the images of з и х
          and are absent from modern Georgian: 0 hits in the 91,700 lines of
          TED2020, KDE4, translatewiki, GNOME, ELRC-5218, QED and model-generated
          Georgian, against 66.8% of OpenSubtitles.

  TIER 2  a character-trigram likelihood ratio. Tier 1 misses the third of the
          corrupt lines that happen to contain no з/и/х, so every line is scored
          twice: by a Georgian trigram model trained on --clean-ref, and by a
          Slavic trigram model trained on the DEMAPPED tier-1 hits of the corpus
          being cleaned. A line is dropped when the Slavic model beats the
          Georgian one by --margin.

Scoring the demapped text against a Slavic model is what makes tier 2 safe on
short lines. A Georgian-only perplexity cut has to reject anything that scores
oddly, so it eats transliterated place names, UI abbreviations and loanwords
("ხაჭო", "სლაიდშოუ", "$1 მბ/წმ"); the ratio keeps them, because they are not
Bulgarian either.

Two approaches that do NOT work, measured on 200 hand-labelled archaic-free
OpenSubtitles lines (66 corrupt / 134 genuine):

  - demap then fastText lid.176: lid calls 104 of the 134 GENUINE Georgian lines
    Russian, because demapped Georgian is still Cyrillic and ru is the block's
    default answer. Restricting the drop set to bg/mk to dodge that costs recall:
    59% catch at best, against 98% for the ratio.
  - demap then look for Bulgarian stopwords: "და" (and, the single most common
    Georgian word) demaps to "да" and "ნა" to "на", so the naive wordlist fires
    on ~1.1% of genuine Georgian.

Lines with fewer than MIN_LETTERS Georgian letters carry too little signal to
score and are always kept; tier 1 still applies to them.

Usage:
    mojibake_filter.py --clean-ref TED2020.en-ka.ka --clean-ref QED.en-ka.ka \
        --in OpenSubtitles.en-ka.ka --out kept.ka --dropped mojibake.ka

    mojibake_filter.py --clean-ref clean.ka --column 2 \
        --in train.en-ka.tsv --out kept.tsv --dropped dropped.tsv

    # a corpus with too few tier-1 hits to learn the Slavic side from itself
    mojibake_filter.py --clean-ref clean.ka --mojibake-ref OpenSubtitles.en-ka.ka \
        --in TED2020.en-ka.ka --out kept.ka
"""

from __future__ import annotations

import argparse
import math
import re
import sys
from collections import Counter
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from typing import Iterable, Iterator

GEORGIAN_TRADITIONAL = "აბგდევზჱთიკლმნჲოპჟრსტჳუფქღყშჩცძწჭხჴჯჰ"
CP1251_CYRILLIC = "абвгдежзийклмнопрстуфхцчшщъыьэюя"
DEMAP = dict(zip(GEORGIAN_TRADITIONAL, CP1251_CYRILLIC))

ARCHAIC = frozenset(chr(c) for c in range(0x10F1, 0x10FB))
GEORGIAN_LETTERS = frozenset(chr(c) for c in range(0x10D0, 0x10FB))
# CP1251 lowercase plus the Serbian/Macedonian letters the demapped text keeps
# where the subtitle file was not CP1251 to begin with.
CYRILLIC_LETTERS = frozenset(chr(c) for c in range(0x430, 0x450)) | frozenset("ѐёђѓєѕіїјљњћќѝўџ")

MIN_LETTERS = 4
# Enough tier-1 hits for the Slavic model to be stable: at 500 the operating point
# already catches 95.5% of the labelled corrupt residue, at 5,000 it reaches its
# 98.5% ceiling and does not move again through 163,112.
MIN_MOJIBAKE_LINES = 500

WHITESPACE = re.compile(r"\s+")


def demap(text: str) -> str:
    return "".join(DEMAP.get(c, c) for c in text)


class TrigramModel:
    """Interpolated character trigram model over one alphabet.

    Everything outside the alphabet collapses to a space, so punctuation, digits
    and Latin runs neither score nor break the context.
    """

    def __init__(self, alphabet: frozenset[str]) -> None:
        self.alphabet = alphabet
        self.unigrams: Counter[str] = Counter()
        self.bigrams: Counter[str] = Counter()
        self.trigrams: Counter[str] = Counter()
        self.chars = 0
        self.vocab = 0

    def _stream(self, text: str) -> str:
        letters = "".join(c if c in self.alphabet else " " for c in text)
        return " " + WHITESPACE.sub(" ", letters).strip() + " "

    def train(self, texts: Iterable[str]) -> None:
        for text in texts:
            stream = self._stream(text)
            if len(stream) < 5:
                continue
            self.unigrams.update(stream)
            self.chars += len(stream)
            for i in range(len(stream) - 1):
                self.bigrams[stream[i:i + 2]] += 1
            for i in range(len(stream) - 2):
                self.trigrams[stream[i:i + 3]] += 1
        self.vocab = len(self.unigrams)

    def logprob(self, text: str) -> float | None:
        """Mean log-probability per scored character, or None when the line holds
        fewer than MIN_LETTERS letters of this alphabet."""
        stream = self._stream(text)
        if sum(1 for c in stream if c in self.alphabet) < MIN_LETTERS:
            return None
        floor = 0.1 * self.vocab
        total = 0.0
        for i in range(2, len(stream)):
            context, char = stream[i - 2:i], stream[i]
            p3 = (self.trigrams[context + char] + 0.1) / (self.bigrams[context] + floor)
            p2 = (self.bigrams[stream[i - 1] + char] + 0.1) / (self.unigrams[stream[i - 1]] + floor)
            p1 = (self.unigrams[char] + 0.1) / (self.chars + floor)
            total += math.log(0.7 * p3 + 0.25 * p2 + 0.05 * p1)
        return total / (len(stream) - 2)


class Verdict(Enum):
    GEORGIAN = "georgian"
    ARCHAIC = "archaic"
    SLAVIC = "slavic"
    UNSCORED = "unscored"


@dataclass(frozen=True)
class Detector:
    georgian: TrigramModel
    slavic: TrigramModel
    margin: float

    def classify(self, text: str) -> Verdict:
        if ARCHAIC & frozenset(text):
            return Verdict.ARCHAIC
        georgian = self.georgian.logprob(text)
        slavic = self.slavic.logprob(demap(text))
        if georgian is None or slavic is None:
            return Verdict.UNSCORED
        if slavic - georgian > self.margin:
            return Verdict.SLAVIC
        return Verdict.GEORGIAN


def train_detector(clean: Iterable[str], mojibake: Iterable[str], margin: float) -> Detector:
    georgian = TrigramModel(GEORGIAN_LETTERS)
    georgian.train(clean)
    slavic = TrigramModel(CYRILLIC_LETTERS)
    slavic.train(demap(line) for line in mojibake)
    return Detector(georgian=georgian, slavic=slavic, margin=margin)


def field_of(line: str, column: int | None) -> str:
    if column is None:
        return line
    fields = line.split("\t")
    if len(fields) < column:
        raise ValueError(f"row has {len(fields)} fields, need column {column}: {line[:120]!r}")
    return fields[column - 1]


def read_lines(path: Path) -> Iterator[str]:
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            yield line.rstrip("\n")


def archaic_lines(path: Path, column: int | None) -> Iterator[str]:
    for line in read_lines(path):
        field = field_of(line, column)
        if ARCHAIC & frozenset(field):
            yield field


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="src", type=Path, required=True,
                    help="corpus to filter; read twice, so it must be a file")
    ap.add_argument("--out", type=Path, default=None, help="kept rows (default stdout)")
    ap.add_argument("--dropped", type=Path, default=None, help="write dropped rows here")
    ap.add_argument("--column", type=int, default=None,
                    help="1-based TSV field holding the Georgian; default classifies the whole line")
    ap.add_argument("--clean-ref", type=Path, action="append", required=True,
                    help="known-clean Georgian text, one sentence per line; repeatable")
    ap.add_argument("--mojibake-ref", type=Path, default=None,
                    help="plain-text corpus to learn the Slavic side from, one Georgian line per "
                         "line; default is the tier-1 hits of --in itself")
    ap.add_argument("--margin", type=float, default=0.5,
                    help="drop when the Slavic model beats the Georgian model by more than this")
    args = ap.parse_args()

    if args.column is not None and args.column < 1:
        ap.error("--column is 1-based")

    mojibake_src = args.mojibake_ref or args.src
    mojibake = list(archaic_lines(mojibake_src, args.column if args.mojibake_ref is None else None))
    if len(mojibake) < MIN_MOJIBAKE_LINES:
        ap.error(f"{mojibake_src} has {len(mojibake)} archaic-bearing lines, need {MIN_MOJIBAKE_LINES} "
                 f"to train the Slavic model; pass --mojibake-ref pointing at a corrupt corpus")

    clean: list[str] = []
    for ref in args.clean_ref:
        clean.extend(read_lines(ref))
    detector = train_detector(clean, mojibake, args.margin)

    keep = args.out.open("w", encoding="utf-8") if args.out else sys.stdout
    dropped = args.dropped.open("w", encoding="utf-8") if args.dropped else None
    counts: Counter[Verdict] = Counter()
    try:
        for line in read_lines(args.src):
            verdict = detector.classify(field_of(line, args.column))
            counts[verdict] += 1
            if verdict in (Verdict.ARCHAIC, Verdict.SLAVIC):
                if dropped is not None:
                    dropped.write(line + "\n")
                continue
            keep.write(line + "\n")
    finally:
        for handle in (keep, dropped):
            if handle is not None and handle is not sys.stdout:
                handle.close()

    total = sum(counts.values())
    if total == 0:
        raise SystemExit(f"{args.src} is empty")
    for verdict in Verdict:
        n = counts[verdict]
        print(f"{verdict.value:9s} {n:>10,}  {100.0 * n / total:6.2f}%", file=sys.stderr)
    kept = counts[Verdict.GEORGIAN] + counts[Verdict.UNSCORED]
    print(f"kept {kept:,} / {total:,} = {100.0 * kept / total:.2f}%", file=sys.stderr)


if __name__ == "__main__":
    main()
