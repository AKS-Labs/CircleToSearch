#!/bin/bash
# pipe step wrapper for train_backward.sh: splits the student's 2-col valid into
# the column files marian wants, then trains the backward model.
#
# The backward model runs the REVERSE of the student (student xx->en, backward
# en->xx), so the student's `src \t trg` valid is consumed with its columns
# swapped: the student's target side is the backward model's source.
#
# marian comes from PATH (the marian-cuda image ships it in /usr/local/bin).
#
# Usage: backward_step.sh TRAIN_SRC TRAIN_TRG VOCAB VALID_TSV OUT_NPZ [DEVICES=0]
set -euo pipefail
[ -d /work/out ] && echo train > /work/out/.phase || true

TSRC=$1; TTRG=$2; VOCAB=$3; VALID=$4; OUT=$5; DEVICES=${6:-0}
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(dirname "$OUT")"
mkdir -p "$WORK"

# marian detects SentencePiece by extension; pipe materializes inputs under bare
# names, so re-expose the vocab with its .spm suffix (NOTES pillar 4 gotcha (a)).
if [[ "$VOCAB" != *.spm ]]; then ln -sf "$VOCAB" ./vocab.spm && VOCAB=$PWD/vocab.spm; fi

cut -f2 "$VALID" > "$WORK/valid.src"   # student's target = backward's source
cut -f1 "$VALID" > "$WORK/valid.trg"

MARIAN=${MARIAN:-marian} \
  bash "$HERE/train_backward.sh" "$TSRC" "$TTRG" "$VOCAB" \
    "$WORK/valid.src" "$WORK/valid.trg" "$OUT" "$DEVICES"
