"""Corpus registers: what KIND of text a corpus is, and how much of it to keep.

WHY THIS EXISTS
Concatenating every corpus and sampling uniformly biases hard toward the biggest
source. For en-tl, NLLB alone is 63.5M lines against translatewiki's 23k — a
2,700:1 ratio — so a 10M uniform draw takes ~4k UI lines and the short-input
register effectively does not survive. Measured on the 2026-07-21 en->tl student:
`Right`, `Pull` and `Free` occur ZERO times in the 10M it trained on, and it
emits `Emergency Exit`, `Detour` and `Cash Only` untranslated. The short band it
DID get was ~1.1M XLEnt entity pairs, so what the model learned from short input
was "this is a proper noun, pass it through".

Ratios do not fix this; absolute per-register targets do. `min(cap, available)`
means a pair with 3k UI lines contributes all 3k rather than being diluted to
nothing, which is the common case for the low-resource pairs we care about
(en-sw has 10k UI lines against 872k entity pairs).

WHY ENTITY IS CAPPED RATHER THAN DROPPED
XLEnt/WikiTitles were ADDED deliberately: without any 1-2 word pairs the student
free-runs on short input ("hallo" -> "Hello in the hello"). That fix worked and
the degeneracy checks confirm it. It was simply never bounded, so it took the
whole short band. The cap keeps the anti-degeneracy signal and leaves room for
the rest.
"""

from __future__ import annotations

import re
from collections.abc import Mapping
from dataclasses import dataclass
from enum import StrEnum


class Register(StrEnum):
    HUMAN = "human"
    UI = "ui"
    DIALOGUE = "dialogue"
    ENTITY = "entity"
    CRAWL = "crawl"


# Membership IS the allowlist — a corpus with no register is not downloaded, so
# there is one table to edit rather than a set and a mapping that can disagree.
# Excluded on purpose: ParaCrawl-Bonus (duplicate of ParaCrawl), ELRC-* (tiny
# health leaflets, mostly boilerplate).
# MultiHPLT is NOT a duplicate of HPLT despite the name: measured on en-ka they
# share 82.4% of pairs and each holds 264k the other lacks. Both are listed, and
# the cross-register dedup collapses the overlap.
REGISTER: Mapping[str, Register] = {
    "NLLB": Register.CRAWL,
    "HPLT": Register.CRAWL,
    "MultiHPLT": Register.CRAWL,
    "CCAligned": Register.CRAWL,
    "CCMatrix": Register.CRAWL,
    "ParaCrawl": Register.CRAWL,
    "WikiMatrix": Register.CRAWL,
    "wikimedia": Register.CRAWL,
    "OpenSubtitles": Register.DIALOGUE,
    "XLEnt": Register.ENTITY,
    "WikiTitles": Register.ENTITY,
    "LinguaTools-WikiTitles": Register.ENTITY,
    "GNOME": Register.UI,
    "KDE4": Register.UI,
    "translatewiki": Register.UI,
    "Ubuntu": Register.UI,
    "TED2020": Register.HUMAN,
    "Tatoeba": Register.HUMAN,
    "QED": Register.HUMAN,
    "bible-uedin": Register.HUMAN,
    "tico-19": Register.HUMAN,
}

# Which register wins when the same pair appears in two corpora. Human-checked
# text beats a mined copy of it; a UI string beats the same string scraped into
# a crawl. Applied as first-wins during the global dedup.
PRECEDENCE: tuple[Register, ...] = (
    Register.HUMAN, Register.UI, Register.DIALOGUE, Register.ENTITY, Register.CRAWL,
)


# ---------------------------------------------------------------------------
# per-register filters, appended to the common OpusCleaner chain
#
# Composition rather than a Source subclass per register: every register runs the
# same chain and adds at most a couple of rules, so subclasses would each be
# super().clean() plus one line, which spreads the chain over five classes and
# hides it. These are pure (str, str) -> (str, str) | None and compose in a list.
# ---------------------------------------------------------------------------

