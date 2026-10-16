"""Spec, job grid and gates for language-parametric bilingual pair generation.

Everything here is pure: a function of the spec plus text that has already been
fetched. The generator shell (gen_pairs.py) owns the model calls and the files.

WHY PAIRS ARE GENERATED IN ONE CALL
Authoring the target text and translating it in a second pass loses the setting
that disambiguates it. A bare label off a hardware shelf, a dosage line, a device
error string and a menu heading all read differently depending on where they are
printed, and the second call never sees the shelf. One call that writes both
sides in the same setting keeps the register attached to the pair, and a pair
whose two sides come out of one generation agrees on its digits by construction,
so the number gate below is checking for a slip rather than doing the alignment.

WHY THE GRID HAS FORMS AND BANDS
A camera meets a surface form, not a headword, and the shipped student fails on
exactly the short end: sense inversions on two-word signs, dropped figures in
dosages, garbled safety instructions. Asking one category for "some text" returns
the same handful of nominative noun labels every time. Register x category x
surface form x length band makes each ask a different corner of the same setting,
and the per-cell resume unit lets a thin corner be re-asked on its own.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import re
import unicodedata
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import StrEnum


class SpecError(ValueError):
    """A spec file that cannot be parsed. Raised at the boundary, once."""


class NumberPolicy(StrEnum):
    """Whether a cell demands a figure in every row."""

    FREE = "free"
    REQUIRED = "required"


class GateReason(StrEnum):
    """Why a generated row is not usable. `None` means the row passed."""

    EMPTY = "empty"
    IDENTICAL = "identical"
    CONTROL = "control"
    SCRIPT = "script"
    TARGET_IN_EN = "target_in_en"
    LATIN_LEAK = "latin_leak"
    NUMBER_MISMATCH = "number_mismatch"
    NUMBER_MISSING = "number_missing"
    LENGTH_RATIO = "length_ratio"
    BAND = "band"


class ColumnOrder(StrEnum):
    """Which side leads in the emitted TSV."""

    EN_FIRST = "en_first"
    TARGET_FIRST = "target_first"


@dataclass(frozen=True)
class Band:
    key: str
    lo: int
    hi: int
    text: str


@dataclass(frozen=True)
class Form:
    key: str
    text: str
    bands: tuple[str, ...]
    rows: int
    numbers: NumberPolicy


@dataclass(frozen=True)
class RegisterSpec:
    name: str
    blurb: str
    categories: tuple[str, ...]
    forms: tuple[str, ...]


@dataclass(frozen=True)
class ScriptGate:
    ranges: tuple[tuple[int, int], ...]
    min_share: float

    def share(self, s: str) -> float:
        letters = [c for c in s if c.isalpha()]
        if not letters:
            return 0.0
        hit = sum(any(lo <= ord(c) <= hi for lo, hi in self.ranges) for c in letters)
        return hit / len(letters)

    def contains(self, s: str) -> bool:
        return any(any(lo <= ord(c) <= hi for lo, hi in self.ranges) for c in s)

    def foreign_letters(self, s: str) -> str:
        """Letters that are neither the target script nor Latin.

        The share test above passes a line that is mostly Georgian, which is how
        `直接 კანზე წაუსვით` and an Arabic fragment inside a nutrition line
        reached a build. Latin is exempted here and judged by the allowlist
        instead, because Latin is what every script borrows brand names, model
        codes and units from; any OTHER script in the target column is corruption.
        """
        return "".join(
            c for c in s
            if c.isalpha()
            and not any(lo <= ord(c) <= hi for lo, hi in self.ranges)
            and not any(lo <= ord(c) <= hi for lo, hi in LATIN_RANGES)
        )


# ASCII plus Latin-1 Supplement and Latin Extended-A, so a brand name keeps its
# accents (Nescafé) without opening the column to every script.
LATIN_RANGES = ((0x41, 0x5A), (0x61, 0x7A), (0xC0, 0x24F))


@dataclass(frozen=True)
class Fold:
    """A codepoint block that is the same script in a case the target never prints.

    Georgian is the reason this exists: a model that has seen Mtavruli headings
    emits them, and Mtavruli in training data teaches a casing system Georgian
    prose does not have. Folding is a pure shift, so the row is kept rather than
    thrown away.
    """

    name: str
    lo: int
    hi: int
    delta: int

    def apply(self, s: str) -> str:
        return "".join(
            chr(ord(c) + self.delta) if self.lo <= ord(c) <= self.hi else c for c in s
        )


@dataclass(frozen=True)
class Spec:
    language: str
    code: str
    country: str
    system: str
    prompt_version: str
    notes: tuple[str, ...]
    script: ScriptGate
    folds: tuple[Fold, ...]
    latin_allow: frozenset[str]
    max_latin_run: int
    word_ratio: tuple[float, float]
    word_ratio_slack: tuple[int, int]
    band_slack: int
    have_cap_category: int
    have_cap_global: int
    bands: Mapping[str, Band]
    forms: Mapping[str, Form]
    registers: tuple[RegisterSpec, ...]
    sha: str


@dataclass(frozen=True)
class Job:
    """One generation call. `key` is the resume unit and the output filename."""

    key: str
    register: str
    category: str
    form: str
    band: str
    n: int
    numbers: NumberPolicy
    prompt: str


@dataclass(frozen=True)
class PairRow:
    en: str
    target: str


# ----------------------------------------------------------------- spec loading


def _codepoint(raw: object, where: str) -> int:
    if isinstance(raw, int):
        return raw
    if isinstance(raw, str) and raw:
        return int(raw, 16) if len(raw) > 1 else ord(raw)
    raise SpecError(f"{where}: expected a codepoint (hex string or int), got {raw!r}")


def _ranges(raw: object, where: str) -> tuple[tuple[int, int], ...]:
    if not isinstance(raw, list) or not raw:
        raise SpecError(f"{where}: expected a non-empty list of [lo, hi] pairs")
    out = []
    for pair in raw:
        if not isinstance(pair, list) or len(pair) != 2:
            raise SpecError(f"{where}: expected [lo, hi] pairs, got {pair!r}")
        out.append((_codepoint(pair[0], where), _codepoint(pair[1], where)))
    return tuple(out)


def parse_spec(raw: Mapping[str, object], sha: str) -> Spec:
    try:
        bands = {
            key: Band(key=key, lo=int(b["lo"]), hi=int(b["hi"]), text=str(b["text"]))
            for key, b in raw["bands"].items()
        }
        forms = {
            key: Form(
                key=key,
                text=str(f["text"]),
                bands=tuple(str(b) for b in f["bands"]),
                rows=int(f["rows"]),
                numbers=NumberPolicy(str(f["numbers"])),
            )
            for key, f in raw["forms"].items()
        }
        registers = tuple(
            RegisterSpec(
                name=str(r["name"]),
                blurb=str(r["blurb"]),
                categories=tuple(str(c) for c in r["categories"]),
                forms=tuple(str(f) for f in r["forms"]),
            )
            for r in raw["registers"]
        )
        gates = raw["gates"]
        spec = Spec(
            language=str(raw["language"]),
            code=str(raw["code"]),
            country=str(raw["country"]),
            system=str(raw["system"]),
            prompt_version=str(raw["prompt_version"]),
            notes=tuple(str(n) for n in raw["target_notes"]),
            script=ScriptGate(
                ranges=_ranges(gates["script_ranges"], "gates.script_ranges"),
                min_share=float(gates["script_min_share"]),
            ),
            folds=tuple(
                Fold(
                    name=str(f["name"]),
                    lo=_codepoint(f["lo"], "gates.folds.lo"),
                    hi=_codepoint(f["hi"], "gates.folds.hi"),
                    delta=int(f["delta"]),
                )
                for f in gates["folds"]
            ),
            latin_allow=frozenset(str(w).casefold() for w in gates["latin_allow"]),
            max_latin_run=int(gates["max_latin_run"]),
            word_ratio=(float(gates["word_ratio"][0]), float(gates["word_ratio"][1])),
            word_ratio_slack=(int(gates["word_ratio_slack"][0]),
                              int(gates["word_ratio_slack"][1])),
            band_slack=int(gates["band_slack"]),
            have_cap_category=int(raw["have_cap_category"]),
            have_cap_global=int(raw["have_cap_global"]),
            bands=bands,
            forms=forms,
            registers=registers,
            sha=sha,
        )
    except (KeyError, TypeError, AttributeError) as e:
        raise SpecError(f"malformed spec: {e}") from e

    for form in spec.forms.values():
        unknown = [b for b in form.bands if b not in spec.bands]
        if unknown:
            raise SpecError(f"form {form.key!r} names unknown bands {unknown}")
    for reg in spec.registers:
        unknown = [f for f in reg.forms if f not in spec.forms]
        if unknown:
            raise SpecError(f"register {reg.name!r} names unknown forms {unknown}")
        if not reg.categories:
            raise SpecError(f"register {reg.name!r} has no categories")
    return spec


def load_spec(path: pathlib.Path) -> Spec:
    body = path.read_bytes()
    sha = hashlib.sha256(body).hexdigest()[:12]
    return parse_spec(json.loads(body.decode("utf-8")), sha)


# ------------------------------------------------------------------- job grid


# The spec's `prompt_version` names the prompt AS A WHOLE, these rules included,
# so a change here is a version bump there: a row records which prompt wrote it,
# and two rows labelled the same version must have been asked the same way.
RULES = """Rules:
- Both sides must be text that is really printed, and each side must read
  naturally on its own. The {code} is what is written in {country}; the English
  is what the same sign, label or screen says in an English-speaking country.
