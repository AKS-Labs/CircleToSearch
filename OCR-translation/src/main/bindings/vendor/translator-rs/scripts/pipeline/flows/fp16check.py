"""Validate the marian pin move on one box before committing a 6-hour train.

e8a1a25 (Dec 2021) cannot train this student in fp16: guided-alignment aborts on
a float32/float16 mismatch, and --workspace >12000 aborts in mini-batch-fit's
probe on a 2^31 overflow, leaving 11GB of a 24GB card unused. Upstream fixed both
in 1a743582 (Apr 2022) — "Small fixes around fp16 training and batch fitting" —
which also inherits sparse guided alignment (4b51dcbd), a speedup on exactly the
path this needs enabled.

Measured on a 4090 with alignment OFF (the only way fp16 ran on the old pin):
fp32 142k w/s vs fp16 231k w/s = 1.62x. Whether that survives with
guided-alignment ON is what this answers, and guided-alignment is not optional —
it produces the alignment head, and when align_ensw degraded it the note was
"chrF unaffected; bold/format transfer is what suffers".

One box, every check, then reaped. A per-variant rental would pay an image pull
per data point.

    pipe --run fp16check run fp16check
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
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=60, tries=8, geo=EU, min_cuda=11.8),
    script="fp16_validate.sh",
    deps=deps.VALIDATE,
    outputs={"report": Output(rel="report.txt", kind=Kind.BLOB)},
)
def validate(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("train_tsv"), ctx.inp("vocab"), ctx.out_dir]


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    out = run.do(validate, timeout=2 * 3600,
                 train_tsv=a("train_tsv"), vocab=a("vocab"))
    return {"report": out["report"].to_json()}
