#!/usr/bin/env python3
"""Split multi-sentence bitext into one-sentence pairs, matching the runtime.

The app sentence-splits before translating, so the model only ever sees one
sentence at inference. About 10% of mined and generated bitext is
multi-sentence, which spends capacity on a case that never occurs and skews the
length prior. Splitting the corpus removes that mismatch, and where both sides
agree on the count it yields MORE pairs than it consumed.

NONBREAKING_PREFIXES mirrors the Rust runtime's list deliberately: a boundary in
training has to be a boundary in production, so the two must not drift. A naive
splitter without it reports ~56% of multi-sentence pairs as mis-aligned, all of
it manufactured by breaking on "Mr." and "Ms." and blaming the target side.

Pairs whose two sides disagree on sentence count are DROPPED, not guessed at:
a 3-into-2 alignment is not recoverable without a model, and the pairs are a
low-single-digit fraction.

    split_sentences.py --in pairs.tsv --out split.tsv --report
"""

import argparse
import re
from pathlib import Path

# Mirrors NONBREAKING_PREFIXES in the Rust runtime. Keep in sync.
NONBREAKING = [
    "Mr", "Mrs", "Ms", "Dr", "Drs", "Prof", "Rev", "Hon", "St", "Ste", "Fr",
    "Pres", "Gov", "Sen", "Rep", "Gen", "Col", "Maj", "Capt", "Cmdr", "Lt",
    "Sgt", "Cpl", "Pvt", "Adm", "Messrs", "Jr", "Sr", "Bros",
    "vs", "v", "Mt", "Ft", "Ave", "Blvd", "Rd", "Dept", "Univ", "Inc", "Ltd",
    "Corp", "Co", "Est", "Ph.D", "M.D", "B.A", "M.A", "D.C", "e.g", "i.e",
    "Sra", "Srta", "Ud", "Uds", "Vd", "Vds", "Dña", "Excmo", "Ilmo", "Avda",
    "MM", "Mme", "Mmes", "Mlle", "Mlles", "Me", "Mgr",
    "Hr", "Frau", "Nr", "Str", "z.B", "bzw", "usw", "ca",
    "ul", "al", "prof", "dr", "mgr", "inż", "hab", "im", "św", "ks", "płk",
    "ე.წ", "ელ", "მაგ", "იხ", "წმ", "დაახლ", "თ.წ", "ე.ი",
    "ძვ", "მდ", "გვ", "სთ",
]
NONBREAKING_SET = {p.rstrip(".").lower() for p in NONBREAKING}

# Terminal punctuation, an optional closing quote, whitespace, then something
# that can open a sentence: a capital, a Georgian letter, or an opening quote.
CANDIDATE = re.compile(r'[.!?\u2026।॥։۔؟።፧]["”\']?\s+(?=[A-Zა-ჿᲐ-Ჿ"“„])')
LAST_WORD = re.compile(r'(\S+)$')


def _is_boundary(text: str, dot: int) -> bool:
    """Is the punctuation at `dot` a real sentence end?

    Only `.` is ambiguous -- no abbreviation ends in `!` or `?` -- so those
    always break. For a period, the deciding thing is the token in front of it,
    which is why this is a lookup and not a lookbehind: the split position sits
    AFTER the period, where a regex lookbehind sees "s." rather than "Ms".
    """
    if text[dot] == "\u2026":
        return False
    if text[dot] != ".":
        return True
    # An ellipsis is not a sentence end, and its LAST dot is what the candidate
    # regex lands on. Subtitles are full of "he said... and left", and Georgian
    # opens the continuation with a letter this pattern reads as a capital, so
    # without this the two sides split differently and the pair is discarded.
    if text[:dot].endswith(".") or text[:dot].endswith("\u2026"):
        return False
    m = LAST_WORD.search(text[:dot])
    if m is None:
        return True
    word = m.group(1).lstrip("(\"'“„")
    # A lone letter before a period is an initial. Matching only [A-Z] misses it
    # for every caseless script, splitting a Georgian name at its initial.
    return word.lower() not in NONBREAKING_SET and not re.fullmatch(r"[A-Zა-ჿᲐ-Ჿ]", word)


def split(text: str) -> list[str]:
    text = text.strip()
    out, start = [], 0
    for m in CANDIDATE.finditer(text):
        if not _is_boundary(text, m.start()):
            continue
        piece = text[start:m.end()].strip()
        if piece:
            out.append(piece)
        start = m.end()
    tail = text[start:].strip()
    if tail:
        out.append(tail)
    return out or [text]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--in", dest="src", required=True, type=Path, help="2-col TSV")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--dropped", type=Path, help="write mismatched pairs here for inspection")
    ap.add_argument("--report", action="store_true")
    args = ap.parse_args()

    stats = {"single": 0, "split": 0, "gained": 0, "mismatch": 0, "malformed": 0}
    kept = 0
    drop_f = args.dropped.open("w", encoding="utf-8") if args.dropped is not None else None
    with args.src.open(encoding="utf-8") as fin, args.out.open("w", encoding="utf-8") as fout:
        for line in fin:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 2 or not parts[0].strip() or not parts[1].strip():
                stats["malformed"] += 1
                continue
            s, t = split(parts[0]), split(parts[1])
            if len(s) == len(t) == 1:
                stats["single"] += 1
                fout.write(f"{parts[0].strip()}\t{parts[1].strip()}\n")
                kept += 1
            elif len(s) == len(t):
                stats["split"] += 1
                stats["gained"] += len(s) - 1
                for a, b in zip(s, t):
                    fout.write(f"{a}\t{b}\n")
                kept += len(s)
            else:
                stats["mismatch"] += 1
                if drop_f is not None:
                    drop_f.write(f"{parts[0]}\t{parts[1]}\n")
    if drop_f is not None:
        drop_f.close()

    if args.report:
        n = sum(stats[k] for k in ("single", "split", "mismatch", "malformed"))
        print(f"in {n} pairs -> out {kept} pairs ({kept - n:+d})")
        for k in ("single", "split", "gained", "mismatch", "malformed"):
            print(f"  {k:10s} {stats[k]:>8}")


if __name__ == "__main__":
    main()
