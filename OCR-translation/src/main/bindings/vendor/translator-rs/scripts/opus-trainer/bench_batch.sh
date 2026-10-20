#!/bin/bash
# pipe step wrapper for bench_batch.py: sweep CT2 max-batch-tokens on one GPU box
# and emit the throughput table. Model + ct2 dir come from the image env.
#
# Usage: bench_batch.sh SRC SRC_LANG TGT_LANG BEAM NBEST OUT_TABLE [BATCHES=csv]
set -euo pipefail

# Baked tokenizer only; never a network fetch (see kd_decode.sh — HF 504s
# hard-fail the online etag check).
export HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1

SRC=$1; SL=$2; TL=$3; BEAM=$4; NBEST=$5; OUT=$6; BATCHES=${7:-}

python3 /scripts/bench_batch.py "$SRC" "$SL" "$TL" "$BEAM" "$NBEST" ${BATCHES:+"$BATCHES"} \
  | tee "$OUT"
