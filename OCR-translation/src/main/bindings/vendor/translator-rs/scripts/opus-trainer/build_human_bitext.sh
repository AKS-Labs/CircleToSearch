#!/bin/bash
# The pair's HUMAN bitext: bicleaner-salvaged mined pairs + the curated pool.
# Feeds the student finetune (both sides used as-is) and the backward RNN (which
# must be teacher-independent to be an honest ce-filter judge).
#
# WHY salvage: a low-resource pair's curated set is tiny (en-ug: ~6.2k raw vs
# sw's 9k), and human finetune SUPPLY is the binding constraint on FLORES. The
# mined pool holds genuinely-aligned human pairs that our heuristic filters keep
# but we exclude from finetune for semantic-misalignment risk; bicleaner-ai is
# the ML filter that can tell those apart. sw→en: 908k salvaged at >=0.8 from
# 6M, worth +1.6 chrF++ (2026-07-14).
#
# THRESHOLD is deliberately stricter than the extract-best gate (0.5) that reads
# the SAME scores: a gate mistake costs one rank-1 fallback, but a finetune
# mistake teaches a wrong source->target mapping directly.
#
# Emits the pair in STUDENT order (out/human_src = the non-English side, the
# student's source; out/human_tgt = English). The curated pool arrives as
# en \t xx, so its columns are swapped here.
#
# Usage: build_human_bitext.sh KD_SRC KD_REF GATES THRESHOLD CURATED_POOL OUT_DIR
set -euo pipefail

KD_SRC=$1; KD_REF=$2; GATES=$3; THRESH=$4; CURATED=$5; OUT=$6

n_src=$(wc -l < "$KD_SRC"); n_gate=$(wc -l < "$GATES")
if [ "$n_src" != "$n_gate" ]; then
  echo "build_human_bitext: line mismatch kd_src=$n_src gates=$n_gate" >&2
  exit 1
fi

# Salvaged: mined pairs whose bicleaner score clears the bar. Both sides must be
# non-blank — a zero-token pair nans fast_align's EM downstream.
paste "$GATES" "$KD_SRC" "$KD_REF" \
  | awk -F'\t' -v t="$THRESH" \
      '$1 >= t && $2 ~ /[^[:space:]]/ && $3 ~ /[^[:space:]]/ { print $2 "\t" $3 }' \
  > "$OUT/salvaged.tsv"

# Curated pool is en \t xx; the student is xx -> en, so swap.
awk -F'\t' 'NF == 2 && $1 ~ /[^[:space:]]/ && $2 ~ /[^[:space:]]/ { print $2 "\t" $1 }' \
  "$CURATED" > "$OUT/curated.tsv"

cat "$OUT/salvaged.tsv" "$OUT/curated.tsv" | LC_ALL=C sort -u > "$OUT/human.tsv"
cut -f1 "$OUT/human.tsv" > "$OUT/human_src"
cut -f2 "$OUT/human.tsv" > "$OUT/human_tgt"
rm "$OUT/salvaged.tsv" "$OUT/curated.tsv" "$OUT/human.tsv"

echo "salvaged(>=$THRESH) + curated -> $(wc -l < "$OUT/human_src") human pairs (of $n_src scored)"
