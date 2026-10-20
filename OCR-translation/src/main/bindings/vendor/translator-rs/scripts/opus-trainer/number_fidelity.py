#!/usr/bin/env python3
"""Score how faithfully a translation carries the SOURCE's figures, any pair.

chrF is blind to this. On the ka->en numbers holdout it charged 3.9 points for a
currency convention a reader calls a tie ("8,75 ₾" rewritten "8.75 GEL"), while
the two defects that matter to a camera app — a figure dropped out of a long
sentence, and a figure that comes back changed — moved it by almost nothing
(ka_findings.md 31). This measures the two defects directly and ignores the
convention.

A line's numbers are a MULTISET, not a set: "the 14:05 from Tbilisi at 14:05"
against a source with one 14:05 is an invented figure and has to be visible.
Comparison is between the source and the hypothesis, never the reference, so it
needs no held-out data and works on any slice.

WHAT IS NORMALISED AWAY, AND WHY
- Thousands separators, whichever of comma, point, space or apostrophe the row
  used: "1,250" "1 250" "1'250" "1.250" are one number printed four ways.
- Decimal comma against decimal point: "12,50" and "12.50" are one number.
- Trailing zeros in the fraction, so "12.50" and "12.5" agree.
- Currency spelling: ₾ / lari / GEL all map to the token `cur:GEL`, and the same
  for USD, EUR and GBP. Dropping the currency entirely is still a defect; writing
  it the other way round is not.
- Ordinal suffixes, which never enter a token because a token is digits only.

Leading zeros are KEPT: "Printer error 07" coming back as "Error 7" is a
different string on a photographed panel.

WHAT IS NOT NORMALISED
A digit written as a word counts only where a per-language table says the word
is unambiguous, and only to cancel a figure that is otherwise missing, so the
table can never invent a number that neither side wrote. `2-ჯერ` -> "twice" is
correct and a naive digit match calls it an omission (ka_findings.md 21); "one
of the doors" must not thereby satisfy a source that said "1". Languages with no
table are scored on digits alone, which is the default for a new pair.

    number_fidelity.py --slice numbers data/eval_ka2en/numbers.src out/numbers.hyp \\
                       --slice flores  data/eval_ka2en/flores.src  out/flores.hyp \\
                       --src-lang ka --hyp-lang en --show 10 --json report.json

`--slice NAME SRC HYP` may be repeated; every slice is scored on its own and the
JSON carries both the per-slice counts and a total. `--show N` prints the first N
failing lines of each slice. Exit status is 0 unless a file could not be read:
this reports, it does not judge, because what counts as a regression is a
comparison against another system and lives in gate_pack.sh.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import unicodedata
from collections import Counter
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum

CONFIG_DEFAULT = pathlib.Path(__file__).with_name("configs") / "number_words.json"

# A numeral is a digit run plus any separators INSIDE it. A colon is not a
# separator, so "14:05" is two numerals; that is what makes the doubled-time
# corruption above visible as an added figure rather than one opaque token.
THIN_SPACES = "   "
NUMERAL = re.compile(rf"\d+(?:[.,'{THIN_SPACES} ]\d+)*")
SEPARATOR = re.compile(rf"[.,'{THIN_SPACES} ]")
WORD = re.compile(r"[^\W\d_]+", re.UNICODE)
ALWAYS_THOUSANDS = frozenset("'" + THIN_SPACES + " ")


class Verdict(StrEnum):
    """What happened to one line's figures."""

    OK = "ok"
    OMITTED = "omitted"
    ADDED = "added"
    CORRUPTED = "corrupted"


@dataclass(frozen=True)
class NumberConfig:
    """The digits-vs-words and currency tables, parsed once at the edge.

    `currency_of` folds every spelling onto its code and is what scoring reads.
    `symbol_of` and `word_by_lang` go the other way, from a code to the form a
    given language should print, which is what a corpus rewrite needs when it
    copies a source's currency marker into a target written in another script.
    """

    currency_of: Mapping[str, str]
    words_by_lang: Mapping[str, Mapping[str, str]]
    symbol_of: Mapping[str, str]
    word_by_lang: Mapping[str, Mapping[str, str]]

    def words(self, lang: str | None) -> Mapping[str, str]:
        return self.words_by_lang.get(lang or "", {})


@dataclass(frozen=True)
class LineFidelity:
    index: int
    verdict: Verdict
    omitted: tuple[str, ...]
    added: tuple[str, ...]
    source: str
    hypothesis: str