- A grammar gloss is not a sign. "From card", "By account", "With code" render a
  suffix, not a notice. If English would not print it in this setting, leave the
  row out.
- Keep the sense the text carries HERE. A word with several senses takes the one
  this setting gives it, and the English must carry that same sense -- an
  inverted or neighbouring sense ("sign out" as "sign in", "yield" as "produce")
  is the worst possible row.
- The two sides must carry the SAME information. Do not put a detail on one side
  that the other side lacks, and never shorten one side to fit the length band by
  dropping what the other says: write a shorter pair instead.
- The English must be the wording an English label really prints for that
  situation ("Avoid contact with eyes"), not a compressed paraphrase of it
  ("Avoid eye contact").
- VARY THE WORDING. Do not reuse one template with a word swapped. Where two rows
  say the same thing, they must differ in surface form: a different construction,
  a different length, the polite and the plain version, the wordy and the terse.
- No duplicates, no numbering, no commentary, no transliteration, no romanization.
- No {language} letters in the English side. Units and currency there are
  written in English (12.50 GEL, 250 ml, 30 minutes), never in {language}.
- No English or Latin-script words in the {code} side except brand names, model
  codes and the units that {language} itself prints in Latin (Wi-Fi, PIN, SIM,
  kg, ml, USB).
- Both sides on one line each: no line breaks, no tabs, no markdown."""

NUMBERS_RULE = """Numbers (this list):
- EVERY row must carry at least one figure: a quantity, a unit, a price, a time,
  a date, a code or a percentage.
