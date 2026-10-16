"""Bench: how much does moving the *movable* generation work to the GPU help?

Text rasterization (PIL FreeType) is stuck on the CPU. Everything after it —
backgrounds, compositing, noise/illumination, blur, resize — is batchable torch and
can run on the GPU. This measures, per CPU core:
  - full CPU sample() rate (current pipeline)
  - raster-only rate (the part that MUST stay on CPU)
and on the GPU:
  - batched augment rate (the movable part)
so we can estimate the realistic ceiling of a CPU-raster + GPU-augment split.

    python3 bench_gpu_gen.py
"""

import math
import random
import time

import numpy as np
import torch
import torch.nn.functional as F

from gen_data import render_coverage, gradient_field, degrade

H, W = 32, 320          # representative native strip
B = 256                 # batch
REPS = 8

# Standard JPEG luminance quantization table (quality 50).
_JPEG_Q50 = torch.tensor(
    [
        [16, 11, 10, 16, 24, 40, 51, 61],
        [12, 12, 14, 19, 26, 58, 60, 55],
        [14, 13, 16, 24, 40, 57, 69, 56],
        [14, 17, 22, 29, 51, 87, 80, 62],
        [18, 22, 37, 56, 68, 109, 103, 77],
        [24, 35, 55, 64, 81, 104, 113, 92],
        [49, 64, 78, 87, 103, 121, 120, 101],
        [72, 92, 95, 98, 112, 100, 103, 99],
    ],
    dtype=torch.float32,
)


def _dct8(device) -> torch.Tensor:
    """Orthonormal 8-point DCT-II matrix `D`; the 2-D DCT of an 8x8 block X is
    `D @ X @ D.T`, the inverse `D.T @ C @ D` (D is orthogonal)."""
    u = torch.arange(8, device=device, dtype=torch.float32).view(8, 1)
    r = torch.arange(8, device=device, dtype=torch.float32).view(1, 8)
    d = math.sqrt(2.0 / 8.0) * torch.cos((2 * r + 1) * u * math.pi / 16.0)
    d[0] *= 1.0 / math.sqrt(2.0)
    return d


def jpeg_quant(img: torch.Tensor, quality: float, dct: torch.Tensor) -> torch.Tensor:
    """Batched JPEG-style block-DCT quantization on the GPU — the costliest CPU op
    (`degrade`'s encode→decode round-trip) as tensor math instead. `img` is
    (B,3,H,W) in 0..1; H and W must be multiples of 8 (strips are 48×{16·n})."""
    b, c, h, w = img.shape
    scale = (5000.0 / quality) if quality < 50 else (200.0 - 2.0 * quality)
    qtab = torch.clamp(torch.floor((_JPEG_Q50.to(img.device) * scale + 50.0) / 100.0), min=1.0)
    x = img * 255.0 - 128.0
    blocks = x.reshape(b, c, h // 8, 8, w // 8, 8).permute(0, 1, 2, 4, 3, 5)  # (B,3,Hb,Wb,8,8)
    coeff = torch.einsum("ur,...rc,vc->...uv", dct, blocks, dct)
    coeff = torch.round(coeff / qtab) * qtab
    out = torch.einsum("ur,...uv,vc->...rc", dct, coeff, dct)
    out = out.permute(0, 1, 2, 4, 3, 5).reshape(b, c, h, w)
    return torch.clamp((out + 128.0) / 255.0, 0.0, 1.0)


def time_it(fn, iters):
    fn()  # warm
    t0 = time.time()
    for _ in range(iters):
        fn()
    return time.time() - t0


def cpu_full(rng):
    cov, fill, _bold, _rule = render_coverage(rng, W, H)
    bg = gradient_field(rng, H, W)
    ink = np.random.rand(3).astype(np.float32)
    img = cov[..., None] * ink + (1 - cov[..., None]) * bg
    degrade(img, rng, H)


def cpu_raster(rng):
    render_coverage(rng, W, H)


def gpu_augment(cov, device, dct):
    """Movable part for a whole batch on the GPU: bg + composite + noise + blur +
    resize + JPEG-style block quantization (the part that used to dominate on CPU)."""
    b = cov.shape[0]
    ink = torch.rand(b, 3, 1, 1, device=device)
    c0 = torch.rand(b, 3, 1, 1, device=device)
    c1 = torch.rand(b, 3, 1, 1, device=device)
    t = torch.rand(b, 1, 1, W, device=device)
    bg = t * c1 + (1 - t) * c0
    img = cov * ink + (1 - cov) * bg
    img = img + torch.randn_like(img) * 0.02
    k = torch.tensor([1.0, 4.0, 6.0, 4.0, 1.0], device=device)
    k = (k / k.sum()).view(1, 1, 1, 5).expand(3, 1, 1, 5)
    img = F.conv2d(img, k, padding=(0, 2), groups=3)
    img = F.interpolate(img, size=(48, W), mode="bilinear", align_corners=False)
    img = jpeg_quant(img.clamp(0, 1), quality=60, dct=dct)
    return img


def main():
    rng = random.Random(0)
    rng.seed(0)
    rate_full = REPS * B / time_it(lambda: [cpu_full(rng) for _ in range(B)], REPS)
    rate_raster = REPS * B / time_it(lambda: [cpu_raster(rng) for _ in range(B)], REPS)

    device = "cuda" if torch.cuda.is_available() else "cpu"
    cov = torch.rand(B, 1, H, W, device=device)
    dct = _dct8(device)
    if device == "cuda":
        def gpu_run():
            gpu_augment(cov, device, dct); torch.cuda.synchronize()
    else:
        gpu_run = lambda: gpu_augment(cov, device, dct)  # noqa: E731
    rate_gpu = REPS * B / time_it(gpu_run, REPS)

    print(f"device={device}  batch={B}")
    print(f"CPU full sample()/core : {rate_full:8.0f} strips/s")
    print(f"CPU raster-only /core  : {rate_raster:8.0f} strips/s  ({rate_raster/rate_full:.2f}x full)")
    print(f"GPU batched augment    : {rate_gpu:8.0f} strips/s")


if __name__ == "__main__":
    main()
