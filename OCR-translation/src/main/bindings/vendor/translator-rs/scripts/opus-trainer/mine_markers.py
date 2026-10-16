#!/usr/bin/env python3
"""Mine Kazakh-discriminative tokens from the certain buckets script_lid.py cuts.

The hand-picked marker set only reaches the residual (no high hamza, no Uyghur
vowel letter) where the tokens happen to be ones a human thought of. This counts
instead: any token frequent in certain-Kazakh and near-absent in certain-Uyghur
is a usable marker, and its cost is directly measurable as the share of the
2.46M certain-Uyghur lines it would falsely drop.

Tokens are whitespace/punctuation-delimited by construction, which is what makes
this safe: Uyghur prefixes ئ onto vowel-initial words, so ەمەس as a SUBSTRING
matches Uyghur ئەمەس (2.6% false-positive) while ەمەس as a TOKEN does not
(0.001%). Never go back to substring matching here.

Selection is greedy set-cover over residual lines under a false-positive budget,
so each added marker buys the most still-uncovered residual per unit of Uyghur
lost.

Usage:
    mine_markers.py --kk train.kk.tsv --uy certain_uy.txt --residual res.txt \
        --min-kk-df 150 --max-uy-rate 0.0002 --fp-budget 0.01
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

from script_lid import KAZAKH_MARKERS, UYGHUR_MARKERS

# Arabic-script letter runs; drops digits, latin, and punctuation.
TOKEN_RE = re.compile(r"[؀-ۿݐ-ݿ]+")


def doc_freq(lines: list[str]) -> Counter[str]:
    df: Counter[str] = Counter()
    for line in lines:
        df.update(set(TOKEN_RE.findall(line)))
    return df


@dataclass(frozen=True)
class Candidate:
    token: str
    kk_df: int
    uy_df: int
    kk_rate: float
    uy_rate: float

    @property
    def lift(self) -> float:
        return self.kk_rate / max(self.uy_rate, 1e-9)


def candidates(
    kk: list[str], uy: list[str], min_kk_df: int, max_uy_rate: float, min_len: int
) -> list[Candidate]:
    kk_df, uy_df = doc_freq(kk), doc_freq(uy)
    out: list[Candidate] = []
    for token, n in kk_df.items():
        # Short tokens are fragments and single letters: rare in Uyghur by accident
        # rather than by orthography, so they do not transfer to a new corpus.
        if n < min_kk_df or len(token) < min_len:
            continue
        # A token already carrying a hand marker is free: script_lid cut those lines
        # before the residual existed.
        if set(token) & (UYGHUR_MARKERS | KAZAKH_MARKERS):
            continue
        uy_n = uy_df.get(token, 0)
        uy_rate = uy_n / len(uy)
        if uy_rate > max_uy_rate:
            continue
        out.append(Candidate(token, n, uy_n, n / len(kk), uy_rate))
    return sorted(out, key=lambda c: c.kk_df, reverse=True)


def postings(lines: list[str], tokens: set[str]) -> dict[str, set[int]]:
    index: dict[str, set[int]] = {t: set() for t in tokens}
    for i, line in enumerate(lines):
        for token in TOKEN_RE.findall(line):
            if token in index:
                index[token].add(i)
    return index


def greedy_cover(
    cands: list[Candidate], residual: list[str], uy: list[str], fp_budget: float,
    max_markers: int,
) -> list[Candidate]:
    """Pick markers that cover the most uncovered residual lines per Uyghur line lost."""
    pool = {c.token: c for c in cands}
    res_idx = postings(residual, set(pool))
    uy_idx = postings(uy, set(pool))
    uncovered = set(range(len(residual)))
    fp_hit: set[int] = set()
    chosen: list[Candidate] = []
    max_fp = int(fp_budget * len(uy))

    while pool and len(chosen) < max_markers:
        best: tuple[float, str] | None = None
        for token in pool:
            gain = len(res_idx[token] & uncovered)
            if gain == 0:
                continue
            new_fp = len(uy_idx[token] - fp_hit)
            if len(fp_hit) + new_fp > max_fp:
                continue
            score = gain / max(new_fp, 0.5)
            if best is None or score > best[0]:
                best = (score, token)
        if best is None:
            break
        token = best[1]
        chosen.append(pool.pop(token))
        uncovered -= res_idx[token]
        fp_hit |= uy_idx[token]
    return chosen


def read(path: Path, column: int | None, limit: int | None) -> list[str]:
    lines: list[str] = []
    with path.open(encoding="utf-8") as f:
        for line in f:
            text = line.rstrip("\n")
            if column is not None:
                text = text.split("\t")[column - 1]
            lines.append(text)
            if limit is not None and len(lines) >= limit:
                break
    return lines


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--kk", type=Path, required=True)
    ap.add_argument("--uy", type=Path, required=True)
    ap.add_argument("--residual", type=Path, required=True)
    ap.add_argument("--kk-column", type=int, default=1)
    ap.add_argument("--min-kk-df", type=int, default=150)
    ap.add_argument("--max-uy-rate", type=float, default=0.0002)
    ap.add_argument("--fp-budget", type=float, default=0.01)
    ap.add_argument("--uy-sample", type=int, default=400_000,
                    help="cap certain-Uyghur lines used for cover search (full set still scored)")
    ap.add_argument("--min-len", type=int, default=3,
                    help="skip tokens shorter than this: fragments do not transfer")
    ap.add_argument("--max-markers", type=int, default=150)
    ap.add_argument("--markers-out", type=Path, default=None, help="write chosen markers, one per line")
    ap.add_argument("--top", type=int, default=40)
    args = ap.parse_args()

    kk = read(args.kk, args.kk_column, None)
    uy = read(args.uy, None, None)
    residual = read(args.residual, None, None)
    print(f"kk={len(kk):,}  uy={len(uy):,}  residual={len(residual):,}", file=sys.stderr)

    cands = candidates(kk, uy, args.min_kk_df, args.max_uy_rate, args.min_len)
    print(f"\n=== {len(cands)} candidates (kk_df>={args.min_kk_df}, uy_rate<={args.max_uy_rate:.4%}) ===")
    print(f"{'token':<20} {'kk_df':>8} {'kk%':>7} {'uy_df':>7} {'uy%':>8} {'lift':>9}")
    for c in cands[: args.top]:
        print(f"{c.token:<20} {c.kk_df:>8,} {c.kk_rate:>6.2%} {c.uy_df:>7,} {c.uy_rate:>7.4%} {c.lift:>9,.0f}")

    chosen = greedy_cover(cands, residual, uy[: args.uy_sample], args.fp_budget, args.max_markers)
    tokens = {c.token for c in chosen}
    covered = sum(1 for line in residual if tokens & set(TOKEN_RE.findall(line)))
    fp = sum(1 for line in uy if tokens & set(TOKEN_RE.findall(line)))
    print(f"\n=== greedy set ({len(chosen)} markers, fp budget {args.fp_budget:.2%}) ===")
    print(" ".join(c.token for c in chosen))
    print(f"\ncovers residual: {covered:,} / {len(residual):,} = {covered / len(residual):.2%}")
    print(f"false-drop certain-uy: {fp:,} / {len(uy):,} = {fp / len(uy):.3%}")
    if args.markers_out:
        args.markers_out.write_text("\n".join(c.token for c in chosen) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
