#!/usr/bin/env bash
# Bootstrap a fresh GPU box to fine-tune the GEORGIAN recognizer (Mkhedruli +
# Mtavruli + Latin). Sibling of setup_indic.sh / setup_server.sh; shares the
# recgen.py core. Stage first from the local machine:
#   scp -P <port> recgen.py gen_georgian.py ../synth_core.py prep_corpus.py build_corpus.py \
#       paddle/georgian_finetune.yml data/georgian_corpus.txt root@<host>:/root/rec/
#   ssh -p <port> root@<host> 'bash /root/rec/setup_georgian.sh'
# data/georgian_corpus.txt is the RAW corpus (rebuilt from Leipzig if absent); build_corpus
# derives the trimmed corpus + georgian_latin_dict.txt (keys) at bootstrap. The committed keys
# are the canonical class list (for the bucket/Rust side); the box regenerates an identical one.
set -euo pipefail

REC_DIR=${REC_DIR:-/root/rec}
DATA_DIR=${DATA_DIR:-/dev/shm/georgian}
PADDLEOCR_DIR=${PADDLEOCR_DIR:-/root/PaddleOCR}
CONFIG=${CONFIG:-$REC_DIR/georgian_finetune.yml}
PRETRAINED_URL="https://paddle-model-ecology.bj.bcebos.com/paddlex/official_pretrained_model/PP-OCRv6_small_rec_pretrained.pdparams"
# kat = ISO 639-3 for Georgian. kat_newscrawl_2017_100K does NOT exist; these three do.
LEIPZIG="kat_wikipedia_2021_100K,kat_news_2020_100K,kat-ge_web_2019_100K"

VENV=${VENV:-$REC_DIR/venv}
PY="$VENV/bin/python"

N_WORKERS=${N_WORKERS:-20}
N_PER=${N_PER:-15000}      # 20*15000 = 300K (one script + Latin; matches the Hebrew scale)
VAL_N=${VAL_N:-8000}
EPOCHS=${EPOCHS:-20}
BS=${BS:-512}

echo "=== [1/6] reclaim disk ==="
rm -rf /root/.cache/uv /root/.cache/pip /root/.cache/huggingface /root/.cache/torch /tmp/pip-* 2>/dev/null || true
# Reclaim /dev/shm: leaked paddle dataloader shm segments from any prior killed run, plus a
# stale sibling dataset dir (e.g. /dev/shm/indic left from an Indic run) — both fill the tmpfs.
find /dev/shm -maxdepth 1 -name 'paddle_*' -exec rm -rf {} + 2>/dev/null || true
df -h / /dev/shm | tail -2

echo "=== [2/6] uv venv + paddlepaddle-gpu (cu129) + GPU smoke ==="
# uv's standalone installer needs no pip, which keeps this off Ubuntu 24.04's PEP 668
# externally-managed system Python entirely, and everything lands in a venv that uv
# resolves and installs into far faster than pip.
command -v uv >/dev/null 2>&1 || curl -LsSf https://astral.sh/uv/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
[ -x "$PY" ] || uv venv "$VENV"
# setuptools must be installed explicitly. A bare uv venv has none, and on Python 3.12+ `uv
# venv --seed` seeds pip ONLY — uv dropped setuptools/wheel seeding there. paddle imports
# setuptools unconditionally from utils/cpp_extension, so `import paddle` dies without it.
uv pip install --python "$PY" -q setuptools
if ! "$PY" -c "import paddle" 2>/dev/null; then
  uv pip install --python "$PY" "paddlepaddle-gpu==3.2.0" --index-url https://www.paddlepaddle.org.cn/packages/stable/cu129/
fi
"$PY" - <<'PY'
import paddle
assert paddle.is_compiled_with_cuda() and paddle.device.cuda.device_count() >= 1, "no GPU paddle"
z = paddle.matmul(paddle.randn([1024, 1024]), paddle.randn([1024, 1024]))
print("paddle", paddle.__version__, "GPU matmul ok on", str(z.place))
PY

echo "=== [3/6] system libs + Georgian fonts + repo + python deps ==="
apt-get update -qq
apt-get install -y -qq fontconfig libgl1 libglib2.0-0 libxcb1 libsm6 libxext6 libxrender1
apt-get install -y -qq software-properties-common 2>/dev/null || true
add-apt-repository -y universe 2>/dev/null || true
apt-get update -qq || true
# fonts-bpg-georgian (universe) is the whole ballgame: Debian's Noto Georgian is 2 families
# and is a script-only SUBSET carrying no Latin and no digits, while BPG is what Georgian
# print and street signage actually use. It adds ~17 designs, all with Latin+digits, and its
# three Caps families supply Mtavruli shapes (gen_georgian routes them; see GEORGIAN.md).
# This is the Georgian equivalent of fonts-culmus for Hebrew — the round-2 font lesson,
# applied before round 1 rather than after it.
apt-get install -y -qq fonts-bpg-georgian fonts-noto-core fonts-noto-extra || echo "WARN: some font packages unavailable"
fc-cache -f >/dev/null 2>&1 || true
echo "  ka fonts: $(fc-list :lang=ka | wc -l) files, $(fc-list :lang=ka family | sed 's/,.*//' | sort -u | wc -l) families"
[ -d "$PADDLEOCR_DIR" ] || git clone --depth 1 -q https://github.com/PaddlePaddle/PaddleOCR.git "$PADDLEOCR_DIR"
uv pip install --python "$PY" -r "$PADDLEOCR_DIR/requirements.txt" uharfbuzz freetype-py

