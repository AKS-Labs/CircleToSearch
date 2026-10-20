"""Canonical `deps` tuples for step definitions — one place, so a renamed script
or a new config is one edit, not a hunt across every flow that reused it.

WHY deps EXIST
The step key hashes only the wrapper `.sh` named in `script=`. That wrapper is an
argv adapter; the file that decides what the step DOES — the Python script it
calls, the marian config it passes — is invisible to the key unless listed here.
Two failures on 2026-07-22 came from exactly this: a placeholder-regex fix to
`registers.py` would have been served from the memo (the wrapper was unchanged),
and an `early-stopping-epsilon` line in `student.train.yml` (also unhashed) rode
onto a box and aborted marian. `deps` closes both by digesting these files into
the key, and `_key` raises loudly if a declared dep is missing.

Paths are relative to the scripts dir (`scripts/opus-trainer/`). Transitive local
imports are included — they are as invisible to the key as the entry script
(e.g. prep_data.py imports registers.py; eval_pair.py imports probe_check.py).

A test (test_step_deps.py) asserts every path here resolves to a real file, so a
typo fails in CI rather than at key-computation on a rented box.
"""

from __future__ import annotations

# --- data prep / KD-source draw ---
PREP = ("prep_data.py", "registers.py")
KD_MIX = ("sample_mix.py", "registers.py")

# --- teacher decode families ---
KD_DECODE = ("distill_data.py", "hf_offline.py")      # kd_decode.sh, nllb_flores.sh
NLLB_ALL = ("nllb_gate.py", "nllb_decode.py")
HY_GATE = ("hy_mt2_gate.py", "nllb_gate.py")
HY_MT2 = ("hy_mt2_run.py",)
GEMMA = ("gemma_fewshot.py",)
VLLM_KD = ("vllm_kd.py",)
BENCH_BATCH = ("bench_batch.py", "hf_offline.py")

# --- eval / probe ---
PROBE_GATE = ("probe_decode.py",)
EVAL_SCORE = ("eval_pair.py", "probe_check.py", "probe_review.py")
STUDENT_EVAL = ("benchmark_slimt.py",)

# --- KD post-decode ---
SELECT_BEST = ("extract_best.py",)

# --- pack ---
QUANTIZE = ("extract_stats.py", "configs/quantize.decoder.yml")

# --- training ---
# The three configs travel together: opustrainer perturbation, the base-memory
# architecture, and the schedule. train_student.sh reads all three; train_eval.sh
# additionally shells out to decode_flores.sh (the eval beam settings).
_STUDENT_CONFIGS = (
    "configs/opustrainer.student.yml",
    "configs/student.base-memory.yml",
    "configs/student.train.yml",
)
TRAIN_STUDENT = _STUDENT_CONFIGS
TRAIN_EVAL = ("train_student.sh", "decode_flores.sh", *_STUDENT_CONFIGS)
FINETUNE = (
    "configs/opustrainer.student.yml",
    "configs/student.base-memory.yml",
    "configs/student.finetune.yml",
)
# fp16_validate.sh / gpu2_validate.sh invoke marian directly (no train_student.sh),
# so only the two configs they pass.
VALIDATE = ("configs/student.base-memory.yml", "configs/student.train.yml")
BACKWARD = ("train_backward.sh", "configs/backward.s2s.yml")
