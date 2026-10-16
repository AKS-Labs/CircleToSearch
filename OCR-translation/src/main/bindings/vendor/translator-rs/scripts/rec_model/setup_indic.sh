#!/usr/bin/env bash
# Bootstrap a fresh GPU box to fine-tune the MERGED Indic recognizer
# (Bengali + Gujarati + Kannada + Malayalam + Latin). Sibling of setup_server.sh
# (Hebrew); shares the recgen.py core. Stage first from the local machine:
#   scp -P <port> recgen.py gen_indic.py ../synth_core.py prep_corpus.py build_corpus.py \
#       paddle/indic_finetune.yml data/indic_corpus.txt root@<host>:/root/rec/
#   ssh -p <port> root@<host> 'bash /root/rec/setup_indic.sh'
# data/indic_corpus.txt is the RAW corpus (rebuilt from Leipzig if absent); build_corpus derives
# the balanced corpus + indic_latin_dict.txt (keys) at bootstrap. The committed keys are the
# canonical class list (for the bucket/Rust side); the box regenerates an identical one.
set -euo pipefail

REC_DIR=${REC_DIR:-/root/rec}
DATA_DIR=${DATA_DIR:-/dev/shm/indic}
PADDLEOCR_DIR=${PADDLEOCR_DIR:-/root/PaddleOCR}
CONFIG=${CONFIG:-$REC_DIR/indic_finetune.yml}
PRETRAINED_URL="https://paddle-model-ecology.bj.bcebos.com/paddlex/official_pretrained_model/PP-OCRv6_small_rec_pretrained.pdparams"
LEIPZIG="ben_wikipedia_2021_100K,guj_wikipedia_2021_100K,kan_wikipedia_2021_100K,mal_wikipedia_2021_100K,ben_newscrawl_2017_100K"

N_WORKERS=${N_WORKERS:-20}
N_PER=${N_PER:-18000}      # 20*18000 = 360K (4 scripts + Latin)
VAL_N=${VAL_N:-8000}
EPOCHS=${EPOCHS:-20}
BS=${BS:-512}

echo "=== [1/6] reclaim disk ==="
rm -rf /root/.cache/uv /root/.cache/pip /root/.cache/huggingface /root/.cache/torch /tmp/pip-* 2>/dev/null || true
# Reclaim /dev/shm: leaked paddle dataloader shm segments from any prior killed run, plus a
# stale sibling dataset dir (e.g. /dev/shm/heb left from a Hebrew run) — both fill the 15G tmpfs.
find /dev/shm -maxdepth 1 -name 'paddle_*' -exec rm -rf {} + 2>/dev/null || true
df -h / /dev/shm | tail -2

echo "=== [2/6] paddlepaddle-gpu (cu129) + GPU smoke ==="
if ! python3 -m pip --version >/dev/null 2>&1; then apt-get update -qq && apt-get install -y -qq python3-pip; fi
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

echo "=== [3/6] system libs + Indic fonts + repo + python deps ==="
apt-get update -qq
apt-get install -y -qq fontconfig libgl1 libglib2.0-0 libxcb1 libsm6 libxext6 libxrender1
# fonts-indic = Lohit (Bengali/Gujarati/Kannada/Malayalam), fonts-samyak + Noto add
# variety (the round-2 lesson: varied faces separate confusable glyphs). universe.
apt-get install -y -qq software-properties-common 2>/dev/null || true
add-apt-repository -y universe 2>/dev/null || true
apt-get update -qq || true
apt-get install -y -qq fonts-indic fonts-samyak fonts-noto-core fonts-noto-extra fonts-smc || echo "WARN: some font packages unavailable"
# Google Fonts Indic faces for real-world variety (Debian gives only ~10 families/
# script; book/print styles in real scans need more — the Hebrew Culmus lesson).
# Prefer the staged set ($REC_DIR/gf_indic, scp'd from the checkout) — the box-side
# clone of google/fonts is ~2.5GB and slow/unreliable.
if [ ! -d /usr/share/fonts/gf_indic ]; then
  if [ -d "$REC_DIR/gf_indic" ]; then
    cp -r "$REC_DIR/gf_indic" /usr/share/fonts/gf_indic
  else
    git clone --depth 1 -q https://github.com/google/fonts.git /tmp/gf || true
    mkdir -p /usr/share/fonts/gf_indic
    python3 - <<'PY' || true
