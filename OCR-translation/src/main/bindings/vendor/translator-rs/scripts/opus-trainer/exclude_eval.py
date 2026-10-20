#!/usr/bin/env python3
"""Drop held-out eval lines out of a training TSV, and record that it happened.

42 of the 175 lines in `probes/check.en` reached the shipped en->ka finetune
corpus because the exclusion list was applied to one band of the corpus and not
to the band the check lines were drawn from. The gain that set was credited with
was partly memorisation. Nothing in the pipeline could have caught it: the
exclusion lived inside one generator's `build`, so any corpus assembled by hand
or by another generator skipped it silently.

This is that step as a corpus-level pass. It runs on the TSV that will actually
train, whatever produced it, and writes `<out>.evalclean.json` beside it.
`finetune_student.sh` refuses to start without that report, so a corpus can no
longer reach a GPU un-checked.

A `--eval` source matches a training row if EITHER its sha256 (raw, stripped --
the form `data/eval_exclude.sha256` stores) OR its normalised text (NFC,
punctuation stripped, whitespace collapsed, casefolded) matches an eval line, in
any text column. Both, because an eval line that comes back re-cased or with a
stripped full stop is the same contamination, and because a digest list carries
no text to normalise.

That either-column rule is right only while the eval file holds the SOURCE side
of the direction being trained. Applied to an X->en holdout's English references
it threw away 435 legitimate rows of the ka->en corpus, because "Boil" and
"Ironing" are the references of some other Georgian word: the row taught a
different pair and leaked nothing. `--eval-pair SRC REF` is the rule for those
files. It reads two aligned sources and holds out the PAIRS, so a row is dropped
only when one of its text columns matches SRC line i and another matches REF
line i. Column order is not required to match, because a pair is the same
bitext knowledge whichever direction reads it.

Usage:

    exclude_eval.py --train ft.tsv --out ft.clean.tsv --text-columns 2 \\
      --eval probes/check.en probes/adversarial.en data/eval_exclude.sha256 \\
             'probes/check.ka.gen.jsonl:en,ka' data/eval_ka2en/*.src \\
      --eval-pair data/eval_ka2en/oneword_ho.src data/eval_ka2en/oneword_ho.ref

An eval source is a path, optionally suffixed with `:field,field` to name the
JSON fields to hold out. `.sha256` files are digest lists; `.jsonl` files
REQUIRE the field suffix; anything else is text, one held-out line per line,
tab-separated columns each held out separately. The two sides of an
`--eval-pair` are the same specs, must yield the same number of lines, and
cannot be digest lists, which carry no text to pair up.
"""

from __future__ import annotations

import argparse
import collections
import dataclasses
import datetime
import enum
import hashlib
import json
import pathlib
import sys

from pair_spec import norm, sha

DROPPED_SAMPLE = 25


class SourceKind(enum.Enum):
    DIGESTS = "digests"
    JSONL = "jsonl"
    TEXT = "text"


@dataclasses.dataclass(frozen=True)
class EvalSource:
    """One held-out file, parsed from its CLI spec."""

    path: pathlib.Path
    kind: SourceKind
    fields: tuple[str, ...]


@dataclasses.dataclass(frozen=True)
class EvalPair:
    """Two aligned held-out sources: line i of `left` is the translation of
    line i of `right`."""

    left: EvalSource
    right: EvalSource

    @property
    def tag(self) -> str:
        return f"{self.left.path}+{self.right.path}"


@dataclasses.dataclass(frozen=True)
class ExclusionIndex:
    """Digest, normalised-text and normalised-pair keys, each attributed to the
    source that first introduced it. First-match attribution, so the per-file
    drop counts in the report sum to the total rather than double-counting a
    line that two eval files share.

    A pair key is stored under the sorted pair of normalised sides, so a lookup
    finds it whichever column order the training row uses."""

    digests: dict[str, str]
    texts: dict[str, str]
    pairs: dict[tuple[str, str], str]

    def match(self, columns: list[str]) -> str | None:
        for col in columns:
            if not col.strip():
                continue
            hit = self.digests.get(sha(col)) or self.texts.get(norm(col))
            if hit is not None:
                return hit
        if not self.pairs:
            return None
        keys = [norm(col) for col in columns if col.strip()]
        for i, a in enumerate(keys):
            for b in keys[i + 1:]:
                hit = self.pairs.get(pair_key(a, b))
                if hit is not None:
                    return hit
        return None


