"""Synthetic Indic text-line strips for PP-OCRv6 rec fine-tuning — one MERGED
model over Bengali, Gujarati, Kannada, Malayalam (+ Latin for mixed content).

Indic reordering (pre-base matras, conjuncts) is LOCAL to a syllable, and HarfBuzz
keeps clusters in logical order, so we train on LOGICAL-order labels and let HB do
the visual layout (recgen Spec.reorder=True). CTC + the CNN receptive field absorb
the local glyph reorder; no downstream reordering at inference (unlike Hebrew RTL).
Verified via the HB cluster probe — see train_paddle_rec.md.

Each corpus line is a single script (concatenate the per-language Leipzig corpora);
a font covering the line's chars is auto-selected. ~25% Latin lines keep Latin alive.

  python gen_indic.py --out /tmp/indic --n 2000 --corpus indic_corpus.txt
  python gen_indic.py --out /tmp/indic-insp --n 32 --inspect --corpus indic_corpus.txt
"""

import random
import unicodedata as ud
from functools import lru_cache

import recgen

# TODO (v3 — only if tightening Indic real-world accuracy further):
# Real-data eval (data/indic_eval/* + data/{gujarati,malayalam}*sign*) was strong:
# Kannada 0.21%, Bengali 1.1%, Malayalam 3.9% (crops) / perfect on signs, Gujarati
# good on modern signs. Residual polish items seen on real photos:
#  1. Gujarati rakar conjunct ્ર dropped (ત્ર -> તર, સ્ટ્રી -> સ્ટી). Likely under-
#     represented in corpus spans / the conjunct renders subtly. Upweight rakar/
#     conjunct-heavy words, or add fonts where rakar is more distinct.
#  2. o/i and a/aa matra confusion (Gujarati આરોગ્ય -> આરીગ્ય; Malayalam dropped ൈ).
#     Targeted matra-minimal-pair augmentation (cf. the Hebrew confusable approach).
#  3. Old-letterpress Gujarati books (Mozhi) are OOD (20% CER) — out of the live/
#     modern use case. Only chase if old-print Gujarati matters: needs letterpress-
#     style fonts (scarce) or much heavier ink-spread degradation.
#  4. Malayalam emits atomic chillu (ർ); GT sources vary (base+virama+ZWJ). Fine for
#     us, but if a downstream consumer wants one form, normalize in ppocr.rs.
#  5. BOLD/DISPLAY signage headers are the main real-world gap (all scripts): big
#     stylized shop-sign titles garble (malayalam-sign-2 മലബാർ/അവിൽ മിൽക്ക്), while
#     normal-weight text on the same sign reads perfectly. Same signature as Hebrew
#     banners. Fix = add real heavy/display fonts (the harvested GF set is mostly
#     text weights) — NOT stroke dilation (that regressed Hebrew round 3).
# Perf: ext_data_num is 0 in the configs now (RecConAug was the CPU bottleneck on the
# ~15-core vast container; our gen already supplies length/combination variety).

LATIN = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
DIGITS = "0123456789"
COMMON_PUNCT = " .,:;!?'\"()/%-+&"
# Rare in running prose but real on signs/prices/documents, so kept regardless of corpus
# frequency (the frequency trim would drop them) and synth-covered to a training floor:
# brackets, danda + double danda (shared sentence terminators), and currency marks.
KEEP_PUNCT = "[]@#।॥₹৳૱"

# Zero-width joiner/non-joiner steer conjunct formation but are invisible; strip them
# from text/labels (the model can't predict an invisible char) and accept canonical forms.
ZW = str.maketrans({"‌": None, "‍": None})

# Per-script Unicode blocks (for charset/coverage) and the assigned consonant /
# dependent-vowel (matra) / independent-vowel / digit subranges (for random fallback).
SCRIPTS = {
    "beng": {"lang": "bn", "block": (0x0980, 0x09FF), "cons": (0x0995, 0x09B9), "matra": (0x09BE, 0x09CC), "digit": (0x09E6, 0x09EF)},
    "gujr": {"lang": "gu", "block": (0x0A80, 0x0AFF), "cons": (0x0A95, 0x0AB9), "matra": (0x0ABE, 0x0ACC), "digit": (0x0AE6, 0x0AEF)},
    "knda": {"lang": "kn", "block": (0x0C80, 0x0CFF), "cons": (0x0C95, 0x0CB9), "matra": (0x0CBE, 0x0CCC), "digit": (0x0CE6, 0x0CEF)},
    "mlym": {"lang": "ml", "block": (0x0D00, 0x0D7F), "cons": (0x0D15, 0x0D39), "matra": (0x0D3E, 0x0D4C), "digit": (0x0D66, 0x0D6F)},
}

_NATIVE_DIGITS = "".join(chr(c) for s in SCRIPTS.values() for c in range(s["digit"][0], s["digit"][1] + 1))
BASE = frozenset(LATIN + DIGITS + COMMON_PUNCT)
KEEP_SET = frozenset(KEEP_PUNCT) | frozenset(_NATIVE_DIGITS)

# Map a codepoint to its script for routing corpus lines.
_BLOCK2SCRIPT = {name: s["block"] for name, s in SCRIPTS.items()}


def _assigned(ch: str) -> bool:
    try:
        ud.name(ch)
        return True
    except ValueError:
        return False


