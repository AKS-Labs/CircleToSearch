#!/bin/bash
# Validate the marian pin move (e8a1a25 -> 1a743582) before committing a full train.
#
# e8a1a25 could not train this student in fp16 at all: guided-alignment aborted
# with "Child 1 has different type (first: float32 != child: float16)", and
# --workspace >12000 aborted in mini-batch-fit's probe with a 2^31 overflow
# ("Labels not matching logits shape (2621440000 != -1673527296)"), leaving 11GB
# of a 24GB card unused. 1a743582 casts the multi-loss and catches
# ShapeSizeException during fitting, so both should now work.
#
# Every earlier throughput number (fp32 142k w/s, fp16 231k w/s = 1.62x) was
# measured with guided-alignment OFF, because that was the only way fp16 ran.
# The numbers that matter are the ones with it ON, which is what this measures.
#
# Usage: fp16_validate.sh TRAIN_TSV VOCAB OUT_DIR
set -euo pipefail

TSV=$1; VOCAB=$2; OUT=$3
mkdir -p "$OUT"
M=/usr/local/bin/marian

# marian sniffs SentencePiece by extension; pipe materializes inputs bare.
[[ "$VOCAB" == *.spm ]] || { ln -sf "$VOCAB" "$OUT/vocab.spm"; VOCAB="$OUT/vocab.spm"; }
head -200000 "$TSV" > "$OUT/small.tsv"

BASE=(-c /scripts/configs/student.base-memory.yml /scripts/configs/student.train.yml
      --vocabs "$VOCAB" "$VOCAB" --dim-vocabs 32000 32000
      --tsv --tsv-fields 3 --train-sets "$OUT/small.tsv"
      --devices 0 --disp-freq 20 --save-freq 999999 --valid-freq 999999
      --after-batches 120)

run() {  # name, then marian flags
  local name=$1; shift
  local log="$OUT/$name.log"
  if timeout 600 "$M" "${BASE[@]}" "$@" --model "$OUT/m_$name.npz" > "$log" 2>&1; then :; fi
  local wps sen ups
  wps=$(grep -aE "Up\. " "$log" | tail -3 | grep -oE "[0-9.]+ words/s" \
        | awk '{s+=$1;n++} END{if(n)printf "%.0f",s/n}')
  sen=$(grep -aE "Up\. " "$log" | tail -1 | grep -oE "Sen\. [0-9,]+" | tr -d "Sen.,: ")
  ups=$(grep -aE "Up\. " "$log" | tail -1 | grep -oE "Up\. [0-9]+" | grep -oE "[0-9]+")
  if [ -n "$wps" ]; then
    echo "$name: ${wps} w/s | sen/update $((sen / ups))"
  else
    echo "$name: FAIL $(grep -aiE 'error' "$log" | head -1 | cut -c1-100)"
  fi
}

exec > >(tee "$OUT/report.txt") 2>&1

echo "=== 1. fp32 + guided-alignment (baseline, the shipped config) ==="
run fp32_ga --workspace 9000

echo "=== 2. fp16 + guided-alignment (aborted on e8a1a25 — the whole point) ==="
run fp16_ga --fp16 --workspace 9000 --mini-batch 4000

echo "=== 3. workspace ceiling, now that ShapeSizeException is caught ==="
run fp16_ga_ws12 --fp16 --workspace 12000 --mini-batch 4000
run fp16_ga_ws18 --fp16 --workspace 18000 --mini-batch 4000
# -2000 = all GPU memory minus 2000MB; new in 1a743582.
run fp16_ga_wsauto --fp16 --workspace -2000 --mini-batch 4000

echo "=== 4. checkpoint for the conversion test ==="
ls -la "$OUT"/m_fp16_ga.npz* 2>/dev/null | head -3 || echo "no fp16 checkpoint produced"
