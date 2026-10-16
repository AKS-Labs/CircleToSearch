#!/bin/bash
# Deploy pipe to the hub. This is the ONLY thing that moves code; `pipe` itself never
# syncs, so a read-only verb can't rewrite the scripts a live job has bind-mounted.
#
#   PIPE_REMOTE  ssh destination of the hub, e.g. user@host
#
# Where the code goes comes from the hub's own config, so the root is stated once.
# One-time hub setup (not done here): ~/.local/bin/pipe, ~/.config/pipe/config.toml,
# ~/.ssh/vast_pipe, ~/.config/vastai/vast_api_key.
set -euo pipefail
: "${PIPE_REMOTE:?set PIPE_REMOTE to the hub ssh destination, e.g. user@host}"

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT=$(ssh "$PIPE_REMOTE" 'sed -n "s/^root *= *\"\(.*\)\"/\1/p" ~/.config/pipe/config.toml')
if [ -z "$ROOT" ]; then
  echo "no root= in ~/.config/pipe/config.toml on $PIPE_REMOTE" >&2
  exit 1
fi
DEST="$ROOT/code"
STAMP="$(date '+%Y-%m-%d %H:%M %Z') $(git -C "$HERE" rev-parse --short HEAD)"
git -C "$HERE" diff --quiet || STAMP="$STAMP-dirty"

ssh "$PIPE_REMOTE" mkdir -p "$DEST/pipeline" "$DEST/opus-trainer"

rsync -a --delete --exclude '__pycache__' --exclude '.venv' \
  "$HERE/" "$PIPE_REMOTE:$DEST/pipeline/"
rsync -a --delete --exclude '__pycache__' --exclude 'venv' --exclude '.cache' \
  "$HERE/../opus-trainer/" "$PIPE_REMOTE:$DEST/opus-trainer/"

printf '%s\n' "$STAMP" | ssh "$PIPE_REMOTE" "cat > $DEST/pipeline/SYNCED"
ssh "$PIPE_REMOTE" "cd $DEST/pipeline && \$HOME/.local/bin/uv sync --quiet"

echo "synced: $STAMP"