- Every digit in one side must appear in the other, unchanged. Do not round, do
  not reformat, do not convert units, do not drop a figure.
- Use MULTI-DIGIT values on most rows: 3 and 4 digit numbers, decimals, ranges
  and pairs of figures, not only single digits. Move the figure around the line."""


def _slug(s: str) -> str:
    return re.sub(r"\W+", "_", s).strip("_")


def _already_have(
    category: str, have: Mapping[str, Sequence[str]], spec: Spec
) -> str:
    """Name what this cell already holds, so the repeat budget buys new rows.

    Listed in ENGLISH rather than in the target: the English side is what a
    category repeats first, it costs roughly half the tokens of the same list in
    Georgian, and dedup drops a row on a collision in EITHER column, so
    suppressing the English repeat suppresses the pair.
    """
    if not have:
        return ""
    items = list(have.get(category, ())[: spec.have_cap_category])
    items += list(have.get("", ())[: spec.have_cap_global])
    items = list(dict.fromkeys(items))
    if not items:
        return ""
    return (
        "\n\nALREADY WRITTEN -- do not output any of these, and do not output a "
        "near-variant of one. Spend the list on text this set does not hold:\n"
        + "\n".join(f"- {i}" for i in items)
    )


def build_prompt(
    spec: Spec, reg: RegisterSpec, category: str, form: Form, band: Band, n: int,
    have: Mapping[str, Sequence[str]],
) -> str:
    notes = "\n".join(f"- {n}" for n in spec.notes)
    numbers = f"\n\n{NUMBERS_RULE}" if form.numbers is NumberPolicy.REQUIRED else ""
    rules = RULES.format(code=spec.code, country=spec.country, language=spec.language)
    return (
        f"You are documenting real {reg.blurb} as it appears in {spec.country}, "
        f"specifically: {category}.\n\n"
        f"Write {n} DISTINCT bilingual rows. Each row is one piece of text, given "
        f'twice: "{spec.code}" is the {spec.language} as it is actually written and '
        f'printed in {spec.country} for this setting, and "en" is what the same '
        f"text says in an English-speaking country. The {spec.language} is not a "
        f"translation of the English and the English is not a gloss of the "
        f"{spec.language}; they are the two versions of the same thing.\n\n"
        f"Surface form for this list: {form.text}\n\n"
        f"Length for this list: {band.text} Aim for {band.lo} to {band.hi} English "
        f"words per row.{numbers}\n\n"
        f"{spec.language} usage:\n{notes}\n\n"
        f"{rules}"
        f"{_already_have(category, have, spec)}\n\n"
        f"Output ONLY a JSON array of {n} objects, each "
        f'{{"en": "<English text>", "{spec.code}": "<{spec.language} text>"}}.'
    )


def build_jobs(
    spec: Spec,
    rounds: int,
    have: Mapping[str, Sequence[str]],
    registers: Sequence[str] = (),
    forms: Sequence[str] = (),
    bands: Sequence[str] = (),
    per_cell: int = 0,
) -> list[Job]:
    unknown = [r for r in registers if r not in {x.name for x in spec.registers}]
    if unknown:
        raise SpecError(f"unknown registers {unknown}")
    jobs: list[Job] = []
    for reg in spec.registers:
        if registers and reg.name not in registers:
            continue
        for category in reg.categories:
            for form_key in reg.forms:
                if forms and form_key not in forms:
                    continue
                form = spec.forms[form_key]
                for band_key in form.bands:
                    if bands and band_key not in bands:
                        continue
                    band = spec.bands[band_key]
                    n = per_cell or form.rows
                    for r in range(rounds):
                        jobs.append(
                            Job(
                                key=f"{reg.name}.{_slug(category)}.{form_key}."
                                    f"{band_key}.r{r}",
                                register=reg.name,
                                category=category,
                                form=form_key,
                                band=band_key,
                                n=n,
                                numbers=form.numbers,
                                prompt=build_prompt(
                                    spec, reg, category, form, band, n, have
                                ),
                            )
                        )
    return jobs


# ------------------------------------------------------------------- parsing


def parse_rows(payload: object, spec: Spec) -> tuple[list[PairRow], int]:
    """(rows, malformed). A bad row invalidates that row only.

    A payload that is not an array is a malformed BATCH and is the caller's
    problem: it raises, so the shell can retry the call once and then record the
    job as failed rather than silently banking a fraction of it.
    """
    if not isinstance(payload, list):
        raise ValueError(f"expected a JSON array, got {type(payload).__name__}")
    rows: list[PairRow] = []
    malformed = 0
    for item in payload:
        if not isinstance(item, dict):
            malformed += 1
            continue
        en, target = item.get("en"), item.get(spec.code)
        if not isinstance(en, str) or not isinstance(target, str):
            malformed += 1
            continue
        en, target = en.strip(), fold(spec, target.strip())
        if not en or not target:
            malformed += 1
            continue
        rows.append(PairRow(en=en, target=target))
    return rows, malformed


def fold(spec: Spec, s: str) -> str:
    for f in spec.folds:
        s = f.apply(s)
    return s


# --------------------------------------------------------------------- gates


NUM = re.compile(r"\d+(?:[.,  ]\d+)*")
DIGITS = re.compile(r"\d")
LATIN_RUN = re.compile(r"[A-Za-z]+")
MULTISPACE = re.compile(r"\s+")
PUNCT = re.compile(r"[^\w\s]", re.UNICODE)
CONTROL = re.compile(r"[\t\n\r\x00-\x08\x0b-\x1f]")

# The number vocabulary is English-side only: the target column carries its own
# number words, which we do not enumerate, so a figure written out in the target
# is not accepted as a match. Rejecting it costs a row; accepting it blind would
# teach the model the exact corruption this set exists to repair.
ONES = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight",
        "nine", "ten", "eleven", "twelve", "thirteen", "fourteen", "fifteen",
        "sixteen", "seventeen", "eighteen", "nineteen", "twenty"]
ORDINALS = ["zeroth", "first", "second", "third", "fourth", "fifth", "sixth",
            "seventh", "eighth", "ninth", "tenth", "eleventh", "twelfth",
            "thirteenth", "fourteenth", "fifteenth", "sixteenth", "seventeenth",
            "eighteenth", "nineteenth", "twentieth"]
TENS = {30: "thirty", 40: "forty", 50: "fifty", 60: "sixty", 70: "seventy",
        80: "eighty", 90: "ninety"}
SCALE = {100: ("hundred",), 1000: ("thousand",), 1000000: ("million",),
         1000000000: ("billion",)}
MULT = {1: ("once", "single"), 2: ("twice", "double", "twofold"),
        3: ("thrice", "triple", "threefold"), 4: ("quadruple", "fourfold")}
MONTHS = ["january", "february", "march", "april", "may", "june", "july",
          "august", "september", "october", "november", "december"]


def _words_for(value: int) -> set[str]:
    out: set[str] = set()
    if 0 <= value < len(ONES):
        out.add(ONES[value])
    if 0 <= value < len(ORDINALS):
        out.add(ORDINALS[value])
    if value in TENS:
        out.add(TENS[value])
        out.add(TENS[value][:-1] + "ieth")
    out.update(SCALE.get(value, ()))
    out.update(MULT.get(value, ()))
    return out


def _digits_of(tok: str) -> str:
    return "".join(DIGITS.findall(tok))


def _parts(tok: str) -> list[str]:
    return [p for p in re.split(r"[.,  ]", tok) if p]


def _matches(tok: str, other_digits: set[str], other_parts: set[str],
             words: set[str]) -> bool:
    d = _digits_of(tok)
    if d in other_digits or d in other_parts:
        return True
    parts = _parts(tok)
    # A date written 15.03.2024 may come back as "15 March 2024": each component
    # must still be accounted for, by digits or by a month name.
    if len(parts) > 1 and all(len(p) <= 4 for p in parts):
        for p in parts:
            stripped = p.lstrip("0") or "0"
            if p in other_digits or p in other_parts or stripped in other_parts:
                continue
            if p.isdigit() and 1 <= int(p) <= 12 and MONTHS[int(p) - 1] in words:
                continue
            if p.isdigit() and _words_for(int(p)) & words:
                continue
            break
        else:
            return True
    if d.isdigit() and len(d) <= 4:
        stripped = d.lstrip("0") or "0"
        if stripped in other_digits or stripped in other_parts:
            return True
        if _words_for(int(d)) & words:
            return True
    return False


def numbers_agree(target: str, en: str) -> bool:
    """Every figure on one side is accounted for on the other.

    Symmetric on purpose: a figure dropped from the English and a figure invented
    in the English are the same defect seen from two ends, and the invented one is
    the more dangerous because it stays plausible (2039 read back as 2019).
    """
    t_toks, en_toks = NUM.findall(target), NUM.findall(en)
    en_digits = {_digits_of(t) for t in en_toks}
    en_parts = {p for t in en_toks for p in _parts(t)}
    en_parts |= {(p.lstrip("0") or "0") for p in en_parts}
    t_digits = {_digits_of(t) for t in t_toks}
    t_parts = {p for t in t_toks for p in _parts(t)}
    t_parts |= {(p.lstrip("0") or "0") for p in t_parts}
    en_words = set(re.findall(r"[a-z]+", en.lower()))

    if any(not _matches(t, en_digits, en_parts, en_words) for t in t_toks):
        return False
    return all(_matches(t, t_digits, t_parts, set()) for t in en_toks)


def gate(spec: Spec, row: PairRow, band: Band, numbers: NumberPolicy) -> GateReason | None:
    en, target = row.en, row.target
    if not en or not target:
        return GateReason.EMPTY
    if CONTROL.search(en) or CONTROL.search(target):
        return GateReason.CONTROL
    if norm(en) == norm(target):
        return GateReason.IDENTICAL
    if spec.script.share(target) < spec.script.min_share:
        return GateReason.SCRIPT
    if spec.script.foreign_letters(target):
        return GateReason.SCRIPT
    if spec.script.contains(en):
        return GateReason.TARGET_IN_EN
    for run in LATIN_RUN.findall(target):
        if len(run) >= spec.max_latin_run and run.casefold() not in spec.latin_allow:
            return GateReason.LATIN_LEAK
    if numbers is NumberPolicy.REQUIRED and not (DIGITS.search(en) and DIGITS.search(target)):
        return GateReason.NUMBER_MISSING
    if not numbers_agree(target, en):
        return GateReason.NUMBER_MISMATCH
    # Counted in WORDS, not characters, and with an absolute allowance at both
    # ends: a two-word label legitimately runs from ხაჭო to a five-word Georgian
    # prohibition, so a character ratio rejects real pairs at the short end,
    # which is the whole band this set exists to buy. What the gate is actually
    # looking for is one side truncated to a stub, and that shows in word counts.
    en_words, target_words = len(en.split()), len(target.split())
    lo = spec.word_ratio[0] * en_words - spec.word_ratio_slack[0]
    hi = spec.word_ratio[1] * en_words + spec.word_ratio_slack[1]
    if not lo <= target_words <= hi:
        return GateReason.LENGTH_RATIO
    if not band.lo - spec.band_slack <= en_words <= band.hi + spec.band_slack:
        return GateReason.BAND
    return None


# ------------------------------------------------------- dedup and exclusion


def norm(s: str) -> str:
    """Normalised key for dedup and for eval exclusion: NFC, no punctuation,
    collapsed space, casefolded. Georgian has no case, but the English column
    does and `Exit` and `EXIT` are one line for our purposes."""
    s = unicodedata.normalize("NFC", s)
    return PUNCT.sub("", MULTISPACE.sub(" ", s)).strip().casefold()


def sha(s: str) -> str:
    """Digest as `data/eval_exclude.sha256` stores it: raw text, stripped only."""
    return hashlib.sha256(s.strip().encode("utf-8")).hexdigest()