import freetype, glob, shutil
reps = (0x0995, 0x0A95, 0x0C95, 0x0D15)  # bn/gu/kn/ml base consonant
n = 0
for f in glob.glob("/tmp/gf/ofl/**/*.ttf", recursive=True):
    try:
        face = freetype.Face(f)
    except Exception:
        continue
    if any(face.get_char_index(cp) != 0 for cp in reps):
        shutil.copy(f, "/usr/share/fonts/gf_indic/"); n += 1
print(f"harvested {n} google-fonts indic faces")
PY
    rm -rf /tmp/gf
  fi
fi
fc-cache -f >/dev/null 2>&1 || true
for l in bn gu kn ml; do echo "  $l fonts: $(fc-list :lang=$l | wc -l)"; done
miss=0; for l in bn gu kn ml; do [ "$(fc-list :lang=$l | wc -l)" -gt 0 ] || miss=1; done
[ "$miss" = 0 ] || { echo "FATAL: missing fonts for some Indic script; generation would hang"; exit 1; }
[ -d "$PADDLEOCR_DIR" ] || git clone --depth 1 -q https://github.com/PaddlePaddle/PaddleOCR.git "$PADDLEOCR_DIR"
uv pip install --system -r "$PADDLEOCR_DIR/requirements.txt" uharfbuzz freetype-py

echo "=== [4/6] pretrained v6 small weights ==="
mkdir -p "$REC_DIR/pretrain"
[ -f "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" ] || \
  wget -q -O "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" "$PRETRAINED_URL"

echo "=== [5/6] dataset -> $DATA_DIR ($((N_WORKERS*N_PER)) train + $VAL_N val) ==="
cd "$REC_DIR"
# indic_corpus.txt is the RAW Leipzig corpus (rebuilt via prep_corpus if absent). build_corpus
# turns it into the balanced/glyph-floored training corpus + the matching keys.txt in one pass:
# drops dead/archaic CTC classes, equalizes the four scripts (Bengali wiki+news is 2x the rest),
# and floors naturally-rare glyphs (native digits, danda, currency). keys + corpus emit together
# so they can't drift. The balanced corpus is a build artifact (not committed/staged).
[ -f indic_corpus.txt ] || python3 prep_corpus.py --charset-from gen_indic --download --out indic_corpus.txt --names "$LEIPZIG"
python3 build_corpus.py --module gen_indic --raw indic_corpus.txt \
  --out-corpus indic_corpus.bal.txt --out-keys indic_latin_dict.txt
rm -rf "$DATA_DIR" && mkdir -p "$DATA_DIR/images"
for k in $(seq 0 $((N_WORKERS-1))); do
  python3 gen_indic.py --out "$DATA_DIR" --n "$N_PER" --seed "$k" --prefix "w$k" --corpus indic_corpus.bal.txt --dict indic_latin_dict.txt >/dev/null 2>&1 &
done
python3 gen_indic.py --out "$DATA_DIR" --n "$VAL_N" --seed 1000 --prefix val --corpus indic_corpus.bal.txt --dict indic_latin_dict.txt >/dev/null 2>&1 &
wait || true   # a worker's non-zero exit must not abort the bootstrap (set -e)
cat "$DATA_DIR"/labels_w*.txt > "$DATA_DIR/train_list.txt"
cat "$DATA_DIR"/labels_val*.txt > "$DATA_DIR/val_list.txt"
echo "train=$(wc -l < "$DATA_DIR/train_list.txt") val=$(wc -l < "$DATA_DIR/val_list.txt")"

echo "=== [6/6] fine-tune (detached; tail /root/rec/train.log) ==="
cd "$PADDLEOCR_DIR"
nohup python3 tools/train.py -c "$CONFIG" \
  -o Global.epoch_num="$EPOCHS" Global.save_epoch_step=5 \
     Train.sampler.first_bs="$BS" Train.loader.batch_size_per_card="$BS" \
  > "$REC_DIR/train.log" 2>&1 &
echo "train PID $!  ->  tail -f $REC_DIR/train.log"
