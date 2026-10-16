#!/bin/bash
# Ordered gather: concatenate shard outputs in argv order. Concatenated gzip
# streams are themselves valid gzip, so .gz shards need no special casing.
#
# Usage: gather_cat.sh OUT PART...
set -euo pipefail

OUT=$1; shift
cat "$@" > "$OUT"
