#!/usr/bin/env python3
"""Gate a freshly-trained joint SPM against the corpus it was trained on.

A joint vocab is pair-specific and silently load-bearing: everything downstream
(KD targets, guided alignment, the student, the shortlist) is expressed in its
pieces, and a vocab that missed a script does not error — it byte-fallbacks, so
training "works" and only the scores are bad. This runs at prep time, on the
real pool, so that failure lands at build instead of on a rented box (the same
reason the Dockerfiles end in an ldd gate).

Checks, per side of the pool:
  - piece count is exactly what was asked for
  - zero <unk> (byte_fallback means an unk is a genuine defect, not OOV)
  - encode->decode roundtrips for all but a small fraction. NOT exact: SPM
    applies NMT_NFKC normalization, which legitimately rewrites a handful of
    lines (measured 1/2000 on a healthy en-ug vocab), so this is a rate.
  - FERTILITY (pieces per word) is sane. This is the check that catches the real
    failure: if the vocab never learned a script, byte_fallback still encodes it
    at roughly one piece per BYTE (~2 for an Arabic-script char), so fertility
    explodes while unk stays 0 and roundtrip still passes. Measured on the en-ug
    build: 1.36 (en) / 1.63 (ug) with its own vocab, vs 8.62 for ug scored
    against the en-sw vocab — a 5x separation, so the 6.0 default is a wide gate.

Usage: check_vocab.py VOCAB_SPM POOL_TSV [EXPECT_PIECES=32000] [MAX_FERTILITY=6.0]
"""

from __future__ import annotations

import sys

import sentencepiece as spm

SAMPLE = 2000
# NMT_NFKC rewrites a few lines legitimately; anything past this is a real defect.
MAX_ROUNDTRIP_FAIL = 0.01


def side_report(sp: spm.SentencePieceProcessor, lines: list[str], label: str, max_fert: float) -> list[str]:
    if not lines:
        return [f"{label}: no lines sampled"]
    unk_id = sp.unk_id()
    unks = pieces = words = bad_roundtrip = 0
    for s in lines:
        ids = sp.encode(s)
        unks += ids.count(unk_id)
        pieces += len(ids)
        words += len(s.split())
        bad_roundtrip += sp.decode(ids) != s
    fert = pieces / max(words, 1)
    print(f"  {label}: {len(lines)} lines, {fert:.2f} pieces/word, {unks} unk, {bad_roundtrip} roundtrip fails")
    errs = []
    if unks:
        errs.append(f"{label}: {unks} <unk> pieces — byte_fallback should make this impossible")
    if bad_roundtrip / len(lines) > MAX_ROUNDTRIP_FAIL:
        errs.append(
            f"{label}: {bad_roundtrip}/{len(lines)} lines did not roundtrip "
            f"(> {MAX_ROUNDTRIP_FAIL:.1%}, beyond what NFKC normalization explains)"
        )
    if fert > max_fert:
        errs.append(
            f"{label}: fertility {fert:.2f} pieces/word > {max_fert} — the vocab likely never "
            "learned this side's script and is byte-falling-back through it"
        )
    return errs


def digit_split_report(sp: spm.SentencePieceProcessor) -> list[str]:
    # A figure must segment into one piece per digit (plus the word marker), or
    # the decoder has to guess a segmentation when copying it and drops pieces on
    # runs longer than the ones it saw most (ka_findings.md §32-§33).
    probes = ["2387", "1201000", "Error 404", "12,50", "01012034567"]
    bad = []
    for text in probes:
        pieces = sp.encode(text, out_type=str)
        multi_digit = [pc for pc in pieces if sum(ch.isdigit() for ch in pc) > 1]
        if multi_digit:
            bad.append(f"{text!r} -> {pieces} (pieces with several digits: {multi_digit})")
    if not bad:
        return []
    return ["vocab was trained without split_digits; figures do not segment one piece per digit:\n  "
            + "\n  ".join(bad)]


def main() -> None:
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    vocab, pool = sys.argv[1], sys.argv[2]
    expect = int(sys.argv[3]) if len(sys.argv) > 3 else 32000
    max_fert = float(sys.argv[4]) if len(sys.argv) > 4 else 6.0

    sp = spm.SentencePieceProcessor(model_file=vocab)
    print(f"vocab {vocab}: {sp.get_piece_size()} pieces")
    errs = []
    if sp.get_piece_size() != expect:
        errs.append(f"piece count {sp.get_piece_size()} != requested {expect}")

    src: list[str] = []
    tgt: list[str] = []
    with open(pool, encoding="utf-8") as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) != 2 or not parts[0].strip() or not parts[1].strip():
                continue
            src.append(parts[0])
            tgt.append(parts[1])
            if len(src) >= SAMPLE:
                break
    errs += side_report(sp, src, "src", max_fert)
    errs += side_report(sp, tgt, "tgt", max_fert)
    errs += digit_split_report(sp)

    if errs:
        for e in errs:
            print(f"check_vocab: {e}", file=sys.stderr)
        sys.exit(1)
    print("check_vocab: OK")


if __name__ == "__main__":
    main()
