#!/bin/bash
# Phase 4: quantize a trained marian .npz to a slimt-loadable int8 model.
#
# MUST use the BROWSERMT marian (github.com/browsermt/marian-dev). Upstream
# marian-conv emits an intgemm tensor type slimt rejects (65537 vs slimt's
# legacy 0x4101); browsermt also quantizes embeddings (int8), giving ~30MB.
# Run inside the marian-bmt image. extract_stats.py needs numpy: the image now
# installs python3-numpy (rebuild it), else run step 2 on a host python w/ numpy.
#
# The lexical SHORTLIST is built separately by shortlist.sh (it must be SPM-subword
# level, so it can't reuse this quantization). The final slimt pack = this
# model.intgemm.alphas.bin + shortlist.sh's lex.50.50.s2t.bin + the vocab.spm.
#
# Feed the .best-ce-mean-words.npz from finetune_student.sh (the shippable model).
#
# Usage: quantize_export.sh MODEL_NPZ VOCAB_SPM DEVTEST_SRC OUT_DIR
set -euo pipefail
[ -d /work/out ] && echo quantize > /work/out/.phase || true

MODEL=$1; VOCAB=$2; DEVTEST=$3; OUT=$4
HERE="$(cd "$(dirname "$0")" && pwd)"
BMT=/opt/marian-dev/build
mkdir -p "$OUT"

# marian detects SentencePiece vocabs by the .spm extension; pipe materializes
# inputs under bare names, so re-expose the vocab with its extension.
if [[ "$VOCAB" != *.spm ]]; then
  ln -sf "$VOCAB" "$OUT/vocab.spm" && VOCAB="$OUT/vocab.spm"
fi

# Same for the model: marian sniffs .npz/.bin by extension and aborts on a bare
# name ("Unknown model format for file"), which is how the entl v4 pack failed.
if [[ "$MODEL" != *.npz && "$MODEL" != *.bin ]]; then
  ln -sf "$MODEL" "$OUT/model.npz" && MODEL="$OUT/model.npz"
fi

# 1) collect typical quantization multipliers by decoding a sample (beam 1)
"$BMT/marian-decoder" --models "$MODEL" --vocabs "$VOCAB" "$VOCAB" \
  --config "$HERE/configs/quantize.decoder.yml" \
  --input "$DEVTEST" --output "$OUT/sample.out" \
  --dump-quantmult --quiet --quiet-translation 2>"$OUT/quantmults"

# 2) bake alpha stats into the model (needs numpy)
python3 "$HERE/extract_stats.py" "$OUT/quantmults" "$MODEL" "$OUT/model.alphas.npz"

# 3) convert to browsermt int8 intgemm (the format slimt loads)
"$BMT/marian-conv" --from "$OUT/model.alphas.npz" \
  --to "$OUT/model.intgemm.alphas.bin" --gemm-type intgemm8

echo "done: $OUT/model.intgemm.alphas.bin  (build the shortlist with shortlist.sh)"
