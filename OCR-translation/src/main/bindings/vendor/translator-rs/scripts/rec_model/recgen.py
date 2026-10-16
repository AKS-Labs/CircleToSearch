"""Shared core for synthetic text-line recognizer data (script-agnostic).

Holds everything that doesn't depend on the script: HarfBuzz shaping + FreeType
rasterization, procedural backgrounds, camera/scan degradations, compositing,
non-text negatives, and the PaddleOCR-format dataset/inspect CLI loop.

A per-script generator (gen_hebrew.py, gen_indic.py) supplies a `Spec`:
- `fonts`   : candidate font paths (from `discover_fonts`)
- `charset` : every char the script can emit (for font coverage + keys.txt)
- `reorder` : HarfBuzz shaping mode. False = force LTR with no reordering (caller
              already produced a visual-order string, e.g. Hebrew via python-bidi).
              True = natural shaping, let HB reorder glyphs (Indic conjuncts +
              pre-base matras); the label stays LOGICAL order (HB keeps clusters
              logical, and the local reorder is within the CNN receptive field).
- `gen_pair(rng, corpus) -> (render_text, label)` : the script's text generator.
              `render_text` is fed to the shaper; `label` is written to the dataset.
              They differ only for Hebrew (render=label=visual). For Indic both are
              the logical string.

Heights/length match the PP-OCRv6 rec config (48px, max_text_length 25).
"""

import argparse
import io
import os
import random
import subprocess
import sys
import traceback
from dataclasses import dataclass, field
from functools import lru_cache
from typing import Callable

import freetype
import numpy as np
import uharfbuzz as hb
from PIL import Image, ImageDraw, ImageFilter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from synth_core import apply_warp, legible, warp_maps  # noqa: E402

STRIP_HEIGHT = 48
MAX_LABEL_LEN = 25


@dataclass
class Spec:
    name: str
    fonts: tuple[str, ...]
    charset: str
    reorder: bool
    gen_pair: Callable[[random.Random, list[str], frozenset], tuple[str, str]]
    # The model's output classes (keys.txt), used to constrain the generators' random
    # fallbacks so a synthesized label can never contain an out-of-dict glyph. Filled
    # from --dict by run_cli; the corpus path is already kept-only (build_corpus).
    vocab: frozenset = frozenset()
    # Per-font overrides of how this charset is drawn, as {font_path: ((char, drawn_as), ...)}.
    # `drawn_as = None` bars the face from that character. Needed where a face draws a
    # character at a codepoint other than the label's: BPG's pre-Unicode-11 Georgian Caps
    # fonts carry caps-shaped glyphs at the *Mkhedruli* codepoints, so they render Mtavruli
    # only after mapping down, and must be barred from Mkhedruli or they would teach
    # caps shapes under lowercase labels. Applied to coverage and to the shaped text
    # together, so font choice and rasterization cannot disagree.
    font_remap: dict[str, tuple[tuple[str, str | None], ...]] = field(default_factory=dict)


# ---------------------------------------------------------------- fonts / shaping


def discover_fonts(lang: str) -> tuple[str, ...]:
    out = subprocess.run(["fc-list", f":lang={lang}", "file"], capture_output=True, text=True, check=True).stdout
    paths = [ln.split(":")[0].strip() for ln in out.splitlines()
             if ln.split(":")[0].strip().lower().endswith((".ttf", ".otf"))]
    if not paths:
        raise RuntimeError(f"fc-list found no fonts for lang={lang}")
    return tuple(sorted(set(paths)))


@lru_cache(maxsize=None)
def _ft_face(path: str) -> freetype.Face:
    return freetype.Face(path)


@lru_cache(maxsize=None)
def _hb_font(path: str) -> hb.Font:
    return hb.Font(hb.Face(hb.Blob.from_file_path(path)))


@lru_cache(maxsize=None)
def _covered(path: str, charset: str) -> frozenset:
    face = _ft_face(path)
    return frozenset(cp for cp in map(ord, charset) if face.get_char_index(cp) != 0)


