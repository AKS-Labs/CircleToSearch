#!/bin/bash
# fp32 beam-6 eval decode — the exact settings behind the run-log FLORES numbers,
# so pipe-produced hyps stay comparable to the hand-run ones.
#
# Usage: decode_flores.sh MODEL_NPZ VOCAB SRC_TXT OUT_HYP [DEVICES=0]
set -euo pipefail

MODEL=$1; VOCAB=$2; SRC=$3; OUT=$4; DEVICES=${5:-0}
if [[ "$VOCAB" != *.spm ]]; then ln -sf "$VOCAB" ./vocab.spm && VOCAB=$PWD/vocab.spm; fi
if [[ "$MODEL" != *.npz && "$MODEL" != *.bin ]]; then ln -sf "$MODEL" ./model.npz && MODEL=$PWD/model.npz; fi

marian-decoder --models "$MODEL" --vocabs "$VOCAB" "$VOCAB" --input "$SRC" \
  --beam-size 6 --normalize 1 --mini-batch 32 --maxi-batch 100 --maxi-batch-sort src \
  --devices $DEVICES --workspace 4000 --quiet-translation > "$OUT"
