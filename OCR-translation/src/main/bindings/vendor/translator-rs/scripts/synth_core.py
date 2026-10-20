"""Shared synthesis core for the recognizer (rec) and ink-matting generators.

Both models learn from the same underlying distribution — *degraded text on a
noisy background, gated to what stays legible* — they only differ in the head
they supervise (rec: a transcription label; ink: a per-pixel matte + bold). The
script-agnostic image primitives therefore live here so a lesson learned for one
model (the native-height render floor, the min-contrast legibility gate, a new
degradation) lands in both instead of being re-discovered separately.

What stays in each generator: the glyph rasterizer and its label derivation
(rec needs HarfBuzz clusters in visual order; ink needs a bold/fill channel),
and the per-model compositing. Everything below is shared.

`degrade` is the CPU source of truth; the ink training loop's GPU-batched mirror
(`gpu_degrade.py`) is validated against it.
"""

import io
import math
import random
from functools import lru_cache

import cv2
import numpy as np
from PIL import Image, ImageFilter

# OpenCV ignores OMP_NUM_THREADS and defaults its pool to the host core count (nproc),
# which on a CFS-quota'd container (e.g. vast.ai: nproc 80, quota 19) means every
# dataloader worker spawns ~80 cv2 threads — the container self-oversubscribes its own
# quota and gets CFS-throttled to a crawl. One thread per process; parallelism comes from
# the workers, not from cv2 internally.
cv2.setNumThreads(1)


@lru_cache(maxsize=64)
def coord_grid(h: int, w: int) -> tuple[np.ndarray, np.ndarray]:
    """Read-only (yy, xx) pixel grids, cached per size — callers only read them."""
    yy, xx = np.mgrid[0:h, 0:w]
    yy = np.ascontiguousarray(yy, dtype=np.float32)
    xx = np.ascontiguousarray(xx, dtype=np.float32)
    yy.flags.writeable = False
    xx.flags.writeable = False
    return yy, xx


def random_color(rng: random.Random, banner_prob: float = 0.0) -> np.ndarray:
    """A random RGB in 0..1. With probability `banner_prob`, a saturated single-channel
    "banner" color (one channel high, others low) — the white-on-red signage regime rec
    needs for legibility; ink leaves it at 0 (the default draws no extra rng value)."""
    if banner_prob and rng.random() < banner_prob:
        c = [rng.uniform(0.0, 0.22) for _ in range(3)]
        c[rng.randrange(3)] = rng.uniform(0.55, 1.0)
        return np.array(c, dtype=np.float32)
    return np.array([rng.random(), rng.random(), rng.random()], dtype=np.float32)


def warp_maps(rng: random.Random, h: int, w: int) -> tuple[np.ndarray, np.ndarray]:
    """cv2.remap sampling maps for a *mild* geometric warp: ±3° rotation, a small
    parabolic baseline bend, and a small keystone/perspective tilt.

    Deliberately gentle: the live pipeline de-warps each detected box before rec, so
    what survives to the recognizer is mostly-flat residual, not strong distortion.
    A cheap vectorised remap replaces PaddleOCR's pure-Python MLS warp (`tia`), which
    ate ~85% of dataloader CPU. Maps are returned (not applied) so a caller can warp an
    image and its label channels with the identical transform (ink: matte + bold)."""
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    cx, cy = (w - 1) / 2.0, (h - 1) / 2.0
    nx = (xx - cx) / max(cx, 1.0)                                  # ~[-1, 1] across width
    ang = math.radians(rng.uniform(-2.5, 2.5))
    ca, sa = math.cos(ang), math.sin(ang)
    sx = ca * (xx - cx) - sa * (yy - cy) + cx
    sy = sa * (xx - cx) + ca * (yy - cy) + cy
    sy = sy - rng.uniform(-0.12, 0.12) * h * (nx * nx - 0.5)       # parabolic baseline bend (~6px max)
    # Perspective foreshortening: text taller on one side, shorter on the other (viewed at
    # an angle). Vertical scale varies linearly across the width.
    sy = cy + (sy - cy) / (1.0 + rng.uniform(-0.2, 0.2) * nx)
    return sx, sy


