#!/usr/bin/env bash
# Bootstrap a fresh (ephemeral) GPU box to fine-tune the PP-OCRv6 small recognizer.
# Tested on vast.ai RTX 5070 Ti (Blackwell sm_120), CUDA 13 driver, Ubuntu 22.04,
# torch preinstalled, ~16G overlay disk + /dev/shm.
#
# Stage the rec files first from the local machine, then run this on the server:
#   scp -i ~/.ssh/davidkey -P <port> \
#       gen_hebrew.py recgen.py ../synth_core.py prep_hebrew_corpus.py build_corpus.py \
#       paddle/hebrew_finetune.yml data/hebrew_corpus.txt \
#       root@<host>:/root/rec/
#   ssh -i ~/.ssh/davidkey -p <port> root@<host> 'bash /root/rec/setup_server.sh'
#
# data/hebrew_corpus.txt is the RAW corpus (rebuilt from Leipzig if absent); build_corpus
# derives the balanced corpus + hebrew_latin_dict.txt (keys) at bootstrap.
set -euo pipefail

REC_DIR=${REC_DIR:-/root/rec}
DATA_DIR=${DATA_DIR:-/dev/shm/heb}          # dataset in RAM: overlay disk is tiny
PADDLEOCR_DIR=${PADDLEOCR_DIR:-/root/PaddleOCR}
CONFIG=${CONFIG:-$REC_DIR/hebrew_finetune.yml}
PRETRAINED_URL="https://paddle-model-ecology.bj.bcebos.com/paddlex/official_pretrained_model/PP-OCRv6_small_rec_pretrained.pdparams"

# PERF SIZING (vast.ai/Docker box): `nproc` lies — the container is CFS time-quota-capped,
# not core-pinned. Check `cat /sys/fs/cgroup/cpu.max` (quota/period = real cores; one box was
# ~11.5) and `cpu.stat` nr_throttled. The quota is smeared across all visible cores, so `top`
# shows every one of the 24 at ~50% = the real ~12 fully used (NOT headroom; top's idle% is
# host-wide = other tenants). Size train num_workers (yml) + these gen workers to the QUOTA
# (~12), not nproc: more oversubscribes & throttles the driver, fewer starves the GPU.
# Don't tune from nvidia-smi GPU-util% either — it's "time a kernel ran", not SM busyness; a
# sawtooth to 0% at 70% "util" + ~50% power.draw is the GPU starving on the CPU pipeline, not
# compute. Use power.draw, avg_reader_cost vs avg_batch_cost, and the throttle counters.
N_WORKERS=${N_WORKERS:-20}                   # parallel generator processes
N_PER=${N_PER:-15000}                        # lines per worker (20*15000 = 300K)
VAL_N=${VAL_N:-8000}
# Non-text negatives default OFF: the confidence gate already cleanly separates
# garbage (<=0.6) from real text (>=0.93). Set NEG_FRAC>0 only to A/B the gate.
NEG_FRAC=${NEG_FRAC:-0}                        # fraction of non-text negatives in TRAIN (empty labels)
EPOCHS=${EPOCHS:-20}
BS=${BS:-512}                                 # 16G fits ~512 at 48px multiscale; bs256 left the GPU at ~50% power

echo "=== [1/6] reclaim disk (uv/pip/hf caches from any prior tenant) ==="
rm -rf /root/.cache/uv /root/.cache/pip /root/.cache/huggingface /root/.cache/torch /tmp/pip-* 2>/dev/null || true
# Reclaim /dev/shm: leaked paddle dataloader shm segments from any prior killed run fill the
# 15G tmpfs and OOM the next dataset regen ("No space left on device").
find /dev/shm -maxdepth 1 -name 'paddle_*' -exec rm -rf {} + 2>/dev/null || true
df -h / /dev/shm | tail -2

echo "=== [2/6] paddlepaddle-gpu (cu129, runs on Blackwell) + GPU smoke test ==="
if ! python3 -m pip --version >/dev/null 2>&1; then
  apt-get update -qq && apt-get install -y -qq python3-pip
fi
# uv resolves + installs the PaddleOCR dep tree far faster than pip (parallel downloads).
python3 -m pip install -q uv
if ! python3 -c "import paddle" 2>/dev/null; then
  uv pip install --system "paddlepaddle-gpu==3.2.0" --index-url https://www.paddlepaddle.org.cn/packages/stable/cu129/
fi
python3 - <<'PY'
import paddle
assert paddle.is_compiled_with_cuda() and paddle.device.cuda.device_count() >= 1, "no GPU paddle"
z = paddle.matmul(paddle.randn([1024, 1024]), paddle.randn([1024, 1024]))
print("paddle", paddle.__version__, "GPU matmul ok on", str(z.place))
PY

