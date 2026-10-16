#!/bin/bash
# Deterministic K-way line split for sharded KD decode. split -n l/K never breaks
# a line and shard boundaries depend only on (file, K), so cat'ing shard outputs
# by index reproduces the input order exactly.
#
# Usage: split_kd.sh IN K OUT_DIR
set -euo pipefail

IN=$1; K=$2; OUT=$3
split -n "l/$K" -d "$IN" "$OUT/shard_"
wc -l "$OUT"/shard_*
