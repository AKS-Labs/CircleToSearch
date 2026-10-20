"""Build a real-strip ink-eval corpus from files/live-overlay.

For each image: EXIF-upright, run viz_pipeline (det + deskew + rec), keep boxes whose
recognition is non-empty (drops noise boxes), then render the keeper ink matte as PURE
red (255,0,0) over each strip into a review dir. The user deletes strips where the red
is wrong; the survivors' red = bootstrap ground-truth (mask = pixels == [255,0,0]).
Deleted strips get a clean copy in clean/ for manual/classical relabeling.
"""

import glob
import os
import re
import subprocess
import sys

import numpy as np
import torch
from PIL import Image, ImageOps

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from model import InkUNet  # noqa: E402

REPO = "/home/david/git/translator-rs"
SRC = f"{REPO}/files/live-overlay"
V5 = "/home/david/AndroidStudioProjects/bucket/ocr/1/PP-OCRv5"
VIZ = f"{REPO}/target/release/viz_pipeline"
CKPT = f"{REPO}/scripts/ink_model/ink-tv-8k.pt"
INK_MNN = f"{REPO}/scripts/ink_model/ink-tv-8k_int8.mnn"
OUT = "/tmp/eval"
INK_CUT = 40

os.makedirs(f"{OUT}/review", exist_ok=True)
os.makedirs(f"{OUT}/clean", exist_ok=True)
os.makedirs(f"{OUT}/upright", exist_ok=True)

ck = torch.load(CKPT, map_location="cpu")
net = InkUNet(base=ck["base"], levels=ck["levels"], bold_from=ck.get("bold_from", 1),
              bold_head=ck.get("bold_head", "dilated"))
net.load_state_dict(ck["model"])
net.eval()


def matte_of(strip: Image.Image) -> np.ndarray:
    """Keeper matte at the strip's own resolution (run at 48px, upscaled back)."""
    H = 48
    w = max(16, round(strip.width * H / strip.height))
    s = strip.resize((w, H), Image.BILINEAR)
    pad = (-w) % 16
    arr = np.asarray(s.convert("RGB"), np.float32) / 255.0
    if pad:
        arr = np.pad(arr, ((0, 0), (0, pad), (0, 0)), mode="edge")
    x = torch.from_numpy(arr.transpose(2, 0, 1))[None]
    with torch.no_grad():
        m = torch.sigmoid(net(x))[0, 0].numpy()
    m = m[:, :w]  # drop the mult-of-16 padding columns
    return np.asarray(Image.fromarray((m * 255).astype(np.uint8)).resize(strip.size, Image.BILINEAR))


def sanitize(t: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "-", t).strip("-")[:24] or "x"


SKIP = {"thai-banner", "colors"}
MIN_AREA = 15_000
# Idempotent: skip any image that already has strips in clean/ (so re-running only adds new
# images without clobbering the curated review/ pruning).
done = {os.path.basename(f).split("__")[0] for f in glob.glob(f"{OUT}/clean/*.png")}
images = sorted(
    f for f in os.listdir(SRC)
    if f.lower().endswith((".jpg", ".jpeg", ".png"))
    and os.path.splitext(f)[0] not in SKIP
    and os.path.splitext(f)[0] not in done
)
print(f"processing {len(images)} new image(s); {len(done)} already done")
kept = dropped_noise = 0
for fn in images:
    name = os.path.splitext(fn)[0]
    up = f"{OUT}/upright/{name}.jpg"
    ImageOps.exif_transpose(Image.open(f"{SRC}/{fn}")).convert("RGB").save(up, quality=95)
    vdir = f"{OUT}/viz/{name}"
    r = subprocess.run(
        [VIZ, up, "--model-dir", V5, "--ink", INK_MNN,
         "--stages", "boxes,oriented-boxes,deskewed,ink", "--out", vdir],
        capture_output=True, text=True, timeout=300,
    )
    labels = {}
    for ln in r.stdout.splitlines():
        m = re.search(r'box (\d+): .*"(.*)"', ln)
        if m:
            labels[int(m.group(1))] = m.group(2).strip()
    n_img = 0
    for bi, text in labels.items():
        if len(text) < 2:
            dropped_noise += 1
            continue
        sp = f"{vdir}/deskewed/box-{bi:03d}.png"
        if not os.path.exists(sp):
            continue
        strip = Image.open(sp).convert("RGB")
        if strip.width * strip.height < MIN_AREA:
            continue
        matte = matte_of(strip)
        ink = matte >= INK_CUT
        red = np.asarray(strip).copy()
        red[ink] = (255, 0, 0)
        base = f"{name}__b{bi:03d}__{sanitize(text)}"
        Image.fromarray(red).save(f"{OUT}/review/{base}.png")
        strip.save(f"{OUT}/clean/{base}.png")
        kept += 1
        n_img += 1
    print(f"{name}: {n_img} strips kept")

print(f"\nTOTAL kept {kept}  dropped(noise) {dropped_noise}")
print(f"review dir: {OUT}/review   clean dir: {OUT}/clean")
