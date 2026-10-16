#!/usr/bin/env bash
# tl->en finetune diagnostic: decode the PRE-finetune KD checkpoint and the
# POST-finetune checkpoint on (1) FLORES-200 devtest and (2) a held-out KD-domain
# slice, then score both with the same chrF++/spBLEU harness as the teacher gates.
#
# Reads:  pre = ckpt.tlen.best.npz   post = model_ft.tlen.best.npz
# Decode: marian-decoder (fp32, no shortlist/int8) inside marian-train:cpu on CPU.
#
# Interpretation:
#   FLORES pre ~= post  -> finetune barely moves it: KD-label/domain limited (knobs 1+2)
#   FLORES pre  > post  -> finetune overfits/hurts (knob 3)
#   KD-domain student-vs-teacher HIGH -> student learned the teacher on-domain
#     (so a low FLORES is a domain gap, not underfit); LOW -> underfit/capacity.
#
# Run ON bigserver.  bash diag_finetune_tlen.sh 2>&1 | tee diag_tlen.log
set -euo pipefail

W=/nvme2/prom/opus-trainer-v2
OUT="$W/diag"; mkdir -p "$OUT"
VOCAB="$W/vocab.entl.spm"
PRE="$W/ckpt.tlen.best.npz"
POST="$W/model_ft.tlen.best.npz"
FL=/fast_storage/david/opus-trainer/.cache/flores200_dataset/devtest
VENV=/fast_storage/david/opus-trainer/venv
KD_SLICE="${KD_SLICE:-1000}"
MARIAN=/opt/marian-dev/build/marian-decoder

# --- inputs -----------------------------------------------------------------
cp "$FL/tgl_Latn.devtest" "$OUT/flores.src.tl"
cp "$FL/eng_Latn.devtest" "$OUT/flores.ref.en"
# KD-domain held-out-ish slice (tail of the 10M aligned KD pair; source + teacher ref)
tail -n "$KD_SLICE" "$W/kd.tl" > "$OUT/kd.src.tl"
tail -n "$KD_SLICE" "$W/kd.en" > "$OUT/kd.ref.en"   # teacher (NLLB) output = KD-domain reference

decode() {  # <model.npz> <src> <out>
    docker run --rm -i -v /nvme2/prom:/nvme2/prom -v /fast_storage:/fast_storage \
        marian-train:cpu "$MARIAN" \
        -m "$1" -v "$VOCAB" "$VOCAB" \
        --beam-size 4 --mini-batch 16 --maxi-batch 100 --maxi-batch-sort src \
        --max-length 256 --max-length-crop --cpu-threads 16 --quiet-translation --quiet \
        < "$2" > "$3"
}

echo ">>> decoding (4 passes, CPU)…"
decode "$PRE"  "$OUT/flores.src.tl" "$OUT/pre.flores.en"
decode "$POST" "$OUT/flores.src.tl" "$OUT/post.flores.en"
decode "$PRE"  "$OUT/kd.src.tl"     "$OUT/pre.kd.en"
decode "$POST" "$OUT/kd.src.tl"     "$OUT/post.kd.en"

# --- score ------------------------------------------------------------------
"$VENV/bin/python" -c "import sacrebleu" 2>/dev/null || "$VENV/bin/pip" install -q sacrebleu
S="$VENV/bin/python $(dirname "$0")/chrf_score.py"

echo; echo "================ tl->en finetune diagnostic ================"
echo "--- FLORES-200 devtest  (ref = human eng_Latn) ---"
$S "pre-ft  FLORES" "$OUT/pre.flores.en"  "$OUT/flores.ref.en"
$S "post-ft FLORES" "$OUT/post.flores.en" "$OUT/flores.ref.en"
echo "  (teacher NLLB-600M on FLORES from the gate: chrF++ 66.4)"
echo "--- KD-domain slice n=$KD_SLICE  (ref = teacher NLLB output; measures on-domain imitation) ---"
$S "pre-ft  KD-dom" "$OUT/pre.kd.en"  "$OUT/kd.ref.en"
$S "post-ft KD-dom" "$OUT/post.kd.en" "$OUT/kd.ref.en"
