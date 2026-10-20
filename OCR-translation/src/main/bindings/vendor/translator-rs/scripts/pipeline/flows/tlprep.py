"""en->tl corpus prep + KD source, through the steps rather than around them.

Redoing what was first done by hand on 2026-07-20, because doing it by hand lost
every fix the steps carry: `prep_data.py` run directly emitted two .gz files and
a `.model` vocab (marian rejects that extension) with no 2-col pool, and the KD
source was then built with `sort -u | shuf | head`, which is the recipe
build_kd_source.sh exists to replace — source-side dedup destroys the src/tgt
pairing, so there is no kd_ref for ce-filter or extract-best, and an unseeded
shuf is reproducible by nobody. None of it was an artifact, so none of it had a
digest, a line count, or a memo.

Both scripts are now atomic (prep_data.py does its own pool paste, vocab naming
and cleanup; build_kd_source.sh takes the LIMIT), so the partial forms that made
those mistakes reachable no longer exist.

    pipe --run tlprep run tlprep
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver
from pipe.types import Kind

PREP = "prep:next"
BMT = "marian-bmt:next"
KD_LINES = "10000000"


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    deps=deps.PREP,
    outputs={
        "pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES),
        "vocab": Output(rel="vocab.spm", kind=Kind.BLOB),
    },
)
def prep(ctx: Ctx) -> list[str]:
    # train: a NEW joint vocab over the re-cleaned pool. A joint SPM is
    # pair-specific and the corpus changed, so the shipped entl vocab cannot be
    # reused for a corpus it was not trained on.
    return [ctx.script, "tl", "16", ctx.out_dir, "en", "train"]


@step(
    image=BMT,
    target=Bigserver(cpus=8),
    script="build_kd_source.sh",
    outputs={
        "kd_src": Output(rel="kd_src", kind=Kind.LINES),
        "kd_ref": Output(rel="kd_ref", kind=Kind.LINES),
    },
)
def kd_source(ctx: Ctx) -> list[str]:
    # pool is en\ttl; column 1 (en) is the en->tl KD source, column 2 the ref.
    return [ctx.script, ctx.inp("pool_tsv"), "1", ctx.out_dir, KD_LINES]


def main(run: Run, argv: list[str]) -> dict:
    prepped = run.do(prep, timeout=4 * 3600)
    kd = run.do(kd_source, timeout=2 * 3600, pool_tsv=prepped["pool_tsv"])
    return {
        "pool_tsv": prepped["pool_tsv"].to_json(),
        "vocab": prepped["vocab"].to_json(),
        "kd_src": kd["kd_src"].to_json(),
        "kd_ref": kd["kd_ref"].to_json(),
    }