@lru_cache(maxsize=None)
def _covered_as(path: str, charset: str, remap: tuple[tuple[str, str | None], ...]) -> frozenset:
    face = _ft_face(path)
    sub = dict(remap)
    return frozenset(ord(ch) for ch in charset
                     if sub.get(ch, ch) is not None and face.get_char_index(ord(sub.get(ch, ch))) != 0)


def _draws(spec: "Spec", path: str) -> frozenset:
    return _covered_as(path, spec.charset, spec.font_remap.get(path, ()))


def _as_drawn(spec: "Spec", path: str, text: str) -> str:
    remap = spec.font_remap.get(path)
    return text.translate({ord(a): b for a, b in remap}) if remap else text


def fonts_for(text: str, spec: "Spec") -> list[str]:
    need = set(map(ord, text)) - {0x20}
    return [p for p in spec.fonts if need <= _draws(spec, p)]


MAX_FALLBACK_FONTS = 3


def plan_runs(text: str, spec: "Spec", rng: random.Random) -> list[tuple[str, str]] | None:
    """Split `text` into (run_text, font_path) runs, or None if no font set covers it.

    A single covering face stays a single run, so lines that render today are unaffected.
    When no one face covers the line, the text is split across up to MAX_FALLBACK_FONTS
    by greedy set cover, which is what a real renderer does: distribution builds of
    script fonts routinely carry no Latin or digits (Debian's Noto Sans Georgian has
    neither), so <script> <email> <script> and all-caps-plus-price lines exist on real
    pages only as multi-font renders. Refusing to synthesize them teaches the model that
    the script never co-occurs with Latin, and the NRTR head then suppresses those decodes.
    """
    single = fonts_for(text, spec)
    if single:
        font = rng.choice(single)
        return [(_as_drawn(spec, font, text), font)]

    remaining = set(map(ord, text)) - {0x20}
    chosen: list[str] = []
    while remaining and len(chosen) < MAX_FALLBACK_FONTS:
        gains = [(len(_draws(spec, p) & remaining), p) for p in spec.fonts if p not in chosen]
        best = max(g for g, _ in gains)
        if best == 0:
            return None
        chosen.append(rng.choice([p for g, p in gains if g == best]))
        remaining -= _draws(spec, chosen[-1])
    if remaining:
        return None

    runs: list[tuple[str, str]] = []
    for ch in text:
        # A space carries no glyph worth switching fonts for, so it extends the current run
        # (and only opens a new one when it leads the line).
        font = runs[-1][1] if ch == " " and runs else next(p for p in chosen if ord(ch) in _draws(spec, p))
        if runs and runs[-1][1] == font:
            runs[-1] = (runs[-1][0] + ch, font)
        else:
            runs.append((ch, font))
    return [(_as_drawn(spec, f, t), f) for t, f in runs]


