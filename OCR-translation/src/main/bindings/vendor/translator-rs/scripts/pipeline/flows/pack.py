"""Shared deploy tail: quantize + shortlist + pack a student into a slimt bucket pack.

quantize_export.sh and shortlist.sh were run by hand after every pair; folding
them into the graph makes a run produce the three gzip'd bucket files plus the
custom_models.json metadata (uncompressed size + sha256) with no manual step.
Any flow imports build_pack() and hands it the trained model, the joint vocab,
the aligned corpus sides, and a FLORES source devtest.
"""

from __future__ import annotations

from pipe import deps
from pipe.artstore import StoredWrapper
from pipe.step import Ctx, Output, Run, StepDef, step
from pipe.target import Bigserver
from pipe.types import Artifact, Kind

BMT = "marian-bmt:next"
TOOLS = "/opt/tools"

# A lexical shortlist saturates well before the full KD corpus, and fast_align
# cost is ~tokens x EM-passes x 2 directions over the SUBWORD stream (subword
# fragmentation inflates the token count hard for scripts like Uyghur). The cap
# turns a ~2h build into ~15min on bigserver CPU; fast_align has no GPU build, so
# the sample is the lever, not the device.
SHORTLIST_MAX_LINES = 2_000_000


@step(
    image=BMT,
    target=Bigserver(cpus=2),
    script="quantize_export.sh",
    deps=deps.QUANTIZE,
    outputs={"model_bin": Output(rel="model.intgemm.alphas.bin", kind=Kind.BLOB)},
)
def quantize(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("model"), ctx.inp("vocab"), ctx.inp("devtest"), ctx.out_dir]


@step(
    image=BMT,
    target=Bigserver(cpus=16),
    script="shortlist.sh",
    outputs={"lex": Output(rel="lex.50.50.s2t.bin", kind=Kind.BLOB)},
)
def shortlist(ctx: Ctx) -> list[str]:
    return [
        ctx.script, ctx.inp("src"), ctx.inp("tgt"), ctx.inp("vocab"),
        ctx.out_dir, TOOLS, str(SHORTLIST_MAX_LINES),
    ]


def _pack_step(infix: str) -> StepDef:
    # infix is the compact pair tag in the bucket filenames (ugen / swen / entl).
    # It rides in the argv rather than an input, so it is also passed to run.do as
    # an arg to keep it in the step key.
    @step(
        image=BMT,
        target=Bigserver(cpus=2),
        script="pack_slimt.sh",
        outputs={
            "model_gz": Output(rel=f"model.{infix}.intgemm.alphas.bin.gz", kind=Kind.BLOB),
            "lex_gz": Output(rel=f"lex.50.50.{infix}.s2t.bin.gz", kind=Kind.BLOB),
            "vocab_gz": Output(rel=f"vocab.{infix}.spm.gz", kind=Kind.BLOB),
            "meta": Output(rel="meta.json", kind=Kind.BLOB),
        },
    )
    def pack(ctx: Ctx) -> list[str]:
        return [ctx.script, ctx.inp("model_bin"), ctx.inp("lex"), ctx.inp("vocab"), infix, ctx.out_dir]

    return pack


def build_pack(
    run: Run,
    *,
    model: Artifact,
    vocab: Artifact | StoredWrapper,
    src: Artifact,
    tgt: Artifact,
    devtest: Artifact | StoredWrapper,
    infix: str,
) -> dict[str, Artifact]:
    model_bin = run.do(quantize, timeout=3600, model=model, vocab=vocab, devtest=devtest)["model_bin"]
    lex = run.do(shortlist, timeout=3 * 3600, src=src, tgt=tgt, vocab=vocab)["lex"]
    return run.do(
        _pack_step(infix), timeout=1800, args={"infix": infix},
        model_bin=model_bin, lex=lex, vocab=vocab,
    )
