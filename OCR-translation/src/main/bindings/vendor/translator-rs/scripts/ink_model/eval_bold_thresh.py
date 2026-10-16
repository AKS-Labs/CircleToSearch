"""Bold threshold sweep for the asymmetric preference: never embolden THIN text
(false positive), but missing a bold (rendering it normal) is fine. For each bold
threshold t, report on uniform lines: FP (normal->bold), FN (bold->normal, benign),
and the false-positive rate split by the font's MEASURED stroke (not OS/2 weight): thin
(ratio<=0.065, genuinely hairline→light, must stay ~0) vs regular-band (0.065<ratio<=0.105,
the genuine regulars incl. the heaviest-400, bold-ish is benign). Bucketing by measured
stroke matches the training target and keeps display faces that *declare* weight 400 but
draw thick (Anton, AlfaSlabOne…) out of the regular bucket — they're genuinely bold, so the
model scoring them bold is correct, not a false positive.

  python3 eval_bold_thresh.py ckpt/ink-latest.pt
"""

import random
import sys

import numpy as np
import torch

import gen_data as g
from model import InkUNet

ck = torch.load(sys.argv[1] if len(sys.argv) > 1 else "ckpt/ink-latest.pt", map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1),
                bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()
g._cjk_units = lambda r, txt, ib: [(txt, ib)]

# Training widened the pools to ≤550/≥550 (the continuum is the bold *target* now), but a
# categorical FP/FN threshold sweep wants cleanly-separated references, so force the classic
# ≤400 regular / ≥700 bold split here. Per-script (operates on each script's font paths).
g._weight_pool = lambda paths, bold: tuple(
    p for p in paths if (w := g._font_weight(p)) is not None and (w >= 700 if bold else w <= 400)
) or paths


def pooled_scores(plan_fn, n, pool=None):
    if pool is not None:
        g._weighted_fonts = lambda script, bold: pool
    rng = random.Random(7)
    out = []
    for _ in range(n):
        g._run_plan = plan_fn(rng)
        img, cov, bold, _ = g.sample(rng, width=320)
        ink = cov > 0.5
        if ink.sum() < 40:
            continue
        x = torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1)))[None]
        with torch.no_grad():
            pred = torch.sigmoid(model(x))[0, 1].numpy()
        out.append(float(pred[ink].mean()))
    return np.array(out)


def uniform(target):
    def make(_rng):
        def plan(r):
            o = []
            for _ in range(r.randint(1, 4)):
                rr = r.random()
                s = "cjk" if rr < 0.18 else (g.SHAPED_NAMES[r.randrange(len(g.SHAPED_NAMES))] if rr < 0.36 else "latin")
                o.append((s, target))
            return o
        return plan
    return make


# Buckets keyed on measured stroke (latin pool is drawn latin-only below), not OS/2 weight.
_ratio = lambda p: g._font_stroke_ratio(p, "latin")  # noqa: E731
thin = tuple(p for p in g.font_paths() if _ratio(p) <= 0.065)
regband = tuple(p for p in g.font_paths() if 0.065 < _ratio(p) <= 0.105)

reg = pooled_scores(uniform(False), 1000)
bold = pooled_scores(uniform(True), 1000)
g._run_plan = lambda r: [("latin", False)]
thin_s = pooled_scores(lambda _r: g._run_plan, 600, pool=thin)
regband_s = pooled_scores(lambda _r: g._run_plan, 600, pool=regband)

print(f"head={ck.get('bold_head','dilated')} base={ck['base']}  (reg {len(reg)} / bold {len(bold)} "
      f"/ thin {len(thin)}f={len(thin_s)} / reg-band {len(regband)}f={len(regband_s)})\n")
print("bold_thr   FP(norm→bold)  FN(bold→norm)   thin FP(meas)   reg-band FP(meas)")
for t in (0.50, 0.60, 0.70, 0.80):
    fp = (reg >= t).mean()
    fn = (bold < t).mean()
    tfp = (thin_s >= t).mean()
    rfp = (regband_s >= t).mean()
    print(f"  {t:.2f}      {fp:6.1%}        {fn:6.1%}         {tfp:6.1%}          {rfp:6.1%}")
