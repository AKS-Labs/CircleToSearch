#!/usr/bin/env python3
"""Per-segment COMET over a probe decode, printed worst-first.

A corpus mean cannot surface a low-rate catastrophic failure: 5 inverted safety
instructions in 102 lines move the system score by almost nothing, which is the
same blind angle the sw->en ce-filter retrain hit ("the TAIL IS UNCHANGED").
So the question is not what the mean says but whether the metric ranks the
KNOWN-BAD lines into its own bottom tail.

    probe_tail.py SRC HYP REF N
"""

import sys

from comet import download_model, load_from_checkpoint


def main() -> None:
    src_p, hyp_p, ref_p, n = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
    src = open(src_p, encoding="utf-8").read().splitlines()
    hyp = open(hyp_p, encoding="utf-8").read().splitlines()
    ref = open(ref_p, encoding="utf-8").read().splitlines()

    model = load_from_checkpoint(download_model("Unbabel/wmt22-comet-da"))
    data = [{"src": s, "mt": h, "ref": r} for s, h, r in zip(src, hyp, ref)]
    scores = model.predict(data, batch_size=32, gpus=0).scores

    ranked = sorted(zip(scores, src, hyp), key=lambda t: t[0])
    for sc, s, h in ranked[:n]:
        print(f"{sc*100:6.2f}  SRC {s}\n        HYP {h}")


if __name__ == "__main__":
    main()
