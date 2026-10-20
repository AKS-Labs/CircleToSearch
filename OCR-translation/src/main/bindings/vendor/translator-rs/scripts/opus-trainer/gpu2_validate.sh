#!/bin/bash
# 1-GPU vs 2-GPU on one box, with per-GPU utilisation sampled during each run.
#
# NOTES' efficiency gate, which this exists to satisfy: per-GPU util must be >80%
# EACH (reject a 25%-idle second card) and throughput 1.7-1.9x vs 1 GPU. The named
# risk is that OpusTrainer's single stdin pipe starves 2+ GPUs. Measured
# 2026-07-21: it used 13-16% of one core to feed 142k w/s, so ~425k w/s wants
# ~45-50% of a core — tight but plausible. Caveat: every throughput number so far
# came from marian reading --train-sets directly, NOT through the pipe, so the
# producer has never actually been measured under load.
#
# Usage: gpu2_validate.sh TRAIN_TSV VOCAB OUT_DIR
set -uo pipefail
TSV=$1; VOCAB=$2; OUT=$3
mkdir -p "$OUT"
exec > >(tee "$OUT/report.txt") 2>&1
M=/usr/local/bin/marian
[[ "$VOCAB" == *.spm ]] || { ln -sf "$VOCAB" "$OUT/vocab.spm"; VOCAB="$OUT/vocab.spm"; }

echo "=== box ==="
nvidia-smi --query-gpu=index,name,memory.total --format=csv,noheader
NGPU=$(nvidia-smi --list-gpus | wc -l)
echo "GPUs visible: $NGPU"

BASE=(-c /scripts/configs/student.base-memory.yml /scripts/configs/student.train.yml
      --vocabs "$VOCAB" "$VOCAB" --dim-vocabs 32000 32000
      --tsv --tsv-fields 3 --train-sets "$TSV"
      --disp-freq 25 --save-freq 999999 --valid-freq 999999 --after-batches 200
      --fp16 --workspace 12000 --mini-batch 4000)

run() {
  local name=$1; shift
  local log="$OUT/$name.log"
  nvidia-smi --query-gpu=index,utilization.gpu,power.draw --format=csv,noheader,nounits \
    -lms 500 > "$OUT/$name.util.csv" 2>/dev/null &
  local smi=$!
  timeout 900 "$M" "${BASE[@]}" "$@" --model "$OUT/m_$name.npz" > "$log" 2>&1
  kill $smi 2>/dev/null; wait $smi 2>/dev/null
  local wps
  wps=$(grep -aE "Up\. " "$log" | tail -4 | grep -oE "[0-9.]+ words/s" \
        | awk '{s+=$1;n++} END{if(n)printf "%.0f",s/n}')
  if [ -z "$wps" ]; then
    echo "$name: FAIL $(grep -aiE 'error|abort' "$log" | head -1 | cut -c1-110)"
    return
  fi
  echo "$name: ${wps} w/s"
  python3 - "$OUT/$name.util.csv" <<'PY'
import sys, collections
per = collections.defaultdict(list)
for line in open(sys.argv[1]):
    f = [x.strip() for x in line.split(',')]
    if len(f) >= 3 and f[0].isdigit():
        per[int(f[0])].append((int(f[1]), float(f[2])))
for idx in sorted(per):
    u = [a for a, _ in per[idx]]; p = [b for _, b in per[idx]]
    # skip the first 20% (fitting/warmup) so the mean describes steady state
    k = len(u) // 5
    u, p = u[k:], p[k:]
    if not u: continue
    idle = 100 * sum(1 for x in u if x < 50) / len(u)
    print(f"    gpu{idx}: util mean {sum(u)/len(u):5.1f}%  power {sum(p)/len(p):6.1f}W  "
          f"below-50% {idle:4.1f}% of samples")
PY
}

echo; echo "=== 1 GPU (fp16 + guided-alignment, ws12000) ==="
run gpu1 --devices 0
if [ "$NGPU" -ge 2 ]; then
  echo; echo "=== 2 GPU (data-parallel, sync-sgd) ==="
  run gpu2 --devices 0 1 --sync-sgd
else
  echo; echo "SECOND GPU ABSENT — box is single-GPU, 2-GPU leg skipped"
fi
