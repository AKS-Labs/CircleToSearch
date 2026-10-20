"""Synthetic Georgian text-line strips for PP-OCRv6 rec fine-tuning.

Georgian is LTR with no bidi, no conjuncts and no pre-base matras, so the render text
and the label are the same logical string and HarfBuzz needs no reordering (recgen
Spec.reorder=False). This is gen_hebrew.py without the visual-order step.

The one Georgian wrinkle is case. Mkhedruli (U+10D0..) is caseless, but all-caps display
text is written in Mtavruli (U+1C90..) — a SEPARATE codepoint range with its own glyph
shapes (uniform height, no ascenders/descenders), not a styling of Mkhedruli. Mtavruli
therefore gets its own CTC classes and the label keeps whatever is on the page; see the
uppercase pass in `gen_pair` for how Mtavruli reaches the training set at all.

  python gen_georgian.py --out /tmp/geo --n 2000 --corpus georgian_corpus.bal.txt --dict georgian_latin_dict.txt
  python gen_georgian.py --out /tmp/geo-insp --n 24 --inspect --corpus georgian_corpus.bal.txt --dict georgian_latin_dict.txt
"""

import collections
import random
import unicodedata as ud
from functools import lru_cache

import freetype
import recgen

# ა..ჰ — the 33 letters of the modern alphabet. U+10F1..U+10FA continue the block with
# letters dropped from the alphabet in the 19th century; they stay out of BASE so the
# corpus frequency trim in build_corpus decides whether they earn a class.
MKHEDRULI_MODERN = "".join(chr(c) for c in range(0x10D0, 0x10F1))
MKHEDRULI_ARCHAIC = "".join(chr(c) for c in range(0x10F1, 0x10FB))
# The whole Mkhedruli block case-maps 1:1 onto Mtavruli (+0xBC0), archaic letters included.
MTAVRULI_MODERN = MKHEDRULI_MODERN.upper()
MTAVRULI_ARCHAIC = MKHEDRULI_ARCHAIC.upper()
_MTAVRULI_OFFSET = 0x1C90 - 0x10D0

LATIN = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
DIGITS = "0123456789"
COMMON_PUNCT = " .,:;!?'\"()/-"
# Rare in cleaned wiki/news prose but real on signage, price lists and documents — kept
# regardless of corpus frequency and synth-covered to the training floor: brackets/symbols,
# currency (₾ lari is on every Georgian price tag), and the Georgian paragraph separator.
KEEP_PUNCT = "[]%+&@#€$₾"
GEORGIAN_PUNCT = "჻"

BASE = frozenset(MKHEDRULI_MODERN + MTAVRULI_MODERN + LATIN + DIGITS + COMMON_PUNCT)
KEEP_SET = frozenset(KEEP_PUNCT) | frozenset(GEORGIAN_PUNCT)

# All-caps rates. Wiki/news prose is ~100% Mkhedruli while shop fronts, banners and road
# signs — the live-camera case — are heavily Mtavruli, so without this pass the 33 Mtavruli
# classes would be trained only on synthetic filler. Uppercasing real corpus text instead
# puts them in genuine word contexts (IMPROVEMENTS.md: coverage from varied real sentences,
# never repetition). Line = a whole banner; word = one emphasised word inside running text.
MTAVRULI_LINE_FRAC = 0.12
MTAVRULI_WORD_FRAC = 0.08
# Corpus instances every Mtavruli letter must reach, enforced by build_corpus's fill stage
# via coverage_lines. Tuned so the rarest letters clear the punctuation classes they were
# losing to, while the coverage stream stays a minority of the corpus (see glyph_targets).
MTAVRULI_CORPUS_TARGET = 8000

