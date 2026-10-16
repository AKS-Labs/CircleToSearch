"""Architecture A/B: can *geometric* bold (stroke_width / x_height, classical, no
learned channel) separate bold from regular as well as the learned bold head?

Largely historical now: the learned bold head regresses a measured stroke-width target
(gen_data `_font_stroke_ratio`), i.e. essentially this same geometric quantity, so the
A/B has collapsed. Kept as the no-learning baseline; GT here is still the is_bold intent
flag (the question is whether geometry recovers the intended weight).

This is the ceiling test — geometric bold is computed on the ground-truth coverage
(a perfect matte). If it can't separate here, it loses to the learned head; if it's
competitive, it's worth running on a real model matte (and would let us ship the
base8 matte + free geometric bold, no base16 upsize, no 2-channel export).

  python3 eval_geom_bold.py
"""

import random

import numpy as np

import gen_data as g

g._cjk_units = lambda r, txt, ib: [(txt, ib)]


def uniform_plan(target: bool):
    def plan(r: random.Random):
        out = []
        for _ in range(r.randint(1, 4)):
            rr = r.random()
            s = "cjk" if rr < 0.18 else (g.SHAPED_NAMES[r.randrange(len(g.SHAPED_NAMES))] if rr < 0.36 else "latin")
            out.append((s, target))
        return out
    return plan


def geom_ratio(cov: np.ndarray) -> float:
    """stroke_width / x_height from coverage. stroke = 2*area/perimeter (≈ width of an
    elongated stroke); x-height = 10-90% ink-mass row span."""
    ink = cov > 0.5
    area = float(ink.sum())
    if area < 30:
        return 0.0
    p = np.zeros_like(ink)
    p[1:, :] |= ink[1:, :] & ~ink[:-1, :]
    p[:-1, :] |= ink[:-1, :] & ~ink[1:, :]
    p[:, 1:] |= ink[:, 1:] & ~ink[:, :-1]
    p[:, :-1] |= ink[:, :-1] & ~ink[:, 1:]
    perim = float(p.sum()) + 1e-6
    stroke = 2.0 * area / perim
    rowmass = cov.sum(axis=1)
    cum = np.cumsum(rowmass) / max(rowmass.sum(), 1e-6)
    lo, hi = np.searchsorted(cum, 0.1), np.searchsorted(cum, 0.9)
    xh = max(hi - lo, 1)
    return stroke / xh


rng = random.Random(7)
ratios, labels = [], []
for _ in range(1200):
    target = rng.random() < 0.5
    g._run_plan = uniform_plan(target)
    img, cov, bold, _ = g.sample(rng, width=320)
    if (cov > 0.5).sum() < 40:
        continue
    ratios.append(geom_ratio(cov))
    labels.append(int(target))

ratios, labels = np.array(ratios), np.array(labels)
b, r = ratios[labels == 1], ratios[labels == 0]
print(f"uniform-line strips: {len(ratios)}")
print(f"geom ratio: bold mean {b.mean():.3f}  regular mean {r.mean():.3f}")
lo, hi = min(ratios.min(), 0), ratios.max()
best = max(((labels == (ratios >= t)).mean(), t) for t in np.linspace(lo, hi, 200))
print(f"best-threshold per-line acc: {best[0]:.1%} at ratio={best[1]:.3f}")
# thin-text false positives at the best threshold
t = best[1]
print(f"(compare learned: base16 88.9% / base24 94.0% uniform-line)")
