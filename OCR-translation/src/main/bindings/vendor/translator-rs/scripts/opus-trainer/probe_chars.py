#!/usr/bin/env python3
"""Measure every Arabic codepoint's Uyghur/Kazakh discrimination, and mine
strictly-zero-FP tokens.

The hand-picked marker sets in script_lid.py were chosen from orthography, not
from counts. This asks the buckets directly: for each codepoint, what share of
certain-Kazakh lines and certain-Uyghur lines contain it? A letter that is
Uyghur-exclusive belongs in UYGHUR_MARKERS, which shrinks the residual from the
keep side without dropping anything.

Bias to keep in mind when reading the output: the buckets are DEFINED by the
current markers, so those markers' own rates are circular. Every other codepoint
is measured on a real (if not perfectly random) sample of each language.
"""

from __future__ import annotations

import argparse
import sys
import unicodedata
from collections import Counter
from pathlib import Path

from mine_markers import TOKEN_RE, doc_freq, read


def char_doc_freq(lines: list[str]) -> Counter[str]:
    df: Counter[str] = Counter()
    for line in lines:
        df.update({c for c in line if "؀" <= c <= "ۿ"})
    return df


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--kk", type=Path, required=True)
    ap.add_argument("--uy", type=Path, required=True)
    ap.add_argument("--residual", type=Path, required=True)
    ap.add_argument("--kk-column", type=int, default=1)
    ap.add_argument("--min-kk-df", type=int, default=50)
    args = ap.parse_args()

    kk = read(args.kk, args.kk_column, None)
    uy = read(args.uy, None, None)
    res = read(args.residual, 1, None)

    kc, uc, rc = char_doc_freq(kk), char_doc_freq(uy), char_doc_freq(res)
    print(f"kk={len(kk):,} uy={len(uy):,} residual={len(res):,}\n")
    print(f"{'ch':<3} {'U+':<6} {'name':<38} {'kk%':>7} {'uy%':>8} {'res%':>7}  verdict")
    rows = sorted(set(kc) | set(uc), key=lambda c: -(uc.get(c, 0) / len(uy)))
    for ch in rows:
        k, u, r = kc.get(ch, 0) / len(kk), uc.get(ch, 0) / len(uy), rc.get(ch, 0) / len(res)
        if max(k, u) < 0.005:
            continue
        verdict = ""
        if u >= 0.02 and k / max(u, 1e-9) < 0.02:
            verdict = "<= UYGHUR-only"
        if k >= 0.02 and u / max(k, 1e-9) < 0.02:
            verdict = "<= KAZAKH-only"
        try:
            name = unicodedata.name(ch)[:38]
        except ValueError:
            name = "?"
        print(f"{ch:<3} {ord(ch):04X}   {name:<38} {k:>6.2%} {u:>7.3%} {r:>6.2%}  {verdict}")

    # Strict zero: tokens never seen in ANY of the certain-Uyghur lines.
    kk_df, uy_df = doc_freq(kk), doc_freq(uy)
    zero = [(t, n) for t, n in kk_df.items() if n >= args.min_kk_df and uy_df.get(t, 0) == 0 and len(t) >= 3]
    zero.sort(key=lambda x: -x[1])
    print(f"\n=== strict-zero tokens: kk_df>={args.min_kk_df}, uy_df==0 of {len(uy):,} ===")
    print(f"{len(zero)} tokens")
    toks = {t for t, _ in zero}
    covered = sum(1 for line in res if toks & set(TOKEN_RE.findall(line)))
    print(f"covers remaining residual: {covered:,} / {len(res):,} = {covered / len(res):.2%}")
    print("top 25:", " ".join(t for t, _ in zero[:25]))


if __name__ == "__main__":
    main()
