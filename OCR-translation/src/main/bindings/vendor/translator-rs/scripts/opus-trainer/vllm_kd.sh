#!/bin/bash
# pipe step wrapper for vllm_kd.py. PURE ARGV ADAPTER — no logic.
# Usage: vllm_kd.sh SRC OUT TARGET_LANGUAGE [LIMIT=0]
set -euo pipefail
[ -d /work/out ] && echo decode > /work/out/.phase || true
python3 /scripts/vllm_kd.py "$1" "$2" "$3" "${4:-0}"
