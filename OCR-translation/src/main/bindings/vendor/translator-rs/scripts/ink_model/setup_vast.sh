#!/usr/bin/env bash
# Provision a fresh GPU box (e.g. vast.ai) to train the ink model.
#
# Run this ON the box after copying the training files:
#   scp -i <key> -P <port> train.py gen_data.py model.py setup_vast.sh root@<host>:~/ink/
#   ssh -i <key> -p <port> root@<host> 'bash ~/ink/setup_vast.sh'
#
# PREFER A PYTORCH IMAGE. torch is ~3GB from the PyPI CDN (files.pythonhosted.org),
# and some vast instances cannot reach that CDN at all (apt mirror works, DNS works,
# but the Fastly CDN times out) — torch is then uninstallable. A PyTorch-template
# image ships torch preinstalled, so you only need the apt fonts below. This script
# skips any package already present, so it's a no-op for torch on such an image.
#
# Other notes baked in from a real run:
#   - Stock vast images can be bare Ubuntu (no torch, no pip, no fonts).
#   - astral.sh is often DNS-blocked while pypi.org resolves, so uv comes from pypi.
#   - The container CPU quota is far below the host core count; derive workers from
#     the cgroup, not nproc.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
# Target a specific interpreter (e.g. a venv) with PYTHON=…; defaults to system python3.
# On a Vast PyTorch image set PYTHON=/venv/main/bin/python so torch is found there and not
# reinstalled into the system.
PY="${PYTHON:-python3}"

echo "== apt: pip, fontconfig, dictionary, libraqm =="
apt-get update -qq
apt-get install -y -qq python3-pip fontconfig libraqm0 wamerican
# libraqm0: Pillow needs it for RAQM layout (Arabic cursive joining, Devanagari
# conjuncts, Tamil/Thai). Without it PIL silently falls back to basic layout and the
# shaped-script samples render as malformed, unjoined glyphs.

# Broad font set (mirrors a desktop's installed fonts) for thin→bold weight COVERAGE: the
# bold head's residual thin-FP was dominated by *unseen* fonts (0.3% on the ~11 training
# fonts vs 1.7% on 160 unfamiliar ones), so the fix is variety, not loss tweaks. These
# families span the full weight range (Lato/Ubuntu/Cantarell Thin→Black, URW base35,
# lmodern, full Noto). Loop with || true so a name missing on this distro doesn't abort.
echo "== installing broad font set for weight coverage =="
# Strict (no || true): a missing/renamed package aborts the script so we never silently
# train on a near-empty font set. If apt fails here, fix the package name for the distro.
apt-get install -y -qq \
  fonts-cantarell fonts-dejavu fonts-dejavu-core fonts-dejavu-extra \
  fonts-droid-fallback fonts-freefont-ttf fonts-hack fonts-inconsolata \
  fonts-lato fonts-liberation fonts-liberation2 \
  fonts-lmodern fonts-noto-core fonts-noto-extra fonts-noto-ui-core fonts-noto-ui-extra \
  fonts-noto-cjk fonts-noto-cjk-extra fonts-noto-mono fonts-tuffy fonts-ubuntu \
  fonts-urw-base35 fonts-vlgothic

# Giga-bold / display-black faces: the stock font set tops out around ExtraBold, so real
# poster/sign weights are under-trained and heavy condensed faces collapse. Most of these
# declare OS/2 weight 400 despite rendering black, which the measured-stroke label handles
# correctly. Archivo Black stays a held-out probe (probe_font.py loads it by path) for honest
# display eval; Anton is now TRAINED (the condensed-grotesque header style — STRONGER/NOT COFFEE
# — that the matte under-covered). wget per-font with || true so a dead URL doesn't abort.
echo "== fetching giga-bold display faces =="
GB=/usr/share/fonts/truetype/gigabold; mkdir -p "$GB"
GF=https://github.com/google/fonts/raw/main
for f in \
  ofl/alfaslabone/AlfaSlabOne-Regular.ttf ofl/titanone/TitanOne-Regular.ttf \
  ofl/changaone/ChangaOne-Regular.ttf ofl/luckiestguy/LuckiestGuy-Regular.ttf \
  ofl/bowlbyone/BowlbyOne-Regular.ttf ofl/sigmarone/SigmarOne-Regular.ttf \
  ofl/ultra/Ultra-Regular.ttf ofl/bungee/Bungee-Regular.ttf \
  ofl/passionone/PassionOne-Black.ttf ofl/bebasneue/BebasNeue-Regular.ttf \
  ofl/poppins/Poppins-Black.ttf ofl/lato/Lato-Black.ttf \
  ofl/barlow/Barlow-Black.ttf apache/blackopsone/BlackOpsOne-Regular.ttf \
  ofl/anton/Anton-Regular.ttf ofl/staatliches/Staatliches-Regular.ttf \
  ofl/oswald/static/Oswald-Bold.ttf ofl/oswald/static/Oswald-SemiBold.ttf \
  ofl/fjallaone/FjallaOne-Regular.ttf ofl/khand/Khand-Bold.ttf \
  ofl/squadaone/SquadaOne-Regular.ttf ofl/teko/static/Teko-Bold.ttf; do
  wget -q "$GF/$f" -O "$GB/$(basename "$f")" || true
