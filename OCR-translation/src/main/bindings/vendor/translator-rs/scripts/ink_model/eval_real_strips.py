"""Score an ink checkpoint against the curated real-strip GT.

GT masks are the pure-red (255,0,0) pixels in:
  /tmp/eval/review/      verified keeper-correct strips (correctness/regression set)
  /tmp/eval/gt_classical/ classical-segmented masks for the deleted display failures
The clean input strip is /tmp/eval/clean/<name>. For each strip we run the model, threshold
the matte at INK_CUT, and compute precision/recall/marble-grab/under-mark, bucketed by GT ink
coverage (thin/normal/heavy) and split by set (verified vs failure-display).

  python eval_real_strips.py ink-tv-8k.pt
"""

import glob
import os
import sys

import numpy as np
import torch
from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from model import InkUNet  # noqa: E402

INK_CUT = 40
CLEAN = "/tmp/eval/clean"


def gt_sources():
    """name -> (gt_red_path, set_label). review/ wins if a strip is in both."""
    out = {}
    for f in glob.glob("/tmp/eval/gt_classical/*.png"):
        out[os.path.basename(f)] = (f, "failure")
    for f in glob.glob("/tmp/eval/review/*.png"):
        out[os.path.basename(f)] = (f, "verified")
    return out


def red_mask(path):
    a = np.asarray(Image.open(path).convert("RGB"))
    return (a[..., 0] == 255) & (a[..., 1] == 0) & (a[..., 2] == 0)


def model_mask(net, strip):
    H = 48
    w = max(16, round(strip.width * H / strip.height))
    s = strip.resize((w, H), Image.BILINEAR)
    pad = (-w) % 16
    arr = np.asarray(s.convert("RGB"), np.float32) / 255.0
    if pad:
        arr = np.pad(arr, ((0, 0), (0, pad), (0, 0)), mode="edge")
    x = torch.from_numpy(arr.transpose(2, 0, 1))[None]
    with torch.no_grad():
        m = torch.sigmoid(net(x))[0, 0].numpy()[:, :w]
    big = np.asarray(Image.fromarray((m * 255).astype(np.uint8)).resize(strip.size, Image.BILINEAR))
    return big >= INK_CUT


def bucket(cov):
    return "thin" if cov < 0.12 else "heavy" if cov > 0.30 else "normal"


def main():
    ckpt = sys.argv[1] if len(sys.argv) > 1 else "ink-tv-8k.pt"
    c = torch.load(ckpt, map_location="cpu")
    net = InkUNet(base=c["base"], levels=c["levels"], bold_from=c.get("bold_from", 1),
                  bold_head=c.get("bold_head", "dilated"), rule=c.get("rule", False),
                  rule_head=c.get("rule_head", "dilated"), color=c.get("color", False))
    net.load_state_dict(c["model"])
    net.eval()

    rows = []  # (set, bucket, tp, fp, fn, n_ink, n_bg)
    for name, (gt_path, setlbl) in gt_sources().items():
        clean = f"{CLEAN}/{name}"
        if not os.path.exists(clean):
            continue
        strip = Image.open(clean).convert("RGB")
        gt = red_mask(gt_path)
        if gt.shape != strip.size[::-1]:
            gt = np.asarray(Image.fromarray(gt.astype(np.uint8) * 255).resize(strip.size)) > 127
        pred = model_mask(net, strip)
        tp = int((pred & gt).sum()); fp = int((pred & ~gt).sum()); fn = int((~pred & gt).sum())
        rows.append((setlbl, bucket(gt.mean()), tp, fp, fn, int(gt.sum()), int((~gt).sum())))

    print(f"== {ckpt} ==  ({len(rows)} strips)")
    print(f"{'set':9} {'bucket':7} {'n':>4} {'prec':>6} {'recall':>7} {'marble-grab':>12} {'undermark':>10}")
    for setlbl in ("verified", "failure"):
        for bk in ("thin", "normal", "heavy"):
            sub = [r for r in rows if r[0] == setlbl and r[1] == bk]
            if not sub:
                continue
            tp = sum(r[2] for r in sub); fp = sum(r[3] for r in sub); fn = sum(r[4] for r in sub)
            bg = sum(r[6] for r in sub)
            prec = tp / max(tp + fp, 1); rec = tp / max(tp + fn, 1)
            grab = fp / max(bg, 1); under = fn / max(tp + fn, 1)
            print(f"{setlbl:9} {bk:7} {len(sub):>4} {prec:>6.2f} {rec:>7.2f} "
                  f"{grab:>11.1%} {under:>9.1%}")


if __name__ == "__main__":
    main()