@lru_cache(maxsize=1)
def _fonts() -> tuple[str, ...]:
    return tuple(sorted(set(
        f for s in SCRIPTS.values() for f in recgen.discover_fonts(s["lang"])
    ) | set(recgen.discover_fonts("en"))))


@lru_cache(maxsize=1)
def candidate_charset() -> frozenset:
    """Every glyph the merged model could legitimately emit: base + curated keep-set + the
    four Unicode blocks, restricted to assigned codepoints that at least one discovered font
    can render. The raw block range is ~20% unassigned holes / unrenderable chars; those are
    dead CTC classes that only act as confusable sinks, so they are excluded here (and thus
    from keys.txt and from font-coverage)."""
    block = "".join(chr(c) for s in SCRIPTS.values() for c in range(s["block"][0], s["block"][1] + 1))
    sup = "".join(sorted(BASE | KEEP_SET | set(block)))
    covered = set()
    for p in _fonts():
        covered |= recgen._covered(p, sup)
    return frozenset(ch for ch in sup if ord(ch) in covered and _assigned(ch))


def line_script(line: str) -> str | None:
    for ch in line:
        for name, (lo, hi) in _BLOCK2SCRIPT.items():
            if lo <= ord(ch) <= hi:
                return name
    return None


def _script_of(ch: str) -> dict | None:
    o = ord(ch)
    for s in SCRIPTS.values():
        if s["block"][0] <= o <= s["block"][1]:
            return s
    return None


def _kept_range(lo: int, hi: int, kept: frozenset) -> list[str]:
    return [chr(c) for c in range(lo, hi + 1) if chr(c) in kept]


def synth_tail(glyph: str, rng: random.Random, kept: frozenset) -> str:
    """A short, plausible same-script context line containing `glyph`, drawing only from
    kept glyphs — used by build_corpus to lift a corpus-starved glyph to a training floor.
    Routed by Unicode category so a matra attaches to a consonant, a digit joins a run, an
    independent letter stands as its own syllable, and punctuation/currency wrap a word."""
    s = _script_of(glyph)
    cat = ud.category(glyph)
    if s is None:  # base-plane latin / punctuation / currency
        if cat == "Sc":
            return f"{rng.randint(1, 99999)}{glyph}"
        word = "".join(rng.choice(LATIN) for _ in range(rng.randint(2, 7)))
        return f"{word}{glyph}{word[::-1]}" if cat.startswith("P") else word + glyph
    cons = _kept_range(*s["cons"], kept) or [chr(s["cons"][0])]
    matras = _kept_range(*s["matra"], kept)
    digits = _kept_range(*s["digit"], kept)

    def syllable() -> str:
        return rng.choice(cons) + (rng.choice(matras) if matras and rng.random() < 0.5 else "")

    if cat == "Nd":
        pool = digits or [glyph]
        return "".join([glyph] + [rng.choice(pool) for _ in range(rng.randint(1, 4))])
    if cat in ("Mn", "Mc"):  # matra / combining sign — must follow a consonant
        parts = [syllable() for _ in range(rng.randint(0, 2))] + [rng.choice(cons) + glyph]
        rng.shuffle(parts)
        return " ".join(parts)
    if cat == "Lo":  # independent vowel / rare letter — a syllable of its own
        core = glyph + (rng.choice(matras) if matras and rng.random() < 0.4 else "")
        lead = [syllable()] if rng.random() < 0.7 else []
        return " ".join(lead + [core, syllable()])
    return " ".join(syllable() for _ in range(rng.randint(1, 3))) + glyph


def _rand_word(rng, s, vocab):
    cons = _kept_range(*s["cons"], vocab) or [chr(s["cons"][0])]
    matras = _kept_range(*s["matra"], vocab)
    out = []
    for _ in range(rng.randint(1, 5)):
        syl = rng.choice(cons)
        if matras and rng.random() < 0.55:
            syl += rng.choice(matras)
        out.append(syl)
    return "".join(out)


def _latin_line(rng):
    toks = []
    for _ in range(rng.randint(1, 4)):
        if rng.random() < 0.4:
            toks.append(str(rng.randint(0, 99999)))
        else:
            toks.append("".join(rng.choice(LATIN) for _ in range(rng.randint(2, 9))))
    return " ".join(toks)


def gen_pair(rng: random.Random, corpus: list[str], vocab: frozenset) -> tuple[str, str]:
    budget = rng.randint(6, recgen.MAX_LABEL_LEN)
    if rng.random() < 0.25:
        text = recgen.join_to_budget(rng, _latin_line(rng).split(), budget)
    elif corpus and rng.random() < 0.9:
        words = rng.choice(corpus).translate(ZW).split()
        start = rng.randint(0, max(0, len(words) - 1))
        text = recgen.join_to_budget(rng, words[start:], budget)
    else:
        s = SCRIPTS[rng.choice(list(SCRIPTS))]
        text = recgen.join_to_budget(rng, [_rand_word(rng, s, vocab) for _ in range(rng.randint(1, 5))], budget)
    return text, text  # Indic: render and label are both the logical string


def _build_spec() -> recgen.Spec:
    return recgen.Spec(
        name="indic", fonts=_fonts(), charset="".join(sorted(candidate_charset())),
        reorder=True, gen_pair=gen_pair,
    )


if __name__ == "__main__":
    recgen.run_cli(_build_spec())
