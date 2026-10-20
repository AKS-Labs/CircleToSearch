#!/usr/bin/env bash
# MiLiC-Eval en<->ug gate: NLLB-600M/1.3B vs pkupie Uyghur-CPT base (few-shot).
# Run ON the box.  bash run_milic_gate.sh 2>&1 | tee milic.log
set -uo pipefail
pip install -q sacrebleu sacremoses 2>&1 | tail -1

echo "############ NLLB-200-distilled-600M ############"
python3 milic_gate.py --backend nllb --model facebook/nllb-200-distilled-600M --device cuda

echo "############ NLLB-200-distilled-1.3B ############"
python3 milic_gate.py --backend nllb --model facebook/nllb-200-distilled-1.3B --device cuda

echo "############ pkupie/gemma-3-4b-ug-cpt (5-shot) ############"
python3 milic_gate.py --backend fewshot --model pkupie/gemma-3-4b-ug-cpt --nshot 5

echo "=== DONE ==="
