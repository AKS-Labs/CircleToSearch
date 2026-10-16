"""Production-relevant bold metric: one weight decision per *uniform* line.

eval_bold_sep is adversarial (bold + regular in one strip). At runtime we pool the
bold prob over a word/line's ink (CTC char spans) and a line is usually a single
weight — a heading is all-bold, body all-regular. This measures that: render
uniform-weight strips, pool predicted bold over all ink, one call per line.

  python3 eval_bold_line.py ckpt/ink-latest.pt
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


# A uniform line never splits weight mid-string, so CJK emphasis is disabled here.
g._cjk_units = lambda r, txt, ib: [(txt, ib)]

rng = random.Random(7)
scores, labels = [], []
for _ in range(1000):
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
print(f"uniform-line strips: {len(scores)}  (bold fraction {labels.mean():.2f})")
print(f"pooled bold mean    : bold {scores[labels == 1].mean():.3f}  regular {scores[labels == 0].mean():.3f}")
print(f"per-line acc @0.5   : {((scores >= 0.5).astype(int) == labels).mean():.1%}")
best = max(((labels == (scores >= t)).mean(), t) for t in np.linspace(0, 1, 101))
print(f"best-threshold acc  : {best[0]:.1%} at t={best[1]:.2f}")
