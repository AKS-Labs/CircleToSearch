#!/bin/bash
# Phase 5: package a quantized model + shortlist + vocab into a slimt bucket pack.
#
# Emits the three gzip'd files under the bucket names the app expects and a
# meta.json carrying the model's UNCOMPRESSED size + sha256 (the app verifies the
# decompressed model against these, so they are measured pre-gzip). Drop the
# meta.json values straight into custom_models.json.
#
# Usage: pack_slimt.sh MODEL_BIN LEX_BIN VOCAB_SPM PAIR_INFIX OUT_DIR
#   PAIR_INFIX is the compact pair tag in the filenames, e.g. ugen / swen / entl.
set -euo pipefail
[ -d /work/out ] && echo pack > /work/out/.phase || true

MODEL=$1; LEX=$2; VOCAB=$3; INFIX=$4; OUT=$5
mkdir -p "$OUT"

gzip -c "$MODEL" > "$OUT/model.$INFIX.intgemm.alphas.bin.gz"
gzip -c "$LEX"   > "$OUT/lex.50.50.$INFIX.s2t.bin.gz"
gzip -c "$VOCAB" > "$OUT/vocab.$INFIX.spm.gz"

size=$(stat -c %s "$MODEL")
hash=$(sha256sum "$MODEL" | cut -d' ' -f1)
printf '{\n  "uncompressedSize": %s,\n  "uncompressedHash": "%s"\n}\n' "$size" "$hash" > "$OUT/meta.json"

echo "done: $OUT (uncompressedSize=$size hash=$hash)"