@dataclasses.dataclass(frozen=True)
class Drop:
    line_number: int
    matched: str
    row: str


def pair_key(left: str, right: str) -> tuple[str, str]:
    """Order-free key for a held-out pair. `(ka, en)` and `(en, ka)` are the
    same bitext knowledge, and the two directions of this pair write their
    corpora in opposite column orders."""
    return (left, right) if left <= right else (right, left)


def parse_source(spec: str) -> EvalSource:
    """`path` or `path:field,field`. Windows-style drive letters are not a case
    this pipeline has, so a bare colon is unambiguous."""
    path_part, _, field_part = spec.partition(":")
    path = pathlib.Path(path_part)
    fields = tuple(f for f in field_part.split(",") if f)
    if path.suffix == ".sha256":
        if fields:
            raise ValueError(f"{spec}: a .sha256 digest list takes no fields")
        return EvalSource(path=path, kind=SourceKind.DIGESTS, fields=())
    if path.suffix == ".jsonl":
        if not fields:
            raise ValueError(
                f"{spec}: a .jsonl eval source must name its fields, e.g. "
                f"'{path_part}:en,ka' -- guessing which fields hold eval text "
                f"is how a set gets half-excluded")
        return EvalSource(path=path, kind=SourceKind.JSONL, fields=fields)
    if fields:
        raise ValueError(f"{spec}: only .jsonl sources take fields")
    return EvalSource(path=path, kind=SourceKind.TEXT, fields=())


def parse_pair(specs: list[str]) -> EvalPair:
    left, right = (parse_source(spec) for spec in specs)
    for side in (left, right):
        if side.kind is SourceKind.DIGESTS:
            raise ValueError(
                f"{side.path}: a digest list cannot be one side of an "
                f"--eval-pair; there is no text to pair up")
    return EvalPair(left=left, right=right)


def source_lines(source: EvalSource, body: str) -> list[str]:
    """The held-out strings a source contributes. Pure: `body` is the file's
    text, so the caller owns the I/O."""
    if source.kind is SourceKind.DIGESTS:
        return sorted(set(body.split()))
    if source.kind is SourceKind.JSONL:
        out: list[str] = []
        for n, line in enumerate(body.splitlines(), 1):
            if not line.strip():
                continue
            record = json.loads(line)
            if not isinstance(record, dict):
                raise ValueError(f"{source.path}:{n}: expected a JSON object")
            missing = [f for f in source.fields if f not in record]
            if missing:
                raise ValueError(
                    f"{source.path}:{n}: missing field(s) {', '.join(missing)}")
            for field in source.fields:
                value = record[field]
                if value is None:
                    continue
                if not isinstance(value, str):
                    raise ValueError(
                        f"{source.path}:{n}: field {field} is "
                        f"{type(value).__name__}, expected a string")
                out.append(value)
        return out
    out = []
    for line in body.splitlines():
        for col in line.split("\t"):
            if col.strip():
                out.append(col)
    return out


def build_index(
    loaded: list[tuple[EvalSource, list[str]]],
    loaded_pairs: list[tuple[EvalPair, list[tuple[str, str]]]] | None = None,
) -> ExclusionIndex:
    digests: dict[str, str] = {}
    texts: dict[str, str] = {}
    pairs: dict[tuple[str, str], str] = {}
    for source, lines in loaded:
        tag = str(source.path)
        for line in lines:
            if source.kind is SourceKind.DIGESTS:
                digests.setdefault(line, tag)
                continue
            digests.setdefault(sha(line), tag)
            key = norm(line)
            if key:
                texts.setdefault(key, tag)
    for pair, rows in loaded_pairs or []:
        for left, right in rows:
            a, b = norm(left), norm(right)
            if not a or not b:
                continue
            pairs.setdefault(pair_key(a, b), pair.tag)
    return ExclusionIndex(digests=digests, texts=texts, pairs=pairs)


def zip_pair(pair: EvalPair, left: list[str],
             right: list[str]) -> list[tuple[str, str]]:
    """The held-out pairs two aligned sources contribute. A length mismatch is
    fatal: silently zipping to the shorter side would hold out a prefix of one
    file against a mis-shifted prefix of the other, which is worse than not
    holding out at all."""
    if len(left) != len(right):
        raise ValueError(
            f"{pair.tag}: {pair.left.path} has {len(left)} lines and "
            f"{pair.right.path} has {len(right)}; an --eval-pair must be aligned")
    return list(zip(left, right))


