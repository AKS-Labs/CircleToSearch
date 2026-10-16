#!/usr/bin/env python3
"""Emit copies of a training row with every figure rewritten the same way on both sides.

The ka->en ft5 and ft6 finetunes re-segment digit runs: 2387 comes back "237",
7002 "702", ISO 10012:2003 "101012:2003", 1201000 "120,000" (ka_findings.md 31
and the ft6 note in LIVE.md). The luna tranche that both were trained on is
signage and price tags, so almost every figure it holds is three digits or
fewer, and a finetune on it teaches a short-figure prior that overrides what the
source actually printed. Nothing in the corpus makes the model READ the digits;
a two-digit and a seven-digit answer are both plausible under the prior.

So this rewrites the figures instead of adding data. Every figure in an eligible
row is replaced by a fresh random one, the SAME replacement in the source and in
the target, and the row is emitted K times over. The only thing that stays
constant across the variants of a row is the relation between the two sides, so
copying the source is the only strategy that fits all of them, and long runs
appear at the rate the draw asks for rather than the rate a price tag does.

WHAT "THE SAME REPLACEMENT" MEANS WHEN THE TWO SIDES PRINT DIFFERENTLY
A row may write one value two ways: "8,75 GEL" against "8.75 lari", "1 250"
against "1,250". Replacement is therefore keyed on the VALUE, in
`number_fidelity.canonical_numeral` form, and each occurrence is re-printed in
its own shape: its thousands grouping, its decimal mark, its leading zeros, its
fraction width. The two sides then still carry identical digit multisets, which
is what `number_fidelity.py` measures and what the finetune has to learn.

WHAT IS NOT TOUCHED
- A row whose two sides do not already agree on their figures. One side writing
  a figure the other spells out, or a generation defect where 250 faces 350, is
  not something to repair: the row is emitted once, unchanged.
- A row carrying a number word from the table ("twice", "ორჯერ", "2-ჯერ").
  Perturbing the digits would leave the word behind saying something else.
- Anything outside a numeral. A suffix or unit welded to the figure ("25 Nm-მდე",
  "404-ე") is left where it is, because only the digit span is substituted.

Times stay times and dates stay dates: an hour draws 0-23, a minute 0-59, a day
1-28, a month 1-12, a year 1900-2099, each printed at the width it had. Every
other figure draws a length long-tailed enough that four- to seven-digit runs are
ordinary, which is the band the failures are in.

Token counts per column never move, so a Pharaoh alignment column stays valid: a
variant whose replacement would split or join a whitespace token is dropped, and
the row's other variants are kept.

TWO DOSES, BECAUSE THE FIRST ONE MOVED THE CORPUS MIX
`Emit.APPEND` keeps the row and adds `--variants` perturbed copies. ft7 ran it at
K=3 and the digits came back (ka_findings.md 33), but the copies carry no content
the row did not already have, so the finetune half grew by half again, its
non-numeric share fell from 83% to 56%, and every short-label register was
diluted along with it. `Emit.REPLACE` emits ONE perturbed row in place of the
original for a `--share` of the eligible rows and leaves the rest alone. The row
count, the register mix and the non-numeric share are then exactly what they were,
and the model still reads real figures on the rows that were not drawn.

    perturb_numbers.py --in ft6.kaen/aligned/train.tsv --out ft7.kaen/ft.perturbed.tsv \\
      --pair ka-en --seed ka-ft7 --variants 3 --report ft7.kaen/perturb.json

    perturb_numbers.py --in ft6.kaen/aligned/train.tsv --out ft8.kaen/ft.perturbed.tsv \\
      --pair ka-en --seed ka-ft8 --mode replace --share 0.7 --report ft8.kaen/perturb.json
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import random
import re
import unicodedata
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum

from number_fidelity import (
    ALWAYS_THOUSANDS,
    CONFIG_DEFAULT,
    NUMERAL,
    SEPARATOR,
    WORD,
    canonical_numeral,
    numeral_multiset,
)

# A date written with dots is ONE numeral to `number_fidelity` ("12.05.2024" has
# no character that ends a numeral), so it is replaced as a whole. A date written
# with slashes or dashes is three, and so is a time, so those are recognised
# component by component below.
DOT_DATE = re.compile(r"(?<!\d)(\d{1,2})\.(\d{1,2})\.(\d{4})(?!\d)")
SLASH_DATE = re.compile(r"(?<!\d)(\d{1,2})([/-])(\d{1,2})\2(\d{4})(?!\d)")
ISO_DATE = re.compile(r"(?<!\d)(\d{4})-(\d{2})-(\d{2})(?!\d)")
CLOCK = re.compile(r"(?<!\d)(\d{1,2}):([0-5]\d)(?::([0-5]\d))?(?!\d)")

DIGITS = "0123456789"
DEFAULT_VARIANTS = 3

# 40% of replacements keep the original length so the corpus still holds the
# lengths the row was written with; the rest are drawn across 1-8 digits with the
# weight on 4-7, the band where the re-segmentation failures live.
SAME_LENGTH_SHARE = 0.4
LENGTH_WEIGHTS: tuple[tuple[int, int], ...] = (
    (1, 4), (2, 6), (3, 8), (4, 16), (5, 18), (6, 18), (7, 16), (8, 8),
)


class Role(StrEnum):
    """What a figure means, which is what constrains its replacement."""

    PLAIN = "plain"
    HOUR = "hour"
    MINUTE = "minute"
    DAY = "day"
    MONTH = "month"
    YEAR = "year"
    DATE = "date"


BOUNDS: Mapping[Role, tuple[int, int]] = {
    Role.HOUR: (0, 23),
    Role.MINUTE: (0, 59),
    Role.DAY: (1, 28),
    Role.MONTH: (1, 12),
    Role.YEAR: (1900, 2099),
}


class Skip(StrEnum):
    """Why a row was emitted unchanged."""

    NO_FIGURES = "no figures"
    FIGURES_DISAGREE = "figures disagree across the two sides"
    NUMBER_WORDS = "a number word would have to change with the figure"
    ROLE_CONFLICT = "one value is used in two incompatible ways"


class Emit(StrEnum):
    """What an eligible row turns into."""

    APPEND = "append"
    REPLACE = "replace"


@dataclass(frozen=True)
class Dose:
    """How much perturbed material an eligible row produces.

    Under APPEND the row survives and `variants` perturbed copies join it, which
    is what ft7 ran; under REPLACE a `share` of the eligible rows are emitted
    perturbed instead of as they were and the corpus keeps its size and its
    register mix.
    """

    mode: Emit
    variants: int
    share: float

    @property
    def copies(self) -> int:
        return self.variants if self.mode is Emit.APPEND else 1


@dataclass(frozen=True)
class PerturbConfig:
    """The per-language tables that decide whether a row may be perturbed at all.

    `words` is `number_words.json`'s table casefolded; `suffixes` names the
    endings that turn a digit into a number word where the language welds them on
    rather than spelling them out, as Georgian does with `2-ჯერ` for "twice".
    """

    words: Mapping[str, frozenset[str]]
    suffixes: Mapping[str, tuple[str, ...]]

    def words_of(self, lang: str) -> frozenset[str]:
        return self.words.get(lang, frozenset())

    def suffixes_of(self, lang: str) -> tuple[str, ...]:
        return self.suffixes.get(lang, ())


@dataclass(frozen=True)
class Scalar:
    """How one numeral is printed, so a replacement can be printed the same way."""

    integer_width: int
    leading_zeros: int
    thousands: str
    decimal: str
    fraction_width: int


@dataclass(frozen=True)
class DateForm:
    """A whole dotted date, and the widths its three components are printed at."""

    day_width: int
    month_width: int
    year_width: int


@dataclass(frozen=True)
class Occurrence:
    """One numeral in one side of a row: where it is, what it means, how it prints."""

    column: int
    start: int
    end: int
    key: str
    role: Role
    scalar: Scalar
    date: DateForm | None


@dataclass(frozen=True)
class Figure:
    """The replacement for one value, in the pieces its occurrences need.

    `integer` and `fraction` are the digits every scalar occurrence prints, the
    fraction being only the SIGNIFICANT digits so an occurrence that prints two
    decimal places and one that prints one still agree on the value. `components`
    is the day/month/year a dotted date prints, already at its widths.
    """

    integer: str
    fraction: str
    components: tuple[str, ...]


@dataclass(frozen=True)
class Row:
    """One training row: the two text columns and whatever follows them."""

    source: str
    target: str
    trailing: tuple[str, ...]

    def rendered(self, source: str, target: str) -> str:
        return "\t".join((source, target, *self.trailing))


@dataclass(frozen=True)
class RowResult:
    """What one input row produced.

    `perturbed` counts the emitted lines whose figures were redrawn, which is a
    count of added lines under APPEND and of replaced ones under REPLACE.
    """

    lines: tuple[str, ...]
    skip: Skip | None
    perturbed: int
    variants_dropped: int


def parse_config(raw: object) -> PerturbConfig:
    if not isinstance(raw, dict):
        raise ValueError("number config must be a JSON object")
    words = {
        lang: frozenset(w.casefold() for w in table)
        for lang, table in raw.get("number_words", {}).items()
    }
    suffixes = {
        lang: tuple(sorted(endings, key=len, reverse=True))
        for lang, endings in raw.get("numeral_word_suffixes", {}).items()
    }
    return PerturbConfig(words, suffixes)


def load_config(path: pathlib.Path) -> PerturbConfig:
    return parse_config(json.loads(path.read_text(encoding="utf-8")))


def parse_scalar(token: str) -> Scalar:
    """Read a numeral's printed shape, splitting it the way `canonical_numeral` does.

    A separator before exactly three digits is thousands and anything else is the
    decimal mark, so the shape a replacement is printed in is the shape the value
    was read out of and the two cannot disagree.
    """
    groups = SEPARATOR.split(token)
    separators = SEPARATOR.findall(token)
    integer, fraction = groups[0], ""
    thousands = decimal = ""
    for separator, group in zip(separators, groups[1:]):
        if (separator in ALWAYS_THOUSANDS or len(group) == 3) and not fraction:
            integer += group
            thousands = separator
        else:
            if not fraction:
                decimal = separator
            fraction += group
    return Scalar(len(integer), len(integer) - len(integer.lstrip("0")),
                  thousands, decimal, len(fraction))


def role_spans(text: str) -> dict[tuple[int, int], Role]:
    """The span of every digit run whose meaning the surrounding punctuation fixes.

    Keyed by span so a numeral is given a role only when it IS that component
    exactly. "05 30" after a time is one numeral to `number_fidelity`, overlaps
    the minute and is not it, and stays plain.
    """
    spans: dict[tuple[int, int], Role] = {}
    for match in CLOCK.finditer(text):
        for group, role in ((1, Role.HOUR), (2, Role.MINUTE), (3, Role.MINUTE)):
            if match.group(group) is not None:
                spans[match.span(group)] = role
    for match in SLASH_DATE.finditer(text):
        for group, role in ((1, Role.DAY), (3, Role.MONTH), (4, Role.YEAR)):
            spans[match.span(group)] = role
    for match in ISO_DATE.finditer(text):
        for group, role in ((1, Role.YEAR), (2, Role.MONTH), (3, Role.DAY)):
            spans[match.span(group)] = role
    for match in DOT_DATE.finditer(text):
        spans[match.span()] = Role.DATE
    return spans


def scan(text: str, column: int) -> list[Occurrence]:
    """Every numeral of one side, in `number_fidelity`'s own decomposition.

    Occurrences are the NUMERAL matches and nothing else, so the keys this
    produces are exactly the multiset that decides eligibility.
    """
    roles = role_spans(text)
    found = []
    for match in NUMERAL.finditer(text):
        span = match.span()
        token = match.group()
        role = roles.get(span, Role.PLAIN)
        date = None
        if role is Role.DATE:
            day, month, year = DOT_DATE.match(token).groups()
            date = DateForm(len(day), len(month), len(year))
        found.append(Occurrence(column, span[0], span[1], canonical_numeral(token),
                                role, parse_scalar(token), date))
    return found


def number_words_present(text: str, words: frozenset[str], suffixes: Sequence[str]) -> bool:
    normalised = unicodedata.normalize("NFC", text)
    if any(word.casefold() in words for word in WORD.findall(normalised)):
        return True
    return any(re.search(rf"\d-?{re.escape(s)}(?!\w)", normalised) for s in suffixes)


def bound_of(occurrences: Sequence[Occurrence]) -> tuple[int, int] | None:
    """The range every occurrence of one value agrees on, or None when it is free.

    A value used as both an hour and a minute has to satisfy both; a value used
    as an hour somewhere and as a plain figure elsewhere is still an hour,
    because the clock is the reading that can be wrong.
    """
    low, high = 0, None
    for occurrence in occurrences:
        if occurrence.role not in BOUNDS:
            continue
        role_low, role_high = BOUNDS[occurrence.role]
        low = max(low, role_low)
        high = role_high if high is None else min(high, role_high)
    return None if high is None else (low, high)


def draw_length(rng: random.Random, original: int) -> int:
    if rng.random() < SAME_LENGTH_SHARE:
        return original
    lengths = [length for length, _ in LENGTH_WEIGHTS]
    weights = [weight for _, weight in LENGTH_WEIGHTS]
    return rng.choices(lengths, weights)[0]


def draw_integer(rng: random.Random, form: Scalar, fixed_width: bool) -> str:
    """Integer digits for a free value, keeping whatever the original zero-padded.

    A figure with a leading zero is a code or a padded field ("Error 07"), so its
    width is part of what the row prints and the draw keeps it; a figure that is
    all zeros stays zero, since there is no other number that shape can hold.
    """
    if form.leading_zeros == form.integer_width:
        return "0" * form.integer_width
    if form.leading_zeros or fixed_width:
        body = form.integer_width - form.leading_zeros
        head = rng.choice(DIGITS[1:]) if body > 1 or form.leading_zeros else rng.choice(DIGITS)
        return "0" * form.leading_zeros + head + "".join(rng.choices(DIGITS, k=body - 1))
    width = draw_length(rng, form.integer_width)
    if width == 1:
        return rng.choice(DIGITS)
    return rng.choice(DIGITS[1:]) + "".join(rng.choices(DIGITS, k=width - 1))


def draw_figure(rng: random.Random, occurrences: Sequence[Occurrence]) -> Figure:
    """One replacement for one value, satisfying every place the row prints it."""
    form = occurrences[0].scalar
    if occurrences[0].role is Role.DATE:
        date = occurrences[0].date
        components = (
            f"{rng.randint(*BOUNDS[Role.DAY]):0{date.day_width}d}",
            f"{rng.randint(*BOUNDS[Role.MONTH]):0{date.month_width}d}",
            f"{rng.randint(*BOUNDS[Role.YEAR]):0{date.year_width}d}",
        )
        return Figure("", "", components)

    significant = min(o.scalar.fraction_width for o in occurrences)
    fraction = "".join(rng.choices(DIGITS, k=significant))
    bound = bound_of(occurrences)
    if bound is not None:
        low, high = bound
        integer = f"{rng.randint(low, min(high, 10 ** form.integer_width - 1)):0{form.integer_width}d}"
        return Figure(integer, fraction, ())

    # A group separator that is itself a space cannot change how many groups the
    # figure has without changing how many whitespace tokens the row has, so
    # those figures keep their length rather than spending variants on the drop.
    fixed_width = any(o.scalar.thousands in ALWAYS_THOUSANDS for o in occurrences)
    return Figure(draw_integer(rng, form, fixed_width), fraction, ())


def group_thousands(digits: str, separator: str) -> str:
    if not separator:
        return digits
    head = len(digits) % 3 or 3
    return separator.join([digits[:head]] + [digits[i:i + 3] for i in range(head, len(digits), 3)])


def render(occurrence: Occurrence, figure: Figure) -> str:
    if occurrence.role is Role.DATE:
        return ".".join(figure.components)
    form = occurrence.scalar
    out = group_thousands(figure.integer, form.thousands)
    if form.decimal:
        out += form.decimal + figure.fraction.ljust(form.fraction_width, "0")
    return out


def substitute(text: str, occurrences: Sequence[Occurrence], figures: Mapping[str, Figure]) -> str:
    out, cursor = [], 0
    for occurrence in occurrences:
        out.append(text[cursor:occurrence.start])
        out.append(render(occurrence, figures[occurrence.key]))
        cursor = occurrence.end
    out.append(text[cursor:])
    return "".join(out)


def eligibility(row: Row, config: PerturbConfig, source_lang: str, target_lang: str) -> Skip | None:
    figures = numeral_multiset(row.source)
    if not figures:
        return Skip.NO_FIGURES
    if figures != numeral_multiset(row.target):
        return Skip.FIGURES_DISAGREE
    for text, lang in ((row.source, source_lang), (row.target, target_lang)):
        if number_words_present(text, config.words_of(lang), config.suffixes_of(lang)):
            return Skip.NUMBER_WORDS
    return None


def perturb_row(row: Row, config: PerturbConfig, source_lang: str, target_lang: str,
                dose: Dose, rng: random.Random) -> RowResult:
    original = row.rendered(row.source, row.target)
    skip = eligibility(row, config, source_lang, target_lang)
    if skip is not None:
        return RowResult((original,), skip, 0, 0)

    occurrences = scan(row.source, 0) + scan(row.target, 1)
    by_key: dict[str, list[Occurrence]] = collections.defaultdict(list)
    for occurrence in occurrences:
        by_key[occurrence.key].append(occurrence)
    for group in by_key.values():
        bound = bound_of(group)
        roles = {o.role for o in group}
        if (Role.DATE in roles and len(roles) > 1) or (bound is not None and bound[0] > bound[1]):
            return RowResult((original,), Skip.ROLE_CONFLICT, 0, 0)

    # APPEND spends no randomness on the share, which is always 1.0 there, so the
    # ft7 corpus still comes back byte for byte from its own seed.
    drawn = dose.mode is Emit.APPEND or rng.random() < dose.share
    source_spans = [o for o in occurrences if o.column == 0]
    target_spans = [o for o in occurrences if o.column == 1]
    source_tokens, target_tokens = len(row.source.split()), len(row.target.split())
    perturbed: list[str] = []
    dropped = 0
    for _ in range(dose.copies if drawn else 0):
        figures = {key: draw_figure(rng, group) for key, group in by_key.items()}
        source = substitute(row.source, source_spans, figures)
        target = substitute(row.target, target_spans, figures)
        if len(source.split()) != source_tokens or len(target.split()) != target_tokens:
            dropped += 1
            continue
        perturbed.append(row.rendered(source, target))

    if dose.mode is Emit.APPEND:
        return RowResult((original, *perturbed), None, len(perturbed), dropped)
    # A replacement whose token counts moved is dropped, and then the row it was
    # replacing is what stands, so REPLACE emits exactly one line per input row.
    return RowResult(tuple(perturbed) or (original,), None, len(perturbed), dropped)


def parse_row(line: str, number: int) -> Row:
    columns = line.split("\t")
    if len(columns) < 2:
        raise ValueError(f"line {number}: a training row needs at least a source and a target column")
    return Row(columns[0], columns[1], tuple(columns[2:]))


def parse_dose(mode: Emit, variants: int | None, share: float) -> Dose:
    """The dose the two flags describe, refusing the combinations that mean nothing.

    A share belongs to REPLACE, which has one row slot per input row to give away,
    and a variant count belongs to APPEND, where the copies are additional.
    Accepting either flag in the other mode would silently produce a corpus whose
    composition is not the one the caller asked for.
    """
    if not 0.0 < share <= 1.0:
        raise ValueError(f"--share is a fraction of the eligible rows, got {share}")
    if mode is Emit.APPEND:
        if share != 1.0:
            raise ValueError("--share applies to replace mode; append adds every variant it draws")
        variants = DEFAULT_VARIANTS if variants is None else variants
        if variants < 1:
            raise ValueError("--variants must be at least 1 in append mode")
        return Dose(mode, variants, 1.0)
    if variants is not None:
        raise ValueError("--variants applies to append mode; replace emits one row per input row")
    return Dose(mode, 1, share)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--in", dest="source", type=pathlib.Path, required=True,
                    help="training TSV, src<TAB>trg[<TAB>alignment]")
    ap.add_argument("--out", type=pathlib.Path, required=True)
    ap.add_argument("--pair", required=True, help="e.g. ka-en: the language of each text column")
    ap.add_argument("--seed", required=True)
    ap.add_argument("--mode", type=Emit, choices=list(Emit), default=Emit.APPEND,
                    help="append: keep the row and add variants; replace: perturb it in place")
    ap.add_argument("--variants", type=int, default=None,
                    help=f"append mode: perturbed copies added to each eligible row "
                         f"(default {DEFAULT_VARIANTS})")
    ap.add_argument("--share", type=float, default=1.0,
                    help="replace mode: fraction of eligible rows whose figures are redrawn")
    ap.add_argument("--config", type=pathlib.Path, default=CONFIG_DEFAULT)
    ap.add_argument("--report", type=pathlib.Path, default=None)
    ap.add_argument("--samples", type=int, default=20, help="perturbed rows to print")
    args = ap.parse_args()

    source_lang, target_lang = args.pair.split("-")
    dose = parse_dose(args.mode, args.variants, args.share)
    config = load_config(args.config)
    skipped: collections.Counter[str] = collections.Counter()
    rows = eligible = emitted = dropped = written = 0
    samples: list[dict[str, str]] = []

    with args.source.open(encoding="utf-8") as handle, args.out.open("w", encoding="utf-8") as out:
        for number, line in enumerate(handle, 1):
            row = parse_row(line.rstrip("\n"), number)
            result = perturb_row(row, config, source_lang, target_lang, dose,
                                 random.Random(f"{args.seed}\x00{number}"))
            rows += 1
            written += len(result.lines)
            dropped += result.variants_dropped
            if result.skip is None:
                eligible += 1
                emitted += result.perturbed
                if len(samples) < args.samples and result.perturbed:
                    samples.append({"src": row.source, "trg": row.target,
                                    "variant_src": result.lines[-1].split("\t")[0],
                                    "variant_trg": result.lines[-1].split("\t")[1]})
            else:
                skipped[str(result.skip)] += 1
            out.write("".join(line + "\n" for line in result.lines))

    report = {
        "rows_in": rows,
        "eligible": eligible,
        "perturbed_emitted": emitted,
        "variants_dropped_for_tokenisation": dropped,
        "rows_out": written,
        "skipped": dict(skipped),
        "seed": args.seed,
        "mode": str(dose.mode),
        "variants_asked": dose.variants,
        "share_asked": dose.share,
        "samples": samples,
    }
    print(json.dumps({k: v for k, v in report.items() if k != "samples"}, indent=2))
    for sample in samples:
        print(f"  SRC {sample['src']}\n  ->  {sample['variant_src']}")
        print(f"  TRG {sample['trg']}\n  ->  {sample['variant_trg']}")
    if args.report:
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