def shape_render(runs: list[tuple[str, str]], px: int, rng: random.Random | None, reorder: bool) -> np.ndarray | None:
    """Coverage (H x W float 0..1) for the text in `runs`, or None on a .notdef glyph.

    Shapes with HarfBuzz and rasterizes each glyph at the shaped pen positions. `runs` is
    the (text, font) segmentation from plan_runs, laid out left to right on one baseline at
    one em size — the metric mismatch between a script face and its Latin fallback is what
    a real renderer produces too. `reorder=False` forces LTR with no bidi/reordering (caller
    pre-ordered the string); `reorder=True` uses HB's natural per-script shaping. When `rng`
    is given, inter-word gaps are jittered while the label keeps single spaces.
    """
    shaped = []
    for run_text, font_path in runs:
        hbfont = _hb_font(font_path)
        hbfont.scale = (px * 64, px * 64)
        buf = hb.Buffer()
        buf.add_str(run_text)
        buf.guess_segment_properties()
        if not reorder:
            buf.direction = "ltr"
        hb.shape(hbfont, buf)
        infos, poss = buf.glyph_infos, buf.glyph_positions
        if any(i.codepoint == 0 for i in infos):
            return None
        advances = []
        for info, pos in zip(infos, poss):
            adv = pos.x_advance
            if rng is not None and run_text[info.cluster] == " ":
                adv = int(adv * rng.uniform(0.7, 2.4))
            advances.append(adv)
        shaped.append((font_path, infos, poss, advances))

    pad = max(4, px // 6)
    width = sum(a for _, _, _, advs in shaped for a in advs) // 64 + 2 * pad
    height = int(px * 1.7)
    baseline = int(px * 1.3)
    canvas = np.zeros((height, width), np.float32)
    x = pad * 64
    for font_path, infos, poss, advances in shaped:
        ft = _ft_face(font_path)
        ft.set_pixel_sizes(0, px)
        for info, pos, adv in zip(infos, poss, advances):
            ft.load_glyph(info.codepoint, freetype.FT_LOAD_RENDER)
            bm = ft.glyph.bitmap
            gw, gh = bm.width, bm.rows
            if gw and gh:
                glyph = np.asarray(bm.buffer, np.uint8).reshape(gh, gw).astype(np.float32) / 255.0
                gx = (x + pos.x_offset) // 64 + ft.glyph.bitmap_left
                gy = baseline - ft.glyph.bitmap_top - pos.y_offset // 64
                y0, x0 = max(0, gy), max(0, gx)
                y1, x1 = min(height, gy + gh), min(width, gx + gw)
                if y1 > y0 and x1 > x0:
                    canvas[y0:y1, x0:x1] = np.maximum(canvas[y0:y1, x0:x1], glyph[y0 - gy:y1 - gy, x0 - gx:x1 - gx])
            x += adv
    rows = np.where(canvas.max(axis=1) > 0.05)[0]
    cols = np.where(canvas.max(axis=0) > 0.05)[0]
    if rows.size < 4 or cols.size < 4:
        return None
    r0, r1 = max(0, rows[0] - pad), min(height, rows[-1] + 1 + pad)
    c0, c1 = max(0, cols[0] - pad), min(width, cols[-1] + 1 + pad)
    return canvas[r0:r1, c0:c1]


# ---------------------------------------------------------------- image ops


def random_color(rng: random.Random) -> np.ndarray:
    if rng.random() < 0.22:
        # Saturated banner color (one channel high, others low).
        c = [rng.uniform(0.0, 0.22) for _ in range(3)]
        c[rng.randrange(3)] = rng.uniform(0.55, 1.0)
        return np.array(c, dtype=np.float32)
    return np.array([rng.random(), rng.random(), rng.random()], dtype=np.float32)


def background(rng: random.Random, h: int, w: int) -> np.ndarray:
    kind = rng.random()
    c0 = random_color(rng)
    if kind < 0.45:
        return np.broadcast_to(c0, (h, w, 3)).copy()
    if kind < 0.8:
        c1 = random_color(rng)
        angle = rng.uniform(0, 2 * np.pi)
        yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
        t = np.cos(angle) * xx / max(w, 1) + np.sin(angle) * yy / max(h, 1)
        t = (t - t.min()) / max(t.max() - t.min(), 1e-6)
        return t[..., None] * c1 + (1 - t[..., None]) * c0
    grid = rng.choice([6, 10, 16])
    blob = np.random.default_rng(rng.getrandbits(32)).random((max(2, h // grid), max(2, w // grid), 3)).astype(np.float32)
    blob = np.asarray(Image.fromarray((blob * 255).astype(np.uint8)).resize((w, h), Image.BILINEAR), np.float32) / 255.0
    mix = rng.uniform(0.4, 0.85)
    return mix * blob + (1 - mix) * c0


def degrade(img: np.ndarray, rng: random.Random, native_h: int) -> np.ndarray:
    pil = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8))
    if rng.random() < 0.5:
        pil = pil.filter(ImageFilter.GaussianBlur(rng.uniform(0.3, min(0.4 + native_h * 0.03, 1.8))))
    if rng.random() < 0.3:
        s = rng.uniform(0.55, 0.85)
        small = pil.resize((max(8, int(pil.width * s)), max(8, int(pil.height * s))), Image.BILINEAR)
        pil = small.resize((pil.width, pil.height), Image.BILINEAR)
    if rng.random() < 0.75:
        buf = io.BytesIO()
        pil.save(buf, format="JPEG", quality=rng.randint(40, 95))
        buf.seek(0)
        pil = Image.open(buf).convert("RGB")
    out = np.asarray(pil, np.float32) / 255.0
    if rng.random() < 0.5:
        yy, xx = np.mgrid[0:out.shape[0], 0:out.shape[1]].astype(np.float32)
        angle = rng.uniform(0, 2 * np.pi)
        t = np.cos(angle) * xx / out.shape[1] + np.sin(angle) * yy / out.shape[0]
        t = (t - t.min()) / max(t.max() - t.min(), 1e-6)
        out = out * np.clip(rng.uniform(0.6, 1.0) + t * rng.uniform(0.0, 0.4), 0.4, 1.2)[..., None]
    if rng.random() < 0.6:
        out = out + np.random.default_rng(rng.getrandbits(32)).normal(0, rng.uniform(0.005, 0.035), out.shape).astype(np.float32)
    return np.clip(out, 0, 1)


def compose(rng: random.Random, cov: np.ndarray, native_h: int) -> np.ndarray:
    h, w = cov.shape
    bg = background(rng, h, w)
    ink = random_color(rng)
    if rng.random() < 0.35:
        # Low-contrast band: ink sits close to the background mean, so the model trains on
        # faint text down to barely-readable (the legibility gate trims anything below
        # readable). Without this every sample is high-contrast and the recognizer fails on
        # faint text. Mirrors the ink generator's low-contrast force.
        mean_bg = bg.mean(axis=(0, 1))
        ink = np.clip(mean_bg + np.sign(ink - mean_bg) * rng.uniform(0.05, 0.30), 0, 1).astype(np.float32)
    elif float(bg.mean()) > 0.5:
        ink = ink * rng.uniform(0.0, 0.35)
    else:
        ink = 1.0 - (1.0 - ink) * rng.uniform(0.0, 0.35)
    img = cov[..., None] * ink + (1 - cov[..., None]) * bg
    return degrade(img, rng, native_h)


def _to_strip(img: np.ndarray) -> np.ndarray:
    out_w = max(16, round(img.shape[1] * STRIP_HEIGHT / img.shape[0]))
    return np.asarray(
        Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).resize((out_w, STRIP_HEIGHT), Image.BILINEAR),
        np.uint8,
    )


# ---------------------------------------------------------------- sampling


def make_negative(rng: random.Random, spec: Spec, corpus: list[str]) -> np.ndarray:
    """Non-text strip with an empty label (detector false-positive)."""
    h, w = rng.randint(22, 60), rng.randint(48, 320)
    if rng.random() < 0.5:
        pim = Image.fromarray((background(rng, h, w) * 255).astype(np.uint8))
        draw = ImageDraw.Draw(pim)
        for _ in range(rng.randint(1, 7)):
            c = tuple(int(v * 255) for v in random_color(rng))
            shape, wd = rng.random(), rng.randint(1, 4)
            xs, ys = sorted(rng.randint(0, w) for _ in range(2)), sorted(rng.randint(0, h) for _ in range(2))
            if shape < 0.5:
                draw.line([rng.randint(0, w), rng.randint(0, h), rng.randint(0, w), rng.randint(0, h)], fill=c, width=wd)
            elif shape < 0.8:
                draw.rectangle([xs[0], ys[0], xs[1], ys[1]], outline=c, width=wd)
            else:
                draw.ellipse([xs[0], ys[0], xs[1], ys[1]], outline=c, width=wd)
        img = np.asarray(pim, np.float32) / 255.0
    else:
        cov = None
        for _ in range(4):
            render_text, _ = spec.gen_pair(rng, corpus, spec.vocab)
            runs = plan_runs(render_text, spec, rng) if render_text.strip() else None
            if runs:
                cov = shape_render(runs, rng.randint(16, 40), rng, spec.reorder)
                if cov is not None and cov.shape[1] >= 8:
                    break
                cov = None
        if cov is None:
            return make_negative(rng, spec, [])
        band = max(3, int(cov.shape[0] * rng.uniform(0.18, 0.32)))
        off = rng.choice([0, cov.shape[0] - band])
        sliver = np.zeros((h, cov.shape[1]), np.float32)
        top = rng.randint(0, max(0, h - band))
        sliver[top:top + band] = cov[off:off + band]
        img = compose(rng, sliver, h)
    return _to_strip(degrade(img, rng, h))


def sample_safe(rng, spec, corpus, neg_frac, errs):
    """sample() but never raises — a rare bad font/text/shape combo must not kill a
    long-running worker (that exits non-zero and takes the whole bootstrap with it)."""
    try:
        return sample(rng, spec, corpus, neg_frac)
    except Exception:
        if not errs:
            traceback.print_exc()
        errs.append(1)
        return None


def sample(rng: random.Random, spec: Spec, corpus: list[str], neg_frac: float = 0.0) -> tuple[np.ndarray, str] | None:
    if neg_frac and rng.random() < neg_frac:
        return make_negative(rng, spec, corpus), ""
    for _ in range(8):
        render_text, label = spec.gen_pair(rng, corpus, spec.vocab)
        if not render_text.strip():
            continue
        runs = plan_runs(render_text, spec, rng)
        if runs is None:
            continue
        # native_h is the detector's oriented-box height; real source text sits ~30-48 px
        # tall, so floor at 30 — below it the strip is a 2x+ upsample of text the detector
        # never emits. Floor the em at 20 px as well: a small box renders sub-pixel strokes,
        # and few-pixel discriminative glyph features collapse before the head can separate
        # confusable letters. Mirrors the ink generator's render floor.
        native_h = rng.randint(30, 60)
        px = max(20, int(native_h * rng.uniform(0.62, 0.9)))
        cov = shape_render(runs, px, rng, spec.reorder)
        if cov is None or cov.shape[1] < 8:
            continue
        strip_h = max(native_h, cov.shape[0])
        top = (strip_h - cov.shape[0]) // 2
        # Cap horizontal whitespace to ~32px in the resized 48px strip: the detector's boxes
        # never trail more than that, so don't train on wider blank margins (native_h*2//3
        # native px -> 32 display px after the 48-row resize, independent of native_h).
        strip = np.zeros((strip_h, cov.shape[1] + rng.randint(2, max(3, native_h * 2 // 3))), np.float32)
        lpad = rng.randint(1, max(2, strip.shape[1] - cov.shape[1] - 1))
        strip[top:top + cov.shape[0], lpad:lpad + cov.shape[1]] = cov
        # Mild geometric warp (rotation/bend/perspective) on the coverage, so the residual
        # distortion left after the live pipeline's de-warp is represented. Mostly flat.
        if rng.random() < 0.6:
            strip = apply_warp(strip, *warp_maps(rng, *strip.shape))
        img = compose(rng, strip, strip_h)
        # Drop strips whose degraded ink no longer contrasts with its background: det/rec
        # gate what reaches the recognizer, so illegible (to a human) text is label noise.
        if not legible(img, strip, strip_h):
            continue
        if img.shape[1] * STRIP_HEIGHT / img.shape[0] > 1200:
            continue
        return _to_strip(img), label
    return None


def join_to_budget(rng: random.Random, words: list[str], budget: int) -> str:
    """Greedily join words within `budget` chars (bidi preserves char count)."""
    out, length = [], 0
    for w in words:
        add = len(w) + (1 if out else 0)
        if length + add > budget:
            break
        out.append(w)
        length += add
    return " ".join(out) if out else words[0][:budget]


# ---------------------------------------------------------------- CLI


def write_inspect_sheet(samples, path, annotate, spec: Spec):
    """QA sheet: each strip with its label above it, for eyeballing before a training run.

    The label is drawn through the same shaping and font-fallback path as the sample, since
    PIL's default bitmap font covers only Latin and drew every other script as tofu boxes —
    which defeats the one thing the sheet is for. Annotation shapes with reorder=True so an
    RTL label reads in logical order, whatever the sample itself was rendered from.
    """
    band, width = 30, max(im.shape[1] for im, _ in samples)
    rng = random.Random(0)
    cells = []
    for im, label in samples:
        cell = np.full((STRIP_HEIGHT + band, width, 3), 255, np.uint8)
        cell[band:, :im.shape[1]] = im
        text = annotate(label) or "(empty)"
        runs = plan_runs(text, spec, rng)
        cov = shape_render(runs, 18, None, True) if runs else None
        if cov is None:
            raise RuntimeError(f"inspect: no font renders label {text!r}")
        h, w = min(cov.shape[0], band), min(cov.shape[1], width)
        ink = cov[:h, :w, None]
        cell[:h, :w] = (ink * np.array([200, 0, 0], np.float32) + (1 - ink) * 255).astype(np.uint8)
        cells.append(np.asarray(Image.fromarray(cell).resize((720, int(cell.shape[0] * 720 / width)))))
    Image.fromarray(np.concatenate(cells, axis=0)).save(path)


def run_cli(spec: Spec, *, annotate=lambda s: s, extra_args=lambda ap: None):
    ap = argparse.ArgumentParser(description=f"synthetic rec data: {spec.name}")
    ap.add_argument("--out", required=True)
    ap.add_argument("--n", type=int, default=1000)
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--corpus", help="UTF-8 file, one line per row")
    ap.add_argument("--dict", required=True, help="keys.txt (the model's output classes); constrains random fallbacks")
    ap.add_argument("--neg-frac", type=float, default=0.0)
    ap.add_argument("--inspect", action="store_true")
    ap.add_argument("--prefix", default="")
    extra_args(ap)
    args = ap.parse_args()

    spec.vocab = frozenset(
        ch for ch in (ln.rstrip("\n") for ln in open(args.dict, encoding="utf-8")) if ch
    )
    corpus = [ln.strip() for ln in open(args.corpus, encoding="utf-8")] if args.corpus else []
    corpus = [c for c in corpus if c]
    rng = random.Random(args.seed)
    os.makedirs(args.out, exist_ok=True)

    errs: list = []
    if args.inspect:
        got = []
        while len(got) < args.n:
            s = sample_safe(rng, spec, corpus, args.neg_frac, errs)
            if s:
                got.append(s)
        write_inspect_sheet(got, os.path.join(args.out, "inspect.png"), annotate, spec)
        print(f"wrote {args.out}/inspect.png ({len(got)} samples)")
        return

    os.makedirs(os.path.join(args.out, "images"), exist_ok=True)
    tag = f"{args.prefix}_" if args.prefix else ""
    labels_path = os.path.join(args.out, f"labels{('_' + args.prefix) if args.prefix else ''}.txt")
    n = 0
    with open(labels_path, "w", encoding="utf-8") as labels:
        while n < args.n:
            s = sample_safe(rng, spec, corpus, args.neg_frac, errs)
            if not s:
                continue
            img, label = s
            name = f"images/{tag}{spec.name}_{n:07d}.png"
            Image.fromarray(img).save(os.path.join(args.out, name))
            labels.write(f"{name}\t{label}\n")
            n += 1
            if n % 2000 == 0:
                print(f"  [{args.prefix or '0'}] {n}/{args.n}", flush=True)
    print(f"wrote {n} samples -> {labels_path} (skipped {len(errs)} errors)")
