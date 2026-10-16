#!/usr/bin/env python3
"""Format {src,tgt} pair files into Gemma chat-template SFT JSONL (messages
format), one direction. Loss-masking to the response is handled at train time.

  format_sft.py --pairs sft/article_en2ug/pairs.jsonl \
      --instr "Translate English to Uyghur" --out sft/train --valid 200
"""
import argparse
import json
import pathlib
import random


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--pairs", nargs="+", required=True, help="one or more pairs.jsonl files")
    ap.add_argument("--instr", required=True, help='e.g. "Translate English to Uyghur"')
    ap.add_argument("--out", required=True, help="output prefix (writes <out>.train.jsonl / <out>.valid.jsonl)")
    ap.add_argument("--valid", type=int, default=200)
    a = ap.parse_args()

    rows = []
    for p in a.pairs:
        for line in open(p, encoding="utf-8"):
            r = json.loads(line)
            rows.append({
                "messages": [
                    {"role": "user", "content": f"{a.instr}:\n{r['src']}"},
                    {"role": "assistant", "content": r["tgt"]},
                ]
            })
    random.seed(13)
    random.shuffle(rows)
    valid, train = rows[:a.valid], rows[a.valid:]
    out = pathlib.Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    for name, data in (("train", train), ("valid", valid)):
        with open(f"{a.out}.{name}.jsonl", "w") as f:
            for r in data:
                f.write(json.dumps(r, ensure_ascii=False) + "\n")
    print(f"train {len(train)}  valid {len(valid)}  -> {a.out}.train.jsonl / .valid.jsonl")


if __name__ == "__main__":
    main()
