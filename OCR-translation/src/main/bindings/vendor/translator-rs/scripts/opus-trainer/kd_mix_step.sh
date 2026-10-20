#!/bin/bash
# pipe step wrapper for sample_mix.py. PURE ARGV ADAPTER — it maps pipe's
# positional step args onto the script's flags and does NOTHING ELSE, for the
# same reason prep_step.sh does: a wrapper that also does work makes a partial
# form of the step runnable, and a partial step is a step you can do wrong.
#
# MIX is passed through verbatim rather than defaulted here. The mix decides what
# registers the model will be able to translate at all, so it belongs in the flow
# where it is reviewable next to the rest of the graph, not buried in a ${5:-...}.
#
# All five pools are positional and required. sample_mix.py refuses a partial set
# rather than treating a missing register as empty.
#
# Usage: kd_mix_step.sh OUT_DIR KD_COL TOTAL MIX SEED HUMAN UI DIALOGUE ENTITY CRAWL
set -euo pipefail

OUT=$1; KD_COL=$2; TOTAL=$3; MIX=$4; SEED=$5
HUMAN=$6; UI=$7; DIALOGUE=$8; ENTITY=$9; CRAWL=${10}

python3 /scripts/sample_mix.py \
  --out "$OUT" --kd-col "$KD_COL" --total "$TOTAL" --mix "$MIX" --seed "$SEED" \
  --pool "human=$HUMAN" --pool "ui=$UI" --pool "dialogue=$DIALOGUE" \
  --pool "entity=$ENTITY" --pool "crawl=$CRAWL"
