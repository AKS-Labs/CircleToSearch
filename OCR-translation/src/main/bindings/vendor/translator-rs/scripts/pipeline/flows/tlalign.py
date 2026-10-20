"""en->tl guided alignment over the KD pairs -> train.tsv.

Bigserver, free, and the last CPU stage before training. fast_align both
directions + atools symmetrisation; align.sh fails loudly if either direction
comes back >1% empty, which is the guard added after align_ensw silently trained
en->sw on forward-only alignments (the reverse pass had died on a nan and exited
0 behind a swallowed stderr).

drop_empty already ran in tlkd, so the zero-token pairs that send fast_align's EM
to nan are gone before this starts.

    pipe --run tlkd run tlalign
"""

from __future__ import annotations

from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver
from pipe.types import Kind

BMT = "marian-bmt:next"
TOOLS = "/opt/tools"


@step(
    image=BMT,
    target=Bigserver(cpus=16),
    script="align.sh",
    outputs={"train_tsv": Output(rel="train.tsv", kind=Kind.LINES)},
)
def align(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("src"), ctx.inp("tgt"), ctx.out_dir, TOOLS]


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    src, tgt = a("pairs_src"), a("pairs_tgt")
    if src.lines != tgt.lines:
        raise RuntimeError(f"KD pairs desynced: {src.lines} src vs {tgt.lines} tgt")

    train_tsv = run.do(align, timeout=6 * 3600, src=src, tgt=tgt)["train_tsv"]
    if train_tsv.lines != src.lines:
        raise RuntimeError(f"align lost lines: {src.lines} -> {train_tsv.lines}")
    return {"train_tsv": train_tsv.to_json()}