# printf/ICU/Qt placeholders and Java MessageFormat indices. A UI string is
# still useful with these removed ("Delete %d files" -> "Delete files"); it is
# noise with them kept, because the teacher translates them inconsistently and
# the student learns to emit stray format specifiers.
#
# The printf branch carries flags/width/precision/length: a naive `%[sdifgu]`
# misses `%.1f` and `%02d`, which is how "%.1f EB" survived into pool.ui on the
# first smoke run.
# ORDER IS LOAD-BEARING. Alternation is first-match-wins at each position, so
# every %-delimited form must precede the bare printf branch. With printf first,
# `%language%` matched `%la` (l = length modifier, a = conversion) and left
# "nguage%"; `%nation%` became "ation%", `%colony%` became "olony%". That
# silently corrupted 312 UI lines into gibberish ("the ation ve declared war on
# us") rather than leaving a placeholder behind — worse than not stripping at
# all, and invisible unless you read the pool.
PLACEHOLDER = re.compile(
    r"%[A-Za-z_][A-Za-z0-9_]*%"                                  # %NAME% — FIRST
    r"|%\([A-Za-z_]\w*\)[-+ 0#']*[\d*]*(?:\.[\d*]+)?[diouxXeEfFgGaAcsp]"  # %(name)s
    r"|%\d+\$[A-Za-z]"                                           # positional: %1$s
    r"|\{\{.{0,120}?\}\}"                                        # {{PLURAL:...|...}}, nestable
    r"|\{[^{}]{0,60}\}"                                          # {0}, {count}, {VAR: img_info}
    r"|\$\{[^}]*\}|\$\d+"                                        # ${VAR}, $1
    r"|%[-+ 0#']*[\d*]*(?:\.[\d*]+)?(?:hh|h|ll|l|L|q|j|z|t)?"
    r"[diouxXeEfFgGaAcspn%]"                                     # printf proper — LAST
)
# strftime specifiers are not printf conversions (%m, %Y) and would otherwise
# survive; they only ever appear in date-format strings, which carry no
# translatable content anyway and are dropped by the word check below.
STRFTIME = re.compile(r"%[aAbBcCdDeFgGhHIjklmMnprRsSTuUVwWxXyYzZ]")
# At least one real word (two or more letters) must survive the stripping, or the
# line was format scaffolding rather than text: "%m/" and "%02d:%02d" are not UI
# strings a translator ever saw.
WORD = re.compile(r"[^\W\d_]{2,}")
EMPTY_BRACKETS = re.compile(r"\(\s*\)|\[\s*\]|\{\s*\}")
RESIDUAL_FORMAT = re.compile(r"[%$]")
# GTK/Qt keyboard accelerators: "_Save", "&Save", "Sa&ve".
ACCEL = re.compile(r"(?<![A-Za-z0-9])[_&](?=[A-Za-z])|(?<=[A-Za-z])&(?=[A-Za-z])")
# A token mixing letters and digits is a crawl identifier, not a name:
# SilkyCat3795, Studentin2024, Picture4, Bath35137, Dream71, Asian1066028.
# Tens of thousands of these were in the 10M en->tl KD source, each one an
# example of a short line passing through untranslated.
#
# Deliberately NOT also matching internal camelCase (NGdesign): no rule
# separates it from McDonald, DeVries, iPhone or eBay, which are real entities
# this register exists to carry. The digit form is unambiguous and is the bulk
# of it; the rest stays rather than guessing.
JUNK_ID = re.compile(r"^(?=.*[A-Za-z])(?=.*\d)[A-Za-z0-9_.\-]+$")


def ui_strip(s: str, t: str) -> tuple[str, str] | None:
    """Drop placeholder-bearing UI strings; clean the rest.

    DROPPED, not stripped. Removing a placeholder leaves a HOLE, and the hole is
    ungrammatical on both sides: `%nation% have declared war` becomes "the have
    declared war on us", paired with equally broken Tagalog. That teaches the
    student the gap. Worse, it is undetectable in general — a placeholder at the
    end of a clause leaves no trace at all, so a crude article+verb scan found
    only 15 of an unknown true count.

    The cost is bounded and was measured before choosing: 6,684 of 26,393 raw
    en-tl UI lines carry a placeholder (25.3%), so three quarters of the register
    survives. Against that, a UI string minus its variable is often not a usable
    example anyway ("Delete files", "Copied of"), and UI is the register we are
    adding deliberately — its quality matters more than its volume, since one
    occurrence of a short line is ~28 exposures at the epoch counts we train at.
    """
    if PLACEHOLDER.search(s) or PLACEHOLDER.search(t):
        return None
    if STRFTIME.search(s) or STRFTIME.search(t):
        return None
    # Accelerators are NOT placeholders: "_Save" -> "Save" removes a keyboard
    # hint and leaves the string intact, so these are stripped rather than dropped.
    s, t = ACCEL.sub("", s), ACCEL.sub("", t)
    s, t = EMPTY_BRACKETS.sub(" ", s), EMPTY_BRACKETS.sub(" ", t)
    s, t = " ".join(s.split()), " ".join(t.split())
    if not WORD.search(s) or not WORD.search(t):
        return None
    # Backstop for placeholder forms the patterns above do not know about. A bare
    # % or $ surviving in a UI string is format scaffolding, not content —
    # percentages and currency are rare here and abundant in CRAWL, so the line
    # costs nothing we cannot get elsewhere. Scoped to this register on purpose:
    # "50% off" and "$20" are ordinary CRAWL sentences.
    if RESIDUAL_FORMAT.search(s) or RESIDUAL_FORMAT.search(t):
        return None
    return s, t


