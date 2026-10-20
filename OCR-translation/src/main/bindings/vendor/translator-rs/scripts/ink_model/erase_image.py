"""Full-image erase demo: matte every detected box, inpaint, paste back.

Boxes come from viz_pipeline's box-heights CSV (cx,cy,width,tight_h,inflated_h,...).
Each box is cropped axis-aligned with margin, matted at height 48, and erased with a
dilated+hardened matte. The background field is a masked block-median grid, sampled
only where a *wider* dilation of the matte is clear — half-ink fringe pixels
contaminating the medians are what caused the smudge in the naive version.

python erase_image.py --image colors.jpg --csv box_heights.csv \
    --ckpt ckpt/ink-latest.pt --out erased.png [--dilate 5]
"""

import argparse
import csv

import numpy as np
import torch
from PIL import Image
from scipy.ndimage import distance_transform_edt, grey_dilation

from model import InkUNet

MATTE_HEIGHT = 48


def matte_for_crop(model: InkUNet, crop: np.ndarray) -> np.ndarray:
    h, w = crop.shape[:2]
    scale = MATTE_HEIGHT / h
    sw = max(16, round(w * scale))
    sw += (-sw) % 8
    small = np.asarray(
        Image.fromarray((crop * 255).astype(np.uint8)).resize((sw, MATTE_HEIGHT), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0
    with torch.no_grad():
        x = torch.from_numpy(np.ascontiguousarray(small.transpose(2, 0, 1)))[None]
        m = torch.sigmoid(model(x))[0, 0].numpy()
    return np.asarray(
        Image.fromarray(m, mode="F").resize((w, h), Image.BILINEAR), dtype=np.float32
    )


def background_field(crop: np.ndarray, exclude: np.ndarray, block: int = 10) -> np.ndarray:
    h, w = exclude.shape
    gh, gw = (h + block - 1) // block, (w + block - 1) // block
    grid = np.zeros((gh, gw, 3), dtype=np.float32)
    ok = np.zeros((gh, gw), dtype=bool)
    for gy in range(gh):
        for gx in range(gw):
            ys, xs = gy * block, gx * block
            tile = crop[ys : ys + block, xs : xs + block]
            m = exclude[ys : ys + block, xs : xs + block] < 0.15
            if m.sum() >= 4:
                grid[gy, gx] = np.median(tile[m], axis=0)
                ok[gy, gx] = True
    if not ok.any():
        grid[:] = np.median(crop.reshape(-1, 3), axis=0)
    elif not ok.all():
        _, (iy, ix) = distance_transform_edt(~ok, return_indices=True)
        grid = grid[iy, ix]
    return np.asarray(
        Image.fromarray((np.clip(grid, 0, 1) * 255).astype(np.uint8)).resize(
            (w, h), Image.BILINEAR
        ),
        dtype=np.float32,
    ) / 255.0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--image", required=True)
    ap.add_argument("--csv", required=True)
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--out", required=True)
    # JPEG ringing extends ~half a coding block (4-6 px) past glyph edges at *image*
    # scale regardless of text size, and the matte correctly excludes it (it isn't
    # ink); the erase margin has to cover it instead.
    ap.add_argument("--dilate", type=int, default=9)
    args = ap.parse_args()

    model = InkUNet()
    state = torch.load(args.ckpt, map_location="cpu")
    model.load_state_dict(state["model"])
    model.eval()

    img = np.asarray(Image.open(args.image).convert("RGB"), dtype=np.float32) / 255.0
    ih, iw = img.shape[:2]
    boxes = list(csv.DictReader(open(args.csv)))
    for b in boxes:
        cx, cy = float(b["cx"]), float(b["cy"])
        w, infl = float(b["width"]), float(b["inflated_h"])
        pad = max(6.0, infl * 0.25)
        x0 = int(max(0, cx - w / 2 - pad))
        x1 = int(min(iw, cx + w / 2 + pad))
        y0 = int(max(0, cy - infl / 2 - pad))
        y1 = int(min(ih, cy + infl / 2 + pad))
        if x1 - x0 < 12 or y1 - y0 < 8:
            continue
        crop = img[y0:y1, x0:x1]
        matte = matte_for_crop(model, crop)
        k = args.dilate
        composite_a = np.clip(grey_dilation(matte, size=(k, k)) * 1.8, 0, 1)
        sample_excl = grey_dilation(matte, size=(k + 4, k + 4))
        bg = background_field(crop, sample_excl)
        img[y0:y1, x0:x1] = composite_a[..., None] * bg + (1 - composite_a[..., None]) * crop

    Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).save(args.out)
    print(f"erased {len(boxes)} boxes -> {args.out}")


if __name__ == "__main__":
    main()
