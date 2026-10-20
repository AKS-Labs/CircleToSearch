#!/bin/bash
# pipe step wrapper for eval_pair.py. PURE ARGV ADAPTER — no logic.
#
# It used to invoke eval_pair.py AND probe_review.py, which made
# "metrics without the review" a runnable partial of the step. eval_pair.py now
# emits both, so the whole step is the only thing that runs.
#
# Usage: eval_score.sh F_HYP F_REF F_SRC C_HYP C_SRC C_REF OUT_METRICS OUT_REVIEW
set -euo pipefail

python3 /scripts/eval_pair.py \
    --flores-hyp "$1" --flores-ref "$2" --flores-src "$3" \
    --check-hyp "$4" --check-src "$5" --check-ref "$6" \
    --out "$7" --review-out "$8"
