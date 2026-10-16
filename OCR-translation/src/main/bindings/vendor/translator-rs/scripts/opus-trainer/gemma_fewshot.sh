#!/bin/bash
# One-off 5-shot ug->en eval of a CPT/base LM, on a rented GPU box. Reuses the
# nllb-ct2 image (CUDA 12.1 + transformers) and swaps its CPU torch for a GPU
# build at runtime — cheaper than a bespoke image for a single experiment.
#
# Usage: gemma_fewshot.sh MODEL EX_UG EX_EN TEST_UG OUT [NSHOT=5] [LIMIT=100]
set -euo pipefail

MODEL=$1; EX_UG=$2; EX_EN=$3; TEST_UG=$4; OUT=$5; NSHOT=${6:-5}; LIMIT=${7:-100}

pip install --no-cache-dir torch --index-url https://download.pytorch.org/whl/cu121 2>&1 | tail -1
pip install --no-cache-dir --upgrade accelerate transformers 2>&1 | tail -1

python3 /scripts/gemma_fewshot.py "$MODEL" "$EX_UG" "$EX_EN" "$TEST_UG" "$OUT" "$NSHOT" "$LIMIT"