def apply_warp(arr: np.ndarray, sx: np.ndarray, sy: np.ndarray, border: float = 0.0) -> np.ndarray:
    return cv2.remap(arr, sx, sy, interpolation=cv2.INTER_LINEAR,
                     borderMode=cv2.BORDER_CONSTANT, borderValue=border)


def degrade(img: np.ndarray, rng: random.Random, native_h: int, log: dict | None = None,
            photometric_aux: list[np.ndarray] | None = None) -> np.ndarray:
    """Camera/screen degradations applied to the composited image only.

    Blur scales with the native text height: a 1.8 px gaussian erases 12 px text
    outright but is realistic camera softness on 40 px text. Det/rec gate what
    reaches the ink model, so training must not contain text they would reject.

    `photometric_aux`: HxWx3 label images (e.g. the ink/background colour fields)
    updated *in place* (list slots reassigned) with only the colour-changing ops —
    shade, hard shadow, contrast squeeze. Illumination genuinely changes the colour
    the labels should carry, while blur/JPEG/noise are observation noise the model
    must see through, so those leave the labels untouched. The photometric ops are
    per-pixel affine, so applying them to the fields independently keeps the
    compositing identity `img ≈ cov·F + (1−cov)·B` exact up to observation noise.
    """
    pil = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8))
    if rng.random() < 0.5:
        sigma = rng.uniform(0.3, min(0.35 + native_h * 0.032, 2.0))
        pil = pil.filter(ImageFilter.GaussianBlur(sigma))
        if log is not None:
            log["blur"] = round(sigma, 2)
    if native_h >= 24 and rng.random() < 0.25:
        # Crude motion blur: directional box kernel. Skipped on tiny text, where even a
        # 3px smear destroys the glyph.
        k = rng.choice([3, 5])
        if log is not None:
            log["motion"] = k
        kernel = [0.0] * (k * k)
        if rng.random() < 0.5:
            for i in range(k):
                kernel[(k // 2) * k + i] = 1.0 / k
        else:
            for i in range(k):
                kernel[i * k + k // 2] = 1.0 / k
        pil = pil.filter(ImageFilter.Kernel((k, k), kernel, scale=1.0))
    if rng.random() < 0.35:
        scale = rng.uniform(0.55, 0.85)
        small = pil.resize(
            (max(8, int(pil.width * scale)), max(8, int(pil.height * scale))), Image.BILINEAR
        )
        pil = small.resize((pil.width, pil.height), Image.BILINEAR)
        if log is not None:
            log["downsample"] = round(scale, 2)
    if rng.random() < 0.7:
        q = rng.randint(45, 95)
        buf = io.BytesIO()
        pil.save(buf, format="JPEG", quality=q)
        buf.seek(0)
        pil = Image.open(buf).convert("RGB")
        if log is not None:
            log["jpeg"] = q
    out = np.asarray(pil, dtype=np.float32) / 255.0
    if rng.random() < 0.5:
        h, w = out.shape[:2]
        yy, xx = coord_grid(h, w)
        angle = rng.uniform(0, 2 * np.pi)
        t = (np.cos(angle) * xx / w) + (np.sin(angle) * yy / h)
        t = (t - t.min()) / max(t.max() - t.min(), 1e-6)
        shade = np.clip(rng.uniform(0.55, 1.0) + t * rng.uniform(0.0, 0.45), 0.4, 1.2)
        out = out * shade[..., None]
        if photometric_aux is not None:
            for i, aux in enumerate(photometric_aux):
                photometric_aux[i] = aux * shade[..., None]
        if log is not None:
            log["shade"] = 1
    if rng.random() < 0.25:
        # Hard-edged cast shadow: a sharp brightness step across the strip. A strong
        # illumination edge looks like a stroke to the model unless it has trained on
        # shadows that aren't ink (the label is untouched).
        h, w = out.shape[:2]
        yy, xx = coord_grid(h, w)
        angle = rng.uniform(0, 2 * np.pi)
        proj = (np.cos(angle) * xx / w) + (np.sin(angle) * yy / h)
        edge = rng.uniform(proj.min(), proj.max())
        shadow = np.where(proj < edge, rng.uniform(0.4, 0.8), 1.0).astype(np.float32)
        out = out * shadow[..., None]
        if photometric_aux is not None:
            for i, aux in enumerate(photometric_aux):
                photometric_aux[i] = aux * shadow[..., None]
        if log is not None:
            log["hardshadow"] = 1
    if rng.random() < 0.6:
        nsig = rng.uniform(0.005, 0.04)
        out = out + np.random.default_rng(rng.getrandbits(32)).normal(
            0, nsig, out.shape
        ).astype(np.float32)
        if log is not None:
            log["noise"] = round(nsig, 3)
    if rng.random() < 0.3:
        lo, hi = rng.uniform(0.0, 0.08), rng.uniform(0.85, 1.0)
        out = out * (hi - lo) + lo
        if photometric_aux is not None:
            for i, aux in enumerate(photometric_aux):
                photometric_aux[i] = aux * (hi - lo) + lo
        if log is not None:
            log["squeeze"] = round(hi - lo, 2)
    if photometric_aux is not None:
        # Shade's 1.2x ceiling can push a bright field past 1; the labels must stay in
        # the sigmoid's range.
        for i, aux in enumerate(photometric_aux):
            photometric_aux[i] = np.clip(aux, 0, 1).astype(np.float32, copy=False)
    return np.clip(out, 0, 1)


def legible(img: np.ndarray, cov: np.ndarray, native_h: int) -> bool:
    """Reject pairs whose degraded ink no longer contrasts with its background.

    Det/rec sit upstream of the ink model at inference, so unreadable text never
    reaches it — training on it would teach the model to hallucinate ink. Compare
    median ink color against median background color within the text's own rows
    (global background medians lie on gradient strips).
    """
    ink_mask = cov > 0.6
    if ink_mask.sum() < 30:
        return False
    text_rows = ink_mask.any(axis=1)
    bg_mask = (cov < 0.05) & text_rows[:, None]
    if bg_mask.sum() >= 30:
        bg_px = img[bg_mask]
    else:
        # Dense display text (~50% ink): clean cov<0.05 background is scarce because the
        # tight inter-stroke gaps are antialiased. Sample the lowest-coverage pixels in the
        # text band (the gaps/counters) as background instead, so high-coverage strips aren't
        # wrongly rejected (and retried). Genuinely all-ink (no gaps) still fails.
        band_cov = np.where(text_rows[:, None], cov, 2.0).ravel()
        k = min(400, max(30, int(text_rows.sum()) * cov.shape[1] // 5))
        k = min(k, band_cov.size - 1)
        if k < 30:
            return False
        idx = np.argpartition(band_cov, k)[:k]
        if band_cov[idx].max() >= 0.6:
            return False
        bg_px = img.reshape(-1, img.shape[-1])[idx]
    # Subsample before the median: a contrast gate doesn't need the exact median over
    # tens of thousands of pixels, and np.median's partition over the full native-res
    # ink/bg arrays was ~6% of total gen time (worse on the big signage strips).
    ink_px = img[ink_mask]
    if len(ink_px) > 400:
        ink_px = ink_px[:: len(ink_px) // 400]
    if len(bg_px) > 400:
        bg_px = bg_px[:: len(bg_px) // 400]
    d = np.abs(np.median(ink_px, axis=0) - np.median(bg_px, axis=0))
    # Small text needs more contrast: its fine inter-stroke gaps vanish at low contrast,
    # so the floor rises as native height shrinks (nh14 ~0.23, nh30+ flat at 0.13).
    thresh = 0.13 + max(0, 30 - native_h) * 0.006
    return float(d.max()) > thresh
