#!/usr/bin/env bash
# Build a UTF-8 Galician surface-word list from cotovia's morphological dicts.
# The dicts are ISO-8859-1; the gl synthesis path consumes UTF-8 (synthesize.py
# does not iconv for gl), so we convert here and feed UTF-8 to the binary.
set -euo pipefail

GL_DICT_DIR="${1:?usage: build_wordlist.sh <cotovia/lang/gl dir> <out.txt>}"
OUT="${2:?usage: build_wordlist.sh <cotovia/lang/gl dir> <out.txt>}"

for f in principal.txt nomes.txt adxectivos.txt; do
  iconv -f ISO-8859-1 -t UTF-8 "$GL_DICT_DIR/$f"
done \
  | awk -F',' 'NF{w=$1; sub(/^#/,"",w); print w}' \
  | grep -vE '\$|^$' \
  | grep -ZE '^[[:alpha:]]+$' \
  | sort -u > "$OUT"

echo "wrote $(wc -l <"$OUT") words to $OUT"
