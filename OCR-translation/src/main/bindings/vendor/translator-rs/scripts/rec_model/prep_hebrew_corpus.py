"""Build a clean unpointed modern-Hebrew corpus for the recognizer generator.

Source: Leipzig Corpora Collection (wikipedia + news), one sentence per line,
already sentence-segmented. We strip the leading `id\\t`, remove niqqud and
cantillation (real-world camera text is unpointed, and the renderer can't place
combining marks), normalize typographic punctuation to the generator's charset,
and drop lines with foreign characters. Output is one sentence per line for
gen_hebrew.py --corpus.

  python prep_hebrew_corpus.py --download --out data/hebrew_corpus.txt
  python prep_hebrew_corpus.py --leipzig-dir /tmp/lz --out data/hebrew_corpus.txt
"""

import argparse
import os
import re
import sys
import tarfile
import unicodedata
import urllib.request
from glob import glob

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_hebrew import candidate_charset

LEIPZIG = {
    "heb_wikipedia_2021_100K": "https://downloads.wortschatz-leipzig.de/corpora/heb_wikipedia_2021_100K.tar.gz",
    "heb_news_2020_100K": "https://downloads.wortschatz-leipzig.de/corpora/heb_news_2020_100K.tar.gz",
}

# Hebrew accents (U+0591-05AF) and points/niqqud (U+05B0-05BD, 05BF, 05C1-05C2,
# 05C4-05C5, 05C7); keep maqaf 05BE, geresh/gershayim 05F3/05F4 (they are in CHARSET).
NIQQUD = re.compile(r"[֑-ׇֽֿׁׂׅׄ׀׃׆]")
TRANSLATE = str.maketrans({
    "“": '"', "”": '"', "„": '"', "″": '"',
    "‘": "'", "’": "'", "′": "'", "׳": "'", "״": '"',
    "–": "-", "—": "-", "−": "-",
    " ": " ", "‎": "", "‏": "", "…": "...",
})
ALLOWED = set(candidate_charset())
HEBREW = re.compile(r"[א-ת]")


def clean(line: str) -> str | None:
    s = line.split("\t", 1)[-1].strip()
    s = unicodedata.normalize("NFC", s)
    s = NIQQUD.sub("", s).translate(TRANSLATE)
    s = re.sub(r"\s+", " ", s).strip().strip('"').strip()
    if len(s.split()) < 2 or not HEBREW.search(s):
        return None
    # Drop the whole line if it carries any character outside the generator's
    # charset, so spans sampled from it never produce out-of-dict labels.
    if any(c not in ALLOWED for c in s):
        return None
    return s


def download(work: str) -> str:
    os.makedirs(work, exist_ok=True)
    for name, url in LEIPZIG.items():
        tar = os.path.join(work, name + ".tar.gz")
        if not os.path.exists(tar):
            print(f"downloading {name} ...")
            urllib.request.urlretrieve(url, tar)
        with tarfile.open(tar) as t:
            t.extractall(work)
    return work


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--out", required=True)
    ap.add_argument("--leipzig-dir", default="/tmp/lz")
    ap.add_argument("--download", action="store_true")
    args = ap.parse_args()

    src = download(args.leipzig_dir) if args.download else args.leipzig_dir
    files = glob(os.path.join(src, "**", "*-sentences.txt"), recursive=True)
    if not files:
        raise SystemExit(f"no *-sentences.txt under {src} (use --download?)")

    seen: set[str] = set()
    out_lines = []
    for fp in files:
        with open(fp, encoding="utf-8") as f:
            for line in f:
                s = clean(line)
                if s and s not in seen:
                    seen.add(s)
                    out_lines.append(s)
    os.makedirs(os.path.dirname(os.path.abspath(args.out)), exist_ok=True)
    with open(args.out, "w", encoding="utf-8") as f:
        f.write("\n".join(out_lines) + "\n")
    print(f"wrote {len(out_lines)} lines from {len(files)} files -> {args.out}")


if __name__ == "__main__":
    main()
