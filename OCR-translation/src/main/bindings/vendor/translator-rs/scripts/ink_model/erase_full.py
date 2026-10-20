"""Full-image ink erase using viz_pipeline's deskewed strips + coordmaps.

Each box-NNN.png is the dewarped strip the ink model sees; box-NNN.map is its
per-pixel source coordinate (written by viz_pipeline's deskewed stage). We matte
the strip, reconstruct its background, then splat the erase back onto the original
image through the coordmap, so rotated/curved text lands where it actually sits.

python erase_full.py --image <orig> --strips <dir>/deskewed --ckpt ckpt/ink-latest.pt \
    --out erased.png [--dilate 7]
"""

import argparse
import glob
import os
import struct

import numpy as np
import torch
from PIL import Image, ImageDraw
from scipy.ndimage import distance_transform_edt, grey_dilation

from model import InkUNet

HEIGHT = 48


def load_coordmap(path: str) -> np.ndarray:
    with open(path, "rb") as f:
        w, h = struct.unpack("<II", f.read(8))
        data = np.frombuffer(f.read(), dtype="<f4").reshape(h, w, 2)
    return data  # [..., 0]=src_x, [..., 1]=src_y


def model_strip(model: InkUNet, strip: np.ndarray) -> np.ndarray:
    """Sigmoid channel stack at the strip's own (h, w): [..., 0]=matte, [..., 2]=rule
    (when the model has the rule head). Channels resized back from the 48px model input."""
    h, w = strip.shape[:2]
    mult = 2 ** model.levels  # H/W must divide by 2**levels for the U-Net pooling
    sw = max(mult, round(w * HEIGHT / h))
    sw += (-sw) % mult
    small = np.asarray(
        Image.fromarray((strip * 255).astype(np.uint8)).resize((sw, HEIGHT), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0
    with torch.no_grad():
        x = torch.from_numpy(np.ascontiguousarray(small.transpose(2, 0, 1)))[None]
        y = torch.sigmoid(model(x))[0].numpy()  # (C, HEIGHT, sw)
    return np.stack(
        [np.asarray(Image.fromarray(y[c], mode="F").resize((w, h), Image.BILINEAR), dtype=np.float32)
         for c in range(y.shape[0])],
        axis=-1,
    )


def erase_ink(chans: np.ndarray) -> np.ndarray:
    """Pixels to erase = matte ∪ rule. ch2 (under/strike/over rule) is a thin structure the
    matte under-covers, so the dedicated channel is what makes rules erase reliably."""
    matte = chans[..., 0]
    return matte if chans.shape[-1] < 3 else np.maximum(matte, chans[..., 2])


def background_field(strip: np.ndarray, exclude: np.ndarray, block: int = 10) -> np.ndarray:
    h, w = exclude.shape
    gh, gw = (h + block - 1) // block, (w + block - 1) // block
    grid = np.zeros((gh, gw, 3), dtype=np.float32)
    ok = np.zeros((gh, gw), dtype=bool)
    for gy in range(gh):
        for gx in range(gw):
            ys, xs = gy * block, gx * block
            tile = strip[ys : ys + block, xs : xs + block]
            m = exclude[ys : ys + block, xs : xs + block] < 0.15
            if m.sum() >= 4:
                grid[gy, gx] = np.median(tile[m], axis=0)
                ok[gy, gx] = True
    if not ok.any():
        grid[:] = np.median(strip.reshape(-1, 3), axis=0)
    elif not ok.all():
        _, (iy, ix) = distance_transform_edt(~ok, return_indices=True)
        grid = grid[iy, ix]
    return np.asarray(
        Image.fromarray((np.clip(grid, 0, 1) * 255).astype(np.uint8)).resize((w, h), Image.BILINEAR),
        dtype=np.float32,
    ) / 255.0


def load_ink_model(ckpt: str) -> InkUNet:
    state = torch.load(ckpt, map_location="cpu")
    model = InkUNet(base=state.get("base", 16), levels=state.get("levels", 2),
                    bold_from=state.get("bold_from", 1), bold_head=state.get("bold_head", "dilated"),
                    rule=state.get("rule", False), rule_head=state.get("rule_head", "dilated"),
                    color=state.get("color", False))
    model.load_state_dict(state["model"])
    model.eval()
    return model


def erase_from_strips(model: InkUNet, img: np.ndarray, strips_dir: str, dilate: int = 7):
    """Matte every deskewed strip and splat the erase back through its coordmap.

    `img` is HxWx3 float32 in 0..1. Returns `(out, alpha_img, raw_img, quads)`: the erased
    image, the final composite erase alpha (dilated/hardened — what actually got removed),
    the raw (undilated) matte projected to image space, and the per-box dewarp quads.
    """
    ih, iw = img.shape[:2]
    alpha_img = np.zeros((ih, iw), dtype=np.float32)
    color_img = np.zeros((ih, iw, 3), dtype=np.float32)
    raw_img = np.zeros((ih, iw), dtype=np.float32)  # raw matte, no dilate/harden
    quads = []

    maps = sorted(glob.glob(os.path.join(strips_dir, "box-*.map")))
    for mp in maps:
        png = mp[:-4] + ".png"
        if not os.path.exists(png):
            continue
        strip = np.asarray(Image.open(png).convert("RGB"), dtype=np.float32) / 255.0
        coord = load_coordmap(mp)
        if strip.shape[:2] != coord.shape[:2]:
            continue
        ink = erase_ink(model_strip(model, strip))
        k = dilate
        comp_a = np.clip(grey_dilation(ink, size=(k, k)) * 1.8, 0, 1)
        bg = background_field(strip, grey_dilation(ink, size=(k + 4, k + 4)))

        # Splat strip pixels onto the image through the coordmap; assign in order of
        # increasing alpha so the strongest-ink contributor wins at each target pixel.
        sx = np.clip(np.round(coord[..., 0]).astype(np.int64), 0, iw - 1).ravel()
        sy = np.clip(np.round(coord[..., 1]).astype(np.int64), 0, ih - 1).ravel()
        order = np.argsort(comp_a.ravel(), kind="stable")
        ty, tx = sy[order], sx[order]
        alpha_img[ty, tx] = np.maximum(alpha_img[ty, tx], comp_a.ravel()[order])
        color_img[ty, tx] = bg.reshape(-1, 3)[order]
        raw_img[ty, tx] = np.maximum(raw_img[ty, tx], ink.ravel()[order])
        h, w = coord.shape[:2]
        quads.append([tuple(coord[0, 0]), tuple(coord[0, w - 1]),
                      tuple(coord[h - 1, w - 1]), tuple(coord[h - 1, 0])])

    # Close splat holes / cover JPEG ringing at image scale, then composite.
    alpha_img = grey_dilation(alpha_img, size=(dilate, dilate))
    color_img = grey_dilation(color_img, size=(dilate, dilate, 1))
    out = alpha_img[..., None] * color_img + (1 - alpha_img[..., None]) * img
    return out, alpha_img, raw_img, quads


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--image", required=True)
    ap.add_argument("--strips", required=True, help="the deskewed/ dir with box-NNN.png + .map")
    ap.add_argument("--ckpt", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--mask-out", help="overlay of the raw ink matte + detection quads")
    ap.add_argument("--dilate", type=int, default=7)
    args = ap.parse_args()

    model = load_ink_model(args.ckpt)
    img = np.asarray(Image.open(args.image).convert("RGB"), dtype=np.float32) / 255.0
    out, _alpha, raw_img, quads = erase_from_strips(model, img, args.strips, args.dilate)
    Image.fromarray((np.clip(out, 0, 1) * 255).astype(np.uint8)).save(args.out)
    print(f"erased {len(quads)} boxes -> {args.out}")

    if args.mask_out:
        # Tint the original toward magenta by the raw matte: pixels the model calls
        # "ink" glow, so a wrong-pixel matte and a right-pixel-but-ghosting erase
        # look different. Detection quads drawn in cyan show the 0.60-gated boxes.
        m = grey_dilation(raw_img, size=(2, 2))[..., None]
        ink = np.array([1.0, 0.0, 0.8], dtype=np.float32)
        over = (1 - 0.7 * m) * img + 0.7 * m * ink
        pim = Image.fromarray((np.clip(over, 0, 1) * 255).astype(np.uint8))
        draw = ImageDraw.Draw(pim)
        for q in quads:
            draw.line([*q, q[0]], fill=(0, 255, 255), width=2)
        pim.save(args.mask_out)
        print(f"ink-mask overlay -> {args.mask_out}")


if __name__ == "__main__":
    main()
