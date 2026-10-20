"""Failure-mode split on uniform lines: false positives (regular called bold) vs
false negatives (real bold called regular), at t=0.5 and the best threshold.

  python3 eval_bold_fpfn.py ckpt/ink-latest.pt
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

g._cjk_units = lambda r, txt, ib: [(txt, ib)]


def uniform_plan(target: bool):
    def plan(r: random.Random):
        runs = []
        for _ in range(r.randint(1, 4)):
            rr = r.random()
            script = (
                "cjk" if rr < 0.18
                else (g.SHAPED_NAMES[r.randrange(len(g.SHAPED_NAMES))] if rr < 0.36 else "latin")
            )
            runs.append((script, target))
        return runs
    return plan


rng = random.Random(7)
scores, labels = [], []
for _ in range(1500):
    target = rng.random() < 0.5
    g._run_plan = uniform_plan(target)
    img, cov, bold, _ = g.sample(rng, width=320)
    ink = cov > 0.5
    if ink.sum() < 40:
        continue
    x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
    with torch.no_grad():
        pred = torch.sigmoid(model(x))[0, 1].numpy()
    scores.append(float(pred[ink].mean()))
    # GT is the *measured* stroke target, not the is_bold intent flag: a weight-550
    # semibold drawn from the bold pool genuinely reads ~0.4, and the head is trained to
    # output that, so judging it bold would be wrong.
    labels.append(int(bold[ink].mean() >= g.BOLD_GT_THRESHOLD))

scores, labels = np.array(scores), np.array(labels)
reg, bold = scores[labels == 0], scores[labels == 1]


def report(t: float):
    fp = (reg >= t).mean()    # regular called bold
    fn = (bold < t).mean()    # bold called regular
    print(f"  t={t:.2f}:  FP (regular->bold) {fp:.1%}   FN (bold->regular) {fn:.1%}")


print(f"strips: {len(scores)}  regular {len(reg)}  bold {len(bold)}")
report(0.50)
best_t = max(((labels == (scores >= t)).mean(), t) for t in np.linspace(0, 1, 101))[1]
report(best_t)
print(f"score spread: regular p50={np.percentile(reg,50):.2f} p95={np.percentile(reg,95):.2f}"
      f" | bold p5={np.percentile(bold,5):.2f} p50={np.percentile(bold,50):.2f}")
