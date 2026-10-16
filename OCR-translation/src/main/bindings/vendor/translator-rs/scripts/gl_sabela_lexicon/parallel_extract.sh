#!/usr/bin/env bash
# Shard a wordlist across N parallel extract_lexicon.py workers and merge.
# Each worker gets its own interposer dump file (ORT_DUMP_FILE), so the captured
# IDS lines never interleave. Merge is a plain concat (shards are disjoint).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
JOBS="${JOBS:-4}"
AHO="${1:?usage: parallel_extract.sh <ahotts-dir> <wordlist> <out>}"
WORDLIST="${2:?usage: parallel_extract.sh <ahotts-dir> <wordlist> <out>}"
OUT="${3:?usage: parallel_extract.sh <ahotts-dir> <wordlist> <out>}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Round-robin split keeps shards balanced regardless of word-length ordering.
split -n "r/$JOBS" --numeric-suffixes=1 "$WORDLIST" "$TMP/shard."

pids=()
for shard in "$TMP"/shard.*; do
  n="${shard##*.}"
  python3 "$HERE/extract_lexicon.py" \
    --ahotts-dir "$AHO" \
    --wordlist "$shard" \
    --voice "$AHO/ahotts/voices/gl/stub" \
    --interpose "$HERE/ort_intercept.so" \
    --dump "$TMP/dump.$n.txt" \
    --out "$TMP/lex.$n.txt" &
  pids+=("$!")
done

fail=0
for p in "${pids[@]}"; do wait "$p" || fail=1; done
[ "$fail" = 0 ] || { echo "a worker failed" >&2; exit 1; }

sort -m "$TMP"/lex.*.txt 2>/dev/null | sort -u > "$OUT" || cat "$TMP"/lex.*.txt | sort -u > "$OUT"
echo "merged $(wc -l <"$OUT") entries -> $OUT (JOBS=$JOBS)"
