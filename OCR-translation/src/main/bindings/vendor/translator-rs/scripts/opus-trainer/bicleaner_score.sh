#!/bin/bash
# Score (kd_src, kd_ref) pairs with bicleaner-ai — the gate for extract-best:
# below-threshold pairs keep the teacher's rank-1 because their reference may be
# semantically misaligned. Model is baked into the image (HF_HOME=/opt/hf).
# --disable_hardrules: the pool is already heuristically cleaned.
#
# Usage: bicleaner_score.sh KD_SRC KD_REF SRC_LANG REF_LANG OUT_SCORES
set -euo pipefail

SRC=$1; REF=$2; SL=$3; RL=$4; OUT=$5

paste "$SRC" "$REF" > pairs.tsv
bicleaner-ai-classify --scol 1 --tcol 2 -s "$SL" -t "$RL" \
  --score_only --disable_hardrules --mixed_precision \
  pairs.tsv "$OUT" bitextor/bicleaner-ai-full-en-xx
rm pairs.tsv
wc -l "$OUT"
