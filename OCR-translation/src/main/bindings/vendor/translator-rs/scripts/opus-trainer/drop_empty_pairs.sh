#!/bin/bash
# Drop pairs where either side is empty/whitespace. A zero-token side sends
# fast_align's EM to nan and the ENTIRE reverse pass comes back as empty
# alignments (2026-07-16: 5 such lines out of 2.87M did exactly that; same
# signature as the align_ensw defect). Sources: pool lines with an empty column
# (awk NF==2 passes "text\t") and CT2 empty/whitespace hypotheses.
#
# Usage: drop_empty_pairs.sh SRC TGT OUT_DIR
set -euo pipefail

SRC=$1; TGT=$2; OUT=$3
paste "$SRC" "$TGT" | awk -F'\t' '$1 ~ /[^[:space:]]/ && $2 ~ /[^[:space:]]/' > "$OUT/pairs"
cut -f1 "$OUT/pairs" > "$OUT/src"
cut -f2 "$OUT/pairs" > "$OUT/tgt"
rm "$OUT/pairs"
wc -l "$OUT/src" "$OUT/tgt"
