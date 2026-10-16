#!/bin/bash
# Phase 4b: build the lexical shortlist for a slimt pack.
#
# CRITICAL: the shortlist must be over SPM SUBWORDS, not raw words. slimt's
# shortlisted output projection is over the SPM vocab, so a word-level shortlist
# constrains it to the wrong candidates -> garbage / repetition loops (this cost
# a long debug: model was fine at 58 chrF++, a word-level shortlist dragged the
# slimt score to 34). Mirrors mozilla pipeline/alignments/generate-shortlist.sh.
#
# Run inside the marian-bmt image (spm_encode, fast_align, extract_lex, marian-conv).
#
# A lexical translation table saturates well before the full KD corpus, and
# fast_align cost is ~tokens x EM-passes x 2 directions on the SUBWORD stream
# (subword fragmentation inflates the token count hard for scripts like Uyghur).
# MAX_LINES caps the corpus to a uniform deterministic sample so the step is
# ~minutes, not hours; sampling by identical line stride on both files keeps the
# pairs aligned, and the fixed stride keeps the output reproducible (pipe memoizes).
#
# Usage: shortlist.sh SRC_FILE TRG_FILE VOCAB_SPM OUT_DIR [TOOLS=/work] [MAX_LINES=0]
#   -> OUT_DIR/lex.50.50.s2t.bin   (binarized; goes in the pack next to the model)
set -euo pipefail
[ -d /work/out ] && echo shortlist > /work/out/.phase || true

SRC=$1; TRG=$2; VOCAB=$3; OUT=$4; TOOLS=${5:-/work}; MAX_LINES=${6:-0}
BMT=/opt/marian-dev/build
FA="$TOOLS/fast_align/build"; XL="$TOOLS/extract-lex/build"
mkdir -p "$OUT"

# marian-conv detects SentencePiece vocabs by the .spm extension; pipe materializes
# inputs under bare names, so re-expose the vocab with its extension.
if [[ "$VOCAB" != *.spm ]]; then
  ln -sf "$VOCAB" "$OUT/vocab.spm" && VOCAB="$OUT/vocab.spm"
fi

if [ "$MAX_LINES" -gt 0 ]; then
  total=$(wc -l < "$SRC")
  if [ "$total" -gt "$MAX_LINES" ]; then
    stride=$(( total / MAX_LINES + 1 ))
    awk -v s="$stride" 'NR % s == 0' "$SRC" > "$OUT/samp.src"
    awk -v s="$stride" 'NR % s == 0' "$TRG" > "$OUT/samp.trg"
    SRC="$OUT/samp.src"; TRG="$OUT/samp.trg"
    echo "shortlist: sampled 1-in-$stride of $total lines -> $(wc -l < "$SRC")"
  fi
fi

# 1) SPM-segment both sides
"$BMT/spm_encode" --model "$VOCAB" < "$SRC" > "$OUT/spm.src"
"$BMT/spm_encode" --model "$VOCAB" < "$TRG" > "$OUT/spm.trg"

# 2) align the SUBWORD streams
paste "$OUT/spm.src" "$OUT/spm.trg" | sed 's/\t/ ||| /' > "$OUT/fa_in"
"$FA/fast_align" -i "$OUT/fa_in" -d -o -v    > "$OUT/fwd" 2>/dev/null
"$FA/fast_align" -i "$OUT/fa_in" -d -o -v -r > "$OUT/rev" 2>/dev/null
"$FA/atools" -i "$OUT/fwd" -j "$OUT/rev" -c grow-diag-final-and > "$OUT/aln"

# 3) lexical table (extract_lex args are TARGET then SOURCE); drop NULL alignments
"$XL/extract_lex" "$OUT/spm.trg" "$OUT/spm.src" "$OUT/aln" "$OUT/lex.s2t" "$OUT/lex.t2s"
grep -v NULL "$OUT/lex.s2t" > "$OUT/lex.s2t.clean"

# 4) binarize (firstNum bestNum threshold = 50 50 0)
"$BMT/marian-conv" --shortlist "$OUT/lex.s2t.clean" 50 50 0 \
  --dump "$OUT/lex.50.50.s2t.bin" --vocabs "$VOCAB" "$VOCAB"

echo "done: $OUT/lex.50.50.s2t.bin"
