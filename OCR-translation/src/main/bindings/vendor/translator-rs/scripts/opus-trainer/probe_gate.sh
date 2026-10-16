#!/bin/bash
# pipe step wrapper for probe_decode.py. PURE ARGV ADAPTER — no logic.
# Both directions go in one invocation so "one direction decoded" is not a
# runnable partial, and so the engine loads once.
#
# Usage: probe_gate.sh MODEL EN_SRC TL_SRC OUT_DIR
set -euo pipefail
python3 /scripts/probe_decode.py "$1" "$4" "$2:Filipino" "$3:English"
