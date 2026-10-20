"""Build a real (strip, matte) training set from the OTR train split, storage-bounded.

Runs ON the GPU box (fast network, many cores). Per shard, until --n-images reached:
  download parquet -> dump image+gt PNGs -> `extract_strips` (production det+dewarp) ->
  per strip, warp gt through the coordmap, diff = matte label, drop boxes below an ink floor
  (kills det false-positives) -> save 48px (strip, matte) -> delete the shard + temp -> next.

Bold/rule are left unlabelled here (real data carries no rule; bold derivation is a follow-up) —
the training mix masks those losses on real samples.

    python build_otr_real.py --det PP-OCRv5_mobile_det.mnn --extract-bin ./extract_strips \
        --out otr_real --n-images 15000 --threads 36
"""

import argparse
import glob
import io
import os
import shutil
import struct
import subprocess

import numpy as np
import pyarrow.parquet as pq
from huggingface_hub import hf_hub_download
from PIL import Image, PngImagePlugin
from scipy.ndimage import map_coordinates

# Some OTR PNGs carry oversized tEXt chunks that trip Pillow's decompression-bomb guard
# (default 1 MB) and abort the whole run; raise it and skip any image that still fails.
PngImagePlugin.MAX_TEXT_CHUNK = 16 * 1024 * 1024

REPO, NSHARDS = "cyberagent/OTR", 148
HEIGHT = 48


def load_map(path):
    with open(path, "rb") as f:
        w, h = struct.unpack("<II", f.read(8))
        return np.frombuffer(f.read(), dtype="<f4").reshape(h, w, 2)


def warp_gt(gt, coord):
    # bilinear-sample gt at the strip's source coords (coord[...,0]=src_x, [...,1]=src_y),
    # so the warped gt aligns with the (bilinear-warped) strip PNG.
    sx, sy = coord[..., 0].ravel(), coord[..., 1].ravel()
    out = np.stack([map_coordinates(gt[:, :, c], [sy, sx], order=1, mode="nearest")
                    for c in range(3)], axis=-1)
    return out.reshape(coord.shape[0], coord.shape[1], 3)


def resize48(arr, is_mask):
    h, w = arr.shape[:2]
    nw = max(16, round(w * HEIGHT / h))
    mode = "L" if is_mask else "RGB"
    im = Image.fromarray(arr).resize((nw, HEIGHT), Image.NEAREST if is_mask else Image.BILINEAR)
    return np.asarray(im)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--det", required=True)
    ap.add_argument("--extract-bin", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--n-images", type=int, default=15000)
    ap.add_argument("--threads", type=int, default=36)
    ap.add_argument("--tau", type=float, default=0.02, help="|img-gt| ink threshold (a hair above lossless 0)")
    ap.add_argument("--ink-floor", type=float, default=0.015, help="min strip ink fraction; below = det false-positive, drop")
    ap.add_argument("--tmp", default="_otr_tmp")
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    done_imgs = kept = dropped = 0
    for s in range(NSHARDS):
        if done_imgs >= args.n_images:
            break
        shard = f"data/train-{s:05d}-of-{NSHARDS:05d}.parquet"
        pdir = os.path.join(args.tmp, "parquet")
        path = hf_hub_download(REPO, shard, repo_type="dataset", local_dir=pdir)
        img_dir = os.path.join(args.tmp, "img"); gt_dir = os.path.join(args.tmp, "gt")
        for d in (img_dir, gt_dir):
            shutil.rmtree(d, ignore_errors=True); os.makedirs(d)
        gts = {}
        pf = pq.ParquetFile(path)
        for batch in pf.iter_batches(batch_size=64):
            d = batch.to_pydict()
            for i in range(len(d["id"])):
                if done_imgs >= args.n_images:
                    break
                rid = f"s{s}_{d['id'][i]}"
                try:
                    Image.open(io.BytesIO(d["image"][i]["bytes"])).convert("RGB").save(os.path.join(img_dir, f"{rid}.png"))
                    gts[rid] = np.asarray(Image.open(io.BytesIO(d["gt_image"][i]["bytes"])).convert("RGB"), np.float32) / 255
                except Exception as e:
                    print(f"skip {rid}: {e}", flush=True)
                    continue
                done_imgs += 1
            if done_imgs >= args.n_images:
                break

        strip_root = os.path.join(args.tmp, "strips")
        shutil.rmtree(strip_root, ignore_errors=True)
        subprocess.run([args.extract_bin, "--images", img_dir, "--det", args.det,
                        "--out", strip_root, "--threads", str(args.threads)],
                       check=True, stdout=subprocess.DEVNULL)
        for mp in glob.glob(os.path.join(strip_root, "*", "box-*.map")):
            rid = os.path.basename(os.path.dirname(mp))
            png = mp[:-4] + ".png"
            if rid not in gts or not os.path.exists(png):
                continue
            strip = np.asarray(Image.open(png).convert("RGB"), np.float32) / 255
            coord = load_map(mp)
            if strip.shape[:2] != coord.shape[:2]:
                continue
            matte = (np.abs(strip - warp_gt(gts[rid], coord)).max(2) > args.tau)
            if matte.mean() < args.ink_floor:
                dropped += 1
                continue
            base = f"{rid}_{os.path.basename(png)[4:7]}"
            Image.fromarray(resize48((strip * 255).astype(np.uint8), False)).save(os.path.join(args.out, base + ".png"))
            Image.fromarray(resize48((matte * 255).astype(np.uint8), True)).save(os.path.join(args.out, base + ".matte.png"))
            kept += 1
        os.remove(path)  # free the shard
        print(f"shard {s}: imgs={done_imgs} kept={kept} dropped={dropped}", flush=True)

    shutil.rmtree(args.tmp, ignore_errors=True)
    print(f"DONE: {kept} real (strip,matte) pairs from {done_imgs} images ({dropped} dropped as low-ink) -> {args.out}")


if __name__ == "__main__":
    main()
