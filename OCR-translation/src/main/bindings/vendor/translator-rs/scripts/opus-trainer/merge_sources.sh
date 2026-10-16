#!/bin/bash
# Concatenate per-source (src, tgt) pairs into one corpus, in argument order.
#
# The sources are kept as separate artifacts precisely so a source can be dropped
# later without re-decoding; this is the step that joins them for training. Order
# is fixed by the caller and both sides are cat'd in the same order, so src line N
# still pairs with tgt line N.
#
# Every source's src/tgt line counts are asserted BEFORE any concatenation. A
# short decode that slipped past the per-shard assert would otherwise desync every
# pair after it in the merged file — silently, since a desynced corpus still
# trains, it just trains on garbage pairs (the same failure class as the round-1
# Kazakh: nothing errors, only the scores are bad).
#
# Usage: merge_sources.sh OUT_DIR SRC1 TGT1 [SRC2 TGT2 ...]
set -euo pipefail

OUT=$1; shift
mkdir -p "$OUT"

if [ $# -eq 0 ] || [ $(($# % 2)) -ne 0 ]; then
  echo "merge_sources.sh: need OUT_DIR followed by (src tgt) pairs, got $# file args" >&2
  exit 1
fi

srcs=(); tgts=()
while [ $# -gt 0 ]; do
  s=$1; t=$2; shift 2
  for f in "$s" "$t"; do
    [ -f "$f" ] || { echo "merge_sources.sh: missing $f" >&2; exit 1; }
  done
  ls=$(wc -l < "$s"); lt=$(wc -l < "$t")
  if [ "$ls" -ne "$lt" ]; then
    echo "merge_sources.sh: $s has $ls lines but $t has $lt -- refusing to desync the corpus" >&2
    exit 1
  fi
  if [ "$ls" -eq 0 ]; then
    echo "merge_sources.sh: $s is empty" >&2
    exit 1
  fi
  echo "  $(basename "$s"): $ls lines"
  srcs+=("$s"); tgts+=("$t")
done

cat "${srcs[@]}" > "$OUT/src"
cat "${tgts[@]}" > "$OUT/tgt"

n_src=$(wc -l < "$OUT/src"); n_tgt=$(wc -l < "$OUT/tgt")
if [ "$n_src" -ne "$n_tgt" ]; then
  echo "merge_sources.sh: merged src=$n_src tgt=$n_tgt" >&2
  exit 1
fi
echo "done: $OUT/src + $OUT/tgt ($n_src lines from ${#srcs[@]} sources)"