@dataclass(frozen=True)
class SliceFidelity:
    name: str
    counts: Mapping[Verdict, int]
    failures: tuple[LineFidelity, ...]

    @property
    def scored(self) -> int:
        return sum(self.counts.values())

    @property
    def bad(self) -> int:
        """Omitted + corrupted: the two classes that change what a reader gets.

        An added figure is nearly always a doubled one and is counted separately
        so that a gate can weight it differently from a lost one.
        """
        return self.counts[Verdict.OMITTED] + self.counts[Verdict.CORRUPTED]

    @property
    def rate(self) -> float:
        return 100.0 * self.counts[Verdict.OK] / self.scored if self.scored else 100.0


def parse_config(raw: object) -> NumberConfig:
    if not isinstance(raw, dict):
        raise ValueError("number config must be a JSON object")
    currency_of: dict[str, str] = {}
    for code, spellings in raw.get("currencies", {}).items():
        for spelling in spellings:
            currency_of[spelling.casefold()] = code
    words = {
        lang: {word.casefold(): value for word, value in table.items()}
        for lang, table in raw.get("number_words", {}).items()
    }
    return NumberConfig(currency_of, words, raw.get("currency_symbols", {}),
                        raw.get("currency_words", {}))


def load_config(path: pathlib.Path) -> NumberConfig:
    return parse_config(json.loads(path.read_text(encoding="utf-8")))


def canonical_numeral(token: str) -> str:
    """One numeral in the form both sides of a pair are compared in.

    A comma or point before exactly three digits is a thousands separator and a
    comma or point before any other run is a decimal mark; a space or apostrophe
    is always thousands. The rule is applied identically to both sides, so a row
    that writes 2,500 for two and a half agrees with itself even where the
    reading is wrong.
    """
    groups = SEPARATOR.split(token)
    separators = SEPARATOR.findall(token)
    integer, fraction = groups[0], ""
    for separator, group in zip(separators, groups[1:]):
        thousands = separator in ALWAYS_THOUSANDS or len(group) == 3
        if thousands and not fraction:
            integer += group
        else:
            fraction += group
    fraction = fraction.rstrip("0")
    return f"{integer}.{fraction}" if fraction else integer


def numeral_tokens(text: str) -> list[str]:
    return [canonical_numeral(t) for t in NUMERAL.findall(text)]


def currency_tokens(text: str, config: NumberConfig) -> list[str]:
    """Currency markers as `cur:CODE`, symbols and spellings alike.

    Symbols carry no word boundary, so they are counted by scanning the raw text;
    spellings are matched on whole words to keep "gel" out of "gelatin".
    """
    found: list[str] = []
    for spelling, code in config.currency_of.items():
        if spelling.isalpha():
            continue
        found += [f"cur:{code}"] * text.count(spelling)
    for word in WORD.findall(unicodedata.normalize("NFC", text).casefold()):
        code = config.currency_of.get(word)
        if code is not None:
            found.append(f"cur:{code}")
    return found


def numeral_multiset(text: str) -> Counter[str]:
    return Counter(numeral_tokens(text))


def number_multiset(text: str, config: NumberConfig) -> Counter[str]:
    return Counter(numeral_tokens(text) + currency_tokens(text, config))


def word_values(text: str, table: Mapping[str, str]) -> Counter[str]:
    if not table:
        return Counter()
    words = WORD.findall(unicodedata.normalize("NFC", text).casefold())
    return Counter(table[w] for w in words if w in table)


def residuals(
    source: str,
    hypothesis: str,
    src_numbers: Counter[str],
    hyp_numbers: Counter[str],
    config: NumberConfig,
    src_lang: str | None,
    hyp_lang: str | None,
) -> tuple[Counter[str], Counter[str]]:
    """The figures one side carries and the other does not, after word rescue.

    Words only cancel a figure the other side already wrote as digits, so a table
    can turn a miss into a hit but can never manufacture a figure.
    """
    omitted, added = src_numbers - hyp_numbers, hyp_numbers - src_numbers
    omitted -= word_values(hypothesis, config.words(hyp_lang))
    added -= word_values(source, config.words(src_lang))
    return omitted, added


def compare_line(
    index: int,
    source: str,
    hypothesis: str,
    config: NumberConfig,
    src_lang: str | None = None,
    hyp_lang: str | None = None,
) -> LineFidelity:
    omitted, added = residuals(
        source, hypothesis,
        number_multiset(source, config), number_multiset(hypothesis, config),
        config, src_lang, hyp_lang,
    )
    if omitted and added:
        verdict = Verdict.CORRUPTED
    elif omitted:
        verdict = Verdict.OMITTED
    elif added:
        verdict = Verdict.ADDED
    else:
        verdict = Verdict.OK
    return LineFidelity(
        index,
        verdict,
        tuple(sorted(omitted.elements())),
        tuple(sorted(added.elements())),
        source,
        hypothesis,
    )


