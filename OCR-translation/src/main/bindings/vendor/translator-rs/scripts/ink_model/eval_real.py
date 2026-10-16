"""Run a checkpoint on real strips and write inspection sheets.

Input strips: any PNGs (e.g. from `viz_pipeline --stages deskewed`). Each strip is
resized to height 48 (aspect preserved) and width-padded to a multiple of 8.

Output per strip: original | matte | overlay | reconstructed background.
The background reconstruction is the same masked block-median + bilinear idea as
src/color_matting.rs, but fed by the predicted matte — it previews erase quality.

python eval_real.py --ckpt ckpt/ink-latest.pt --strips <dir> --out <dir>
"""

import argparse
import os

import numpy as np
import torch
from PIL import Image

from model import InkUNet

HEIGHT = 48


def load_strip(path: str) -> np.ndarray:
    img = Image.open(path).convert("RGB")
    w = max(16, round(img.width * HEIGHT / img.height))
    img = img.resize((w, HEIGHT), Image.BILINEAR)
    pad = (-w) % 8
    arr = np.asarray(img, dtype=np.float32) / 255.0
    if pad:
        arr = np.pad(arr, ((0, 0), (0, pad), (0, 0)), mode="edge")
    return arr


def reconstruct_background(img: np.ndarray, matte: np.ndarray, block: int = 12) -> np.ndarray:
    """Masked block-median grid + bilinear upsample (colors.jpg gradient-safe)."""
    h, w = matte.shape
    gh, gw = (h + block - 1) // block, (w + block - 1) // block
    grid = np.zeros((gh, gw, 3), dtype=np.float32)
    ok = np.zeros((gh, gw), dtype=bool)
    for gy in range(gh):
        for gx in range(gw):
            ys, xs = gy * block, gx * block
            tile = img[ys : ys + block, xs : xs + block]
            m = matte[ys : ys + block, xs : xs + block] < 0.25
            if m.sum() >= 4:
                grid[gy, gx] = np.median(tile[m], axis=0)
                ok[gy, gx] = True
    # Fill ink-only tiles from nearest valid neighbors (grid scale, cheap).
    if ok.any() and not ok.all():
        from scipy.ndimage import distance_transform_edt  # noqa: PLC0415

        _, (iy, ix) = distance_transform_edt(~ok, return_indices=True)
        grid = grid[iy, ix]
    bg = np.asarray(
        Image.fromarray((np.clip(grid, 0, 1) * 255).astype(np.uint8)).resize(
            (w, h), Image.BILINEAR
        ),
        dtype=np.float32,
    ) / 255.0
    return bg


def ink_color(img: np.ndarray, matte: np.ndarray, bg: np.ndarray) -> np.ndarray:
    core = matte > 0.85
    if core.sum() < 10:
        core = matte > 0.6
    if core.sum() < 10:
        return np.array([0.0, 0.0, 0.0], dtype=np.float32)
    a = matte[core][:, None]
    est = (img[core] - (1 - a) * bg[core]) / np.clip(a, 0.5, 1.0)
    return np.clip(np.median(est, axis=0), 0, 1)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--strips", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    state = torch.load(args.ckpt, map_location="cpu")
    model = InkUNet(base=state.get("base", 16), levels=state.get("levels", 2))
    model.load_state_dict(state["model"])
    model.eval()
    print(f"loaded {args.ckpt} (step {state.get('step', '?')})")

    os.makedirs(args.out, exist_ok=True)
    names = sorted(
        f for f in os.listdir(args.strips) if f.lower().endswith((".png", ".jpg", ".jpeg"))
    )
    for name in names:
        arr = load_strip(os.path.join(args.strips, name))
        with torch.no_grad():
            x = torch.from_numpy(np.ascontiguousarray(arr.transpose(2, 0, 1)))[None]
            matte = torch.sigmoid(model(x))[0, 0].numpy()
        bg = reconstruct_background(arr, matte)
        color = ink_color(arr, matte, bg)
        overlay = arr * 0.4 + np.stack([matte, matte * 0.2, matte * 0.8], axis=-1) * 0.6
        erased = matte[..., None] * bg + (1 - matte[..., None]) * arr
        rows = [arr, np.repeat(matte[..., None], 3, axis=-1), overlay, erased]
        gap = np.ones((3, arr.shape[1], 3), dtype=np.float32)
        sheet = np.concatenate(sum(([r, gap] for r in rows), [])[:-1], axis=0)
        sheet[:2, :12] = color  # swatch of the recovered ink color
        Image.fromarray((np.clip(sheet, 0, 1) * 255).astype(np.uint8)).save(
            os.path.join(args.out, name)
        )
    print(f"wrote {len(names)} sheets to {args.out}")


if __name__ == "__main__":
    main()
