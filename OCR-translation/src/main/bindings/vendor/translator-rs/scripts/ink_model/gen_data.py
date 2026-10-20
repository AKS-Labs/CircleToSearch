"""Synthetic training strips for the ink model.

Renders text with known per-pixel coverage (the label), composites it over
procedural backgrounds (solids, gradients, noise blobs — the colors.jpg cases),
then degrades the image only (JPEG, blur, noise, illumination) so the label
stays clean. Language is irrelevant for ink masks; glyph shapes are what
matters, so text mixes dictionary words with random charset strings for rare
glyph coverage.

CLI: python gen_data.py --out /tmp/ink-samples --n 16   (writes inspection PNGs)
Importable: sample() -> (image float32 HxWx3 in 0..1, coverage float32 HxW)
"""

import argparse
import glob
import os
import random
import subprocess
import sys
from functools import lru_cache

import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from scipy.ndimage import distance_transform_edt

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from synth_core import coord_grid as _coord_grid, degrade, legible, random_color  # noqa: E402

HEIGHT = 48
WIDTHS = list(range(96, 513, 16))

LATIN_CHARSET = (
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    ".,;:!?¡¿'\"()[]%&@#€$£+-*/=<>_"
    "àáâãäåæçèéêëìíîïñòóôõöøùúûüýÿßœ"
    "ÀÁÂÃÄÅÆÇÈÉÊËÌÍÎÏÑÒÓÔÕÖØÙÚÛÜÝŒ"
    "ąćęłńśźżĄĆĘŁŃŚŹŻčďěňřšťůžČĎĚŇŘŠŤŮŽőűŐŰ"
)

FONT_BLOCKLIST = (
    "emoji",
    "symbol",
    "dingbat",
    "music",
    "math",
    "braille",
    "awesome",
    "d050000l",  # URW Zapf-Dingbats clone: maps ASCII to pictographs, no descriptive name
)

# Unicode blocks for dense-script samples. Noto CJK (the :lang=ko fonts) covers all
# three fully, so generating glyphs straight from the ranges never hits .notdef.
CJK_BLOCKS = ((0xAC00, 0xD7A3), (0x4E00, 0x9FFF), (0x3040, 0x30FF))

# Filename markers for heavy weights. The random font pool is mostly regular weight,
# so display-style superbold strokes are under-trained and the matte under-covers them.
HEAVY_TOKENS = ("bold", "black", "heavy", "extrabold", "extrablack", "semibold", "-bd", "-blk")


def _blocked_font(path: str) -> bool:
    """Pictographic/symbol font we don't want in the Latin pool. Filename tokens catch
    most; the name table catches descriptively-named ones whose file is opaque (a font
    literally named "… Dingbats"). There's no clean metadata flag for an ASCII-mapped
    pictographic font, so this stays an explicit token list rather than a heuristic that
    might drop real text fonts."""
    if any(b in path.lower() for b in FONT_BLOCKLIST):
        return True
    try:
        nm = TTFont(path, fontNumber=0, lazy=True)["name"]
    except Exception:
        return False
    fam = ((nm.getDebugName(1) or "") + " " + (nm.getDebugName(4) or "")).lower()
    return any(b in fam for b in FONT_BLOCKLIST)


