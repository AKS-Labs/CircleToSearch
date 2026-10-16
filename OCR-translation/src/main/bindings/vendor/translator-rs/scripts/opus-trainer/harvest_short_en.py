#!/usr/bin/env python3
"""Harvest short English lines from OPUS corpora, banded by length.

The camera path is short text, and a student trained on mined crawl barely sees
any: the en-ka pool is 96% crawl, and `registers.py` records what that costs --
the shipped tl student had met `Right`, `Pull` and `Free` zero times.

Generating short English was the obvious fix and is mostly the wrong one.
Measured on OpenSubtitles en-ka, 206,943 unique English lines: 62,458 at 2-4
words and 83,400 at 5-8, so subtitles supply the short CONVERSATIONAL bands by
the tens of thousands, already real rather than a model's idea of how people
talk. What subtitles do NOT contain is signage -- no film says "Emergency Exit"
-- and that is the only part worth generating (gen_short_en.py).

The English side is deliberately language-independent, so one harvest feeds
every pair's finetune set and every pair is then scored on the same source.

    harvest_short_en.py --zip OpenSubtitles.enka.zip --out out/harvest \
        --exclude data/eval_exclude.sha256
"""

import argparse
import hashlib
import html
import re
import zipfile
from collections import Counter
from pathlib import Path

# (name, low, high) on whitespace token count, high inclusive.
BANDS = (("w01", 1, 1), ("w02_04", 2, 4), ("w05_08", 5, 8),
         ("w09_15", 9, 15), ("w16_25", 16, 25))

TAG = re.compile(r"</?[a-zA-Z][^>]*>")
# Subtitles open a speaker turn with a dash, and OPUS moses detaches punctuation.
LEAD = re.compile(r"^[-–—\s]+")
MULTISPACE = re.compile(r"\s+")
LETTER = re.compile(r"[A-Za-z]")
# A line that is mostly not Latin letters is a caption artifact, a timestamp, or
# untranslated foreign text sitting in the English column.
NONLATIN = re.compile(r"[^\x00-\x7F]")


def clean(raw: str) -> str:
    s = html.unescape(raw).strip()
    s = TAG.sub("", s)
    s = LEAD.sub("", s)
    s = MULTISPACE.sub(" ", s).strip()
    return s


def acceptable(s: str) -> bool:
    if not s or not LETTER.search(s):
        return False
    if len(NONLATIN.findall(s)) > len(s) * 0.05:
        return False
    letters = sum(c.isalpha() for c in s)
    if letters < len(s) * 0.5:
        return False
    return True


def band_of(s: str) -> str | None:
    n = len(s.split())
    for name, lo, hi in BANDS:
        if lo <= n <= hi:
            return name
    return None


def english_side(path: Path) -> list[str]:
    """Lines of the .en member of an OPUS moses zip, or of a plain text file."""
    if path.suffix != ".zip":
        return path.read_text(encoding="utf-8", errors="replace").splitlines()
    with zipfile.ZipFile(path) as z:
        members = [n for n in z.namelist() if n.endswith(".en")]
        if len(members) != 1:
            raise SystemExit(f"{path}: expected one .en member, found {members}")
        with z.open(members[0]) as f:
            return [l.decode("utf-8", "replace") for l in f]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--zip", required=True, action="append", type=Path,
                    help="OPUS moses zip (or plain .en file); repeatable")
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--exclude", type=Path,
                    help="sha256-per-line file of held-out eval English; those lines "
                         "must not enter a finetune set or the eval is contaminated")
    ap.add_argument("--cap", type=int, default=0, help="max lines per band (0 = all)")
    args = ap.parse_args()

    excluded = set()
    if args.exclude is not None:
        excluded = set(args.exclude.read_text(encoding="utf-8").split())

    stats = Counter()
    kept: dict[str, dict[str, str]] = {name: {} for name, _, _ in BANDS}
    for path in args.zip:
        for raw in english_side(path):
            stats["raw"] += 1
            s = clean(raw)
            if not acceptable(s):
                stats["rejected"] += 1
                continue
            band = band_of(s)
            if band is None:
                stats["out_of_band"] += 1
                continue
            if hashlib.sha256(s.encode("utf-8")).hexdigest() in excluded:
                stats["eval_held_out"] += 1
                continue
            # Case-insensitive key: subtitle corpora carry the same line in several
            # capitalisations and they are one line for our purposes.
            if kept[band].setdefault(s.lower(), s) is not s:
                stats["duplicate"] += 1

    args.out.mkdir(parents=True, exist_ok=True)
    print(f"raw {stats['raw']}  rejected {stats['rejected']}  out-of-band {stats['out_of_band']}  "
          f"eval-held-out {stats['eval_held_out']}  duplicate {stats['duplicate']}")
    for name, lo, hi in BANDS:
        lines = list(kept[name].values())
        if args.cap:
            lines = lines[: args.cap]
        (args.out / f"{name}.en").write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"  {name} ({lo}-{hi} words): {len(lines)}")


if __name__ == "__main__":
    main()
