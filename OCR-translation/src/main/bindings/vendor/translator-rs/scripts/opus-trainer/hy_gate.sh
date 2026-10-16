#!/bin/bash
# pipe step wrapper for hy_mt2_gate.py on the hy-kd image (vLLM already installed;
# only the metrics package is missing). FLORES_CACHE is redirected off /scripts,
# which pipe mounts read-only.
#
# Decodes only. COMET is scored off-box by chrf_score.py from the src/hyp/ref this
# writes, so unbabel-comet never gets pip-installed into the vLLM environment.
#
# Usage: hy_gate.sh PAIRS MODEL LIMIT OUT_DIR SCORES
set -euo pipefail

PAIRS=$1; MODEL=$2; LIMIT=$3; OUT_DIR=$4; SCORES=$5

pip install --no-cache-dir -q sacrebleu

export FLORES_CACHE=/tmp/flores
export PYTHONPATH=/scripts
python3 /scripts/hy_mt2_gate.py --pairs "$PAIRS" --model "$MODEL" --limit "$LIMIT" \
    --out-dir "$OUT_DIR" | tee "$SCORES"
