"""Where do the sporadic uniform-line errors live? FP (regular->bold) and FN
(bold->regular) bucketed by native box height. Isolates whether the ~5% is small
text (sub-pixel stroke gap) or the signage thicken augmentation (>=48).

  python3 eval_bold_fpfn_size.py ckpt/ink-latest.pt
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
BUCKETS = [(24, 32), (32, 40), (40, 48), (48, 9999)]


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
agg = {b: {"reg": [], "bold": []} for b in BUCKETS}
for _ in range(1500):
    target = rng.random() < 0.5
    g._run_plan = uniform_plan(target)
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
    s = float(pred[ink].mean())
    bucket = next((lo, hi) for lo, hi in BUCKETS if lo <= nh < hi)
    # GT from the measured stroke target (see eval_bold_fpfn), not the is_bold flag.
    gt_bold = b[ink].mean() >= g.BOLD_GT_THRESHOLD
    agg[bucket]["bold" if gt_bold else "reg"].append(s)

print("native_h    FP(reg->bold)  FN(bold->reg)   (n_reg/n_bold)")
for lo, hi in BUCKETS:
    reg, bold = np.array(agg[(lo, hi)]["reg"]), np.array(agg[(lo, hi)]["bold"])
    fp = (reg >= 0.5).mean() if len(reg) else float("nan")
    fn = (bold < 0.5).mean() if len(bold) else float("nan")
    print(f"{lo:>3}-{hi if hi < 9999 else '∞':<4}    {fp:6.1%}        {fn:6.1%}        ({len(reg)}/{len(bold)})")
