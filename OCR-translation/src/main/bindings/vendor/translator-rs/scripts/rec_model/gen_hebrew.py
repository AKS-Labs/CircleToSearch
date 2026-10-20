"""Synthetic Hebrew text-line strips for PP-OCRv6 rec fine-tuning.

Hebrew is RTL with whole-line reordering, which a monotonic CTC can't learn from
logical labels — so the label is the VISUAL-order string (python-bidi get_display)
and we render that with no further reordering (recgen Spec.reorder=False). Inference
recovers logical order downstream (run-grouped bidi). Shared rendering/degradation/
CLI live in recgen.py.

  python gen_hebrew.py --out /tmp/heb --n 2000 --corpus hebrew_corpus.txt
  python gen_hebrew.py --out /tmp/heb-insp --n 24 --inspect --corpus hebrew_corpus.txt
"""

import random
import unicodedata as ud
from functools import lru_cache

from bidi import get_display

import recgen

HEBREW_LETTERS = "".join(chr(c) for c in range(0x05D0, 0x05EB))  # א..ת incl finals
LATIN = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
DIGITS = "0123456789"
COMMON_PUNCT = " .,:;!?'\"()/-"
# Rare in cleaned wiki/news prose (typographic marks normalized to ASCII, or just scarce)
# but real and worth recognizing — kept regardless of corpus frequency and synth-covered:
# brackets/symbols, currency, and the Hebrew maqaf/geresh/gershayim (acronyms, ordinals).
KEEP_PUNCT = "[]%+&@#₪€$"
HEBREW_PUNCT = "־׳״"  # maqaf, geresh, gershayim
HEBREW_FINALS = "ךםןףץ"  # final forms — only ever word-final in real Hebrew

BASE = frozenset(HEBREW_LETTERS + LATIN + DIGITS + COMMON_PUNCT)
KEEP_SET = frozenset(KEEP_PUNCT) | frozenset(HEBREW_PUNCT)


def _assigned(ch: str) -> bool:
    try:
        ud.name(ch)
        return True
    except ValueError:
        return False


@lru_cache(maxsize=1)
def _fonts() -> tuple[str, ...]:
    return recgen.discover_fonts("he")


@lru_cache(maxsize=1)
def candidate_charset() -> frozenset:
    """Every glyph the Hebrew model could emit: base + curated keep-set, restricted to
    assigned codepoints at least one Hebrew font renders (drops unrenderable dead classes)."""
    sup = "".join(sorted(BASE | KEEP_SET))
    covered = set()
    for p in _fonts():
        covered |= recgen._covered(p, sup)
    return frozenset(ch for ch in sup if ord(ch) in covered and _assigned(ch))


def line_script(line: str) -> str:
    return "heb"  # single script: build_corpus treats the whole corpus as one bucket


def synth_tail(glyph: str, rng: random.Random, kept: frozenset) -> str:
    """A short LOGICAL-order line containing `glyph` (gen_pair re-bidis it to visual), drawing
    only from kept glyphs — covers the corpus's structural gaps: acronym/ordinal gershayim and
    geresh, maqaf-joined compounds, the apostrophe inside transliterated Latin, and currency."""
    # Draw wrapping letters from NON-FINAL forms only: ך ם ן ף ץ occur word-finally in real
    # Hebrew, so seeding them in these short synthetic contexts (mostly mid-word) taught the
    # model that e.g. final-mem can sit mid-word, corrupting the ס/ם (samekh/final-mem)
    # discrimination. Final forms enter labels only via real corpus words, where position is right.
    letters = [c for c in HEBREW_LETTERS if c in kept and c not in HEBREW_FINALS] \
        or [c for c in HEBREW_LETTERS if c not in HEBREW_FINALS]
    if glyph == "׳":  # geresh: single-letter abbreviation (ר׳, ד׳)
        return rng.choice(letters) + glyph
    if glyph == "״":  # gershayim: acronym (צה״ל, ד״ר)
        return "".join(rng.choice(letters) for _ in range(rng.randint(1, 2))) + glyph + rng.choice(letters)
    if glyph == "־":  # maqaf joins two words
        word = lambda: "".join(rng.choice(letters) for _ in range(rng.randint(2, 4)))
        return f"{word()}{glyph}{word()}"
    if glyph == "'":  # apostrophe inside transliterated Latin (Sh'ma, ma'agal)
        a = "".join(rng.choice(LATIN) for _ in range(rng.randint(1, 3)))
        b = "".join(rng.choice(LATIN) for _ in range(rng.randint(2, 4)))
        return f"{a}{glyph}{b}"
    if ud.category(glyph) == "Sc":  # currency after a number
        return f"{rng.randint(1, 99999)}{glyph}"
    word = "".join(rng.choice(letters) for _ in range(rng.randint(2, 4)))
    return f"{word}{glyph}{word[::-1]}"

HEBREW_WORDS = (
    "של את על לא כן אני אתה הוא היא אנחנו הם זה זאת מה מי איפה מתי למה איך כמה יש "
    "אין היה יהיה עכשיו היום מחר אתמול שלום תודה בבקשה סליחה כסף בית עיר מדינה "
    "ישראל ירושלים תל אביב רחוב מספר טלפון דואר חשבון תאריך שעה דקה שנה חודש שבוע "
    "יום ראשון שני שלישי גדול קטן חדש ישן טוב רע יפה מים אוכל ספר מחשב עבודה "
    "משפחה ילד ילדה איש אישה מורה תלמיד בוקר ערב לילה"
).split()
LATIN_TOKENS = ("WiFi", "Email", "USB", "PDF", "OK", "Tel", "Fax", "App", "GPS", "TV")


def _gen_number(rng):
    k = rng.random()
    if k < 0.3:
        return f"{rng.randint(0, 31):02d}/{rng.randint(1, 12):02d}/{rng.randint(1990, 2026)}"
    if k < 0.5:
        return f"{rng.randint(0, 23):02d}:{rng.randint(0, 59):02d}"
    if k < 0.7:
        return f"{rng.randint(1, 9999)}₪"
    if k < 0.85:
        return f"{rng.randint(1, 100)}%"
    return str(rng.randint(0, 999999))


def gen_logical(rng, corpus):
    budget = rng.randint(6, recgen.MAX_LABEL_LEN)
    if corpus and rng.random() < 0.7:
        words = rng.choice(corpus).split()
        start = rng.randint(0, max(0, len(words) - 1))
        return recgen.join_to_budget(rng, words[start:], budget)
    tokens = []
    for _ in range(8):
        r = rng.random()
        if r < 0.62:
            tokens.append(rng.choice(HEBREW_WORDS))
        elif r < 0.74:
            tokens.append(_gen_number(rng))
        elif r < 0.82:
            tokens.append(rng.choice(LATIN_TOKENS))
        else:
            tokens.append("".join(rng.choice(HEBREW_LETTERS) for _ in range(rng.randint(2, 6))))
    return recgen.join_to_budget(rng, tokens, budget)


def gen_pair(rng: random.Random, corpus: list[str], vocab: frozenset) -> tuple[str, str]:
    visual = get_display(gen_logical(rng, corpus), base_dir="R")
    return visual, visual  # render and label are both the visual-order string


def _build_spec() -> recgen.Spec:
    return recgen.Spec(
        name="heb", fonts=_fonts(), charset="".join(sorted(candidate_charset())),
        reorder=False, gen_pair=gen_pair,
    )


if __name__ == "__main__":
    # Re-bidi the visual label back to logical only for legible inspect annotation.
    recgen.run_cli(_build_spec(), annotate=lambda s: get_display(s, base_dir="R"))