def score_slice(
    name: str,
    sources: Sequence[str],
    hypotheses: Sequence[str],
    config: NumberConfig,
    src_lang: str | None = None,
    hyp_lang: str | None = None,
) -> SliceFidelity:
    """Score the lines whose SOURCE carries a figure; the rest cannot fail.

    Lines with no figure in the source are excluded rather than counted as
    trivially ok, because a slice's rate is then the rate over the lines the
    measurement is about and does not move when the slice grows prose.
    """
    if len(sources) != len(hypotheses):
        raise ValueError(f"{name}: {len(sources)} source lines against {len(hypotheses)} hypothesis lines")
    counts = Counter({v: 0 for v in Verdict})
    failures: list[LineFidelity] = []
    for index, (source, hypothesis) in enumerate(zip(sources, hypotheses)):
        if not number_multiset(source, config):
            continue
        line = compare_line(index, source, hypothesis, config, src_lang, hyp_lang)
        counts[line.verdict] += 1
        if line.verdict is not Verdict.OK:
            failures.append(line)
    return SliceFidelity(name, counts, tuple(failures))


def as_json(slices: Sequence[SliceFidelity]) -> dict:
    total = Counter()
    for s in slices:
        total.update(s.counts)
    return {
        "slices": {
            s.name: {
                "scored": s.scored,
                "ok": s.counts[Verdict.OK],
                "omitted": s.counts[Verdict.OMITTED],
                "added": s.counts[Verdict.ADDED],
                "corrupted": s.counts[Verdict.CORRUPTED],
                "bad": s.bad,
                "rate": round(s.rate, 2),
                "failures": [
                    {"index": f.index, "verdict": str(f.verdict),
                     "omitted": list(f.omitted), "added": list(f.added),
                     "src": f.source, "hyp": f.hypothesis}
                    for f in s.failures
                ],
            }
            for s in slices
        },
        "total": {
            "scored": sum(total.values()),
            "ok": total[Verdict.OK],
            "omitted": total[Verdict.OMITTED],
            "added": total[Verdict.ADDED],
            "corrupted": total[Verdict.CORRUPTED],
            "bad": total[Verdict.OMITTED] + total[Verdict.CORRUPTED],
        },
    }


def read_lines(path: str) -> list[str]:
    return pathlib.Path(path).read_text(encoding="utf-8").splitlines()


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--slice", nargs=3, action="append", metavar=("NAME", "SRC", "HYP"),
                    required=True, help="repeatable: slice name, source file, hypothesis file")
    ap.add_argument("--src-lang", default=None, help="language code of the source side, for the number-word table")
    ap.add_argument("--hyp-lang", default=None, help="language code of the hypothesis side")
    ap.add_argument("--config", type=pathlib.Path, default=CONFIG_DEFAULT)
    ap.add_argument("--show", type=int, default=0, help="print this many failing lines per slice")
    ap.add_argument("--json", type=pathlib.Path, default=None)
    ap.add_argument("--label", default="", help="prefix for the printed table")
    args = ap.parse_args()

    config = load_config(args.config)
    scored = [
        score_slice(name, read_lines(src), read_lines(hyp), config, args.src_lang, args.hyp_lang)
        for name, src, hyp in args.slice
    ]
    head = f"{args.label} " if args.label else ""
    print(f"{head}{'slice':14} {'n':>5} {'ok':>5} {'omit':>5} {'add':>5} {'corr':>5} {'bad':>5} {'rate':>7}")
    for s in scored:
        print(f"{head}{s.name:14} {s.scored:5} {s.counts[Verdict.OK]:5} "
              f"{s.counts[Verdict.OMITTED]:5} {s.counts[Verdict.ADDED]:5} "
              f"{s.counts[Verdict.CORRUPTED]:5} {s.bad:5} {s.rate:6.2f}%")
        for f in s.failures[: args.show]:
            print(f"    [{f.index}] {f.verdict} omitted={list(f.omitted)} added={list(f.added)}")
            print(f"        SRC {f.source}")
            print(f"        HYP {f.hypothesis}")
    if args.json:
        args.json.write_text(json.dumps(as_json(scored), ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