GEORGIAN_WORDS = (
    "და არის ეს ის მე შენ ჩვენ თქვენ რომ მაგრამ თუ როგორ რა ვინ სად როდის რატომ "
    "რამდენი დიახ არა კი გამარჯობა მადლობა ნახვამდის ბოდიში უკაცრავად თბილისი "
    "საქართველო ქართული ენა ქალაქი ქუჩა სახლი ბინა წელი თვე დღე ღამე დილა საღამო "
    "შუადღე დრო საათი წუთი წამი ფული ლარი თეთრი მაღაზია რესტორანი კაფე სასტუმრო "
    "საავადმყოფო აფთიაქი სკოლა უნივერსიტეტი წიგნი მასწავლებელი სტუდენტი მოსწავლე "
    "კაცი ქალი ბავშვი ოჯახი მეგობარი ძმა დედა მამა წყალი პური ღვინო ჩაი ყავა "
    "საჭმელი ხორცი თევზი ხილი ბოსტნეული დიდი პატარა ახალი ძველი კარგი ცუდი ლამაზი "
    "მაღალი დაბალი ნომერი ტელეფონი მისამართი შესასვლელი გასასვლელი ღია დაკეტილი "
    "ავტობუსი მატარებელი მანქანა თვითმფრინავი გზა ხიდი ბანკი ფოსტა ეკლესია მუზეუმი "
    "ბაზარი თეატრი ბიბლიოთეკა ინფორმაცია ყურადღება აკრძალულია ფრთხილად გაყიდვა "
    "ფასდაკლება ჟურნალი გაზეთი ჰაერი ჰოსპიტალი ძალიან ცოტა ბევრი ხალხი სამსახური "
    "სამზარეულო ეზო ბაღი მთა ზღვა მდინარე ტყე ქვეყანა მსოფლიო ჭიშკარი ჭურჭელი "
    "ჟანრი ჰანგი ჩრდილოეთი სამხრეთი აღმოსავლეთი დასავლეთი"
).split()
LATIN_TOKENS = ("WiFi", "Email", "USB", "PDF", "OK", "Tel", "Fax", "App", "GPS", "TV")


def _is_mtavruli(ch: str) -> bool:
    return 0x1C90 <= ord(ch) <= 0x1CBF


def _assigned(ch: str) -> bool:
    try:
        ud.name(ch)
        return True
    except ValueError:
        return False


@lru_cache(maxsize=1)
def _fonts() -> tuple[str, ...]:
    # Latin-only faces ride along so pure-Latin lines get the design variety the nine
    # Georgian families cannot supply; recgen.fonts_for still restricts any line carrying
    # a Georgian glyph to the faces that actually cover it.
    return tuple(sorted(set(recgen.discover_fonts("ka")) | set(recgen.discover_fonts("en"))))


@lru_cache(maxsize=1)
def candidate_charset() -> frozenset:
    """Every glyph the Georgian model could emit: base + curated keep-set + the archaic
    Mkhedruli/Mtavruli tails, restricted to assigned codepoints at least one discovered font
    renders. The archaic letters are offered to build_corpus's frequency trim rather than
    kept outright; unrenderable or unassigned codepoints are dropped here so they never
    become dead CTC classes acting as confusable sinks."""
    sup = "".join(sorted(BASE | KEEP_SET | set(MKHEDRULI_ARCHAIC) | set(MTAVRULI_ARCHAIC)))
    covered = set()
    for p in _fonts():
        covered |= recgen._covered(p, sup)
    return frozenset(ch for ch in sup if ord(ch) in covered and _assigned(ch))


def line_script(line: str) -> str:
    return "geor"  # single script: build_corpus treats the whole corpus as one bucket


CLOSER = {"[": "]", "(": ")", '"': '"'}
OPENER = {c: o for o, c in CLOSER.items()}
SENTENCE_END = ".!?"
CLAUSE_MID = ",;:"


