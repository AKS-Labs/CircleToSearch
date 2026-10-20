"""Localize the uniform-line bold error: per-line accuracy broken down by single
script and by native box height. Tells us if the residual error is one script
(data fix) or uniform across scripts (capacity/training).

  python3 eval_bold_diag.py ckpt/ink-latest.pt
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
SCRIPTS = ["latin", "cjk", "arabic", "devanagari", "tamil", "thai"]
SIZE_BUCKETS = [(24, 32), (32, 40), (40, 48), (48, 9999)]


def single_plan(script: str, target: bool):
    return lambda r: [(script, target)]


def run(score_fn, key_fn):
    rng = random.Random(7)
    agg = {}
    for _ in range(3000):
        script = rng.choice(SCRIPTS)
        target = rng.random() < 0.5
        g._run_plan = single_plan(script, target)
        for _try in range(4):
            cov, fill, bold, rule, nh, nw = g._render_once(rng, 320)
            img, c, b, _r, ok = g._composite_once(rng, cov, fill, bold, rule, nh, nw, 320)
            if ok:
                break
        ink = c > 0.5
        if ink.sum() < 40:
            continue
        x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
        with torch.no_grad():
            pred = torch.sigmoid(model(x))[0, 1].numpy()
        # GT from the measured stroke target (see eval_bold_fpfn), not the is_bold flag.
        gt = int(b[ink].mean() >= g.BOLD_GT_THRESHOLD)
        ok_call = int(float(pred[ink].mean()) >= 0.5) == gt
        k = key_fn(script, nh)
        agg.setdefault(k, [0, 0])
        agg[k][0] += ok_call
        agg[k][1] += 1
    return agg


by_script = run(None, lambda s, nh: s)
print("script        per-line acc   (n)")
for s in SCRIPTS:
    if s in by_script:
        c, n = by_script[s]
        print(f"{s:12}  {c / n:6.1%}        ({n})")

by_size = run(None, lambda s, nh: next((lo, hi) for lo, hi in SIZE_BUCKETS if lo <= nh < hi))
print("\nnative_h      per-line acc   (n)")
for lo, hi in SIZE_BUCKETS:
    if (lo, hi) in by_size:
        c, n = by_size[(lo, hi)]
        print(f"{lo:>3}-{hi if hi < 9999 else '∞':<4}    {c / n:6.1%}        ({n})")