echo "=== [3/6] system libs for opencv + Hebrew book/serif fonts + repo + python deps ==="
apt-get update -qq
# fontconfig is REQUIRED (gen_hebrew uses fc-list); bare images may lack it.
apt-get install -y -qq fontconfig libgl1 libglib2.0-0 libxcb1 libsm6 libxext6 libxrender1
# fonts-noto-core = reliable Hebrew baseline (Noto Sans/Serif Hebrew). The Debian/Ubuntu
# package is `culmus` (NOT `fonts-culmus`) = classic faces (David CLM, Frank Ruehl CLM,
# ...); + fonts-sil-ezra serif, the round-2 fix for confusable letter pairs. Install each
# package separately so one missing name never aborts the whole apt-get (and the others).
apt-get install -y -qq software-properties-common 2>/dev/null || true
add-apt-repository -y universe 2>/dev/null || true
apt-get update -qq || true
for fpkg in culmus fonts-sil-ezra fonts-noto-core; do
  apt-get install -y -qq "$fpkg" || { echo "FATAL: Hebrew font package $fpkg failed to install"; exit 1; }
done
fc-cache -f >/dev/null 2>&1 || true
nheb=$(fc-list :lang=he | wc -l)
echo "hebrew fonts available: $nheb"
[ "$nheb" -gt 0 ] || { echo "FATAL: no Hebrew fonts; generation would hang"; exit 1; }
[ -d "$PADDLEOCR_DIR" ] || git clone --depth 1 -q https://github.com/PaddlePaddle/PaddleOCR.git "$PADDLEOCR_DIR"
# Keep the empty-label negatives: BaseRecLabelEncode drops len-0 labels by default
# (label_ops.py: "if len(text) == 0 or ..."). A len-0 CTC target (all blank) is valid.
sed -i 's/if len(text) == 0 or len(text) > self.max_text_len:/if len(text) > self.max_text_len:/' \
  "$PADDLEOCR_DIR/ppocr/data/imaug/label_ops.py"
uv pip install --system -r "$PADDLEOCR_DIR/requirements.txt" uharfbuzz python-bidi freetype-py

echo "=== [4/6] pretrained v6 small weights ==="
mkdir -p "$REC_DIR/pretrain"
[ -f "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" ] || \
  wget -q -O "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" "$PRETRAINED_URL"

echo "=== [5/6] dataset -> $DATA_DIR ($((N_WORKERS*N_PER)) train + $VAL_N val) ==="
# Stop any train from a previous run BEFORE wiping its dataset, or its dataloader
# crashes mid-read ("... does not exist!") and you get two runs fighting one GPU.
pkill -9 -f "tools/train.py" 2>/dev/null && sleep 2 || true
cd "$REC_DIR"
# hebrew_corpus.txt is the RAW Leipzig corpus (rebuilt via prep_hebrew_corpus if absent).
# build_corpus turns it into the glyph-floored training corpus + the matching keys.txt in one
# pass: drops any unrenderable class and floors corpus-starved-but-real glyphs (geresh, gershayim,
# currency, brackets). keys + corpus emit together so they can't drift. The balanced corpus is a
# build artifact (not committed/staged).
[ -f hebrew_corpus.txt ] || python3 prep_hebrew_corpus.py --download --out hebrew_corpus.txt
python3 build_corpus.py --module gen_hebrew --raw hebrew_corpus.txt \
  --out-corpus hebrew_corpus.bal.txt --out-keys hebrew_latin_dict.txt
rm -rf "$DATA_DIR" && mkdir -p "$DATA_DIR/images"
for k in $(seq 0 $((N_WORKERS-1))); do
  python3 gen_hebrew.py --out "$DATA_DIR" --n "$N_PER" --seed "$k" --prefix "w$k" --corpus hebrew_corpus.bal.txt --dict hebrew_latin_dict.txt --neg-frac "$NEG_FRAC" >/dev/null 2>&1 &
done
# val stays text-only so the acc metric is meaningful (empty-label CTC eval is ill-defined).
python3 gen_hebrew.py --out "$DATA_DIR" --n "$VAL_N" --seed 1000 --prefix val --corpus hebrew_corpus.bal.txt --dict hebrew_latin_dict.txt >/dev/null 2>&1 &
wait
cat "$DATA_DIR"/labels_w*.txt > "$DATA_DIR/train_list.txt"
cat "$DATA_DIR"/labels_val*.txt > "$DATA_DIR/val_list.txt"
echo "train=$(wc -l < "$DATA_DIR/train_list.txt") val=$(wc -l < "$DATA_DIR/val_list.txt")"

echo "=== [6/6] fine-tune (detached; tail /root/rec/train.log) ==="
# NOTE: PaddleOCR's BaseRecLabelEncode returns None for empty labels, so the
# dataset SILENTLY DROPS the negative (empty-label) samples. Before trusting
# NEG_FRAC, verify negatives survive — grep the loaded sample count, or patch
# ppocr/data/imaug/label_ops.py to keep len-0 labels (CTC all-blank target is valid).
cd "$PADDLEOCR_DIR"
nohup python3 tools/train.py -c "$CONFIG" \
  -o Global.epoch_num="$EPOCHS" Global.save_epoch_step=5 \
     Train.sampler.first_bs="$BS" Train.loader.batch_size_per_card="$BS" \
  > "$REC_DIR/train.log" 2>&1 &
echo "train PID $!  ->  tail -f $REC_DIR/train.log"
