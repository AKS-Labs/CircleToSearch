#!/bin/bash
# One box: train the student, then decode FLORES AND the check set from the
# resulting checkpoint. One step because pipe leases per step key — splitting it
# rents a box per phase and ships a checkpoint between them, when the decodes are
# minutes on a GPU already in hand.
#
# Partial runs cannot pass silently: pipe collects the declared outputs and fails
# the step if any is missing, so "trained but never decoded" is not a result.
#
# Finetune is deliberately absent. It bought +2.87 chrF on tl and +1.3 on sw,
# always on FLORES, and its effect on deployment-shaped input has never been
# measured — so it is an experiment to run against this baseline, not a stage.
#
# Usage: train_eval.sh TRAIN_TSV VOCAB VALID_2COL FLORES_SRC CHECK_SRC OUT_DIR [DEVICES=0] [MARIAN_EXTRA]
set -euo pipefail

TSV=$1; VOCAB=$2; VALID=$3; FLORES_SRC=$4; CHECK_SRC=$5; OUT=$6; DEVICES=${7:-0}; MARIAN_EXTRA=${8:-}
mkdir -p "$OUT"

bash /scripts/train_student.sh "$TSV" "$VOCAB" "$VALID" "$OUT/model.npz" "$DEVICES" "$MARIAN_EXTRA"

# --best-ce is what train_student.sh early-stops on; decode THAT, not the last
# checkpoint, or the numbers describe a model nobody would ship.
BEST="$OUT/model.npz.best-ce-mean-words.npz"
[ -f "$BEST" ] || BEST="$OUT/model.npz"

bash /scripts/decode_flores.sh "$BEST" "$VOCAB" "$FLORES_SRC" "$OUT/flores.hyp" "$DEVICES"
bash /scripts/decode_flores.sh "$BEST" "$VOCAB" "$CHECK_SRC"  "$OUT/check.hyp"  "$DEVICES"

wc -l "$OUT/flores.hyp" "$OUT/check.hyp"
