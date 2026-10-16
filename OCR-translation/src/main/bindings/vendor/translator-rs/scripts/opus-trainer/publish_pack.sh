#!/bin/bash
# Publishes one verified slimt pack: bucket files, catalog entry carrying the
# metrics measured on the int8 artifact, regenerated index, CDN sync, and a live
# fetch of the published URLs to prove the deploy landed.
#
# NOT armed automatically. It publishes to a public CDN, which is a release, so
# it needs a person to fire it:
#
#   publish_pack.sh --pair ka-en --pack /nvme2/prom/enka2/pack.kaen.ft5 \
#                   --label lmt60_distill_20260902 --confirm
#
# The pair decides the file infix and the catalog key; the label is the dated
# directory under the pair, and a NEW label per release is what keeps the
# previous pack addressable at its own URLs.
#
# The gate is a PACK_OK file the scoring step writes into the bigserver pack
# directory only after the selection rule passed. This script never decides
# whether a pack is good, it only refuses to publish one that was not judged.
#
# Mutual exclusion is a claim file; there is no pgrep anywhere, because the
# wrapper that launches this carries every pattern one could match on.
set -u
BIG=david@192.168.2.10
REPO=/home/david/AndroidStudioProjects/Translator
BUCKET=/home/david/AndroidStudioProjects/bucket
BASEURL=https://offline-translator.davidv.dev
HERE=/home/david/git/translator-rs/scripts/opus-trainer
CLAIM=$HERE/.publish.claim
LOG=$HERE/data/DEPLOY_STATE.md

PAIR=""; PACK=""; DATED=""; CONFIRM=0
while [ $# -gt 0 ]; do
  case "$1" in
    --pair)  PAIR=$2; shift 2 ;;
    --pack)  PACK=$2; shift 2 ;;
    --label) DATED=$2; shift 2 ;;
    --confirm) CONFIRM=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$PAIR" in
  ??-??) SRC=${PAIR%-*}; TRG=${PAIR#*-}; INFIX=$SRC$TRG ;;
  *) echo "--pair must look like ka-en" >&2; exit 2 ;;
esac
[ -n "$PACK" ] || { echo "--pack is required" >&2; exit 2; }
[ -n "$DATED" ] || { echo "--label is required" >&2; exit 2; }
if [ "$CONFIRM" != 1 ]; then
  echo "This publishes $PAIR to the live CDN. Re-run with --confirm to do it." >&2
  exit 2
fi

STAGE=$HERE/data/publish.$INFIX.$DATED
DEST=$BUCKET/translation/1/models/$PAIR/$DATED/exported
MODEL=model.$INFIX.intgemm.alphas.bin
LEX=lex.50.50.$INFIX.s2t.bin
VOCAB=vocab.$INFIX.spm

state() { printf '%s  %s\n' "$(date -u +%FT%TZ)" "$*" >> "$LOG"; }
fail()  { state "**STOPPED** $*"; state "_nothing was published_"; rm -f "$CLAIM"; exit 1; }

if [ -e "$CLAIM" ]; then
  echo "claim exists holding $(cat "$CLAIM"); refusing to start a second publisher" >&2
  exit 1
fi
echo "$$ $PAIR $(date -u +%FT%TZ)" > "$CLAIM"
trap 'rm -f "$CLAIM"' EXIT

state "### publish_pack $PAIR $DATED started (pid $$)"

if [ -e "$DEST" ]; then
  fail "$DEST already exists; a republish would overwrite a released pack, pick a new --label"
fi

# ---------- stage 1: the pack must have passed the bigserver gate ----------
ssh -o ConnectTimeout=20 "$BIG" "test -f $PACK/PACK_OK" \
  || fail "no PACK_OK in $PACK; the pack has not passed the publish gate"
state "PACK_OK confirmed on bigserver: $(ssh -o ConnectTimeout=20 "$BIG" "tr '\n' ' ' < $PACK/PACK_OK")"

# ---------- stage 2: pull the pack and its measured metrics ----------
mkdir -p "$STAGE"
rsync -a "$BIG:$PACK/out/" "$STAGE/" || fail "could not pull the pack"
scp -q "$BIG:$PACK/publish.json" "$STAGE/publish.json" || fail "could not pull publish.json"
for f in "$MODEL.gz" "$LEX.gz" "$VOCAB.gz"; do
  [ -s "$STAGE/$f" ] || fail "pack file $f is missing or empty"
done
state "pulled pack: $(cd "$STAGE" && ls -la ./*.gz | awk '{print $9"="$5}' | tr '\n' ' ')"

# The app verifies the DECOMPRESSED model against these, so re-measure them here
# rather than trusting the digest the producing host reported.
gunzip -c "$STAGE/$MODEL.gz" > "$STAGE/$MODEL"
local_size=$(stat -c %s "$STAGE/$MODEL")
local_hash=$(sha256sum "$STAGE/$MODEL" | cut -d' ' -f1)
python3 - "$STAGE/publish.json" "$local_size" "$local_hash" "$PAIR" <<'PY' \
  || fail "the pack's publish.json did not survive the transfer"
