"""One-box Hy-MT2-7B-FP8 throughput + quality bench before the 5-box KD.

Decodes a ~5k kd_src sample via vLLM (1-best), reports lines/s (the KD wall-time
input), and leaves the outputs for a quality spot-check + a FLORES score. Confirms
FP8 loads and the throughput estimate before committing the sharded decode.

    pipe --run hybench put sample <~5k Uyghur kd_src lines> --kind lines
    pipe --run hybench run hy_bench
"""

from __future__ import annotations

from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


@step(
    image=HYKD,
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="vllm_decode.sh",
    outputs={"hyp": Output(rel="sample_out.en", kind=Kind.LINES)},
)
def bench(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("sample"), ctx.out("sample_out.en"), "0"]


def main(run: Run, argv: list[str]) -> dict:
    hyp = run.do(bench, timeout=2 * 3600, sample=run.ledger.artifact("sample"))["hyp"]
    return {"hyp": hyp.to_json()}
