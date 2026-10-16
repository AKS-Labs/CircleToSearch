"""Confidence-vs-coverage for the per-word bold call (uniform lines).

For each reject band [lo, hi]: coverage (fraction of words confidently decided,
score<lo or score>hi), accuracy on those decided words, and — if the abstain band
defaults to regular (don't-bold) — the net false-positive / false-negative rates.
Lets us pick a gate that drives FP toward zero at a known coverage cost.

  python3 eval_bold_gate.py ckpt/ink-latest.pt
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
        out = []
        for _ in range(r.randint(1, 4)):
            rr = r.random()
            s = "cjk" if rr < 0.18 else (g.SHAPED_NAMES[r.randrange(len(g.SHAPED_NAMES))] if rr < 0.36 else "latin")
            out.append((s, target))
        return out
    return plan


rng = random.Random(7)
scores, labels = [], []
for _ in range(1600):
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
    # GT from the measured stroke target (see eval_bold_fpfn), not the is_bold flag.
    labels.append(int(bold[ink].mean() >= g.BOLD_GT_THRESHOLD))

scores, labels = np.array(scores), np.array(labels)
nreg, nbold = (labels == 0).sum(), (labels == 1).sum()
print(f"uniform-line words: {len(scores)}  (regular {nreg} / bold {nbold})\n")
print("reject band     coverage  acc(decided)   net FP   net FN   (abstain->regular)")
for lo, hi in [(0.5, 0.5), (0.4, 0.6), (0.35, 0.65), (0.3, 0.7), (0.25, 0.75), (0.2, 0.8), (0.15, 0.85), (0.1, 0.9)]:
    decided = (scores < lo) | (scores > hi)
    cov = decided.mean()
    acc = ((scores[decided] > hi).astype(int) == labels[decided]).mean() if decided.any() else float("nan")
    pred_all = (scores > hi).astype(int)  # abstain band + below-lo both render regular
    fp = ((pred_all == 1) & (labels == 0)).sum() / max(nreg, 1)
    fn = ((pred_all == 0) & (labels == 1)).sum() / max(nbold, 1)
    print(f"[{lo:.2f},{hi:.2f}]      {cov:6.1%}    {acc:6.1%}      {fp:6.1%}   {fn:6.1%}")
