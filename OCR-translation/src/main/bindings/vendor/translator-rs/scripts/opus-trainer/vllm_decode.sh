#!/bin/bash
# pipe step wrapper for vllm_decode.py (baked at /opt in the hy-kd image).
# Prints the decode's THROUGHPUT line, so this doubles as the throughput bench.
#
# Usage: vllm_decode.sh SRC OUT [LIMIT=0]
set -euo pipefail
[ -d /work/out ] && echo decode > /work/out/.phase || true

SRC=$1; OUT=$2; LIMIT=${3:-0}

python3 /opt/vllm_decode.py "$SRC" "$OUT" "$LIMIT"