echo "=== [4/6] pretrained v6 small weights ==="
mkdir -p "$REC_DIR/pretrain"
[ -f "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" ] || \
  wget -q -O "$REC_DIR/pretrain/PP-OCRv6_small_rec_pretrained.pdparams" "$PRETRAINED_URL"

echo "=== [5/6] dataset -> $DATA_DIR ($((N_WORKERS*N_PER)) train + $VAL_N val) ==="
cd "$REC_DIR"
[ -f georgian_corpus.txt ] || "$PY" prep_corpus.py --charset-from gen_georgian --download --out georgian_corpus.txt --names "$LEIPZIG"
"$PY" build_corpus.py --module gen_georgian --raw georgian_corpus.txt \
  --out-corpus georgian_corpus.bal.txt --out-keys georgian_latin_dict.txt
# Renderability gate. A missing font pool does not fail loudly on its own: plan_runs returns
# None, sample() retries and moves on, and the line is dropped silently — so a font gap shows
# up only as a missing co-occurrence in the data. Assert the canonical line shapes render
# BEFORE spending an hour generating, and name the shape that failed.
"$PY" - <<'PY'
import random, sys
import recgen, gen_georgian
spec = gen_georgian._build_spec()
rng = random.Random(0)
cases = {
    "Mkhedruli": "ქუჩა",
    "Mkhedruli + Latin/digits": "ქუჩა info@ge.ge 25",
    "Mtavruli": "ᲥᲣᲩᲐ",
    "Mtavruli + digits": "ᲥᲣᲩᲐ 25",
    "Mtavruli + lari": "ᲥᲣᲩᲐ 25₾",
    "Latin only": "WiFi 24/7",
}
bad = [k for k, v in cases.items() if recgen.plan_runs(v, spec, rng) is None]
caps = len(spec.font_remap)
print(f"  renderable: {len(cases) - len(bad)}/{len(cases)}   caps faces routed to Mtavruli: {caps}")
if bad:
    sys.exit(f"FATAL: no font set renders {bad} — install fonts-bpg-georgian / fonts-noto-core")
PY
rm -rf "$DATA_DIR" && mkdir -p "$DATA_DIR/images"
for k in $(seq 0 $((N_WORKERS-1))); do
  "$PY" gen_georgian.py --out "$DATA_DIR" --n "$N_PER" --seed "$k" --prefix "w$k" --corpus georgian_corpus.bal.txt --dict georgian_latin_dict.txt >/dev/null 2>&1 &
done
"$PY" gen_georgian.py --out "$DATA_DIR" --n "$VAL_N" --seed 1000 --prefix val --corpus georgian_corpus.bal.txt --dict georgian_latin_dict.txt >/dev/null 2>&1 &
wait || true   # a worker's non-zero exit must not abort the bootstrap (set -e)
cat "$DATA_DIR"/labels_w*.txt > "$DATA_DIR/train_list.txt"
cat "$DATA_DIR"/labels_val*.txt > "$DATA_DIR/val_list.txt"
echo "train=$(wc -l < "$DATA_DIR/train_list.txt") val=$(wc -l < "$DATA_DIR/val_list.txt")"
# Mtavruli reaches the set only through gen_georgian's uppercase pass, and it is the one
# thing round 1 is testing, so surface its share instead of discovering a zero after training.
"$PY" - <<PY
rows = [l.split("\t")[1] for l in open("$DATA_DIR/train_list.txt", encoding="utf-8") if "\t" in l]
mt = [t for t in rows if any(0x1C90 <= ord(c) <= 0x1CBF for c in t)]
mix = [t for t in mt if any(c.isascii() and c.isalnum() for c in t)]
print(f"  Mtavruli lines: {len(mt)} ({100*len(mt)/max(len(rows),1):.1f}%), of which with Latin/digits: {len(mix)}")
PY

echo "=== [6/6] fine-tune (detached; tail /root/rec/train.log) ==="
cd "$PADDLEOCR_DIR"
nohup "$PY" tools/train.py -c "$CONFIG" \
  -o Global.epoch_num="$EPOCHS" Global.save_epoch_step=5 \
     Train.sampler.first_bs="$BS" Train.loader.batch_size_per_card="$BS" \
  > "$REC_DIR/train.log" 2>&1 &
echo "train PID $!  ->  tail -f $REC_DIR/train.log"
