#!/bin/bash
# extract-best over the gathered n-best lists: reference-anchored, bicleaner-gated
# target selection (see extract_best.py). CPU-only; runs in the nllb-ct2 image
# because that's where python3 + sacrebleu already live.
#
# Usage: select_best.sh NBEST_GZ REF GATES OUT_SEL [JOBS=16]
set -euo pipefail

NBEST=$1; REF=$2; GATES=$3; OUT=$4; JOBS=${5:-16}

# pipe materializes inputs under bare names; extract_best.py picks gzip by
# extension, so re-expose the gathered n-best with its extension.
ln -sf "$NBEST" ./nbest.tsv.gz

python3 /scripts/extract_best.py \
  --nbest ./nbest.tsv.gz --ref "$REF" \
  --gate-scores "$GATES" --gate-threshold 0.5 \
  --out "$OUT" --jobs "$JOBS"
