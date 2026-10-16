"""Rent ONE 2-GPU box, direct-connected, and measure 1-GPU vs 2-GPU scaling.

Two things are unproven and this settles both on a single rental:

1. `--direct` on create. Every transfer so far relayed through vast's US-west ssh
   gateway — 1.15 MB/s on a 1.96GB upload, ~28 min, with the rented GPU idle at
   17W throughout. Every offer already exposes hundreds of direct ports (52/52
   measured), we simply never asked. `pipe ps` reports which route a box got, so
   a proxy fallback is visible rather than silent.

2. 2-GPU data-parallel scaling under NOTES' efficiency gate: per-GPU util >80%
   EACH and 1.7-1.9x throughput, rejecting the case where a second card idles.
   The named risk is OpusTrainer's single stdin pipe starving both GPUs.

The input is a SMALL slice, not the 10M-line production train_tsv. Shipping the
full 1.96GB to a probe box cost ~28 minutes of paid idle earlier today; a probe
should carry probe-sized inputs.

    pipe --run gpu2check put train_slice <200k lines of train.tsv> --kind lines
    pipe --run gpu2check put vocab <vocab.spm> --kind blob
    pipe --run gpu2check run gpu2check
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

CUDA_FP16 = "ghcr.io/davidventura/offline-translator/marian-cuda:fp16-1a743582"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


@step(
    image=CUDA_FP16,
    # num_gpus=2 is the new knob; the IMAGE is unchanged, because GPU count is a
    # runtime flag (--devices) and one marian-cuda serves 1, 2 or CPU.
    target=Vast(gpu="RTX_4090", num_gpus=2, max_hours=2, disk_gb=60,
                tries=8, geo=EU, min_cuda=11.8),
    script="gpu2_validate.sh",
    deps=deps.VALIDATE,
    outputs={"report": Output(rel="report.txt", kind=Kind.BLOB)},
)
def gpu2(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("train_slice"), ctx.inp("vocab"), ctx.out_dir]


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    out = run.do(gpu2, timeout=2 * 3600,
                 train_slice=a("train_slice"), vocab=a("vocab"))
    return {"report": out["report"].to_json()}
