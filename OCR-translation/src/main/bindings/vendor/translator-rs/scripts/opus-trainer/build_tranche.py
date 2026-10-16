#!/usr/bin/env python3
"""Turn a `gen_pairs` tranche into the finetune TSV, with the two rewrites the
ka->en ft5 read asked for (ka_findings.md 31).

The tranche is authored en->X in one call per cell and used column-swapped when
the direction is X->en, so its English is the ORIGINAL and its target side is
model-written. That is what makes the two defects below corpus-level rather than
per-row noise: they are conventions the generator applied uniformly.

NUMBER CONVENTIONS ARE COPIED FROM THE SOURCE, NOT RE-LOCALISED
The generated English localises what the target row printed: "86.40 ₾" comes
back as "86.40 GEL", "8,75" as "8.75", and a thousands separator appears where
the target had none. The app overlays its output on a photographed price tag, so
the figure on the tag and the figure in the overlay have to be the same string;
a finetune trained on 4,400 rows of re-localisation teaches the model to rewrite
figures it should be copying. Every numeral in the target side is therefore
replaced by the form the source side printed for that value, and a currency
marker is re-rendered from the source's own marker: a symbol is copied verbatim,
a word is written in the target language's own word, a code stays a code.

Rows whose two sides do not already agree on their figures are DROPPED, never
repaired. A row where the English says 250 and the source says 350 is a
generation defect, and rewriting one into the other would invent a pair nobody
authored.

UI LABELS KEEP SENTENCE CASE
The tranche title-cases UI labels ("Read Directory A", "Color Palette") where
the KDE/GNOME strings the app meets are sentence case, and ft5 learned it. The
rule here lowercases a word only if the corpus's own English uses that word in
lowercase mid-sentence: dictionary-common words go down, acronyms (KAB, PIN) and
proper nouns (Georgia, Bluetooth) stay up, because those are the words that are
almost never seen lowercase. The list is derived from the tranche itself, so it
needs no external dictionary and follows the corpus into a new language pair.

    build_tranche.py --pairs data/gen_pairs/ka/pairs.jsonl --direction ka-en \\
      --out ft6/luna.ka-en.tsv --report ft6/tranche.report.json --samples 30

`--direction ka-en` means the TSV is written source-first as ka<TAB>en and the
English is the side being rewritten; `--direction en-ka` writes en<TAB>ka and
rewrites the ka side. Registers named by `--case-register` (default `ui`) get the
casing rule; everything else keeps its case.
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import re
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass

from number_fidelity import (
    NUMERAL,
    WORD,
    CONFIG_DEFAULT,
    NumberConfig,
    canonical_numeral,
    load_config,
    numeral_multiset,
    residuals,
)

# Function words a title-cased label leaves lowercase, so "Read Directory A" is
# recognised as Title Case even though "a" is down. Recognition only; the casing
# rewrite itself never consults this list.
TITLE_MINORS = frozenset(
    "a an the of in on at to for and or nor but with from by as per via".split()
)
LOWER_SHARE = 0.9
LOWER_MIN = 3


@dataclass(frozen=True)
class TranchePair:
    """One generated row, parsed out of the generator's JSONL."""

    source: str
    target: str
    register: str


@dataclass(frozen=True)
class Rewrite:
    """What happened to one row."""

    pair: TranchePair
    target: str
    dropped: bool
    numbers_changed: bool
    case_changed: bool


def parse_pairs(lines: Iterable[str], src_field: str, trg_field: str) -> list[TranchePair]:
    rows = []
    for n, line in enumerate(lines, 1):
        record = json.loads(line)
        missing = [f for f in (src_field, trg_field) if f not in record]
        if missing:
            raise ValueError(f"line {n}: generated pair has no {', '.join(missing)} field")
        rows.append(TranchePair(record[src_field], record[trg_field], record.get("register", "")))
    return rows


