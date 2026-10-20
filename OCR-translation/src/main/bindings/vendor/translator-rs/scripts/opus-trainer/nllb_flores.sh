#!/bin/bash
# One-shot NLLB-1.3B FLORES teacher decode: 1-best beam, plain one-hyp-per-line
# output (for scoring the KD teacher floor). Model + CT2 dir baked in the image.
set -euo pipefail
export HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1

SRC=$1; OUT=$2

python3 /scripts/distill_data.py \
  --model "$NLLB_MODEL" --ct2-dir "$NLLB_CT2_DIR" \
  --src-lang uig_Arab --tgt-lang eng_Latn \
  --src "$SRC" --out "$OUT" \
  --beam 4 --nbest 1 --max-batch-tokens 6144
