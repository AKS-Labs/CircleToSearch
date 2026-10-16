"""Per-box render of what the ink model sees and produces.

For each deskewed strip (the box passed to the ink model), stack:
  1. the input strip (RGB), at the model's native 48px
  2. ink: the soft matte (ch0), grayscale
  3. ink core: matte >= stroke_core_cut(peak) (Rust: max(peak*0.6, 40)), tinted cyan

python viz_ink_core.py --strips <dir>/deskewed --ckpt ckpt/ink-base16-1x1-prod.pt --out cores.png
"""

import argparse
import glob
import os

import numpy as np
import torch
from PIL import Image

from model import InkUNet

HEIGHT = 48
STROKE_CORE_FRAC = 0.6
INK_CUT = 40


def matte_native(model: InkUNet, strip: np.ndarray) -> np.ndarray:
    h, w = strip.shape[:2]
    mult = 2 ** model.levels
    sw = max(mult, round(w * HEIGHT / h))
    sw += (-sw) % mult
    small = np.asarray(
        Image.fromarray((strip * 255).astype(np.uint8)).resize((sw, HEIGHT), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0
    with torch.no_grad():
        x = torch.from_numpy(np.ascontiguousarray(small.transpose(2, 0, 1)))[None]
        m = torch.sigmoid(model(x))[0, 0].numpy()
    return small, m  # input at native res, matte at native res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--strips", required=True)
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--scale", type=int, default=3)
    args = ap.parse_args()

    state = torch.load(args.ckpt, map_location="cpu")
    model = InkUNet(base=state.get("base", 16), levels=state.get("levels", 2),
                    bold_from=state.get("bold_from", 1), bold_head=state.get("bold_head", "dilated"),
                    rule=state.get("rule", False), rule_head=state.get("rule_head", "dilated"),
                    color=state.get("color", False))
    model.load_state_dict(state["model"])
    model.eval()

    pngs = sorted(glob.glob(os.path.join(args.strips, "box-*.png")))
    rows = []
    for p in pngs:
        strip = np.asarray(Image.open(p).convert("RGB"), dtype=np.float32) / 255.0
        inp, m = matte_native(model, strip)
        peak = int(round(m.max() * 255))
        cut = max(int(peak * STROKE_CORE_FRAC), INK_CUT) / 255.0

        rgb_in = (inp * 255).astype(np.uint8)
        gray = np.repeat((np.clip(m, 0, 1) * 255).astype(np.uint8)[..., None], 3, axis=2)
        core_mask = (m >= cut)
        core = rgb_in.copy()
        core[core_mask] = (0.25 * core[core_mask] + 0.75 * np.array([0, 255, 255])).astype(np.uint8)

        sep = np.full((2, rgb_in.shape[1], 3), 90, np.uint8)
        rows.append(np.concatenate([rgb_in, sep, gray, sep, core], axis=0))

    width = max(r.shape[1] for r in rows)
    rows = [np.pad(r, ((0, 0), (0, width - r.shape[1]), (0, 0)), constant_values=30) for r in rows]
    gap = np.full((6, width, 3), 160, np.uint8)
    stacked = rows[0]
    for r in rows[1:]:
        stacked = np.concatenate([stacked, gap, r], axis=0)

    s = args.scale
    out = Image.fromarray(stacked).resize((width * s, stacked.shape[0] * s), Image.NEAREST)
    out.save(args.out)
    print(f"{len(pngs)} boxes -> {args.out}  ({out.size[0]}x{out.size[1]})")


if __name__ == "__main__":
    main()
