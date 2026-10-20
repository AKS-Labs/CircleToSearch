"""One-box CT2 max-batch-tokens sweep for the NLLB teacher.

max-batch-tokens is a per-teacher/per-beam throughput knob and sw's 3072 was
tuned at beam 8; at the beam 4 standing rule VRAM is only ~5.5-6GB/24.6, so 3072
leaves most of the card idle. This rents ONE 4090, loads the teacher once, and
times a sample at a batch-size ladder to find the throughput knee and the OOM
ceiling on the actual fleet — the number to bake into kd_decode.sh before the
real decode commits it into the shard step keys.

    pipe --run uigbench put bench_sample <~5k source lines> --kind lines
    pipe --run uigbench run bench_batch
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

NLLB = "ghcr.io/davidventura/offline-translator/nllb-ct2:1.3b"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


@step(
    image=NLLB,
    target=Vast(gpu="RTX_4090", max_hours=1, disk_gb=40, tries=8, geo=EU, min_cuda=12.1),
    script="bench_batch.sh",
    deps=deps.BENCH_BATCH,
    outputs={"table": Output(rel="table.txt", kind=Kind.LINES)},
)
def bench(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("sample"), "uig_Arab", "eng_Latn", "4", "4", ctx.out("table.txt")]


def main(run: Run, argv: list[str]) -> dict:
    table = run.do(bench, timeout=3600, sample=run.ledger.artifact("bench_sample"))["table"]
    return {"table": table.to_json()}
