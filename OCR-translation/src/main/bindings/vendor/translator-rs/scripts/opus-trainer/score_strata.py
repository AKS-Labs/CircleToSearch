#!/usr/bin/env python3
"""Score an eval slice split by whether the KD corpus already holds the row.

The ka->en `ui` and `subtitles` slices are not held out: 293 of 300 ui sources
and 416 of 500 subtitles sources, and 166 and 157 of the full pairs, are inside
the 4M-row KD corpus every checkpoint of this pair was distilled on
(ka_findings.md 31). On those rows the incumbent is reproducing its training
data, it scores about 87 chrF++ doing it, and a candidate that declines to
reproduce them looks like a regression while a reader sees a paraphrase. Reading
the whole slice therefore measures memorisation, and the numbers a selection
rule wants are the ones from the rows nobody trained on.

Three strata, from the coarsest match to the cleanest:

    pair-leaked   the source AND its reference are both in the KD corpus, on the
                  same row: the answer was in the training data
    source-only   the source is in the corpus with some other English
    clean         neither

Matching is `pair_spec.norm` — NFC, punctuation stripped, whitespace collapsed,
casefolded — the same normalisation `exclude_eval.py` holds out with, so a slice
that this script calls clean is a slice the exclusion pass would also have left
alone.

Building the strata reads the whole KD corpus, which lives on the training box,
so it is split from scoring: build the cache where the corpus is, copy the JSON,
score anywhere.

    # on the box that holds the corpus
    score_strata.py --eval-dir data/eval_ka2en --slice ui --slice subtitles \\
      --kd train.ka2en/aligned/train.tsv --cache strata.kaen.json

    # anywhere, once the cache is there
    score_strata.py --eval-dir data/eval_ka2en --slice ui --slice subtitles \\
      --cache strata.kaen.json --system live=out/kaen.{slice}.samp.hyp \\
      --system ft6=out/kaen6.{slice}.tab.hyp

`--system NAME=TEMPLATE` may be repeated; `{slice}` in the template is the slice
name. With no `--system` the script only builds the cache and prints the stratum
sizes.
"""

from __future__ import annotations

import argparse
import json
import pathlib
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum

from pair_spec import norm


class Stratum(StrEnum):
    PAIR_LEAKED = "pair-leaked"
    SOURCE_ONLY = "source-only"
    CLEAN = "clean"


@dataclass(frozen=True)
class SliceStrata:
    """Which stratum each row of one slice belongs to, by row index."""

    name: str
    rows: tuple[Stratum, ...]

    def indices(self, stratum: Stratum) -> list[int]:
        return [i for i, s in enumerate(self.rows) if s is stratum]


@dataclass(frozen=True)
class PoolIndex:
    """The normalised sources and source/reference pairs a training pool holds."""

    sources: frozenset[str]
    pairs: frozenset[tuple[str, str]]


def index_pool(lines: Iterable[str]) -> PoolIndex:
    sources: set[str] = set()
    pairs: set[tuple[str, str]] = set()
    for line in lines:
        columns = line.split("\t")
        if len(columns) < 2:
            continue
        source, target = norm(columns[0]), norm(columns[1])
        sources.add(source)
        pairs.add((source, target))
    return PoolIndex(frozenset(sources), frozenset(pairs))


def stratify(name: str, sources: Sequence[str], references: Sequence[str], pool: PoolIndex) -> SliceStrata:
    rows = []
    for source, reference in zip(sources, references):
        key = norm(source)
        if (key, norm(reference)) in pool.pairs:
            rows.append(Stratum.PAIR_LEAKED)
        elif key in pool.sources:
            rows.append(Stratum.SOURCE_ONLY)
        else:
            rows.append(Stratum.CLEAN)
    return SliceStrata(name, tuple(rows))


def read_lines(path: pathlib.Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def chrfpp(hypotheses: Sequence[str], references: Sequence[str]) -> float:
    import sacrebleu

    return sacrebleu.metrics.CHRF(word_order=2).corpus_score(list(hypotheses), [list(references)]).score


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--eval-dir", type=pathlib.Path, required=True)
    ap.add_argument("--slice", action="append", required=True, dest="slices")
    ap.add_argument("--kd", type=pathlib.Path, default=None, help="training pool to stratify against")
    ap.add_argument("--cache", type=pathlib.Path, required=True)
    ap.add_argument("--system", action="append", default=[], metavar="NAME=TEMPLATE",
                    help="repeatable; {slice} in the template is the slice name")
    args = ap.parse_args()

    slices = {
        name: (read_lines(args.eval_dir / f"{name}.src"), read_lines(args.eval_dir / f"{name}.ref"))
        for name in args.slices
    }

    if args.kd:
        pool = index_pool(args.kd.read_text(encoding="utf-8").splitlines())
        strata = {name: stratify(name, src, ref, pool) for name, (src, ref) in slices.items()}
        args.cache.write_text(json.dumps(
            {name: [str(s) for s in st.rows] for name, st in strata.items()}, indent=2), encoding="utf-8")
        print(f"strata written to {args.cache} against {args.kd} "
              f"({len(pool.sources)} distinct sources, {len(pool.pairs)} pairs)")
    else:
        cached = json.loads(args.cache.read_text(encoding="utf-8"))
        missing = [name for name in slices if name not in cached]
        if missing:
            raise SystemExit(f"{args.cache} has no strata for {', '.join(missing)}; re-run with --kd")
        strata = {name: SliceStrata(name, tuple(Stratum(s) for s in cached[name])) for name in slices}

    systems: Mapping[str, str] = dict(s.split("=", 1) for s in args.system)
    for name, (sources, references) in slices.items():
        st = strata[name]
        if len(st.rows) != len(references):
            raise SystemExit(f"{name}: strata hold {len(st.rows)} rows against {len(references)} references")
        print(f"== {name} (n={len(references)})")
        hypotheses = {
            label: read_lines(pathlib.Path(template.format(slice=name)))
            for label, template in systems.items()
        }
        for stratum in Stratum:
            index = st.indices(stratum)
            if not index:
                continue
            refs = [references[i] for i in index]
            line = f"   {stratum:12} n={len(index):4}"
            for label, hyp in hypotheses.items():
                picked = [hyp[i] for i in index]
                exact = sum(1 for r, h in zip(refs, picked) if r.strip() == h.strip())
                line += f"   {label} {chrfpp(picked, refs):6.2f} (exact {exact:3})"
            print(line)


if __name__ == "__main__":
    main()
