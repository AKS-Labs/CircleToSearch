"""Sanity-check the GPU degrade on real composited strips: legible fraction (should
track the CPU path's ~72% first-try pass rate) and a before/after montage to eyeball
that the degradation looks like realistic camera/screen capture.

  python3 validate_gpu_degrade.py            # writes /tmp/gpu_degrade_montage.png
"""

import random

import numpy as np
import torch
from PIL import Image

import gen_data as g
import gpu_degrade as gd

rng = random.Random(3)
gen = g.stream(rng, 320, 1, apply_degrade=False)
imgs, covs, nhs = [], [], []
N = 16
for _ in range(N):
    img, cov, bold, nh = next(gen)
    imgs.append(torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1))))
    covs.append(torch.from_numpy(cov[None]))
    nhs.append(nh)
x = torch.stack(imgs)
cov = torch.stack(covs)
native_h = torch.tensor(nhs, dtype=torch.float32)

tgen = torch.Generator().manual_seed(7)
dct = gd._dct8("cpu")
deg = gd.degrade_batch(x, native_h, tgen, dct)
leg = gd.legible_mask(deg, cov, native_h)

# Larger sample for a stable legible-fraction estimate.
imgs2, covs2, nhs2 = [], [], []
for _ in range(600):
    img, cov2, bold, nh = next(gen)
    imgs2.append(torch.from_numpy(np.ascontiguousarray(img.transpose(2, 0, 1))))
    covs2.append(torch.from_numpy(cov2[None]))
    nhs2.append(nh)
X = torch.stack(imgs2)
C = torch.stack(covs2)
NH = torch.tensor(nhs2, dtype=torch.float32)
D = gd.degrade_batch(X, NH, tgen, dct)
frac = float(gd.legible_mask(D, C, NH).float().mean())
print(f"legible fraction (target ~0.70-0.75 like CPU first-try): {frac:.3f}")

rows = []
for i in range(N):
    u = (x[i].permute(1, 2, 0).numpy() * 255).astype("uint8")
    d = (deg[i].permute(1, 2, 0).numpy() * 255).astype("uint8")
    tag = np.full((48, 40, 3), 0 if leg[i] else 200, "uint8")  # green-ish vs red marker
    tag[..., 1] = 200 if leg[i] else 0
    rows.append(np.concatenate([tag, u, np.full((48, 4, 3), 128, "uint8"), d], axis=1))
mont = np.concatenate([np.concatenate([r, np.full((6, r.shape[1], 3), 255, "uint8")], axis=0) for r in rows], axis=0)
Image.fromarray(mont).save("/tmp/gpu_degrade_montage.png")
print("saved /tmp/gpu_degrade_montage.png  (left tag=legible green/red, then undegraded | GPU-degraded)")
