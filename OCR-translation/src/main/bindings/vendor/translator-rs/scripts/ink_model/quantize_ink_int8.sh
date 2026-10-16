#!/usr/bin/env bash
# Full int8 (weight + activation) post-training quantization of an ink checkpoint via
# MNN's quantized.out, calibrated on real dewarped strips (the model's true input
# distribution). Full int8 hits the device's i8sdot GEMM path — load it with the default
# (MemoryMode::Low) session, NOT load_conv/High (which would dequantize it back to float).
#
#   quantize_ink_int8.sh ckpt/ink-v11.pt 'smoke-out/.../deskewed/box-*.png' out.mnn
set -euo pipefail
cd "$(dirname "$0")"
ckpt="$1"; strips_glob="$2"; out="$3"
MNN=/home/david/git/mnn-sys/3rd_party/MNN/build-convert
tmp=$(mktemp -d)

# 1. fp32 MNN (quantizer input)
.venv/bin/python convert_ink_mnn.py --ckpt "$ckpt" --out "$tmp/fp32.mnn" --no-fp16 >/dev/null

# 2. calibration set: ~200 real strips resized to the model's 48px input height
mkdir -p "$tmp/calib"; i=0
for f in $(ls $strips_glob 2>/dev/null | grep -v fallback | shuf | head -200); do
  convert "$f" -resize x48 "$tmp/calib/c$(printf %04d "$i").png"; i=$((i + 1))
done
echo "calibration strips: $i"

# 3. quant config (input is 0..1 RGB → mean 0, normal 1/255) + run PTQ
cat > "$tmp/q.json" <<JSON
{"format":"RGB","mean":[0,0,0],"normal":[0.0039215686,0.0039215686,0.0039215686],
 "width":320,"height":48,"path":"$tmp/calib","used_feature_quantize_method":"KL",
 "used_weight_quantize_method":"MAX_ABS","feature_clamp_value":127,"weight_clamp_value":127,
 "batch_size":16}
JSON
"$MNN/quantized.out" "$tmp/fp32.mnn" "$out" "$tmp/q.json" | tail -2
echo "-> $out ($(du -k "$out" | cut -f1) KB)"
