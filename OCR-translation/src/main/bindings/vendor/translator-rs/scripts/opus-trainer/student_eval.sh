#!/bin/bash
# pipe step wrapper for benchmark_slimt.py. PURE ARGV ADAPTER — no logic.
#
# lang/src arrive as ARGS, not env: the job protocol passes an args file and a
# script path, never environment. benchmark_slimt.py emits metrics, probe
# hypotheses AND the review itself, so no partial of this step is runnable.
#
# Usage: student_eval.sh LANG SRC MODEL VOCAB SHORTLIST C_SRC C_REF OUT_HYP OUT_METRICS OUT_REVIEW
set -euo pipefail

python3 /scripts/benchmark_slimt.py --lang "$1" --src "$2" \
    --bin /usr/local/bin/slimt_load_test \
    --model "$3" --vocab "$4" --shortlist "$5" \
    --probes "$6" --probe-out "$8" --review-out "${10}" | tee "$9"
