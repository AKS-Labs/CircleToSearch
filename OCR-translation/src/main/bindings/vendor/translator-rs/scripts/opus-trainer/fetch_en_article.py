#!/usr/bin/env python3
"""Stream English Wikipedia, sentence-split, filter to clean article-domain
sentences, write a JSON list for gen_sft.py --src. Not committed; working data."""
import json
import pathlib
import random
import re
import sys

from datasets import load_dataset

WANT = int(sys.argv[1]) if len(sys.argv) > 1 else 2600
OUT = pathlib.Path(sys.argv[2]) if len(sys.argv) > 2 else pathlib.Path("sft/en_article.json")
OUT.parent.mkdir(parents=True, exist_ok=True)

SENT = re.compile(r"(?<=[.!?])\s+(?=[A-Z0-9])")
BAD = re.compile(r"[|{}\[\]<>#=]|\\n|http|\.mw-|thumb|\|")


def ok(s: str) -> bool:
    if not (40 <= len(s) <= 220):
        return False
    if not s[0].isupper() or s[-1] not in ".!?":
        return False
    if BAD.search(s):
        return False
    letters = sum(c.isalpha() for c in s)
    if letters / len(s) < 0.6:
        return False
    if sum(c.isdigit() for c in s) / len(s) > 0.2:
        return False
    words = s.split()
    if not (6 <= len(words) <= 40):
        return False
    return True


ds = load_dataset("wikimedia/wikipedia", "20231101.en", split="train", streaming=True)
seen: set[str] = set()
pool: list[str] = []
for i, row in enumerate(ds):
    for para in row["text"].split("\n"):
        for s in SENT.split(para.strip()):
            s = s.strip()
            if ok(s) and s not in seen:
                seen.add(s)
                pool.append(s)
    if len(pool) >= WANT * 3 or i > 4000:
        break
    if i % 200 == 0:
        print(f"  {i} articles, {len(pool)} candidate sentences", flush=True)

random.seed(13)
random.shuffle(pool)
picked = pool[:WANT]
json.dump(picked, open(OUT, "w"), ensure_ascii=False, indent=0)
print(f"wrote {len(picked)} sentences -> {OUT}", flush=True)
for s in picked[:3]:
    print("  e.g.", s)
