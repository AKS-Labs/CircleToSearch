#!/bin/bash
# Phase 3: train a base-memory SSRU student with guided-alignment (KD phase),
# behind OpusTrainer input augmentation (case/typos/noise/end-punct) for
# robustness to messy, casual, and OOD input. Run inside a marian image (CPU:
# marian-*:cpu; GPU: a CUDA marian build, see docker/Dockerfile.marian-cuda).
# Follow with finetune_student.sh on clean data.
#
# OpusTrainer reads the 3-col TSV, perturbs a fraction of lines in place (same
# line count), and pipes the augmented stream into marian's stdin. Marian reads
# --tsv from stdin, so there is NO --train-sets; --shuffle batches +
# --no-restore-corpus are required because the corpus is a non-seekable stream.
#
# TRAIN_TSV is a 3-col TSV: src \t trg \t pharaoh-alignments (from align.sh).
# VALID_TSV must be a 2-COLUMN tsv (src\ttrg) -- marian validation wants 2 fields;
# a 3-col valid aborts with "Excessive field(s)".
# DEVICES e.g. "0" / "0 1" for GPU; omit for CPU.
# MARIAN env var overrides the binary path (default /root/marian-dev/build/marian).
#
# Needs `pip install opustrainer` (console script opustrainer-train) in the env.
# Gotcha: the trainer target is `marian_train` (underscored); `make marian` builds
# only the static lib. See docker/Dockerfile.marian-cuda.
#
# OPUSTRAINER_CFG overrides the augmentation config (default
# configs/opustrainer.student.yml). A pair whose target script has no case
# needs its own, or UpperCase byte-falls-back every perturbed line.
# EXTRA passes additional marian flags verbatim (e.g. "--fp16 --mini-batch 4000").
# Default empty => byte-identical behaviour to before, so an existing run's
# numbers stay comparable.
#
# Usage: train_student.sh TRAIN_TSV VOCAB_SPM VALID_2COL_TSV MODEL_OUT_NPZ [DEVICES] [EXTRA]
set -euo pipefail
[ -d /work/out ] && echo train > /work/out/.phase || true

TSV=$1; VOCAB=$2; VALID=$3; OUT=$4; DEVICES=${5:-}; EXTRA=${6:-}
read -ra EXTRA_ARGS <<< "$EXTRA"
HERE="$(cd "$(dirname "$0")" && pwd)"
MARIAN=${MARIAN:-$(command -v marian || echo /root/marian-dev/build/marian)}
mkdir -p "$(dirname "$OUT")"
# marian detects SentencePiece vocabs by the .spm extension; pipe materializes
# inputs under bare names, so re-expose the vocab with its extension.
if [[ "$VOCAB" != *.spm ]]; then
  ln -sf "$VOCAB" "$(dirname "$OUT")/vocab.spm" && VOCAB="$(dirname "$OUT")/vocab.spm"
fi

# The default workspace is omitted when EXTRA sets one: passing --workspace twice
# is ambiguous (marian may take either), and EXTRA is how a caller tunes it.
if [ -n "$DEVICES" ]; then
  DEV=(--devices $DEVICES)
  [[ "$EXTRA" == *--workspace* ]] || DEV+=(--workspace 9000)
else
  DEV=(--cpu-threads "$(nproc)")
  [[ "$EXTRA" == *--workspace* ]] || DEV+=(--workspace 4000)
fi

OT_CFG="$(dirname "$OUT")/config.opustrainer.yml"
sed "s|__TSV__|$TSV|g" "${OPUSTRAINER_CFG:-$HERE/configs/opustrainer.student.yml}" > "$OT_CFG"

# Streaming, deliberately, after weighing materialising the augmented corpus
# (2026-07-21). `until original inf` re-rolls the modifiers every pass, so a
# ~28-epoch run sees ~28 DIFFERENT perturbations of each sentence; a fixed
# N-pass corpus freezes that to N. And the reasons to materialise turned out to
# be already covered: the augmentation is a pure function of (corpus digest,
# opustrainer==0.5 pinned in the image, seed: 1111), so determinism does not
# need the expansion stored — the seed IS the compressed form. The corpus is
# already shuffled three ways (build_kd_source's seeded shuf, OpusTrainer's
# per-pass reshuffle, marian's maxi-batch window), and checkpoints already
# resume weights, so --shuffle data and corpus-position restore bought little
# for ~23GB of artifact plus ~55GB of shuffle temp on the training box.
opustrainer-train --config "$OT_CFG" --log-level INFO \
  "$MARIAN" \
  -c "$HERE/configs/student.base-memory.yml" "$HERE/configs/student.train.yml" \
  --tsv --tsv-fields 3 \
  --shuffle batches --no-restore-corpus \
  --vocabs "$VOCAB" "$VOCAB" --dim-vocabs 32000 32000 \
  --model "$OUT" \
  --valid-sets "$VALID" --valid-metrics ce-mean-words \
  --keep-best --overwrite \
  "${DEV[@]}" "${EXTRA_ARGS[@]}"

echo "done: ${OUT%.npz}.best-ce-mean-words.npz"