def entity_trim(s: str, t: str) -> tuple[str, str] | None:
    if any(JUNK_ID.match(tok) for tok in s.split()):
        return None
    # Entity/title sets are names, not sentences; anything long in them is a
    # scraped page title carrying boilerplate, and it is already represented in
    # CRAWL if it is real text.
    if len(s.split()) > 6 or len(t.split()) > 6:
        return None
    return s, t


# Leading speaker dashes are a subtitle convention, not content, and the student
# reproduces them at inference on ordinary input.
SPEAKER_DASH = re.compile(r"^\s*[-–—]\s*")


def dialogue_strip(s: str, t: str) -> tuple[str, str] | None:
    s, t = SPEAKER_DASH.sub("", s).strip(), SPEAKER_DASH.sub("", t).strip()
    if not s or not t:
        return None
    return s, t


EXTRA_FILTERS = {
    Register.UI: (ui_strip,),
    Register.ENTITY: (entity_trim,),
    Register.DIALOGUE: (dialogue_strip,),
}


def apply_extra(register: Register, s: str, t: str) -> tuple[str, str] | None:
    for f in EXTRA_FILTERS.get(register, ()):
        pair = f(s, t)
        if pair is None:
            return None
        s, t = pair
    return s, t


# ---------------------------------------------------------------------------
# the mix
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Mix:
    """Per-register line targets for a KD source draw.

    Every register must be named — either capped or the single fill — so a
    register cannot vanish by being forgotten, which is exactly how UI went
    missing when the allowlist was a flat set.
    """

    total: int
    fill: Register
    caps: Mapping[Register, int]

    def __post_init__(self) -> None:
        if self.total <= 0:
            raise ValueError(f"total must be positive, got {self.total}")
        if self.fill in self.caps:
            raise ValueError(f"{self.fill} is the fill register and cannot also be capped")
        unassigned = set(Register) - set(self.caps) - {self.fill}
        if unassigned:
            raise ValueError(
                f"unassigned registers: {sorted(r.value for r in unassigned)}; "
                "name every register (cap it, or make it the fill) so none is dropped silently"
            )
        if any(n < 0 for n in self.caps.values()):
            raise ValueError(f"negative cap in {dict(self.caps)}")
        if sum(self.caps.values()) >= self.total:
            raise ValueError(
                f"caps sum to {sum(self.caps.values())} of a {self.total} total, "
                f"leaving nothing for the {self.fill} fill"
            )

    @classmethod
    def parse(cls, spec: str, total: int) -> Mix:
        """`ui=50000,human=200000,dialogue=1000000,entity=150000,crawl=fill`"""
        caps: dict[Register, int] = {}
        fill: Register | None = None
        for item in spec.split(","):
            key, _, value = item.strip().partition("=")
            try:
                register = Register(key.strip())
            except ValueError:
                raise ValueError(
                    f"unknown register {key!r}; expected one of "
                    f"{sorted(r.value for r in Register)}"
                ) from None
            if value.strip() == "fill":
                if fill is not None:
                    raise ValueError(f"two fill registers: {fill} and {register}")
                fill = register
            else:
                caps[register] = int(value)
        if fill is None:
            raise ValueError("no fill register; one register must take the remainder")
        return cls(total=total, fill=fill, caps=caps)

    @property
    def spec(self) -> str:
        """The canonical `parse` input. Round-trips, so a flow can hold a Mix and
        still hand the step a string without the two drifting apart."""
        parts = [f"{r.value}={self.caps[r]}" for r in Register if r in self.caps]
        return ",".join([*parts, f"{self.fill.value}=fill"])

    def draw(self, available: Mapping[Register, int]) -> dict[Register, int]:
        """How many lines to take from each register, given what survived cleaning.

        Pure, so the arithmetic is testable without a corpus. A capped register
        yields min(cap, available) — a pair with 3k UI lines contributes 3k, it
        is not padded — and the fill absorbs whatever is left, which is where an
        under-supplied pair silently becomes a crawl-heavy corpus. mix.json
        records that rather than hiding it.
        """
        taken = {r: min(n, available.get(r, 0)) for r, n in self.caps.items()}
        taken[self.fill] = min(self.total - sum(taken.values()), available.get(self.fill, 0))
        return taken