def figures_agree(source: str, target: str, config: NumberConfig,
                  src_lang: str, trg_lang: str) -> bool:
    """The two sides carry the same figures, counting a spelled-out small integer.

    Currency markers are deliberately not part of this test: a row whose target
    names a currency its source does not is a convention slip the rewrite below
    handles, while a row whose FIGURES differ is a generation defect and the row
    goes.
    """
    omitted, added = residuals(
        source, target, numeral_multiset(source), numeral_multiset(target),
        config, src_lang, trg_lang,
    )
    return not omitted and not added


def mirror_numerals(source: str, target: str) -> str:
    """Rewrite every numeral in `target` into the form `source` printed for it.

    Occurrences are consumed in order, so a value that appears twice with two
    spellings keeps both, and the caller has already established that the two
    sides carry the same multiset of values.
    """
    by_value: dict[str, collections.deque[str]] = collections.defaultdict(collections.deque)
    for raw in NUMERAL.findall(source):
        by_value[canonical_numeral(raw)].append(raw)

    def replace(match: re.Match[str]) -> str:
        forms = by_value.get(canonical_numeral(match.group()))
        return forms.popleft() if forms else match.group()

    return NUMERAL.sub(replace, target)


def currency_marker(text: str, config: NumberConfig) -> tuple[str, str] | None:
    """The one currency the text names, as (code, kind) with kind symbol/word/code.

    A text naming two currencies is left alone: which marker belongs to which
    figure is a parse this rewrite does not do.
    """
    found = set()
    kinds = {}
    for spelling, code in config.currency_of.items():
        if spelling.isalpha():
            if spelling in [w.casefold() for w in WORD.findall(text)]:
                found.add(code)
                kinds.setdefault(code, "code" if spelling == code.casefold() else "word")
        elif spelling in text:
            found.add(code)
            kinds[code] = "symbol"
    if len(found) != 1:
        return None
    code = found.pop()
    return code, kinds[code]


def mirror_currency(source: str, target: str, config: NumberConfig, target_lang: str) -> str:
    """Print the target's currency marker the way the source printed its own.

    Only a marker the target ALREADY carries is rewritten. Inserting one the
    generator left out would be guessing at its position, and a missing marker is
    a different defect from a re-localised one.
    """
    src, trg = currency_marker(source, config), currency_marker(target, config)
    if src is None or trg is None or src[0] != trg[0]:
        return target
    code, src_kind = src
    if src_kind == "symbol":
        wanted = config.symbol_of.get(code, code)
    elif src_kind == "code":
        wanted = code
    else:
        wanted = config.word_by_lang.get(target_lang, {}).get(code, code)
    spellings = sorted(
        (s for s, c in config.currency_of.items() if c == code), key=len, reverse=True
    )
    out = target
    for spelling in spellings:
        if spelling.isalpha():
            out = re.sub(rf"(?<!\w){re.escape(spelling)}(?!\w)", wanted, out, flags=re.IGNORECASE)
        else:
            out = out.replace(spelling, wanted)
    return out


def lowercase_vocabulary(texts: Sequence[str]) -> frozenset[str]:
    """Words the corpus itself writes lowercase when they are not line-initial.

    A word qualifies when it appears at least LOWER_MIN times away from the start
    of a line and at least LOWER_SHARE of those appearances are lowercase. That
    is the operational meaning of "dictionary-common" here: acronyms and proper
    nouns fail the share test because the corpus almost never writes them down.
    """
    lower: collections.Counter[str] = collections.Counter()
    total: collections.Counter[str] = collections.Counter()
    for text in texts:
        for word in WORD.findall(text)[1:]:
            key = word.casefold()
            total[key] += 1
            if word.islower():
                lower[key] += 1
    return frozenset(
        word for word, n in total.items()
        if lower[word] >= LOWER_MIN and lower[word] >= LOWER_SHARE * n
    )


def is_title_case(text: str) -> bool:
    words = [w for w in WORD.findall(text) if w]
    if len(words) < 2:
        return False
    capitalised = 0
    for i, word in enumerate(words):
        if word[0].isupper():
            capitalised += 1
        elif i and word.casefold() in TITLE_MINORS:
            continue
        else:
            return False
    return capitalised >= 2


def next_to_a_figure(text: str, start: int, end: int) -> bool:
    """Whether a digit sits against the word, or one space away from it."""
    before, after = text[:start], text[end:]
    return bool(re.search(r"\d ?$", before) or re.match(r" ?\d", after))


