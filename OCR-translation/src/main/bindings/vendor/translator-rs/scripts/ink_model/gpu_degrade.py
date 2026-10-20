"""Batched degrade + legibility, on the GPU, for the training loop.

The CPU `gen_data.degrade`/`legible` ran per-strip on the dataloader workers and were
~half of gen time (the bottleneck that starved the GPU). Here the same camera/screen
degradations run once per batch on the device — each op is computed for the whole
batch and blended in per-strip with its own random mask + parameters, so the
distribution matches the CPU path, just vectorised. Strips arrive already resized to
48 high (the model's input height), so blur sigma is rescaled from native to 48 px.

`legible` becomes a per-strip mask returned to the loss instead of a reject-and-retry,
which also removes the ~28% of composites the CPU path threw away.

Shapes: `imgs` (B,3,48,W) in 0..1, `cov` (B,1,48,W) soft coverage, `native_h` (B,).
"""

import math

import torch
import torch.nn.functional as F

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
_BLUR_K = 13  # kernel size; covers sigma up to ~2 px


def _dct8(device) -> torch.Tensor:
    u = torch.arange(8, device=device, dtype=torch.float32).view(8, 1)
    r = torch.arange(8, device=device, dtype=torch.float32).view(1, 8)
    d = math.sqrt(2.0 / 8.0) * torch.cos((2 * r + 1) * u * math.pi / 16.0)
    d[0] *= 1.0 / math.sqrt(2.0)
    return d