done
fc-cache -f "$GB" >/dev/null 2>&1 || true
echo "   installed $(ls "$GB"/*.ttf 2>/dev/null | wc -l) giga-bold faces"

# Thin/light faces: the stock set bottoms out around ExtraLight (~0.042 stroke), with no
# true hairline. Thin text is exactly what the asym bold loss must never embolden, so the
# model needs genuine weight-100..300 faces to learn that boundary.
echo "== fetching thin/light faces =="
TL=/usr/share/fonts/truetype/thinlight; mkdir -p "$TL"
for f in \
  ofl/lato/Lato-Thin.ttf ofl/lato/Lato-Light.ttf \
  ofl/firasans/FiraSans-Thin.ttf ofl/firasans/FiraSans-ExtraLight.ttf ofl/firasans/FiraSans-Light.ttf \
  ofl/titilliumweb/TitilliumWeb-ExtraLight.ttf ofl/titilliumweb/TitilliumWeb-Light.ttf \
  ofl/hind/Hind-Light.ttf; do
  wget -q "$GF/$f" -O "$TL/$(basename "$f")" || true
done
fc-cache -f "$TL" >/dev/null 2>&1 || true
echo "   installed $(ls "$TL"/*.ttf 2>/dev/null | wc -l) thin/light faces"

# Hard coverage gate: the broad set should yield a few hundred Latin faces. If we somehow
# ended up with a near-empty pool (bad mirror, all packages skipped), fail now rather than
# silently train on 7 fonts and wonder why thin-FP is bad.
nlatin=$(fc-list :lang=en | wc -l)
echo "== font coverage: ${nlatin} latin faces =="
[ "$nlatin" -ge 150 ] || { echo "FATAL: only ${nlatin} latin faces — expected >=150; font install is broken"; exit 1; }

# Only fetch Python packages that are missing (torch is preinstalled on a PyTorch image).
missing=""
for mod_pkg in torch:torch numpy:numpy PIL:pillow scipy:scipy fontTools:fonttools cv2:opencv-python-headless; do
  mod=${mod_pkg%%:*}; pkg=${mod_pkg##*:}
  "$PY" -c "import $mod" 2>/dev/null || missing="$missing $pkg"
done
if [ -n "$missing" ]; then
  echo "== installing missing:$missing into $PY =="
  # uv targets the chosen interpreter with --python; fall back to that interpreter's pip.
  UV=$(command -v uv || echo /usr/local/bin/uv)
  if [ -x "$UV" ]; then
    UV_HTTP_TIMEOUT=180 "$UV" pip install --python "$PY" $missing
  else
    "$PY" -m pip install --default-timeout=120 --retries 10 $missing
  fi
else
  echo "== all python deps already present =="
fi

echo "== verify =="
"$PY" -c "import torch, numpy, PIL, scipy, cv2; print('py', '$PY', 'torch', torch.__version__, 'cuda', torch.cuda.is_available())"
"$PY" -c "import subprocess; n=len({l.split(':')[0] for l in subprocess.check_output(['fc-list',':lang=ko','file'],text=True).splitlines()}); print('cjk fonts:', n)"

# Worker count from the container's real CPU quota (cgroup v2 then v1), not nproc.
if [ -r /sys/fs/cgroup/cpu.max ]; then
  read -r q p < /sys/fs/cgroup/cpu.max
  [ "$q" = max ] && cores=$(nproc) || cores=$(( q / p ))
elif [ -r /sys/fs/cgroup/cpu/cpu.cfs_quota_us ]; then
  q=$(cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us); p=$(cat /sys/fs/cgroup/cpu/cpu.cfs_period_us)
  [ "$q" = -1 ] && cores=$(nproc) || cores=$(( q / p ))
else
  cores=$(nproc)
fi
workers=$(( cores > 1 ? cores - 1 : 1 ))

echo
echo "ready. ~${cores} usable cores. suggested run from ~/ink:"
echo "  python3 train.py --steps 8000 --batch 256 --workers ${workers} --lr 2.5e-3 --out ckpt"
echo "(batch*steps = strips seen; dial --steps to your strip budget)"
