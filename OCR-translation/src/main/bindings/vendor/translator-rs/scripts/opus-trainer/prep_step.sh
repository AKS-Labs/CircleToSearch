#!/bin/bash
# pipe step wrapper for prep_data.py. PURE ARGV ADAPTER — it maps pipe's
# positional step args onto flags and does NOTHING ELSE.
#
# It used to also paste the pool, rename the vocab to .spm and delete
# intermediates. That made a PARTIAL prep runnable: `prep_data.py` on its own
# produced two .gz files and a .model, which looks like success and is not — and
# on 2026-07-20 the tl corpus was built exactly that way, losing the src/tgt
# pairing (so build_kd_source could not emit kd_ref) and the marian-loadable
# vocab extension. Those steps now live in prep_data.py's finish(), so the
# complete step is the only thing that can run.
#
# JOBS is explicit because nproc inside a --cpus-limited container reports the
# host's cores, not the quota (the vast-perf-traps lesson).
#
# VOCAB_MODE: reuse (an existing joint vocab is seeded in) | train (a NEW pair
# has none; a joint SPM is pair-specific and cannot be borrowed).
#
# Usage: prep_step.sh TGT_LANG JOBS OUT_DIR [SRC=en] [VOCAB_MODE=reuse] [ONLY=]
set -euo pipefail

TGT=$1; JOBS=$2; OUT=$3; SRC=${4:-en}; VOCAB_MODE=${5:-reuse}; ONLY=${6:-}

case "$VOCAB_MODE" in
  reuse) SPM_ARGS=(--skip-spm) ;;
  train) SPM_ARGS=() ;;
  *) echo "VOCAB_MODE must be 'reuse' or 'train', got '$VOCAB_MODE'" >&2; exit 2 ;;
esac

ONLY_ARGS=()
[ -n "$ONLY" ] && ONLY_ARGS=(--only "$ONLY")

python3 /scripts/prep_data.py --lang "$TGT" --src "$SRC" \
  --workdir "$OUT/work" --out-dir "$OUT" --jobs "$JOBS" \
  "${SPM_ARGS[@]}" "${ONLY_ARGS[@]}" --lid-model /opt/lid.176.ftz
