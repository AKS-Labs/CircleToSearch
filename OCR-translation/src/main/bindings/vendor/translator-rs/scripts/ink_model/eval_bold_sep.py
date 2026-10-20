"""Does the bold channel *separate* bold from regular when pooled per region?

Per-pixel crispness is irrelevant — downstream we pool the bold prob over a word's
ink and threshold. So on mixed strips (both bold and regular ink present), compare
the mean predicted bold over bold-labelled ink vs regular-labelled ink.

  python3 eval_bold_sep.py ckpt/ink-latest.pt
"""

import random
import sys

import numpy as np
import torch

from gen_data import BOLD_GT_THRESHOLD, sample
from model import InkUNet

ck = torch.load(sys.argv[1] if len(sys.argv) > 1 else "ckpt/ink-latest.pt", map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1), bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()

rng = random.Random(99)
bold_scores, reg_scores, correct, n, rank_ok = [], [], 0, 0, 0
for _ in range(400):
    img, cov, bold, _ = sample(rng, width=320)
    ink = cov > 0.5
    # `bold` is the continuous stroke-width target now, so this splits ink into
    # measured-thick vs measured-thin pixels within the strip.
    bmask = ink & (bold > BOLD_GT_THRESHOLD)
    rmask = ink & (bold <= BOLD_GT_THRESHOLD)
    if bmask.sum() < 40 or rmask.sum() < 40:
        continue  # need both regions present to measure separation
    x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
    with torch.no_grad():
        pred = torch.sigmoid(model(x))[0, 1].numpy()
    bs, rs = float(pred[bmask].mean()), float(pred[rmask].mean())
    bold_scores.append(bs)
    reg_scores.append(rs)
    # per-region call at 0.5 → both correct?
    correct += int(bs >= 0.5) + int(rs < 0.5)
    n += 2
    # relative (same-font, within-line): does the bold run outscore the regular run?
    rank_ok += int(bs > rs)

bold_scores, reg_scores = np.array(bold_scores), np.array(reg_scores)
print(f"mixed strips: {len(bold_scores)}")
print(f"pooled bold-region score : mean {bold_scores.mean():.3f}  (want high)")
print(f"pooled regular-region    : mean {reg_scores.mean():.3f}  (want low)")
print(f"separation (bold-reg)    : {bold_scores.mean() - reg_scores.mean():+.3f}")
print(f"per-region acc @0.5      : {correct / max(n,1):.1%}")
print(f"pairwise ranking acc     : {rank_ok / max(len(bold_scores),1):.1%}  (same-font: bold run > regular run)")
# best single threshold separating the two pooled distributions
allv = np.concatenate([bold_scores, reg_scores])
lbl = np.concatenate([np.ones_like(bold_scores), np.zeros_like(reg_scores)])
best = max(((lbl == (allv >= t)).mean(), t) for t in np.linspace(0, 1, 101))
print(f"best-threshold acc       : {best[0]:.1%} at t={best[1]:.2f}")
