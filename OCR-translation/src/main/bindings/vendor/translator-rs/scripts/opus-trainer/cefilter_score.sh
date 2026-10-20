#!/bin/bash
# ce-filter scoring (mozilla pipeline/cefilter/score.sh flags): the BACKWARD model
# scores P(original-source | teacher-output), length-normalized. A teacher
# hallucination can't predict its own source back, so sorting ascending and
# dropping the worst tail removes it (cefilter_cut.sh does the cut).
#
# marian-scorer comes from PATH (the marian-cuda image ships it in /usr/local/bin).
#
# Usage: cefilter_score.sh BACKWARD_NPZ VOCAB TEACHER_OUT ORIG_SRC OUT_SCORES [DEVICES=0]
set -euo pipefail

MODEL=$1; VOCAB=$2; TRG=$3; SRC=$4; OUT=$5; DEVICES=${6:-0}
# marian validates the vocab (.spm) and model (.npz/.bin) file extensions; pipe
# materializes inputs under bare names, so re-expose both with extensions.
if [[ "$VOCAB" != *.spm ]]; then ln -sf "$VOCAB" ./vocab.spm && VOCAB=$PWD/vocab.spm; fi
if [[ "$MODEL" != *.npz && "$MODEL" != *.bin ]]; then ln -sf "$MODEL" ./backward.npz && MODEL=$PWD/backward.npz; fi

marian-scorer --model "$MODEL" --vocabs "$VOCAB" "$VOCAB" \
  --train-sets "$TRG" "$SRC" \
  --mini-batch 64 --mini-batch-words 4000 --maxi-batch 1000 \
  --max-length 250 --max-length-crop --normalize \
  --devices $DEVICES --workspace 9000 > "$OUT"
