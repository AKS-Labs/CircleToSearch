#!/bin/bash
# Writes the publish gate for one pack: publish.json (the catalog metrics, both
# measured on the int8 artifact that will actually ship) and PACK_OK, which is
# the only thing publish_pack.sh will accept as evidence that a pack was judged.
#
# Size and hash are taken from the pack's own meta.json rather than re-measured
# here, so the number in the catalog is the number the packer produced; the
# publisher then re-measures both after the transfer and refuses a mismatch.
#
# THE NUMBER-FIDELITY CLAUSE
# chrF++ and COMET22 are both blind to a dropped or changed figure: ft5 lost a
# figure on twice as many ka->en holdout rows as the pack it was replacing and
# its chrF gap was mostly currency spelling (ka_findings.md 31). So the gate also
# needs `number_fidelity.py --json` reports for the CANDIDATE and for the pack
# that is live today, over the same slices, and refuses when the candidate loses
# or corrupts a figure on more lines than live does. Ties pass: this is a
# no-regression clause, not a target. Both reports are copied into the pack
# directory, so the evidence that a pack was gated travels with the pack.
#
# Everything else stays a human judgement recorded in --verdict. This script
# never decides that a pack is good.
#
# Usage:
#   gate_pack.sh --pair ka-en --pack /nvme2/prom/enka2/pack.kaen.ft6 \
#                --chrfpp 48.90 --comet22 0.8271 \
#                --fidelity out/fid.ft6.json --fidelity-live out/fid.live.json \
#                --verdict "one line saying what passed"
#
#   --chrfpp        chrF++ on FLORES devtest, decoded from the int8 pack with its table
#   --comet22       COMET22 on the same decode, as a fraction (0.8321), as the catalog stores it
#   --fidelity      number_fidelity.py --json report for the candidate pack's decodes
#   --fidelity-live the same report for the live pack, over the same slices
#   --verdict       one line saying what passed, recorded in PACK_OK
set -euo pipefail
BIG=david@192.168.2.10

PAIR=""; PACK=""; CHRFPP=""; COMET=""; FID=""; FIDLIVE=""; VERDICT=""
while [ $# -gt 0 ]; do
  case "$1" in
    --pair)          PAIR=$2; shift 2 ;;
    --pack)          PACK=$2; shift 2 ;;
    --chrfpp)        CHRFPP=$2; shift 2 ;;
    --comet22)       COMET=$2; shift 2 ;;
    --fidelity)      FID=$2; shift 2 ;;
    --fidelity-live) FIDLIVE=$2; shift 2 ;;
    --verdict)       VERDICT=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
for required in PAIR PACK CHRFPP COMET FID FIDLIVE VERDICT; do
  [ -n "${!required}" ] || { echo "--${required,,} is required" >&2; exit 2; }
done
for f in "$FID" "$FIDLIVE"; do
  [ -s "$f" ] || { echo "no number-fidelity report at $f" >&2; exit 2; }
done

python3 - "$FID" "$FIDLIVE" <<'PY' || exit 1
import json, sys
cand, live = (json.load(open(p, encoding="utf-8")) for p in sys.argv[1:3])
cs, ls = cand["slices"], live["slices"]
if set(cs) != set(ls):
    raise SystemExit(f"the two fidelity reports cover different slices: "
                     f"candidate {sorted(cs)} against live {sorted(ls)}")
worse = [(name, cs[name]["bad"], ls[name]["bad"]) for name in sorted(cs)
         if cs[name]["bad"] > ls[name]["bad"]]
for name in sorted(cs):
    c, l = cs[name], ls[name]
    print(f"number fidelity {name:12} n={c['scored']:4}  candidate {c['bad']:3} bad "
          f"({c['omitted']} omitted, {c['corrupted']} corrupted)   live {l['bad']:3} bad "
          f"({l['omitted']} omitted, {l['corrupted']} corrupted)")
if worse:
    for name, c, l in worse:
        print(f"REFUSED: {name} loses or corrupts a figure on {c} lines against live's {l}")
    raise SystemExit(1)
print(f"number fidelity total: candidate {cand['total']['bad']} bad against live "
      f"{live['total']['bad']}, no slice worse")
PY

meta=$(ssh -o ConnectTimeout=20 "$BIG" "cat $PACK/out/meta.json")
python3 - "$PAIR" "$CHRFPP" "$COMET" "$VERDICT" <<PY > /tmp/publish.$$.json
import json, sys
meta = json.loads('''$meta''')
pair, chrfpp, comet, verdict = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), sys.argv[4]
if not 0 < comet < 1:
    raise SystemExit(f"comet22 must be the fraction the catalog stores, got {comet}")
json.dump({"pair": pair, "chrfpp": round(chrfpp, 2), "comet22": round(comet, 4),
           "uncompressedSize": meta["uncompressedSize"],
           "uncompressedHash": meta["uncompressedHash"],
           "verdict": verdict}, sys.stdout, indent=2)
PY
scp -q /tmp/publish.$$.json "$BIG:$PACK/publish.json"
rm -f /tmp/publish.$$.json
scp -q "$FID" "$BIG:$PACK/number_fidelity.candidate.json"
scp -q "$FIDLIVE" "$BIG:$PACK/number_fidelity.live.json"
ssh -o ConnectTimeout=20 "$BIG" "printf '%s  %s\n' \"\$(date -u +%FT%TZ)\" \"$VERDICT\" > $PACK/PACK_OK"
ssh -o ConnectTimeout=20 "$BIG" "cat $PACK/publish.json; echo; cat $PACK/PACK_OK"