def synth_tail(glyph: str, rng: random.Random, kept: frozenset) -> str:
    """A short line containing `glyph`, placed where the language actually puts it.

    build_corpus calls this only for glyphs the corpus leaves under the training floor, so a
    glyph the corpus never supplies at all (`@`, `#`, `[`, `]`, `჻` here) takes 100% of its
    exposure from this function. Position is then taught entirely by these lines, and a
    plausible-looking but wrong slot becomes a rule the model believes: Hebrew round 1 seeded
    final forms into synthetic mid-word contexts and corrupted the ס/ם boundary that way.
    Every branch reproduces the symbol's real syntactic slot; an unhandled glyph raises
    rather than falling into a generic "between two words" placement, because that placement
    is wrong for every symbol that is not a letter.
    """
    words = [w for w in GEORGIAN_WORDS if set(w) <= kept] or GEORGIAN_WORDS

    def geo() -> str:
        return rng.choice(words)

    def lat(lo: int = 2, hi: int = 6) -> str:
        return "".join(rng.choice(LATIN) for _ in range(rng.randint(lo, hi)))

    if glyph in MTAVRULI_MODERN or glyph in MTAVRULI_ARCHAIC:
        # Real words carrying the Mkhedruli counterpart, uppercased whole — the sign-shaped
        # context Mtavruli actually appears in. Random letter salad here would teach the
        # NRTR head a sequence prior that no Georgian text follows.
        low = glyph.lower()
        with_glyph = [w for w in words if low in w] or [low]
        return " ".join(rng.choice(with_glyph) for _ in range(rng.randint(1, 3))).upper()
    if glyph in MKHEDRULI_MODERN or glyph in MKHEDRULI_ARCHAIC:
        w = geo()  # a letter belongs inside a word, not welded between two of them
        cut = rng.randint(0, len(w))
        return f"{geo()} {w[:cut]}{glyph}{w[cut:]}"
    if ud.category(glyph) == "Sc" or glyph == "%":  # trails an amount: 25₾, 40%
        return f"{geo()} {rng.randint(1, 9999)}{glyph}"
    if glyph == "@":  # exists only inside an address
        return f"{lat(3, 8)}@{lat(3, 6)}.ge"
    if glyph == "#":  # leads the thing it numbers
        return f"{geo()} #{rng.randint(1, 999)}"
    if glyph in CLOSER or glyph in OPENER:
        # Brackets and quotes occur in pairs. Emitting one alone teaches an opener that never
        # closes, so the line carries both and whichever half was asked for is covered.
        o = glyph if glyph in CLOSER else OPENER[glyph]
        return f"{geo()} {o}{geo()}{CLOSER[o]} {geo()}"
    if glyph in SENTENCE_END:  # closes a clause, then a space
        return f"{geo()} {geo()}{glyph} {geo()} {geo()}"
    if glyph in CLAUSE_MID:  # binds to the preceding word, space after
        return f"{geo()}{glyph} {geo()} {geo()}"
    if glyph == "-":  # joins two words, or spans a numeric range
        return f"{geo()}-{geo()}" if rng.random() < 0.6 else f"{rng.randint(1, 99)}-{rng.randint(100, 999)}"
    if glyph == "/":  # separates alternatives or the parts of a date
        return f"{geo()}/{geo()}" if rng.random() < 0.5 else f"{rng.randint(1, 31)}/{rng.randint(1, 12)}"
    if glyph == "+":  # dial code, or between figures
        return f"+995{rng.randint(100000000, 999999999)}" if rng.random() < 0.6 else f"{rng.randint(1, 99)}+{rng.randint(1, 99)}"
    if glyph == "&":  # joins two Latin names, as on a shopfront
        return f"{lat()} & {lat()}"
    if glyph == "჻":  # paragraph separator sits between clauses
        return f"{geo()} {glyph} {geo()}"
    if glyph == "'":  # apostrophe inside transliterated Latin (Kartl'is, Sak'art'velo)
        return f"{lat(1, 3)}{glyph}{lat(2, 4)}"
    if glyph in LATIN or glyph in DIGITS:
        return f"{geo()} {glyph}{lat(1, 5)}"
    raise ValueError(f"synth_tail: no syntactic slot defined for {glyph!r} (U+{ord(glyph):04X})")


