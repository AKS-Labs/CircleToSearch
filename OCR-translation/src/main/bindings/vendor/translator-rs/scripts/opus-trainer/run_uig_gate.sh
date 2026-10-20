#!/usr/bin/env bash
# One-box Uyghur teacher gate: NLLB-200 (600M+1.3B) vs Hy-MT2 (1.8B+7B), both
# directions, scored on FLORES-200 devtest with the SAME chrF++/spBLEU harness so
# the numbers line up against the existing en->uig 39.7 / uig->en 47.4 (NOTES p2/57).
#
# Run ON the vast box, from a dir holding nllb_gate.py + hy_mt2_gate.py.
# Recommended image: vllm/vllm-openai:latest (vLLM + transformers preinstalled).
#   bash run_uig_gate.sh 2>&1 | tee uig_gate.log
set -euo pipefail

PAIRS="eng_Latn-uig_Arab,uig_Arab-eng_Latn"
LIMIT="${LIMIT:-300}"
PY="${PY:-python3}"

$PY -m pip install -q sacrebleu sacremoses

echo "############ NLLB-200-distilled-600M ############"
$PY nllb_gate.py --pairs "$PAIRS" --model facebook/nllb-200-distilled-600M \
    --device cuda --limit "$LIMIT" --beam 4 --batch 16

echo "############ NLLB-200-distilled-1.3B ############"
$PY nllb_gate.py --pairs "$PAIRS" --model facebook/nllb-200-distilled-1.3B \
    --device cuda --limit "$LIMIT" --beam 4 --batch 8

echo "############ Hy-MT2-1.8B (greedy) ############"
$PY hy_mt2_gate.py --pairs "$PAIRS" --model tencent/Hy-MT2-1.8B --limit "$LIMIT"

echo "############ Hy-MT2-7B (greedy) ############"
$PY hy_mt2_gate.py --pairs "$PAIRS" --model tencent/Hy-MT2-7B --limit "$LIMIT"

echo "DONE — compare chrF++ per direction against the NLLB baseline; destroy the box."
