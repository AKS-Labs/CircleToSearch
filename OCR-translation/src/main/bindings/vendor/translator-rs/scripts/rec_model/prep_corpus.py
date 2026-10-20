"""Build a clean training corpus from Leipzig Corpora Collection sentence files.

Script-agnostic: downloads the named Leipzig corpora, strips the leading `id\\t`,
NFC-normalizes, maps typographic punctuation to ASCII, drops lines shorter than 2
words or containing any character outside the target charset (imported from a
generator module), dedups, and writes one sentence per line.

  # Indic (merged): Bengali/Gujarati/Kannada/Malayalam wiki + Bengali news
  python prep_corpus.py --charset-from gen_indic --download --out data/indic_corpus.txt \
      --names ben_wikipedia_2021_100K,guj_wikipedia_2021_100K,kan_wikipedia_2021_100K,mal_wikipedia_2021_100K,ben_newscrawl_2017_100K

  # Hebrew
  python prep_corpus.py --charset-from gen_hebrew --download --out data/hebrew_corpus.txt \
      --names heb_wikipedia_2021_100K,heb_news_2020_100K --strip-marks 0591-05C7
"""

import argparse
import importlib
import os
import re
import tarfile
import unicodedata
import urllib.request
from glob import glob

BASE = "https://downloads.wortschatz-leipzig.de/corpora/"
TRANSLATE = str.maketrans({
    "“": '"', "”": '"', "„": '"', "″": '"', "‘": "'", "’": "'", "′": "'",
    "–": "-", "—": "-", "−": "-", " ": " ", "‎": "", "‏": "", "…": "...",
})


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", required=True)
    ap.add_argument("--names", required=True, help="comma-separated Leipzig corpus names")
    ap.add_argument("--charset-from", required=True, help="module exposing candidate_charset() (gen_indic / gen_hebrew)")
    ap.add_argument("--work-dir", default="/tmp/leipzig")
    ap.add_argument("--download", action="store_true")
    ap.add_argument("--strip-marks", default="", help="hex codepoint range to delete, e.g. 0591-05C7 (Hebrew niqqud)")
    args = ap.parse_args()

    allowed = set(importlib.import_module(args.charset_from).candidate_charset())
    marks = None
    if args.strip_marks:
        lo, hi = (int(x, 16) for x in args.strip_marks.split("-"))
        marks = re.compile(f"[{chr(lo)}-{chr(hi)}]")

    def clean(line):
        s = unicodedata.normalize("NFC", line.split("\t", 1)[-1].strip())
        s = s.replace("‌", "").replace("‍", "")  # drop ZWNJ/ZWJ (invisible)
        if marks:
            s = marks.sub("", s)
        s = re.sub(r"\s+", " ", s.translate(TRANSLATE)).strip().strip('"').strip()
        if len(s.split()) < 2 or any(c not in allowed for c in s):
            return None
        return s

    os.makedirs(args.work_dir, exist_ok=True)
    for name in args.names.split(","):
        tar = os.path.join(args.work_dir, name + ".tar.gz")
        if args.download and not os.path.exists(tar):
            print(f"downloading {name} ...", flush=True)
            urllib.request.urlretrieve(BASE + name + ".tar.gz", tar)
        if os.path.exists(tar):
            with tarfile.open(tar) as t:
                t.extractall(args.work_dir)

    seen, out = set(), []
    for fp in sorted(glob(os.path.join(args.work_dir, "**", "*-sentences.txt"), recursive=True)):
        if not any(n in fp for n in args.names.split(",")):
            continue
        for line in open(fp, encoding="utf-8"):
            s = clean(line)
            if s and s not in seen:
                seen.add(s)
                out.append(s)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(out) + "\n")
    print(f"wrote {len(out)} lines -> {args.out}")


if __name__ == "__main__":
    main()
