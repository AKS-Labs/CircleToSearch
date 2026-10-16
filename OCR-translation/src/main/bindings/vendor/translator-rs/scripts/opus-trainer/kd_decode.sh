#!/bin/bash
# One KD shard: NLLB teacher forward-decode with an n-best list. Selection is a
# separate CPU step (select_best.sh) so the bicleaner gate can be computed
# concurrently with the decode shards. Teacher + tokenizer are baked into the
# nllb-ct2 image (NLLB_MODEL/NLLB_CT2_DIR); a rented box makes no HF pulls.
#
# BEAM/NBEST default to 4: the sw→en rank histogram at beam 8 was near-uniform
# (rank-1 won 18%, ranks 2-8 9-14% each) = the 600M's beam differs in surface,
# not content, so the extra 2.4x decode cost bought noise. Recompute the
# histogram whenever the TEACHER changes — a stronger teacher with genuinely
# diverse beams is what would re-justify beam 8.
#
# Usage: kd_decode.sh SRC SRC_LANG TGT_LANG OUT_NBEST_GZ [BEAM=4] [NBEST=4]
set -euo pipefail
[ -d /work/out ] && echo decode > /work/out/.phase || true

# The tokenizer is baked into the image (Dockerfile HF-blob trim keeps it and
# gates it with HF_HUB_OFFLINE=1), so a network fetch is never wanted. Without
# this, transformers does an online etag check that HARD-FAILS on an HF 504
# (observed 2026-07-16) — which would kill every KD shard at once during an HF
# outage. Force offline: use the baked tokenizer, never the network.
export HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1

SRC=$1; SRC_LANG=$2; TGT_LANG=$3; OUT=$4; BEAM=${5:-4}; NBEST=${6:-4}

# max-batch-tokens 6144: bench_batch sweep (2026-07-16, 1.3B, beam 4) put the
# throughput knee at ~4096-6144 (205→225 l/s, +10%) and flat past it (8192-12288
# add ~1% each), with OOM at 16384 on the test box. 6144 = 2x the old 3072,
# captures the gain, and stays well below the OOM ceiling so a weaker fleet box
# has margin. Beam 8 would halve this headroom — recompute if the beam changes.
python3 /scripts/distill_data.py \
  --model "$NLLB_MODEL" --ct2-dir "$NLLB_CT2_DIR" \
  --src-lang "$SRC_LANG" --tgt-lang "$TGT_LANG" \
  --src "$SRC" --out "$OUT" \
  --beam "$BEAM" --nbest "$NBEST" --max-batch-tokens 6144
