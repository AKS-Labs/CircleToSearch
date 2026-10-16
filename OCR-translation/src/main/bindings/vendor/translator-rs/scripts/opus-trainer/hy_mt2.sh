#!/bin/bash
# One-off ug->en eval of an instruction LLM (Hy-MT2) on a rented GPU box, reusing
# the nllb-ct2 image with a runtime GPU-torch swap. The uninstall is load-bearing:
# `pip install torch` no-ops against the image's baked CPU torch, so it must be
# removed first or generation dies with "Torch not compiled with CUDA enabled".
#
# Usage: hy_mt2.sh MODEL TEST_UG OUT [LIMIT=100]
set -euo pipefail

MODEL=$1; TEST_UG=$2; OUT=$3; LIMIT=${4:-100}

pip uninstall -y torch 2>&1 | tail -1
pip install --no-cache-dir torch --index-url https://download.pytorch.org/whl/cu121 2>&1 | tail -1
pip install --no-cache-dir --upgrade accelerate transformers 2>&1 | tail -1

python3 /scripts/hy_mt2_run.py "$MODEL" "$TEST_UG" "$OUT" "$LIMIT"
