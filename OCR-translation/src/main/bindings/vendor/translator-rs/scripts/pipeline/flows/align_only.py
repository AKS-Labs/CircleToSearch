from __future__ import annotations

from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver
from pipe.types import Kind

TOOLS = "/opt/tools"


@step(
    image="marian-bmt:next",
    target=Bigserver(cpus=16),
    script="align.sh",
    outputs={"train_tsv": Output(rel="train.tsv", kind=Kind.LINES)},
)
def align(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_tgt"), ctx.out_dir, TOOLS]


def main(run: Run, argv: list[str]) -> dict:
    out = run.do(
        align,
        timeout=3600,
        kd_src=run.ledger.artifact("kd_src"),
        kd_tgt=run.ledger.artifact("kd_tgt"),
    )
    tsv = out["train_tsv"]
    src = run.ledger.artifact("kd_src")
    if tsv.lines != src.lines:
        raise RuntimeError(f"align dropped lines: {src.lines} in, {tsv.lines} out")
    return {"train_tsv": tsv.to_json()}