def glyph_targets(counts: collections.Counter, kept: frozenset, floor: int) -> dict[str, int]:
    """A flat corpus target for every Mtavruli letter.

    Flat is the point. Mtavruli reaches the labels only through the uppercase pass in
    `gen_pair`, which uppercases lines at a fixed rate and so hands Mtavruli a scaled copy of
    Mkhedruli's Zipfian shape. Round 1 left the tail under the punctuation classes it
    competes with — Mtavruli zen at 4752 against `%` at 5213, and the model duly read `Ზ` as
    `%` on two separate road signs, while `Ჟ` sat at 677. Scaling the uppercase rate cannot
    fix that: it multiplies the whole distribution and would need ~22x to lift the tail,
    collapsing Mkhedruli and teaching the decoder that Georgian is mostly capitals. Setting
    one floor across the alphabet deliberately flattens the tail instead, which is what a
    coverage stream is for — the natural-frequency stream still sets the language prior.

    Mkhedruli, Latin, digits and punctuation are left on the absolute floor. They are not
    starved and lifting them would only dilute the fix.
    """
    return {ch: MTAVRULI_CORPUS_TARGET for ch in MTAVRULI_MODERN if ch in kept}


def coverage_lines(glyph: str, corpus: list[str], rng: random.Random, kept: frozenset) -> list[str]:
    """Distinct real corpus lines carrying `glyph`, for build_corpus's fill stage.

    Prose is ~100% Mkhedruli, so a Mtavruli letter has no lines of its own; it borrows the
    lines of its Mkhedruli counterpart and they are re-cast, not invented. Uppercasing real
    sentences keeps the cross-word sequence statistics the NRTR head learns intact, which
    repetition of one word or a synthetic placement would not — the distinction that decided
    the Hebrew final-form regression.

    Lines are returned shuffled and deduplicated so the fill never leans on one sentence.
    """
    source = chr(ord(glyph) - _MTAVRULI_OFFSET) if _is_mtavruli(glyph) else glyph
    seen: set[str] = set()
    for line in corpus:
        if source not in line:
            continue
        # Excerpt a few words around the glyph rather than keeping the whole sentence.
        # `_gen_line` samples a 6-25 char window from a random word offset, so a full
        # ~80-char line mostly lands outside the window and the glyph never reaches a label:
        # whole sentences lifted the corpus count 22x and the label count only 1.3x.
        words = line.split()
        at = next(i for i, w in enumerate(words) if source in w)
        start = max(0, at - rng.randint(0, 1))
        excerpt = recgen.join_to_budget(rng, words[start:], recgen.MAX_LABEL_LEN)
        if source not in excerpt:
            continue
        if _is_mtavruli(glyph):
            excerpt = excerpt.upper()
        # An archaic Mkhedruli letter can clear the frequency trim while its Mtavruli
        # counterpart never enters BASE, so an uppercased line can carry a glyph with no
        # output class.
        if set(excerpt) - {" "} <= kept:
            seen.add(excerpt)
    out = list(seen)
    rng.shuffle(out)
    return out


def _gen_number(rng: random.Random) -> str:
    k = rng.random()
    if k < 0.3:
        return f"{rng.randint(1, 31):02d}.{rng.randint(1, 12):02d}.{rng.randint(1990, 2026)}"
    if k < 0.5:
        return f"{rng.randint(0, 23):02d}:{rng.randint(0, 59):02d}"
    if k < 0.7:
        return f"{rng.randint(1, 9999)}₾"
    if k < 0.85:
        return f"{rng.randint(1, 100)}%"
    return str(rng.randint(0, 999999))


def _gen_line(rng: random.Random, corpus: list[str]) -> str:
    budget = rng.randint(6, recgen.MAX_LABEL_LEN)
    if corpus and rng.random() < 0.7:
        words = rng.choice(corpus).split()
        start = rng.randint(0, max(0, len(words) - 1))
        return recgen.join_to_budget(rng, words[start:], budget)
    tokens = []
    for _ in range(8):
        r = rng.random()
        if r < 0.66:
            tokens.append(rng.choice(GEORGIAN_WORDS))
        elif r < 0.80:
            tokens.append(_gen_number(rng))
        else:
            tokens.append(rng.choice(LATIN_TOKENS))
    return recgen.join_to_budget(rng, tokens, budget)


