"""Qualify how the ink bold head reads specific fonts: render a clean black-on-white
strip per font, run the model, and compare the pooled predicted stroke-width against the
target the data pipeline would assign. Auto-anchors with a regular/bold from the installed
pool so the probe fonts have reference points.

  python3 probe_font.py ckpt/ink-latest.pt FONT.ttf [FONT2.ttf ...]
"""

import sys

import numpy as np
import torch
from PIL import Image, ImageDraw, ImageFont

import gen_data as g
from model import InkUNet

TEXT = "Heavyweight 80"
NATIVE = 64

ckpt = sys.argv[1]
probe_fonts = sys.argv[2:]

ck = torch.load(ckpt, map_location="cpu")
model = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1),
                bold_head=ck.get("bold_head", "dilated"))
model.load_state_dict(ck["model"])
model.eval()


def strip_for(path):
    font = ImageFont.truetype(path, NATIVE)
    probe = ImageDraw.Draw(Image.new("L", (8, 8)))
    l, t, r, b = probe.textbbox((0, 0), TEXT, font=font)
    canvas = Image.new("L", (r - l + NATIVE, b - t + NATIVE), 0)
    ImageDraw.Draw(canvas).text((NATIVE // 2 - l, NATIVE // 2 - t), TEXT, font=font, fill=255)
    a = np.asarray(canvas, np.float32) / 255.0
    ys, xs = np.where(a > 0.3)
    if len(ys) == 0:
        return None
    m = 6
    a = a[max(0, ys.min() - m): ys.max() + m, max(0, xs.min() - m): xs.max() + m]
    h, w = a.shape
    w48 = max(16, int(round(w * 48 / h / 16)) * 16)
    cov = np.asarray(Image.fromarray((a * 255).astype(np.uint8)).resize((w48, 48), Image.BILINEAR),
                     np.float32) / 255.0
    rgb = np.stack([1.0 - cov] * 3, axis=-1).astype(np.float32)  # black ink on white
    return rgb, cov


def report(name, path):
    out = strip_for(path)
    if out is None:
        print(f"{name:26} (no ink rendered)")
        return
    rgb, cov = out
    x = torch.from_numpy(np.ascontiguousarray(rgb.transpose(2, 0, 1)))[None]
    with torch.no_grad():
        pred = torch.sigmoid(model(x))[0, 1].numpy()
    ink = cov > 0.5
    w = g._font_weight(path)
    ratio = g._font_stroke_ratio(path, "latin")
    tgt = g._target_q(ratio) / 255.0
    p = pred[ink]
    print(f"{name:26} OS/2w={str(w):>4}  ratio={ratio:.3f}  target={tgt:.2f}  ||  "
          f"pred mean={p.mean():.2f}  p50={np.percentile(p, 50):.2f}  p95={np.percentile(p, 95):.2f}")


def pick(wlo, whi):
    for pth in g.font_paths():
        ww = g._font_weight(pth)
        if ww is not None and wlo <= ww <= whi:
            return pth
    return None


print(f"ckpt step={ck.get('step')}  text={TEXT!r}  (target = what the pipeline would label)\n")
print("-- reference fonts from the installed pool --")
for label, (lo, hi) in (("regular(400)", (400, 400)), ("bold(700)", (700, 700)),
                        ("heavy(>=800)", (800, 999))):
    pth = pick(lo, hi)
    report(label, pth) if pth else print(f"{label:26} (none in pool)")
print("\n-- probe fonts --")
for pth in probe_fonts:
    report(pth.split("/")[-1], pth)