def sentence_case(text: str, common: frozenset[str]) -> str:
    """Lower the case of every common word after the first, leave the rest alone.

    A single letter and a word next to a figure are never lowered whatever the
    vocabulary says: "Current 2.4 A" is amperes, "Signal 4G" is a network and
    "Monday 12 May" is a date, while "a", "g" and "may" are as dictionary-common
    as words get.
    """
    seen_first = False

    def fix(match: re.Match[str]) -> str:
        nonlocal seen_first
        word = match.group()
        if not seen_first:
            seen_first = True
            return word
        if len(word) < 2 or word.casefold() not in common:
            return word
        if next_to_a_figure(text, match.start(), match.end()):
            return word
        return word.lower()

    return WORD.sub(fix, text)


def rewrite_rows(
    rows: Sequence[TranchePair],
    config: NumberConfig,
    source_lang: str,
    target_lang: str,
    case_registers: frozenset[str],
) -> list[Rewrite]:
    common = lowercase_vocabulary([r.target for r in rows])
    out = []
    for row in rows:
        if not figures_agree(row.source, row.target, config, source_lang, target_lang):
            out.append(Rewrite(row, row.target, True, False, False))
            continue
        target = mirror_currency(row.source, mirror_numerals(row.source, row.target), config, target_lang)
        numbers_changed = target != row.target
        cased = target
        if row.register in case_registers and is_title_case(target):
            cased = sentence_case(target, common)
        out.append(Rewrite(row, cased, False, numbers_changed, cased != target))
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--pairs", type=pathlib.Path, required=True, help="gen_pairs pairs.jsonl")
    ap.add_argument("--direction", required=True, help="e.g. ka-en: source language first")
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--report", type=pathlib.Path, default=None)
    ap.add_argument("--config", type=pathlib.Path, default=CONFIG_DEFAULT)
    ap.add_argument("--case-register", action="append", default=None,
                    help="register whose title-cased targets are sentence-cased (default: ui)")
    ap.add_argument("--samples", type=int, default=30, help="before/after pairs to print")
    args = ap.parse_args()

    src_lang, trg_lang = args.direction.split("-")
    rows = parse_pairs(args.pairs.read_text(encoding="utf-8").splitlines(), src_lang, trg_lang)
    rewrites = rewrite_rows(
        rows, load_config(args.config), src_lang, trg_lang,
        frozenset(args.case_register or ["ui"]),
    )

    kept = [r for r in rewrites if not r.dropped]
    args.out.write_text(
        "".join(f"{r.pair.source}\t{r.target}\n" for r in kept), encoding="utf-8"
    )
    changed_numbers = [r for r in kept if r.numbers_changed]
    changed_case = [r for r in kept if r.case_changed]
    print(f"rows in            {len(rewrites)}")
    print(f"dropped (figures disagree) {len(rewrites) - len(kept)}")
    print(f"kept               {len(kept)}")
    print(f"number conventions rewritten {len(changed_numbers)}")
    print(f"ui labels sentence-cased     {len(changed_case)}")
    for r in changed_numbers[: args.samples]:
        print(f"  SRC  {r.pair.source}")
        print(f"  WAS  {r.pair.target}")
        print(f"  NOW  {r.target}")
    for r in changed_case[: args.samples]:
        print(f"  CASE {r.pair.target}  ->  {r.target}")
    if args.report:
        args.report.write_text(json.dumps({
            "rows_in": len(rewrites),
            "dropped_figures_disagree": len(rewrites) - len(kept),
            "kept": len(kept),
            "numbers_rewritten": len(changed_numbers),
            "case_rewritten": len(changed_case),
            "number_samples": [
                {"src": r.pair.source, "was": r.pair.target, "now": r.target}
                for r in changed_numbers[: args.samples]
            ],
            "case_samples": [
                {"was": r.pair.target, "now": r.target} for r in changed_case[: args.samples]
            ],
            "dropped_samples": [
                {"src": r.pair.source, "trg": r.target}
                for r in rewrites if r.dropped
            ][: args.samples],
        }, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