def _upcase(text: str, rng: random.Random, vocab: frozenset) -> str:
    """All-caps `text`, or one word of it, when the result stays inside the model's classes.

    An archaic Mkhedruli letter can clear the corpus frequency trim while its Mtavruli
    counterpart — absent from prose, absent from BASE — does not, so uppercasing blindly
    would put a glyph in the label that the head has no output class for.
    """
    r = rng.random()
    if r < MTAVRULI_LINE_FRAC:
        candidate = text.upper()
    elif r < MTAVRULI_LINE_FRAC + MTAVRULI_WORD_FRAC:
        words = text.split(" ")
        i = rng.randrange(len(words))
        candidate = " ".join(w.upper() if j == i else w for j, w in enumerate(words))
    else:
        return text
    return candidate if set(candidate) - {" "} <= vocab else text


def gen_pair(rng: random.Random, corpus: list[str], vocab: frozenset) -> tuple[str, str]:
    text = _upcase(_gen_line(rng, corpus), rng, vocab)
    return text, text  # Georgian is LTR and unshaped: render and label are one string


_MKHEDRULI = MKHEDRULI_MODERN + MKHEDRULI_ARCHAIC
_CAPS_REMAP = tuple(
    [(m, k) for k, m in zip(_MKHEDRULI, MTAVRULI_MODERN + MTAVRULI_ARCHAIC)]
    + [(k, None) for k in _MKHEDRULI]
)


# Mkhedruli letters whose lowercase forms carry a descender. Caps forms of the same
# codepoints sit on the baseline, so the deepest descender across this probe separates the
# two designs without needing to know anything about the face.
_CAPS_PROBE = "ბგდვზჟპყჭჯჰ"
_CAPS_PROBE_PX = 44


@lru_cache(maxsize=None)
def _descends_below_baseline(path: str) -> int:
    """Deepest descender, in pixels below the baseline, over the probe letters at a 44px em."""
    face = recgen._ft_face(path)
    face.set_pixel_sizes(0, _CAPS_PROBE_PX)
    deepest = 0
    for ch in _CAPS_PROBE:
        if not face.get_char_index(ord(ch)):
            continue
        face.load_char(ch, freetype.FT_LOAD_RENDER)
        deepest = max(deepest, face.glyph.bitmap.rows - face.glyph.bitmap_top)
    return deepest


def _caps_remap(fonts: tuple[str, ...]) -> dict[str, tuple[tuple[str, str | None], ...]]:
    """Faces that draw caps-shaped Georgian at the Mkhedruli codepoints, mapped onto Mtavruli.

    BPG's Caps families (Excelsior Caps, Nateli Caps, Mrgvlovani Caps) date from 2012, before
    Unicode 11 gave Mtavruli its own block, so all-caps Georgian was set by swapping to a caps
    font while the text stayed in Mkhedruli codepoints. Measured against Noto across the
    alphabet, their glyphs resemble Mtavruli more than Mkhedruli, so they are routed to
    Mtavruli labels and barred from Mkhedruli — otherwise the same caps shapes would train
    under both labels and blunt the distinction the separate classes exist to draw.

    Membership is decided by rendering, never by the file name. A face called *Caps* that is
    really a titling or small-caps variant would feed lowercase shapes to Mtavruli labels and
    blunt exactly the distinction this routing protects, and the name would give no warning —
    the same trap as reading boldness off OS/2 usWeightClass instead of the drawn stroke.
    Lowercase Georgian descends ~9-10px below the baseline at this em; caps forms sit on it.
    """
    return {p: _CAPS_REMAP for p in fonts
            if recgen._ft_face(p).get_char_index(0x10D0)
            and not recgen._ft_face(p).get_char_index(0x1C90)
            and _descends_below_baseline(p) <= _CAPS_PROBE_PX // 20}


def _build_spec() -> recgen.Spec:
    fonts = _fonts()
    return recgen.Spec(
        name="geor", fonts=fonts, charset="".join(sorted(candidate_charset())),
        reorder=False, gen_pair=gen_pair, font_remap=_caps_remap(fonts),
    )


if __name__ == "__main__":
    recgen.run_cli(_build_spec())