import json, sys
p = json.load(open(sys.argv[1]))
size, digest, pair = int(sys.argv[2]), sys.argv[3], sys.argv[4]
if p["uncompressedSize"] != size or p["uncompressedHash"] != digest:
    print(f"MISMATCH size {p['uncompressedSize']} vs {size}, "
          f"hash {p['uncompressedHash']} vs {digest}")
    raise SystemExit(1)
if p["pair"] != pair:
    print(f"publish.json is for {p['pair']}, not {pair}")
    raise SystemExit(1)
for k in ("chrfpp", "comet22"):
    if not isinstance(p.get(k), (int, float)):
        print(f"publish.json has no numeric {k}")
        raise SystemExit(1)
PY
state "uncompressed model re-verified locally: $local_size bytes, sha256 $local_hash"

# ---------- stage 3: put the files in the bucket ----------
mkdir -p "$DEST"
cp "$STAGE"/*.gz "$DEST/"
state "bucket files placed under translation/1/models/$PAIR/$DATED/exported/"

# ---------- stage 4: catalog entry ----------
cd "$REPO" || fail "no repo at $REPO"
cp data_sources/custom_models.json "$STAGE/custom_models.json.bak"
python3 - "$STAGE/publish.json" "$DATED" "$PAIR" "$SRC" "$TRG" "$INFIX" <<'PY' \
  || fail "could not write the catalog entry"
import json, sys
from pathlib import Path
pub = json.load(open(sys.argv[1]))
dated, pair, src, trg, infix = sys.argv[2:7]
p = Path("data_sources/custom_models.json")
doc = json.loads(p.read_text(encoding="utf-8"))
base = f"models/{pair}/{dated}/exported"
doc["models"][pair] = [{
    "architecture": "base-memory",
    "releaseStatus": "Release",
    "sourceLanguage": src,
    "targetLanguage": trg,
    "files": {
        "lexicalShortlist": {"path": f"{base}/lex.50.50.{infix}.s2t.bin.gz"},
        "model": {"path": f"{base}/model.{infix}.intgemm.alphas.bin.gz",
                  "uncompressedSize": pub["uncompressedSize"],
                  "uncompressedHash": pub["uncompressedHash"]},
        "vocab": {"path": f"{base}/vocab.{infix}.spm.gz"},
    },
    "metrics": {"flores200-plus": {"chrfpp": pub["chrfpp"], "comet22": pub["comet22"]}},
}]
p.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
PY
state "catalog: $PAIR set to $DATED with the int8-measured metrics"

# ---------- stage 5: regenerate the index ----------
# download_bucket.py is deliberately skipped: the local mirror is complete and
# the new pack exists only locally, so a download pass has nothing to fetch and
# could only fail on the very files being published.
python3 generate_index.py > "$STAGE/index_internal.log" 2>&1 \
  || fail "generate_index.py (internal) failed, see $STAGE/index_internal.log"
python3 generate_index.py --mode public --base-url "$BASEURL" > "$STAGE/index_public.log" 2>&1 \
  || fail "generate_index.py (public) failed, see $STAGE/index_public.log"
cp app/src/main/assets/index_v6.json "$BUCKET/index_v6.json"
state "index_v6.json regenerated ($(stat -c %s "$BUCKET/index_v6.json") bytes)"
grep -q "$PAIR/$DATED" "$BUCKET/index_v6.json" || fail "$PAIR/$DATED never reached index_v6.json"

# ---------- stage 6: sync and purge ----------
cd /home/david/AndroidStudioProjects || fail "no bucket parent"
bash sync.sh > "$STAGE/sync.log" 2>&1 || fail "bucket sync failed, see $STAGE/sync.log"
state "bucket synced; sync log tail: $(tail -3 "$STAGE/sync.log" | tr '\n' ' ')"

# ---------- stage 7: prove it is live ----------
ok=1
URLBASE=$BASEURL/translation/1/models/$PAIR/$DATED/exported
for u in "$BASEURL/index_v6.json" "$URLBASE/$MODEL.gz" "$URLBASE/$LEX.gz" "$URLBASE/$VOCAB.gz"; do
  code=$(curl -s -o /dev/null -w '%{http_code}' -r 0-1023 "$u")
  state "GET $u -> $code"
  case "$code" in 20*) ;; *) ok=0 ;; esac
done
remote_hash=$(curl -s "$URLBASE/$MODEL.gz" | gunzip -c | sha256sum | cut -d' ' -f1)
state "published model re-downloaded, uncompressed sha256 $remote_hash"
[ "$remote_hash" = "$local_hash" ] || { state "**WARNING** published digest differs from the local pack"; ok=0; }
if curl -s "$BASEURL/index_v6.json" | grep -q "$PAIR/$DATED"; then
  state "live index_v6.json points $PAIR at $DATED"
else
  state "**WARNING** live index_v6.json does not mention $PAIR/$DATED yet (CDN purge may lag)"
  ok=0
fi

[ "$ok" = 1 ] && state "### DEPLOYED -- $PAIR $DATED is live" || state "### DEPLOY INCOMPLETE -- read the warnings above"
scp -q "$LOG" "$BIG:$PACK/DEPLOY_STATE.md" 2>/dev/null || true