@lru_cache(maxsize=1)
def font_paths() -> list[str]:
    out = subprocess.run(
        ["fc-list", ":lang=en", "file"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    paths = []
    for line in out.splitlines():
        p = line.split(":")[0].strip()
        if not p.lower().endswith((".ttf", ".otf")):
            continue
        if _blocked_font(p):
            continue
        paths.append(p)
    if not paths:
        raise RuntimeError("fc-list found no usable fonts")
    return sorted(set(paths))


@lru_cache(maxsize=1)
def cjk_font_paths() -> list[str]:
    out = subprocess.run(
        ["fc-list", ":lang=ko", "file"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    paths = []
    for line in out.splitlines():
        p = line.split(":")[0].strip()
        if p.lower().endswith((".ttf", ".otf", ".ttc")):
            paths.append(p)
    if not paths:
        raise RuntimeError("fc-list found no CJK fonts")
    return sorted(set(paths))


@lru_cache(maxsize=4096)
def _font_weight(path: str) -> int | None:
    """The font's real OS/2 `usWeightClass` (100..900), or None if unreadable.
    Ground truth for the bold label — filename tokens lie (a "heavy" CJK file with
    no bold renders regular; "regular" pools hide medium/semibold faces), and the
    label must match what's actually drawn or the bold head trains on noise."""
    try:
        return TTFont(path, fontNumber=0, lazy=True)["OS/2"].usWeightClass
    except Exception:
        return None


# Pool-selection cuts in MEASURED stroke-ratio space — NOT OS/2 usWeightClass, which lies
# (giga-bold display faces declare 400 yet draw black, so they wrongly landed in the regular
# pool and were never drawn in bold runs). This is the same measurement that produces the
# label (_target_q), so the run's bold/regular intent now matches the font's real thickness —
# and the heavy display tail rides in the bold pool on its own (no special-case oversampling).
# The semibold middle sits in both pools (its mid label is honest either way).
STROKE_BOLD_CUT = 0.085
STROKE_REG_CUT = 0.115


def _stroke_pool(paths: tuple[str, ...], script: str, bold: bool) -> tuple[str, ...]:
    """Fonts biased toward the run's intended weight by *measured* stroke ratio: thick faces
    for bold runs, thin for regular, the semibold middle in both. Never empty (falls back to
    all). Measured (not OS/2) so declared-weight lies can't misfile a face."""
    cut = (lambda r: r >= STROKE_BOLD_CUT) if bold else (lambda r: r <= STROKE_REG_CUT)
    out = tuple(p for p in paths if cut(_font_stroke_ratio(p, script)))
    return out or tuple(paths)


@lru_cache(maxsize=16)
def _weighted_fonts(script: str, bold: bool) -> tuple[str, ...]:
    paths = (
        shaped_fonts(script) if script in SHAPED
        else cjk_font_paths() if script == "cjk"
        else font_paths()
    )
    return _stroke_pool(tuple(paths), script, bold)


# Fraction of strips drawn as a high-coverage condensed display header (env-gated, default 0
# so the standard recipe is unchanged). Targets the out-of-distribution failure: real ultra-bold
# condensed lettering is ~50% ink coverage, where the model over-marks the bright inter-letter
# gaps; the standard gen tops out ~8% of strips above 0.40 coverage. The gap labels stay
# background (cov = glyph coverage), so this teaches "dense ink, gaps still background".
DISPLAY_HEAVY_FRAC = float(os.environ.get("INK_DISPLAY_HEAVY", "0"))
STROKE_DISPLAY_CUT = 0.115


@lru_cache(maxsize=1)
def _display_fonts() -> tuple[str, ...]:
    """Curated condensed/poster display faces — the ones `setup_vast.sh` fetches into the
    `gigabold` dir (Anton, Oswald, BebasNeue, Staatliches, AlfaSlabOne, …). Restricted to that
    known-good set rather than the measured-stroke tail of all system fonts: the tail pulled in
    occasional faces that crash PIL at large display sizes (aborting a dataloader worker), and
    the curated set is exactly the real-header style we want. Falls back to the measured tail
    only if the gigabold dir is absent (local dev)."""
    curated = tuple(p for p in font_paths() if "gigabold" in p)
    if curated:
        return curated
    return tuple(p for p in font_paths() if _font_stroke_ratio(p, "latin") >= STROKE_DISPLAY_CUT) \
        or _weighted_fonts("latin", True)


def _display_text(rng: random.Random, width: int, font_px: int) -> str:
    """Enough uppercase chars (the header style) to span the strip width at this em, so the
    word fills its box like a real tight-cropped header instead of floating in background."""
    n = max(3, int(width / (0.55 * font_px)) + rng.randint(0, 2))
    chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    out, w = [], 0
    while w < n:
        run = rng.randint(3, 8)
        out.append("".join(rng.choice(chars) for _ in range(min(run, n - w))))
        w += run + 1
    return " ".join(out)


# Connected/cursive scripts the Latin+CJK set misses: Arabic (cursive, RTL), the Indic
# scripts (Devanagari's shirorekha roof bar + conjuncts, Tamil's thick curved loops) and
# Thai. (fc-list lang, consonant codepoint range) — sampling consonants avoids stray
# combining marks rendering as dotted-circle. Needs PIL+raqm to shape correctly.
SHAPED = {
    "arabic": ("ar", 0x0627, 0x064A),
    "devanagari": ("hi", 0x0915, 0x0939),
    "tamil": ("ta", 0x0B95, 0x0BB9),
    "thai": ("th", 0x0E01, 0x0E2E),
}
SHAPED_NAMES = tuple(SHAPED)


@lru_cache(maxsize=len(SHAPED))
def shaped_fonts(script: str) -> tuple[str, ...]:
    lang = SHAPED[script][0]
    out = subprocess.run(["fc-list", f":lang={lang}", "file"], capture_output=True,
                         text=True, check=True).stdout
    paths = tuple(
        ln.split(":")[0].strip()
        for ln in out.splitlines()
        if ln.split(":")[0].strip().lower().endswith((".ttf", ".otf"))
    )
    if not paths:
        raise RuntimeError(f"no fonts for script {script}")
    return paths


def shaped_text(rng: random.Random, script: str) -> str:
    _, lo, hi = SHAPED[script]
    parts = []
    for _ in range(rng.randint(1, 4)):
        n = rng.randint(2, 6)
        parts.append("".join(chr(rng.randint(lo, hi)) for _ in range(n)))
    return " ".join(parts)


def cjk_text(rng: random.Random) -> str:
    lo, hi = rng.choice(CJK_BLOCKS)
    chars = []
    for _ in range(rng.randint(1, 6)):
        if rng.random() < 0.15:  # Latin digits/units mix in, like "100% 30ml"
            chars.append(rng.choice("0123456789%."))
        else:
            chars.append(chr(rng.randint(lo, hi)))
    return "".join(chars)


@lru_cache(maxsize=1)
def dictionary_words() -> list[str]:
    for path in ("/usr/share/dict/words", "/usr/share/dict/american-english"):
        if os.path.exists(path):
            with open(path, encoding="utf-8", errors="ignore") as f:
                words = [w.strip() for w in f if 2 <= len(w.strip()) <= 14]
            if words:
                return words
    return []


def random_text(rng: random.Random) -> str:
    words = []
    dictionary = dictionary_words()
    for _ in range(rng.randint(1, 8)):
        if dictionary and rng.random() < 0.6:
            w = rng.choice(dictionary)
            if rng.random() < 0.2:
                w = w.capitalize()
            elif rng.random() < 0.07:
                w = w.upper()
        else:
            n = rng.randint(1, 10)
            w = "".join(rng.choice(LATIN_CHARSET) for _ in range(n))
        words.append(w)
    return " ".join(words)


def _latin_chunk(rng: random.Random) -> str:
    """1–2 words for one run (the line is built from several runs)."""
    words = random_text(rng).split(" ")
    return " ".join(words[: rng.randint(1, 2)]) or "x"


def _pick_font(rng: random.Random, script: str, bold: bool) -> tuple[str, object]:
    # Weight-by-OS/2 for every script, *including* shaped — previously shaped runs
    # ignored `bold` and drew a random-weight font, so their label was noise.
    layout = ImageFont.Layout.RAQM if script in SHAPED else ImageFont.Layout.BASIC
    return rng.choice(_weighted_fonts(script, bold)), layout


def _run_text(rng: random.Random, script: str) -> str:
    if script in SHAPED:
        return shaped_text(rng, script)
    if script == "cjk":
        return cjk_text(rng)
    return _latin_chunk(rng)


def _run_plan(rng: random.Random) -> list[tuple[str, bool]]:
    """A line is 1–4 runs laid left-to-right; each picks its own script and weight,
    so one strip mixes scripts and mixes bold/regular at *run* (word) granularity."""
    runs = []
    for _ in range(rng.randint(1, 4)):
        r = rng.random()
        script = (
            "cjk"
            if r < 0.18
            else (SHAPED_NAMES[rng.randrange(len(SHAPED_NAMES))] if r < 0.36 else "latin")
        )
        runs.append((script, rng.random() < 0.45))
    return runs


def _cjk_units(rng: random.Random, txt: str, is_bold: bool) -> list[tuple[str, bool]]:
    """CJK emphasis: a contiguous middle span goes bold while the rest stays regular,
    rendered flush (no spaces). CJK has no inter-word gap, so this is where the model
    must find the weight seam from stroke thickness alone, mid-string. Latin/shaped
    vary weight only at run boundaries (where a space already cues it)."""
    if is_bold or len(txt) < 3 or rng.random() < 0.5:
        return [(txt, is_bold)]
    i = rng.randint(1, len(txt) - 2)
    j = rng.randint(i + 1, len(txt) - 1)
    return [(txt[:i], False), (txt[i:j], True), (txt[j:], False)]


# Continuous bold label: per-run glyph stroke width / em, mapped to [0,1] and painted
# flat over each run's ink. Measured from the rendered raster (distance transform), never
# from font metadata — a 400-weight face that draws thick reads as thick. The measurement
# is cached per (font, script) so the distance transform runs at most once per font in a
# worker, never per training strip (per-strip it bottlenecks the dataloader). The ratio→target
# map is a logistic (see _target_q): its 0.5-crossing sits at the empirical valley between the
# semibold (~0.088) and bold (~0.125) stroke-ratio modes, and K sets how hard confident weights
# saturate. Calibrated on the font pool so regular p95 target ~0.2 and typical bold ~0.85, which
# pulls the two clusters off the decision boundary (a linear LO..HI ramp left bold sitting at
# ~0.65, smeared across 0.5). The model's ordering is already ~98% (eval_bold_sep); this only
# moves where that ordering lands on the [0,1] axis.
STROKE_RATIO_LO = 0.06  # stroke ratio assumed for a font whose raster can't be measured
BOLD_RATIO_MID = 0.10   # ratio mapped to target 0.5 (valley between semibold and bold modes)
BOLD_RATIO_K = 80.0     # logistic steepness; higher = harder saturation to the rails
# Boundary in normalised target space where a strip/region counts as "should render bold".
# target >= 0.5 <=> stroke ratio >= BOLD_RATIO_MID, a real semibold/bold weight boundary.
BOLD_GT_THRESHOLD = 0.5
_REF_EM = 96
_REF_LATIN = "Hxnodbpqgesa AOMW 2580"
_REF_CJK = "永国體書速達"


def _ref_text(script: str) -> str:
    if script in SHAPED:
        _, lo, hi = SHAPED[script]
        return "".join(chr(c) for c in range(lo, min(hi + 1, lo + 8)))
    return _REF_CJK if script == "cjk" else _REF_LATIN


@lru_cache(maxsize=8192)
def _font_stroke_ratio(path: str, script: str) -> float:
    """Stroke width / em for a font, measured once and cached. The per-strip hot path must
    never run a distance transform, so this renders a representative string at a reference
    em, distance-transforms the ink, and takes a high percentile of the radius as the
    stroke half-width. One font file is one weight, so this is the per-font-weight stroke
    width; per-glyph variation within a font is second-order and pooled out downstream."""
    layout = ImageFont.Layout.RAQM if script in SHAPED else ImageFont.Layout.BASIC
    try:
        font = ImageFont.truetype(path, _REF_EM, layout_engine=layout)
    except OSError:
        return STROKE_RATIO_LO
    text = _ref_text(script)
    canvas = Image.new("L", (_REF_EM * (len(text) + 2), _REF_EM * 3), 0)
    ImageDraw.Draw(canvas).text((_REF_EM, _REF_EM), text, font=font, fill=255)
    mask = np.asarray(canvas) > 127
    if mask.sum() < 16:
        return STROKE_RATIO_LO
    half = float(np.percentile(distance_transform_edt(mask)[mask], 85))
    return 2.0 * half / _REF_EM


def _target_q(ratio: float) -> int:
    """Stroke ratio → 0..255 fill: a logistic centred at BOLD_RATIO_MID maps the [0,1]
    regression target the bold head learns, quantised for an antialiased PIL draw. Logistic
    (not a linear ramp) so confident regular/bold weights saturate near the rails and the
    genuinely-ambiguous semibold band stays mid, instead of the whole bold cluster smearing
    across the 0.5 threshold."""
    t = 1.0 / (1.0 + np.exp(-BOLD_RATIO_K * (ratio - BOLD_RATIO_MID)))
    return round(255 * float(t))


# Fraction of strips that carry a horizontal rule (underline / strikethrough / overline).
# The rule is real ink — it goes into `total` so the matte erases it — and is also recorded
# in its own `rule` channel: one learned "is this pixel part of a rule" map. The three rule
# *types* are not separate channels; the runtime names them from the rule's vertical position
# vs the matte's baseline/x-height band. This channel is rules only — bold has its own channel
# and a slant (italic) is not a per-pixel mask at all. Display-heavy headers are excluded.
RULE_FRAC = float(os.environ.get("INK_RULE_FRAC", "0.22"))
RULE_KINDS = ("underline", "strike", "overline")
RULE_WEIGHTS = (0.7, 0.2, 0.1)


def render_coverage(
    rng: random.Random, width: int, height: int, log: dict | None = None
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    """Antialiased glyph coverage for a multi-run line on a height x width canvas.

    Returns `(total, fill, bold, rule)`: `total` is the union coverage incl. any outline
    stroke and any rule (the matte label — all of it is ink); `fill` is the glyph core only
    (so the caller can colour the outline ring); `bold` is the continuous per-pixel
    stroke-width target in [0,1] (each run painted with its cached `_font_stroke_ratio`,
    LO..HI → 0..1), the regression label for the bold head; `rule` is the horizontal-rule
    coverage (underline/strikethrough/overline), the label for the rule head. `fill` aliases
    `total` when there's no outline.
    """
    # Render oversized then rotate, so the rotation doesn't clip glyphs.
    pad = max(8, height // 2)
    size = (width + 2 * pad, height + 2 * pad)
    total = Image.new("L", size, 0)
    td = ImageDraw.Draw(total)
    boldval = Image.new("L", size, 0)  # per-pixel stroke-width target (0..255 = LO..HI)
    bvd = ImageDraw.Draw(boldval)
    rule_img = Image.new("L", size, 0)  # horizontal-rule coverage (under/strike/over)
    rd = ImageDraw.Draw(rule_img)
    jitter = max(1, height // 12)
    stroke = rng.randint(1, max(1, height // 10)) if rng.random() < 0.2 else 0
    outlined = stroke > 0
    fill_canvas = Image.new("L", size, 0) if outlined else None
    fill_draw = ImageDraw.Draw(fill_canvas) if outlined else None

    # The em floors at 20 px (x-height ~10) so the box-height floor actually bites: a
    # 24 px box could otherwise draw an 11 px em (0.45·box) whose strokes are sub-pixel
    # for bold. The 0.45–0.95 box-hug spread stays (loose boxes are real; they just then
    # only occur at larger native heights).
    # Display-heavy mode: a single condensed bold latin word at near-full em with tight gaps,
    # so the strip is ~50% ink (the STRONGER regime). Labels are still glyph-only coverage.
    display = rng.random() < DISPLAY_HEAVY_FRAC
    if display:
        font_px = rng.randint(max(20, int(height * 0.80)), max(21, int(height * 0.95)))
    else:
        font_px = rng.randint(max(20, int(height * 0.45)), max(21, int(height * 0.95)))
    x = pad + rng.randint(-jitter, 2 * jitter)
    y0 = pad + rng.randint(-jitter, jitter)
    # Rule decoration: a strip-level kind, applied either to the whole line or a random subset
    # of its runs (real underlines often cover only the link portion of a line).
    rule_kind = None
    if not display and rng.random() < RULE_FRAC:
        rule_kind = rng.choices(RULE_KINDS, weights=RULE_WEIGHTS)[0]
    rule_all = rng.random() < 0.6
    drew_rule = False
    scripts, bold_runs, last_font = [], 0, "?"
    drew = False
    plan = [("latin", True)] if display else _run_plan(rng)
    for script, is_bold in plan:
        text = _display_text(rng, width, font_px) if display else _run_text(rng, script)
        units = _cjk_units(rng, text, is_bold) if script == "cjk" else [(text, is_bold)]
        for s, ub in units:
            if not s.strip():
                continue
            path = rng.choice(_display_fonts()) if display else None
            path, layout = (path, ImageFont.Layout.BASIC) if path else _pick_font(rng, script, ub)
            try:
                font = ImageFont.truetype(path, size=font_px, layout_engine=layout)
                l, t, r, b = td.textbbox((0, 0), s, font=font)
            except OSError:
                continue
            if r - l < 1 or b - t < 1:
                continue
            y = y0 + (height - (b - t)) // 2 - t
            td.text((x, y), s, font=font, fill=255, stroke_width=stroke, stroke_fill=255)
            if outlined:
                fill_draw.text((x, y), s, font=font, fill=255)
            # Bold target = the glyph-core stroke width (no outline ring), painted flat at
            # this run's cached per-font value. A CJK mid-string span or a run boundary thus
            # carries a real weight seam the model must read from stroke thickness.
            bvd.text((x, y), s, font=font, fill=_target_q(_font_stroke_ratio(path, script)))
            if ub:
                bold_runs += 1
            adv = int(td.textlength(s, font=font))
            # Draw the rule across this run's advance, into `total` (ink, so the matte erases
            # it), `rule_img` (its own label), and the outline-fill core when outlined (so the
            # ring colour split doesn't mistake the rule for an outline edge). Never into
            # `boldval`: a rule has no stroke-width weight.
            if rule_kind and (rule_all or rng.random() < 0.5):
                ascent, descent = font.getmetrics()
                baseline = y + ascent
                thick = max(1, round(font_px * rng.uniform(0.045, 0.085)))
                if rule_kind == "underline":
                    ry = baseline + max(1, round(descent * 0.30))
                elif rule_kind == "strike":
                    ry = baseline - round(ascent * 0.30)
                else:
                    ry = baseline - round(ascent * 0.92)
                rect = [x, ry, x + adv, ry + thick - 1]
                td.rectangle(rect, fill=255)
                rd.rectangle(rect, fill=255)
                if outlined:
                    fill_draw.rectangle(rect, fill=255)
                drew_rule = True
            # CJK is set flush (no inter-unit gap); spaced scripts get a word gap. Display mode
            # packs tight (condensed headers have small inter-letter gaps) to keep coverage high.
            if script == "cjk":
                gap = 0
            elif display:
                gap = rng.randint(max(1, font_px // 12), max(2, font_px // 6))
            else:
                gap = rng.randint(font_px // 6, font_px // 2)
            x += adv + gap
            drew, last_font = True, os.path.basename(path)
            scripts.append(script)
        if x > size[0] - pad:
            break
    if not drew:
        z = np.zeros((height, width), dtype=np.float32)
        return z, z, z, z  # caller retries

    if rng.random() < 0.5:
        angle = rng.uniform(-3.0, 3.0)
        center = (pad, pad + height // 2)
        total = total.rotate(angle, resample=Image.BILINEAR, center=center)
        # NEAREST keeps the stroke-width value flat instead of blending it toward 0 at edges.
        boldval = boldval.rotate(angle, resample=Image.NEAREST, center=center)
        rule_img = rule_img.rotate(angle, resample=Image.BILINEAR, center=center)
        if outlined:
            fill_canvas = fill_canvas.rotate(angle, resample=Image.BILINEAR, center=center)
    tot = np.asarray(total, dtype=np.float32) / 255.0
    bv = np.asarray(boldval, dtype=np.float32) / 255.0
    rl = np.asarray(rule_img, dtype=np.float32) / 255.0
    fl = np.asarray(fill_canvas, dtype=np.float32) / 255.0 if outlined else tot
    thick_k = 0
    if height >= 48 and rng.random() < 0.4:
        # Thicken to simulate super-heavy weight beyond what fonts provide (see note
        # below). Applied identically to total/fill so the matte labels stay aligned.
        thick_k = int(np.clip(round(height / 40) * 2 + 1, 3, 11))

        def mf(a: np.ndarray) -> np.ndarray:
            return (
                np.asarray(
                    Image.fromarray((a * 255).astype(np.uint8)).filter(
                        ImageFilter.MaxFilter(thick_k)
                    ),
                    dtype=np.float32,
                )
                / 255.0
            )

        tot = mf(tot)
        fl = mf(fl) if outlined else tot
        rl = mf(rl)  # keep the rule label aligned with the thickened matte
        # Thickening adds real stroke width, so the target must rise: grow the value region
        # with the matte, then raise it. Added half-width is (thick_k-1)/font_px of stroke
        # ratio, which under the logistic target is a constant logit shift on the grown value.
        shift = BOLD_RATIO_K * (thick_k - 1) / font_px
        grown = mf(bv)
        gc = np.clip(grown, 1e-4, 1.0 - 1e-4)
        shifted = 1.0 / (1.0 + np.exp(-(np.log(gc / (1.0 - gc)) + shift)))
        bv = np.where(grown > 0, shifted, 0.0).astype(np.float32)
    if log is not None:
        log.update(
            font=last_font,
            size=font_px,
            script="+".join(dict.fromkeys(scripts)),
            heavy=bold_runs > 0,
            stroke=stroke,
            thick_k=thick_k,
            rule=rule_kind if drew_rule else "",
        )
    crop = lambda a: a[pad : pad + height, pad : pad + width]  # noqa: E731
    return crop(tot), crop(fl), crop(bv), crop(rl)


# Real photo backgrounds (OTR gt_image plates: text-free scenes). Compositing our synth
# text onto these makes *everything except the glyphs* a real negative — saturated/textured
# objects labelled not-ink — which is the lever against the matte over-marking busy
# backgrounds. Env-gated: off unless INK_OTR_PLATES points at a dir of plate images.
OTR_PLATES_DIR = os.environ.get("INK_OTR_PLATES", "")
OTR_BG_FRAC = float(os.environ.get("INK_OTR_BG_FRAC", "0.0"))


@lru_cache(maxsize=1)
def _otr_plate_paths() -> tuple[str, ...]:
    if not OTR_PLATES_DIR:
        return ()
    return tuple(sorted(glob.glob(os.path.join(OTR_PLATES_DIR, "**", "*.png"), recursive=True)
                        + glob.glob(os.path.join(OTR_PLATES_DIR, "**", "*.jpg"), recursive=True)))


@lru_cache(maxsize=256)
def _otr_plate(path: str) -> Image.Image:
    # Decode once and cache: PNG decode dominates the per-composite cost (resize/crop are
    # cheap), and at reuse>1 the same plate is sampled repeatedly within a worker.
    return Image.open(path).convert("RGB")


def _otr_background(rng: random.Random, h: int, w: int, log: dict | None = None) -> np.ndarray:
    """A random (h, w) crop of a real OTR plate, at a random zoom so texture scale varies."""
    im = _otr_plate(rng.choice(_otr_plate_paths()))
    iw, ih = im.size
    s = max(w / iw, h / ih) * rng.uniform(1.0, 2.5)  # cover the crop, with random zoom-in
    nw, nh = max(w, round(iw * s)), max(h, round(ih * s))
    im = im.resize((nw, nh), Image.BILINEAR)
    x0, y0 = rng.randint(0, nw - w), rng.randint(0, nh - h)
    if log is not None:
        log["bg"] = "otr"
    return np.asarray(im.crop((x0, y0, x0 + w, y0 + h)), dtype=np.float32) / 255.0


def gradient_field(rng: random.Random, h: int, w: int, log: dict | None = None) -> np.ndarray:
    """HxWx3 background in 0..1: solid, gradient, color blocks, or busy texture.

    The block and busy-texture cases are deliberately high-contrast with hard edges:
    saturated colored regions and torn-paper/shaded boundaries are exactly the
    backgrounds the model wrongly mattes as ink, so it must see them with an empty
    label and learn that structure alone isn't ink.
    """
    kind = rng.random()
    if log is not None:
        log["bg"] = ("solid" if kind < 0.20 else "linear" if kind < 0.50
                     else "radial" if kind < 0.62 else "blocks" if kind < 0.80 else "texture")
    yy, xx = _coord_grid(h, w)
    c0, c1 = random_color(rng), random_color(rng)
    if kind < 0.20:
        return np.broadcast_to(c0, (h, w, 3)).copy()
    if kind < 0.50:
        angle = rng.uniform(0, 2 * np.pi)
        t = (np.cos(angle) * xx / w) + (np.sin(angle) * yy / h)
        t = (t - t.min()) / max(t.max() - t.min(), 1e-6)
        if rng.random() < 0.3:
            t = np.abs(2 * t - 1)  # two-tone band through the strip
        return t[..., None] * c1 + (1 - t[..., None]) * c0
    if kind < 0.62:
        cx, cy = rng.uniform(0, w), rng.uniform(0, h)
        r = np.sqrt((xx - cx) ** 2 + (yy - cy) ** 2)
        t = np.clip(r / max(r.max(), 1e-6), 0, 1)
        return t[..., None] * c1 + (1 - t[..., None]) * c0
    if kind < 0.80:
        field = np.broadcast_to(c0, (h, w, 3)).copy()
        for _ in range(rng.randint(2, 6)):
            bx0, bx1 = sorted(rng.randint(0, w) for _ in range(2))
            by0, by1 = sorted(rng.randint(0, h) for _ in range(2))
            field[by0:by1, bx0:bx1] = random_color(rng)
        return field
    grid = rng.choice([6, 8, 12])
    blob = np.random.default_rng(rng.getrandbits(32)).random(
        (max(2, h // grid), max(2, w // grid), 3)
    ).astype(np.float32)
    blob = np.asarray(
        Image.fromarray((blob * 255).astype(np.uint8)).resize((w, h), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0
    mix = rng.uniform(0.5, 1.0)
    return mix * blob + (1 - mix) * c0


def sample(rng: random.Random | None = None, width: int | None = None):
    """One training pair, generated at *native* scale then resized to height 48.

    Real strips are dewarped crops whose source box is mostly 30–48 px tall; the
    resize-to-48 only mildly rescales the antialiased glyph edges. Rendering directly
    at 48 px would teach the model edge statistics it never sees at inference, so we
    render/composite/degrade at a sampled native height and resample both image and
    label exactly like the pipeline does.
    """
    rng = rng or random.Random()
    width = width or rng.choice(WIDTHS)
    for _attempt in range(8):
        cov, fill, bold, rule, native_h, native_w = _render_once(rng, width)
        img, cov_out, bold_out, rule_out, fcol, bcol, ok = _composite_once(
            rng, cov, fill, bold, rule, native_h, native_w, width
        )
        if ok:
            break
    return img, cov_out, bold_out, rule_out, fcol, bcol


def stream(rng: random.Random, width: int, reuse: int = 1, apply_degrade: bool = True):
    """Infinite (img, cov, bold, rule, fcol, bcol, native_h) pairs with glyph-raster reuse.

    Rasterizing text dominates generation cost, so each rendered coverage is reused
    across `reuse` composites with fresh background/ink/degradation — dividing the
    rasterization cost by `reuse`. The label repeats within a reuse group (same
    coverage, different appearance), which is benign augmentation for a matte target.

    With `apply_degrade=False` the per-strip CPU degrade + legibility are skipped (the
    GPU does them on the batch); `native_h` is yielded so the GPU can rescale blur.
    """
    while True:
        cov, fill, bold, rule, native_h, native_w = _render_once(rng, width)
        for _ in range(reuse):
            img = cov_out = bold_out = rule_out = fcol = bcol = None
            for _try in range(3):
                img, cov_out, bold_out, rule_out, fcol, bcol, ok = _composite_once(
                    rng, cov, fill, bold, rule, native_h, native_w, width, apply_degrade=apply_degrade
                )
                if ok:
                    break
            yield img, cov_out, bold_out, rule_out, fcol, bcol, native_h


def _render_once(rng: random.Random, width: int, log: dict | None = None):
    # Tail above 48: signage/display text whose native height far exceeds the strip —
    # after the squash its strokes are very thick and glyph interiors are majority-ink,
    # a density regime body text never reaches (validation: station-sign letters
    # mottled when 12–48 was the whole training range).
    if rng.random() < 0.2:
        native_h = int(rng.uniform(48, 160))
    else:
        # native_h is the detection-box height (ascender-to-descender band + padding),
        # i.e. the oriented-box height the runtime dewarps and resizes to 48. Real source
        # text sits ~30-48 px tall; 24 is a hard floor (below it the resize is a 2x+
        # upsample and the stroke-width cue for bold goes sub-pixel). Mode 40.
        native_h = int(rng.triangular(24, 48, 40))
    if log is not None:
        log["native_h"] = native_h
    native_w = max(16, round(width * native_h / HEIGHT))
    cov, fill, bold, rule = render_coverage(rng, native_w, native_h, log)
    return cov, fill, bold, rule, native_h, native_w


def _composite_once(rng: random.Random, cov, fill, bold, rule, native_h: int, native_w: int, width: int,
                    log: dict | None = None, apply_degrade: bool = True):
    if _otr_plate_paths() and rng.random() < OTR_BG_FRAC:
        bg = _otr_background(rng, native_h, native_w, log)
    else:
        bg = gradient_field(rng, native_h, native_w, log)
    if rng.random() < 0.15:
        # Drop shadow: an offset dark replica behind the glyphs. It is *not* ink —
        # the label stays `cov` — so the model learns to leave shadows to the
        # background reconstruction instead of matting them.
        dy, dx = rng.randint(1, 3), rng.randint(-2, 3)
        shadow = np.roll(np.roll(cov, dy, axis=0), dx, axis=1)
        bg = bg * (1.0 - (shadow * rng.uniform(0.35, 0.7))[..., None])
        if log is not None:
            log["dropshadow"] = 1
    ink = random_color(rng)
    # Sometimes force low contrast against the background mean.
    if rng.random() < 0.35:
        mean_bg = bg.mean(axis=(0, 1))
        ink = np.clip(mean_bg + np.sign(ink - mean_bg) * rng.uniform(0.05, 0.30), 0, 1)
        if log is not None:
            log["locontrast"] = 1
    if rng.random() < 0.1:
        # Gradient ink: lerp between two ink colors across the strip.
        ink2 = random_color(rng)
        t = np.linspace(0, 1, native_w, dtype=np.float32)[None, :, None]
        ink_field = np.broadcast_to(t * ink2 + (1 - t) * ink, (native_h, native_w, 3)).copy()
    else:
        ink_field = np.broadcast_to(ink, (native_h, native_w, 3)).copy()
    ring = np.clip(cov - fill, 0.0, 1.0)
    if ring.max() > 0.05:
        # Outlined text: the ring gets its own color (e.g. white core, blue edge).
        outline = random_color(rng)
        frac = np.divide(ring, np.maximum(cov, 1e-3))[..., None]
        ink_field = ink_field * (1 - frac) + outline * frac
        if log is not None:
            log["outline"] = 1
    img = cov[..., None] * ink_field + (1 - cov[..., None]) * bg
    # F/B colour labels for the colour head: F is the ink colour field the glyphs were
    # composited with (defined over the whole strip — FBA-style everywhere-supervision),
    # B is the background render (incl. drop shadow, which is background, not ink).
    fcol = np.ascontiguousarray(ink_field, dtype=np.float32)
    bcol = np.ascontiguousarray(bg, dtype=np.float32)
    # Training skips this: degrade + the legibility gate run batched on the GPU instead
    # (the legibility result becomes a per-strip loss mask, not a reject-and-retry).
    if apply_degrade:
        aux = [fcol, bcol]
        img = degrade(img, rng, native_h, log, photometric_aux=aux)
        fcol, bcol = aux
        ok = legible(img, cov, native_h)
    else:
        ok = True
    if apply_degrade and log is not None:
        im, bm = cov > 0.6, cov < 0.05
        if im.sum() > 10 and bm.sum() > 10:
            d = np.abs(np.median(img[im], axis=0) - np.median(img[bm], axis=0))
            log["contrast"] = round(float(d.max()), 2)
        log["ok"] = ok
    if native_h != HEIGHT:
        def rgb_resize(a: np.ndarray) -> np.ndarray:
            return np.asarray(
                Image.fromarray((np.clip(a, 0, 1) * 255).astype(np.uint8)).resize(
                    (width, HEIGHT), Image.BILINEAR
                ),
                dtype=np.float32,
            ) / 255.0

        img = rgb_resize(img)
        fcol = rgb_resize(fcol)
        bcol = rgb_resize(bcol)
        cov = np.asarray(
            Image.fromarray(cov, mode="F").resize((width, HEIGHT), Image.BILINEAR),
            dtype=np.float32,
        )
        bold = np.asarray(
            Image.fromarray(bold, mode="F").resize((width, HEIGHT), Image.BILINEAR),
            dtype=np.float32,
        )
        rule = np.asarray(
            Image.fromarray(rule, mode="F").resize((width, HEIGHT), Image.BILINEAR),
            dtype=np.float32,
        )
    return (
        img.astype(np.float32, copy=False),
        np.clip(cov, 0.0, 1.0).astype(np.float32, copy=False),
        np.clip(bold, 0.0, 1.0).astype(np.float32, copy=False),
        np.clip(rule, 0.0, 1.0).astype(np.float32, copy=False),
        fcol.astype(np.float32, copy=False),
        bcol.astype(np.float32, copy=False),
        ok,
    )


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", required=True)
    ap.add_argument("--n", type=int, default=16)
    ap.add_argument("--seed", type=int, default=0)
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)
    rng = random.Random(args.seed)
    lines = []
    for i in range(args.n):
        # Replicate sample() but capture the per-sample params into `log`. Fresh log
        # per attempt — optional fields would otherwise leak from a failed retry.
        width = rng.choice(WIDTHS)
        img = cov = bold = rule = fcol = bcol = None
        log: dict = {}
        for _ in range(8):
            log = {}
            c, fl, bl, rl, nh, nw = _render_once(rng, width, log)
            img, cov, bold, rule, fcol, bcol, ok = _composite_once(rng, c, fl, bl, rl, nh, nw, width, log)
            if ok:
                break
        h, w = cov.shape
        ink = cov > 0.5
        bmu = float(bold[ink].mean()) if ink.any() else 0.0  # mean stroke target over ink
        band = 26
        # Six stacked rows: composited image, matte, bold, rule, then the F (ink colour)
        # and B (background colour) fields for the colour head.
        sheet = np.ones((band + h * 6 + 20, max(w, 360), 3), dtype=np.float32)
        sheet[band : band + h, :w] = img
        sheet[band + h + 4 : band + h * 2 + 4, :w] = cov[..., None]
        sheet[band + h * 2 + 8 : band + h * 3 + 8, :w] = bold[..., None]
        sheet[band + h * 3 + 12 : band + h * 4 + 12, :w] = rule[..., None]
        sheet[band + h * 4 + 16 : band + h * 5 + 16, :w] = fcol
        sheet[band + h * 5 + 20 :, :w] = bcol
        pim = Image.fromarray((sheet * 255).astype(np.uint8))
        deg = " ".join(
            f"{kk}{log[kk]}" for kk in ("blur", "downsample", "jpeg", "noise", "squeeze",
                                        "motion", "shade", "hardshadow") if kk in log)
        head = (f"{i:03d} nh{log.get('native_h')} sz{log.get('size')} k{log.get('thick_k', 0)} "
                f"bμ{bmu:.2f} {log.get('script', '')} {'HVY ' if log.get('heavy') else ''}"
                f"{(log.get('rule') + ' ') if log.get('rule') else ''}"
                f"{log.get('bg', '')} ct{log.get('contrast', '?')}")
        d = ImageDraw.Draw(pim)
        d.text((2, 1), head, fill=(220, 0, 0))
        d.text((2, 13), deg, fill=(0, 0, 200))
        pim.save(os.path.join(args.out, f"sample-{i:03d}.png"))
        lines.append(f"{head} | {deg} | ok={log.get('ok')} font={log.get('font')} "
                     f"stroke{log.get('stroke')} "
                     f"{'locontrast ' if log.get('locontrast') else ''}"
                     f"{'outline ' if log.get('outline') else ''}"
                     f"{'dropshadow' if log.get('dropshadow') else ''}")
    with open(os.path.join(args.out, "params.txt"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote {args.n} annotated samples + params.txt to {args.out}")


if __name__ == "__main__":
    main()
