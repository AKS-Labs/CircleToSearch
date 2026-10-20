#!/usr/bin/env python3
"""Decode a probe file with an OPUS-MT model. CPU; the per-pair models are ~75M.

The OPUS-MT half of the probe eval, mirroring probe_decode.py's interface so both
teachers' hypotheses land in the same shape for side-by-side reading.

    opus_decode.py Helsinki-NLP/opus-mt-en-tl probes/probes_tl.en out.tl
"""

import sys

from opus_gate import translate


def main() -> None:
    model, src, out = sys.argv[1:4]
    lines = [l for l in open(src, encoding="utf-8").read().splitlines() if l.strip()]
    hyps = translate(model, lines, beam=4, batch=16, device="cpu")
    with open(out, "w", encoding="utf-8") as f:
        for h in hyps:
            f.write(h.replace("\n", " ") + "\n")


if __name__ == "__main__":
    main()
