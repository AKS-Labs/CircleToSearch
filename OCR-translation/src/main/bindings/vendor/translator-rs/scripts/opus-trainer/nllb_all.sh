#!/bin/bash
# Both NLLB sizes, both directions, FLORES gate + probe decode, on ONE rented box.
#
# The 1.3B weights (~5.5GB) are prefetched in the BACKGROUND while the 600M work
# runs, so the second model's download overlaps the first model's compute instead
# of serialising behind it.
#
# Reuses the nllb-ct2 image with the same runtime GPU-torch swap hy_mt2.sh does:
# the image bakes CPU torch, and `pip install torch` no-ops against it, so the
# uninstall is load-bearing or generation dies with "Torch not compiled with CUDA".
#
# Usage: nllb_all.sh PROBES_EN PROBES_TL OUT_DIR
set -euo pipefail

PROBES_EN=$1; PROBES_TL=$2; OUT=$3

SMALL=facebook/nllb-200-distilled-600M
BIG=facebook/nllb-200-distilled-1.3B

# torch>=2.6 (NOT the cu121 pin hy_mt2.sh uses): NLLB ships pytorch_model.bin, and
# transformers refuses torch.load on torch<2.6 since CVE-2025-32434. Hy-MT2 ships
# safetensors, which is why the cu121 recipe worked there and dies here.
pip uninstall -y torch 2>&1 | tail -1
pip install --no-cache-dir "torch>=2.6" --index-url https://download.pytorch.org/whl/cu124 2>&1 | tail -1
pip install --no-cache-dir -q sacrebleu huggingface_hub 2>&1 | tail -1

export FLORES_CACHE=/tmp/flores
export PYTHONPATH=/scripts
mkdir -p "$OUT"

# Prefetch the big model while the small one works.
huggingface-cli download "$BIG" > /tmp/prefetch.log 2>&1 &
PREFETCH=$!

run_one() {
    local model=$1 tag=$2
    echo "############ $tag ############"
    python3 /scripts/nllb_gate.py --pairs eng_Latn-tgl_Latn,tgl_Latn-eng_Latn \
        --model "$model" --device cuda --limit 300 --beam 4 --batch 8 \
        --out-dir "$OUT/flores_$tag"
    python3 /scripts/nllb_decode.py "$model" "$PROBES_EN" eng_Latn tgl_Latn \
        "$OUT/probe_$tag.en2tl" cuda
    python3 /scripts/nllb_decode.py "$model" "$PROBES_TL" tgl_Latn eng_Latn \
        "$OUT/probe_$tag.tl2en" cuda
}

run_one "$SMALL" 600m

wait "$PREFETCH" || { echo "prefetch of $BIG failed"; tail -5 /tmp/prefetch.log; exit 1; }
run_one "$BIG" 1.3b
