"""Per-region bold separation, broken down by native text size.

Tells us whether the accuracy ceiling is uniform (capacity/labels) or driven by
small text (weight is genuinely ambiguous on tiny glyphs after squash-to-48).

  python3 eval_bold_size.py ckpt/ink-latest.pt
"""

import random
import sys

import numpy as np
import torch

from gen_data import _composite_once, _render_once, BOLD_GT_THRESHOLD
from model import InkUNet

ck = torch.load(sys.argv[1] if len(sys.argv) > 1 else "ckpt/ink-latest.pt", map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1), bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()


def sample_nh(rng, width):
    for _ in range(8):
        cov, fill, bold, rule, nh, nw = _render_once(rng, width)
        img, c, b, _r, ok = _composite_once(rng, cov, fill, bold, rule, nh, nw, width)
        if ok:
            return img, c, b, nh
    return img, c, b, nh


rng = random.Random(123)
buckets = [(0, 18), (18, 28), (28, 40), (40, 9999)]
agg = {b: [0, 0] for b in buckets}  # correct, total per-region calls
for _ in range(800):
    img, cov, bold, nh = sample_nh(rng, 320)
    ink = cov > 0.5
    # `bold` is the continuous stroke-width target: split ink into measured thick/thin.
    bm, rm = ink & (bold > BOLD_GT_THRESHOLD), ink & (bold <= BOLD_GT_THRESHOLD)
    if bm.sum() < 40 or rm.sum() < 40:
        continue
    x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
    with torch.no_grad():
        pred = torch.sigmoid(model(x))[0, 1].numpy()
    bs, rs = float(pred[bm].mean()), float(pred[rm].mean())
    for lo, hi in buckets:
        if lo <= nh < hi:
            agg[(lo, hi)][0] += int(bs >= 0.5) + int(rs < 0.5)
            agg[(lo, hi)][1] += 2

print("native_h   per-region acc   (n)")
for (lo, hi), (c, n) in agg.items():
    if n:
        print(f"{lo:>3}-{hi if hi < 9999 else '∞':<4}  {c / n:6.1%}        ({n // 2})")
