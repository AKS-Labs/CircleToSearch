"""Where is the training loop bottlenecked — CPU feed (dataloader/IPC) or GPU compute?
GPU util% is unreliable (it's "a kernel was running", not FLOP use), so measure the
two ceilings directly by throughput:

  dataloader-only : iterate the DataLoader, no GPU step  -> CPU+IPC feed ceiling
  GPU-only        : reuse one batch, full train step     -> compute ceiling

End-to-end ≈ min(the two). Worker sweep shows CPU-scaling (CPU-bound) vs plateau
(IPC-bound). Run with the GPU free.

  python3 bench_pipeline.py
"""

import time

import torch
from torch.utils.data import DataLoader

import gpu_degrade
from model import InkUNet
from train import SyntheticStrips, ink_losses

device = "cuda"
B, W, REUSE = 512, 320, 12


def dataloader_rate(workers: int, n: int = 25) -> float:
    loader = DataLoader(SyntheticStrips(batch=B, width=W, reuse=REUSE), batch_size=None,
                        num_workers=workers, persistent_workers=True, prefetch_factor=4,
                        pin_memory=True)
    it = iter(loader)
    for _ in range(5):  # fill the prefetch queue
        next(it)
    t = time.time()
    for _ in range(n):
        next(it)
    r = n * B / (time.time() - t)
    del it, loader
    return r


print("dataloader-only (GPU idle) — CPU+IPC feed ceiling:")
for wk in (16,):
    print(f"  workers={wk:2}: {dataloader_rate(wk):.0f} strips/s")

# GPU-only: one batch reused, full train step (degrade + fwd + bwd).
loader = DataLoader(SyntheticStrips(batch=B, width=W, reuse=REUSE), batch_size=None, num_workers=4)
img, cov, bold, rule, nh = next(iter(loader))
img = img.to(device).float(); cov = cov.to(device); bold = bold.to(device)
rule = rule.to(device); nh = nh.to(device)
model = InkUNet(base=16, levels=4, rule=True).to(device)
opt = torch.optim.Adam(model.parameters(), lr=1e-3)
gen = torch.Generator(device=device).manual_seed(0)
dct = gpu_degrade._dct8(device)
scaler = torch.amp.GradScaler()


def step(do_degrade=True):
    if do_degrade:
        with torch.no_grad():
            di = gpu_degrade.degrade_batch(img, nh, gen, dct)
            w = gpu_degrade.legible_mask(di, cov, nh).float()
    else:
        di, w = img, None
    with torch.autocast("cuda", dtype=torch.float16):
        loss, *_ = ink_losses(model(di), cov, bold, 1.5, strip_w=w, rule=rule)
    opt.zero_grad(set_to_none=True)
    scaler.scale(loss).backward()
    scaler.step(opt)
    scaler.update()


def rate(fn, n):
    for _ in range(8):
        fn()
    torch.cuda.synchronize()
    t = time.time()
    for _ in range(n):
        fn()
    torch.cuda.synchronize()
    return n * B / (time.time() - t)


print(f"\nGPU-only, base16 full step (degrade+fwd+bwd): {rate(lambda: step(True), 60):.0f} strips/s")
print(f"GPU-only, base16 fwd+bwd only (no degrade)  : {rate(lambda: step(False), 60):.0f} strips/s")
