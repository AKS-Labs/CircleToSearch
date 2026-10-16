#!/usr/bin/env bash
set -euo pipefail

WORK_DIR="${1:-/tmp/pulc-language-demo}"
MODEL_DIR="${MODEL_DIR:-$WORK_DIR/language_classification_infer}"
ONNX_OUT="${ONNX_OUT:-$WORK_DIR/language_classification.onnx}"
MNN_OUT="${MNN_OUT:-$WORK_DIR/language_classification_wq8.mnn}"
PADDLE2ONNX="${PADDLE2ONNX:-paddle2onnx}"
MNNCONVERT="${MNNCONVERT:-MNNConvert}"
DOWNLOAD="${DOWNLOAD:-0}"
QUANT_BITS="${QUANT_BITS:-8}"
URL="https://paddleclas.bj.bcebos.com/models/PULC/inference/language_classification_infer.tar"

if [[ "$DOWNLOAD" == "1" && ! -f "$MODEL_DIR/inference.pdmodel" ]]; then
  mkdir -p "$WORK_DIR"
  curl -L --fail "$URL" -o "$WORK_DIR/language_classification_infer.tar"
  tar -xf "$WORK_DIR/language_classification_infer.tar" -C "$WORK_DIR"
fi

if [[ ! -f "$MODEL_DIR/inference.pdmodel" || ! -f "$MODEL_DIR/inference.pdiparams" ]]; then
  echo "missing Paddle inference files under $MODEL_DIR" >&2
  echo "rerun with DOWNLOAD=1, or set MODEL_DIR=/path/to/language_classification_infer" >&2
  exit 1
fi

"$PADDLE2ONNX" \
  --model_dir "$MODEL_DIR" \
  --model_filename inference.pdmodel \
  --params_filename inference.pdiparams \
  --save_file "$ONNX_OUT" \
  --opset_version 11

"$MNNCONVERT" \
  -f ONNX \
  --modelFile "$ONNX_OUT" \
  --MNNModel "$MNN_OUT" \
  --bizCode biz \
  --weightQuantBits "$QUANT_BITS"

printf '%s\n' "$MNN_OUT"
