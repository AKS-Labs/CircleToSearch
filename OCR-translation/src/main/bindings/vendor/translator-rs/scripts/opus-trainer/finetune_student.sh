#!/bin/bash
# Phase 3b: quantize-aware finetune of the KD-trained student on CLEAN parallel,
# behind the same OpusTrainer input augmentation as train_student.sh (case/typos/
# noise/end-punct) so the final polish is also robust to messy/casual input.
#
# The KD-only student (trained on teacher forward-translations of mined web text)
# underperforms on clean-domain test sets. Finetuning on clean parallel closes
# the gap: en->tl went to 57.9 chrF++ vs 59.4 teacher (KD-only was much lower via
# the runtime). This is NOT optional for a shippable model.
#
# OpusTrainer pipes the augmented 3-col TSV into marian's stdin, so there is NO
# --train-sets; --shuffle batches + --no-restore-corpus are required for the
# non-seekable stream.
#
# student.finetune.yml sets quantize-bits: 8 which REQUIRES --sync-sgd.
# VALID_TSV must be a 2-COLUMN tsv (src\ttrg) -- marian validation wants 2 fields,
# a 3-col valid (with alignment) aborts with "Excessive field(s)".
#
# OPUSTRAINER_CFG overrides the augmentation config (default
# configs/opustrainer.student.yml). A pair whose target script has no case
# needs its own, or UpperCase byte-falls-back every perturbed line.
# Needs `pip install opustrainer` (console script opustrainer-train) in the env.
# MARIAN env var = path to the marian trainer binary (default /root/marian-dev/build/marian).
#
# Usage: finetune_student.sh CLEAN_TSV VOCAB PRETRAINED_NPZ VALID_2COL_TSV MODEL_OUT [DEVICES=0]
set -euo pipefail
[ -d /work/out ] && echo train > /work/out/.phase || true

TSV=$1; VOCAB=$2; PRE=$3; VALID=$4; OUT=$5; DEVICES=${6:-0}
HERE="$(cd "$(dirname "$0")" && pwd)"

# 42 of the 175 `probes/check.en` lines trained the shipped en->ka finetune,
# because the eval exclusion lived inside one generator's build step and a
# corpus assembled any other way skipped it in silence. The report is required
# here so that cannot happen again: the check runs on whatever TSV reaches the
# GPU, whoever built it.
if [ ! -s "$TSV.evalclean.json" ]; then
  echo "finetune_student.sh: $TSV has no eval-leak report at $TSV.evalclean.json" >&2
  echo "Run exclude_eval.py over the corpus first, e.g." >&2
  echo "  exclude_eval.py --train $TSV --out ${TSV%.tsv}.clean.tsv --text-columns 2 \\" >&2
  echo "    --eval probes/check.en probes/adversarial.en data/eval_exclude.sha256 ..." >&2
  echo "and train on its output, which carries the report beside it." >&2
  exit 1
fi
MARIAN=${MARIAN:-$(command -v marian || echo /root/marian-dev/build/marian)}
mkdir -p "$(dirname "$OUT")"
if [[ "$VOCAB" != *.spm ]]; then
  ln -sf "$VOCAB" "$(dirname "$OUT")/vocab.spm" && VOCAB="$(dirname "$OUT")/vocab.spm"
fi
if [[ "$PRE" != *.npz && "$PRE" != *.bin ]]; then
  ln -sf "$PRE" "$(dirname "$OUT")/pretrained.npz" && PRE="$(dirname "$OUT")/pretrained.npz"
fi

OT_CFG="$(dirname "$OUT")/config.opustrainer.finetune.yml"
sed "s|__TSV__|$TSV|g" "${OPUSTRAINER_CFG:-$HERE/configs/opustrainer.student.yml}" > "$OT_CFG"

opustrainer-train --config "$OT_CFG" --log-level INFO \
  "$MARIAN" \
  -c "$HERE/configs/student.base-memory.yml" "$HERE/configs/student.finetune.yml" \
  --tsv --tsv-fields 3 \
  --shuffle batches --no-restore-corpus \
  --vocabs "$VOCAB" "$VOCAB" --dim-vocabs 32000 32000 \
  --pretrained-model "$PRE" --model "$OUT" \
  --valid-sets "$VALID" --valid-metrics ce-mean-words \
  --sync-sgd --devices $DEVICES --workspace 9000 --keep-best --overwrite

echo "done: ${OUT%.npz}.best-ce-mean-words.npz (use the .best for quantization)"