def filter_rows(rows: list[str], index: ExclusionIndex,
                text_columns: int) -> tuple[list[str], list[Drop]]:
    """Split a TSV's lines into what may train and what leaked.

    `text_columns` bounds how much of a row is compared, because a 3-column
    guided-alignment TSV's third field is a Pharaoh alignment string and not
    text at all.
    """
    kept: list[str] = []
    drops: list[Drop] = []
    for n, row in enumerate(rows, 1):
        columns = row.split("\t")[:text_columns]
        matched = index.match(columns)
        if matched is None:
            kept.append(row)
            continue
        drops.append(Drop(line_number=n, matched=matched, row=row))
    return kept, drops


def digest_file(path: pathlib.Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--train", nargs="+", required=True, type=pathlib.Path,
                    help="training TSV(s); concatenated in the order given")
    ap.add_argument("--out", required=True, type=pathlib.Path,
                    help="filtered TSV; the report goes to <out>.evalclean.json")
    ap.add_argument("--eval", nargs="+", required=True, metavar="SPEC",
                    help="held-out sources: PATH or PATH:field,field for .jsonl")
    ap.add_argument("--eval-pair", nargs=2, action="append", default=[],
                    metavar=("SRC", "REF"),
                    help="two aligned held-out sources; a row is dropped only "
                         "when its columns carry BOTH sides of the same pair. "
                         "Repeatable.")
    ap.add_argument("--text-columns", type=int, default=2,
                    help="how many leading columns are text (default 2: a "
                         "3-col TSV's alignment field is not text)")
    a = ap.parse_args()

    if a.text_columns < 1:
        sys.exit("--text-columns must be at least 1")

    sources = [parse_source(spec) for spec in a.eval]
    pairs = [parse_pair(specs) for specs in a.eval_pair]
    for source in sources + [s for p in pairs for s in (p.left, p.right)]:
        if not source.path.is_file():
            sys.exit(f"eval source not found: {source.path}")
    for path in a.train:
        if not path.is_file():
            sys.exit(f"training TSV not found: {path}")

    def read(source: EvalSource) -> list[str]:
        return source_lines(source, source.path.read_text(encoding="utf-8"))

    loaded = [(s, read(s)) for s in sources]
    loaded_pairs = [(p, zip_pair(p, read(p.left), read(p.right))) for p in pairs]
    index = build_index(loaded, loaded_pairs)

    rows: list[str] = []
    inputs = []
    for path in a.train:
        body = path.read_text(encoding="utf-8").splitlines()
        rows.extend(body)
        inputs.append({"path": str(path), "rows": len(body),
                       "sha256": digest_file(path)})

    kept, drops = filter_rows(rows, index, a.text_columns)

    a.out.parent.mkdir(parents=True, exist_ok=True)
    a.out.write_text("".join(r + "\n" for r in kept), encoding="utf-8")

    per_file = collections.Counter(d.matched for d in drops)
    report = {
        "generated": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "out": str(a.out),
        "text_columns": a.text_columns,
        "inputs": inputs,
        "eval_sources": [
            {"path": str(s.path), "kind": s.kind.value,
             "fields": list(s.fields), "lines": len(lines),
             "sha256": digest_file(s.path)}
            for s, lines in loaded],
        "eval_pairs": [
            {"src": str(p.left.path), "src_fields": list(p.left.fields),
             "ref": str(p.right.path), "ref_fields": list(p.right.fields),
             "pairs": len(rows),
             "src_sha256": digest_file(p.left.path),
             "ref_sha256": digest_file(p.right.path)}
            for p, rows in loaded_pairs],
        "rows_in": len(rows),
        "rows_out": len(kept),
        "rows_dropped": len(drops),
        "dropped_per_eval_file": {k: per_file[k] for k in sorted(per_file)},
        "dropped_sample": [
            {"line": d.line_number, "matched": d.matched, "row": d.row}
            for d in drops[:DROPPED_SAMPLE]],
    }
    report_path = a.out.with_name(a.out.name + ".evalclean.json")
    report_path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n",
                           encoding="utf-8")

    print(f"{len(rows)} rows in, {len(drops)} dropped, {len(kept)} out "
          f"-> {a.out}")
    for name in sorted(per_file):
        print(f"  {per_file[name]:>6}  {name}")
    print(f"report: {report_path}")


if __name__ == "__main__":
    main()
