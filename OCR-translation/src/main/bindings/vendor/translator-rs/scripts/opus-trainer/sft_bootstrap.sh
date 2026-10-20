#!/bin/bash
set -uo pipefail
cd /root/work
echo "=== GPU ==="; nvidia-smi --query-gpu=name,memory.total --format=csv,noheader
echo "=== install (this takes a few min) ==="
pip install -q -U unsloth unsloth_zoo sacrebleu 2>&1 | tail -3
echo "=== train ==="
python train_sft.py 2>&1 | tail -60
echo "=== eval ==="
python eval_sft.py 2>&1
echo "ALL_DONE"
