#!/usr/bin/env python3
"""Per-segment COMET for named probe lines, with their rank in the corpus.

Answers "does the metric put the KNOWN-BAD line in its bottom tail?" — which is
the only question that matters if COMET is to be used as a screen rather than a
gate.

    probe_lookup.py SRC HYP REF <substring> [<substring> ...]
"""

import sys

from comet import download_model, load_from_checkpoint


def main() -> None:
    src_p, hyp_p, ref_p = sys.argv[1:4]
    needles = sys.argv[4:]
    src = open(src_p, encoding="utf-8").read().splitlines()
    hyp = open(hyp_p, encoding="utf-8").read().splitlines()
    ref = open(ref_p, encoding="utf-8").read().splitlines()

    model = load_from_checkpoint(download_model("Unbabel/wmt22-comet-da"))
    data = [{"src": s, "mt": h, "ref": r} for s, h, r in zip(src, hyp, ref)]
    scores = model.predict(data, batch_size=32, gpus=0).scores

    order = sorted(range(len(scores)), key=lambda i: scores[i])
    rank = {i: r + 1 for r, i in enumerate(order)}
    for needle in needles:
        for i, s in enumerate(src):
            if needle.lower() in s.lower():
                print(f"rank {rank[i]:3d}/{len(src)}  COMET {scores[i]*100:6.2f}  "
                      f"SRC {s}\n                          HYP {hyp[i]}")
                break


if __name__ == "__main__":
    main()
