#!/usr/bin/env bash
# Build [ink-mask + det boxes | erased] pairs for every validation photo under a
# checkpoint. Usage: make_triptychs.sh <tag> <ckpt-path>
set -euo pipefail
cd "$(dirname "$0")"
TAG="$1"; CKPT="$2"
REPO=../..
EVAL="$REPO/smoke-out/ink-eval"
OUT="$EVAL/triptych/$TAG"
mkdir -p "$OUT"
PHOTOS=(coffee-label-dense colors cyrillic-warning-sign festival-poster-sideways \
  korean-bottle peterselie-bag screenshot-aperol-poster station-sign thai-banner \
  wall-typography-night warszawa-poster \
  kindle gluta primer sparkling sligro menu)
for name in "${PHOTOS[@]}"; do
  src=$(ls "$REPO/files/live-overlay/$name".* 2>/dev/null | head -1)
  strips="$EVAL/strips/$name/deskewed"
  [ -n "$src" ] && [ -d "$strips" ] || { echo "skip $name"; continue; }
  .venv/bin/python erase_full.py --image "$src" --strips "$strips" --ckpt "$CKPT" \
    --out "/tmp/erased-$name.png" --mask-out "/tmp/mask-$name.png" >/dev/null 2>&1
  convert "/tmp/mask-$name.png" "/tmp/erased-$name.png" -resize 900x900 \
    -bordercolor white -border 3 +append "$OUT/$name.png"
  echo "  $name"
done
echo "wrote pairs to $OUT"
