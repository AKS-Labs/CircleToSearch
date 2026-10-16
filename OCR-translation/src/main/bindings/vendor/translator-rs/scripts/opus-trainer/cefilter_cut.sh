#!/bin/bash
# Drop the worst PCT% of KD rows by backward-model score. SCORES and TRAIN_TSV
# must be in the same row order (both follow the KD corpus). Output order is
# score-sorted, which is fine: OpusTrainer + --shuffle batches reshuffle anyway.
#
# Usage: cefilter_cut.sh SCORES TRAIN_TSV OUT_TSV [PCT=5]
set -euo pipefail

SCORES=$1; TSV=$2; OUT=$3; PCT=${4:-5}
total=$(wc -l < "$SCORES")
if [ "$total" != "$(wc -l < "$TSV")" ]; then
  echo "cefilter_cut: line mismatch scores=$total tsv=$(wc -l < "$TSV")" >&2
  exit 1
fi
drop=$((total * PCT / 100))
paste "$SCORES" "$TSV" | LC_ALL=C sort -n -k1,1 -S 2G -T . | tail -n +$((drop + 1)) | cut -f2- > "$OUT"
echo "kept $((total - drop))/$total rows (dropped worst ${PCT}%)"
