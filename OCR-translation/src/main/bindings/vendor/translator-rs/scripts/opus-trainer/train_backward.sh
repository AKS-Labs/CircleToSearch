#!/bin/bash
# Train the mozilla-style backward model (shallow s2s RNN) on HUMAN bitext for a
# pair. Direction = REVERSE of the student being filtered: to ce-filter X->en KD
# data, train en->X here. Unlike the student there is no OpusTrainer, no guided
# alignment, no KD data — human pairs only (curated + bicleaner-salvaged), which
# is what makes it an independent judge of teacher output.
#
# Score KD data afterwards with marian-scorer (mozilla pipeline/cefilter/score.sh
# flags): --train-sets <teacher-output> <original-source> --normalize, model =
# this backward npz; sort scores ascending, drop the worst 5%.
#
# Usage: train_backward.sh TRAIN_SRC TRAIN_TRG VOCAB_SPM VALID_SRC VALID_TRG MODEL_OUT_NPZ [DEVICES=0]
set -euo pipefail

SRC=$1; TRG=$2; VOCAB=$3; VSRC=$4; VTRG=$5; OUT=$6; DEVICES=${7:-0}
HERE="$(cd "$(dirname "$0")" && pwd)"
MARIAN=${MARIAN:-/root/marian-dev/build/marian}
mkdir -p "$(dirname "$OUT")"

"$MARIAN" \
  -c "$HERE/configs/backward.s2s.yml" \
  --train-sets "$SRC" "$TRG" \
  --vocabs "$VOCAB" "$VOCAB" --dim-vocabs 32000 32000 \
  --model "$OUT" \
  --valid-sets "$VSRC" "$VTRG" \
  --devices $DEVICES --workspace 9000 \
  --keep-best --overwrite

echo "done: ${OUT%.npz}.best-ce-mean-words.npz"