def _jpeg_quant(img: torch.Tensor, quality: torch.Tensor, dct: torch.Tensor) -> torch.Tensor:
    """Block-DCT quantization with a per-strip quality (B,). H,W multiples of 8."""
    b, c, h, w = img.shape
    scale = torch.where(quality < 50, 5000.0 / quality, 200.0 - 2.0 * quality)  # (B,)
    qtab = torch.clamp(
        torch.floor((_JPEG_Q50.to(img.device)[None] * scale[:, None, None] + 50.0) / 100.0),
        min=1.0,
    )  # (B,8,8)
    x = img * 255.0 - 128.0
    blocks = x.reshape(b, c, h // 8, 8, w // 8, 8).permute(0, 1, 2, 4, 3, 5)
    coeff = torch.einsum("ur,bchwrs,vs->bchwuv", dct, blocks, dct)
    q = qtab[:, None, None, None]  # (B,1,1,1,8,8)
    coeff = torch.round(coeff / q) * q
    out = torch.einsum("ur,bchwuv,vs->bchwrs", dct, coeff, dct)
    out = out.permute(0, 1, 2, 4, 3, 5).reshape(b, c, h, w)
    return torch.clamp((out + 128.0) / 255.0, 0.0, 1.0)


def _gaussian_blur(img: torch.Tensor, sigma: torch.Tensor) -> torch.Tensor:
    """Separable gaussian with a per-strip sigma (B,) via grouped conv."""
    b, c, h, w = img.shape
    half = _BLUR_K // 2
    x = torch.arange(-half, half + 1, device=img.device, dtype=torch.float32)
    k = torch.exp(-(x[None, :] ** 2) / (2.0 * sigma[:, None] ** 2 + 1e-6))  # (B,K)
    k = k / k.sum(dim=1, keepdim=True)
    kc = k.repeat_interleave(c, dim=0)  # (B*C,K)
    xb = img.reshape(1, b * c, h, w)
    xb = F.conv2d(xb, kc.view(b * c, 1, 1, _BLUR_K), padding=(0, half), groups=b * c)
    xb = F.conv2d(xb, kc.view(b * c, 1, _BLUR_K, 1), padding=(half, 0), groups=b * c)
    return xb.reshape(b, c, h, w)


def _proj(h: int, w: int, device, angle: torch.Tensor) -> torch.Tensor:
    """Per-strip normalized directional ramp in 0..1, (B,1,H,W). angle (B,)."""
    yy = torch.linspace(0, 1, h, device=device).view(1, 1, h, 1)
    xx = torch.linspace(0, 1, w, device=device).view(1, 1, 1, w)
    t = torch.cos(angle).view(-1, 1, 1, 1) * xx + torch.sin(angle).view(-1, 1, 1, 1) * yy
    tmin = t.amin(dim=(2, 3), keepdim=True)
    tmax = t.amax(dim=(2, 3), keepdim=True)
    return (t - tmin) / (tmax - tmin).clamp_min(1e-6)


def _u(gen, b, lo, hi, device):
    return lo + (hi - lo) * torch.rand(b, device=device, generator=gen)


def _m(gen, b, p, device):  # per-strip apply mask, prob p
    return (torch.rand(b, device=device, generator=gen) < p)


def degrade_batch(imgs: torch.Tensor, native_h: torch.Tensor, gen: torch.Generator,
                  dct: torch.Tensor) -> torch.Tensor:
    """All of `gen_data.degrade`, batched. Each op is computed for the whole batch and
    selected in per-strip via its own mask (`where`), so unused strips pass through."""
    b, c, h, w = imgs.shape
    dev = imgs.device
    nh = native_h.to(dev).float()
    out = imgs

    # Gaussian blur (p=0.5). CPU blurred at native then downsampled to 48; the
    # equivalent 48-px sigma is the native sigma scaled by 48/native_h.
    sig_native = torch.clamp(0.35 + nh * 0.032, max=2.0)
    sigma = torch.clamp(sig_native * 48.0 / nh.clamp_min(1.0), 0.3, 2.0)
    blurred = _gaussian_blur(out, sigma)
    sel = _m(gen, b, 0.5, dev).view(b, 1, 1, 1)
    out = torch.where(sel, blurred, out)

    # Downsample-upsample (p=0.35, scale 0.55-0.85), bucketed for batched interpolate.
    ds = _m(gen, b, 0.35, dev)
    if ds.any():
        buckets = torch.tensor([0.55, 0.65, 0.75, 0.85], device=dev)
        pick = torch.randint(0, 4, (b,), device=dev, generator=gen)
        for i, s in enumerate(buckets.tolist()):
            grp = ds & (pick == i)
            if not grp.any():
                continue
            small = F.interpolate(out, scale_factor=(s, s), mode="bilinear", align_corners=False)
            back = F.interpolate(small, size=(h, w), mode="bilinear", align_corners=False)
            out = torch.where(grp.view(b, 1, 1, 1), back, out)

    # JPEG block-DCT quantization (p=0.7, quality 45-95), per-strip quality.
    q = _u(gen, b, 45.0, 95.0, dev)
    jp = _jpeg_quant(out.clamp(0, 1), q, dct)
    sel = _m(gen, b, 0.7, dev).view(b, 1, 1, 1)
    out = torch.where(sel, jp, out)

    # Smooth illumination ramp (p=0.5).
    ang = _u(gen, b, 0.0, 2 * math.pi, dev)
    ramp = _proj(h, w, dev, ang)
    shade = (_u(gen, b, 0.55, 1.0, dev).view(b, 1, 1, 1)
             + ramp * _u(gen, b, 0.0, 0.45, dev).view(b, 1, 1, 1))
    shaded = out * torch.clamp(shade, 0.4, 1.2)
    sel = _m(gen, b, 0.5, dev).view(b, 1, 1, 1)
    out = torch.where(sel, shaded, out)

    # Hard cast-shadow edge (p=0.25).
    ang = _u(gen, b, 0.0, 2 * math.pi, dev)
    proj = _proj(h, w, dev, ang)
    edge = _u(gen, b, 0.2, 0.8, dev).view(b, 1, 1, 1)
    shadow = torch.where(proj < edge, _u(gen, b, 0.4, 0.8, dev).view(b, 1, 1, 1),
                         torch.ones(b, 1, 1, 1, device=dev))
    sel = _m(gen, b, 0.25, dev).view(b, 1, 1, 1)
    out = torch.where(sel, out * shadow, out)

    # Gaussian sensor noise (p=0.6, sigma 0.005-0.04).
    nsig = _u(gen, b, 0.005, 0.04, dev).view(b, 1, 1, 1)
    noised = out + torch.randn(b, c, h, w, device=dev, generator=gen) * nsig
    sel = _m(gen, b, 0.6, dev).view(b, 1, 1, 1)
    out = torch.where(sel, noised, out)

    # Dynamic-range squeeze (p=0.3).
    lo = _u(gen, b, 0.0, 0.08, dev).view(b, 1, 1, 1)
    hi = _u(gen, b, 0.85, 1.0, dev).view(b, 1, 1, 1)
    squeezed = out * (hi - lo) + lo
    sel = _m(gen, b, 0.3, dev).view(b, 1, 1, 1)
    out = torch.where(sel, squeezed, out)

    return out.clamp(0, 1)


def legible_mask(imgs: torch.Tensor, cov: torch.Tensor, native_h: torch.Tensor) -> torch.Tensor:
    """Per-strip legibility (B,) bool: ink/bg colour contrast within the text's rows.
    Uses masked means (batchable) where the CPU used medians — a loss gate doesn't need
    the exact statistic. Threshold rises for small text, as on the CPU."""
    dev = imgs.device
    nh = native_h.to(dev).float()
    ink = (cov > 0.6).float()              # (B,1,H,W)
    text_rows = (ink.sum(dim=3, keepdim=True) > 0).float()  # (B,1,H,1)
    bg = (cov < 0.05).float() * text_rows
    ink_n = ink.sum(dim=(2, 3))            # (B,1)
    bg_n = bg.sum(dim=(2, 3))
    ink_mean = (imgs * ink).sum(dim=(2, 3)) / ink_n.clamp_min(1.0)   # (B,3)
    bg_mean = (imgs * bg).sum(dim=(2, 3)) / bg_n.clamp_min(1.0)
    d = (ink_mean - bg_mean).abs().amax(dim=1)                       # (B,)
    thresh = 0.13 + torch.clamp(30.0 - nh, min=0.0) * 0.006
    return (d > thresh) & (ink_n.squeeze(1) >= 30) & (bg_n.squeeze(1) >= 30)
