"""Is the false-positive (regular->bold) error sporadic on genuinely THIN text, or
concentrated on heavy regular faces (acceptable)? Force the lightest fonts (OS/2
<=300) for uniform regular lines and measure how often they're called bold.

  python3 eval_bold_thin.py ckpt/ink-latest.pt
"""

import random
import sys

import numpy as np
import torch

import gen_data as g
from model import InkUNet

ck = torch.load(sys.argv[1] if len(sys.argv) > 1 else "ckpt/ink-latest.pt", map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1), bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()

light = tuple(p for p in g.font_paths() if (w := g._font_weight(p)) is not None and w <= 300)
mid = tuple(p for p in g.font_paths() if (w := g._font_weight(p)) is not None and w == 400)
print(f"light(<=300) fonts: {len(light)}   mid(==400) fonts: {len(mid)}")

g._cjk_units = lambda r, txt, ib: [(txt, ib)]
g._run_plan = lambda r: [("latin", False)]  # uniform regular latin


def fp_rate(pool):
    g._weighted_fonts = lambda script, bold: pool
    rng = random.Random(7)
    scores = []
    for _ in range(700):
        img, cov, bold, _ = g.sample(rng, width=320)
        ink = cov > 0.5
        if ink.sum() < 40:
            continue
        x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
        with torch.no_grad():
            pred = torch.sigmoid(model(x))[0, 1].numpy()
        scores.append(float(pred[ink].mean()))
    scores = np.array(scores)
    return (scores >= 0.5).mean(), scores.mean(), np.percentile(scores, 95), len(scores)


for name, pool in [("light<=300", light), ("mid==400", mid)]:
    if not pool:
        print(f"{name}: no fonts")
        continue
    fp, mean, p95, n = fp_rate(pool)
    print(f"{name:12} FP(->bold) {fp:5.1%}   mean {mean:.3f}  p95 {p95:.2f}   (n={n})")
