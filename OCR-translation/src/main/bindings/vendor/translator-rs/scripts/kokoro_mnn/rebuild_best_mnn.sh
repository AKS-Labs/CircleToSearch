#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

SOURCE_ONNX="${SOURCE_ONNX:-kokoro-v1.0.patched.i32.onnx}"
SOURCE_FP32_ONNX="${SOURCE_FP32_ONNX:-kokoro-v1.0.onnx}"
PATCH_ONNX="${PATCH_ONNX:-1}"
WORK_DIR="${WORK_DIR:-compression_params}"

CLEAN_MNN="${CLEAN_MNN:-kokoro-clean.mnn}"
OPTFAST_EXTERNAL_MNN="${OPTFAST_EXTERNAL_MNN:-kokoro-optfast-external.mnn}"
OPTFAST_DEDUP_MNN="${OPTFAST_DEDUP_MNN:-kokoro.mnn}"
OPTFAST_JSON="${OPTFAST_JSON:-kokoro-optfast-external.json}"

CLEAN_BASE_COMPRESS="$WORK_DIR/block128.base.json"
CLEAN_FINAL_COMPRESS="$WORK_DIR/block128.skip-istft-convpost-duration-bert.json"
CLEAN_BASE_MNN="$WORK_DIR/block128.base.mnn"

OPTFAST_BASE_COMPRESS="$WORK_DIR/block128.optfast.base.json"
OPTFAST_FINAL_COMPRESS="$WORK_DIR/block128.optfast.skip-istft-convpost-duration-bert.json"
OPTFAST_BASE_MNN="$WORK_DIR/block128.optfast.base.mnn"

mkdir -p "$WORK_DIR"
rm -f "$CLEAN_BASE_COMPRESS" "$CLEAN_FINAL_COMPRESS" \
  "$OPTFAST_BASE_COMPRESS" "$OPTFAST_FINAL_COMPRESS"

if [[ ! -f "$SOURCE_ONNX" && "$PATCH_ONNX" == "1" ]]; then
  uv run python patch_resize.py --input "$SOURCE_FP32_ONNX" --output "$SOURCE_ONNX"
fi

if [[ ! -f "$SOURCE_ONNX" ]]; then
  echo "missing source ONNX: $SOURCE_ONNX" >&2
  echo "Download from https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx" >&2
  echo "set SOURCE_ONNX or place $SOURCE_FP32_ONNX here and leave PATCH_ONNX=1" >&2
  exit 1
fi

# First pass: ask MNN's converter to create a block128 weight-only compression
# parameter file. The model emitted in this pass is only an intermediate.
uv run mnnconvert -f ONNX \
  --modelFile "$SOURCE_ONNX" \
  --MNNModel "$CLEAN_BASE_MNN" \
  --weightQuantBits 8 \
  --weightQuantBlock 128 \
  --compressionParamsFile "$CLEAN_BASE_COMPRESS"

# Final pass config:
# - final iSTFT ConvTranspose fp32: removes the 4.8/9.6 kHz whistle;
# - generator conv_post fp32: fixes Android low-memory static/roughness;
# - predictor + BERT-side projections fp32: fixes Android low-memory rounded
#   duration drift on the long sentence.
uv run python edit_compression_params.py \
  --infile "$CLEAN_BASE_COMPRESS" \
  --outfile "$CLEAN_FINAL_COMPRESS" \
  --bits 0 \
  --op /decoder/decoder/generator/istft/stft/ConvTranspose_output_0 \
  --op-prefix /decoder/decoder/generator/conv_post/ \
  --op-prefix /encoder/predictor/ \
  --op /encoder/bert/encoder/embedding_hidden_mapping_in/Add_output_0__matmul_converted \
  --op /encoder/bert_encoder/Add_output_0__matmul_converted

uv run mnnconvert -f ONNX \
  --modelFile "$SOURCE_ONNX" \
  --MNNModel "$CLEAN_MNN" \
  --weightQuantBits 8 \
  --weightQuantBlock 128 \
  --compressionParamsFile "$CLEAN_FINAL_COMPRESS"

# The shipping candidate uses --optimizePrefer 2 for the BERT/ALBERT speedup.
# Generate compression params under that optimized graph, then keep the same
# quality-preserving fp32 exclusions.
uv run mnnconvert -f ONNX \
  --modelFile "$SOURCE_ONNX" \
  --MNNModel "$OPTFAST_BASE_MNN" \
  --weightQuantBits 8 \
  --weightQuantBlock 128 \
  --compressionParamsFile "$OPTFAST_BASE_COMPRESS" \
  --optimizePrefer 2

uv run python edit_compression_params.py \
  --infile "$OPTFAST_BASE_COMPRESS" \
  --outfile "$OPTFAST_FINAL_COMPRESS" \
  --bits 0 \
  --op /decoder/decoder/generator/istft/stft/ConvTranspose_output_0 \
  --op-prefix /decoder/decoder/generator/conv_post/ \
  --op-prefix /encoder/predictor/ \
  --op /encoder/bert/encoder/embedding_hidden_mapping_in/Add_output_0__matmul_converted \
  --op /encoder/bert_encoder/Add_output_0__matmul_converted

uv run mnnconvert -f ONNX \
  --modelFile "$SOURCE_ONNX" \
  --MNNModel "$OPTFAST_EXTERNAL_MNN" \
  --weightQuantBits 8 \
  --weightQuantBlock 128 \
  --compressionParamsFile "$OPTFAST_FINAL_COMPRESS" \
  --optimizePrefer 2 \
  --saveExternalData

# Dump JSON as metadata only. Do not JSON->MNN roundtrip: that perturbs Kokoro
# output. The deduper patches offsets directly in the original flatbuffer.
uv run mnnconvert -f MNN \
  --modelFile "$OPTFAST_EXTERNAL_MNN" \
  --JsonFile "$OPTFAST_JSON"

uv run python dedup_mnn_external_weights.py \
  --json "$OPTFAST_JSON" \
  --mnn-in "$OPTFAST_EXTERNAL_MNN" \
  --weight-in "$OPTFAST_EXTERNAL_MNN.weight" \
  --mnn-out "$OPTFAST_DEDUP_MNN" \
  --weight-out "$OPTFAST_DEDUP_MNN.weight"

stat -c '%n %s bytes' "$CLEAN_MNN" \
  "$OPTFAST_EXTERNAL_MNN" "$OPTFAST_EXTERNAL_MNN.weight" \
  "$OPTFAST_DEDUP_MNN" "$OPTFAST_DEDUP_MNN.weight"
