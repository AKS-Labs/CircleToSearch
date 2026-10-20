"""en->tl student training + both eval decodes on one box, then scoring off-box.

One step for train + decode(FLORES) + decode(check set): pipe leases per step key,
so splitting them rents a box per phase and ships a checkpoint between them, when
the decodes are minutes on a GPU already in hand. train_eval.sh decodes the
best-ce checkpoint, not the last one, or the numbers describe a model nobody
would ship.

Scoring runs on Bigserver via the shared eval_score, so this pair's numbers come
from the same code as every other pair's and are comparable across rounds.

NO FINETUNE. It bought +2.87 chrF on tl and +1.3 on sw, always measured on
FLORES, and its effect on deployment-shaped input has never been measured for any
pair — plausibly negative, since it trains on curated human bitext (full
sentences), the same pressure that makes a model good at FLORES and bad at signs.
It is an experiment to run against this baseline and compare check deltas.

    pipe --run tlkd run tltrain
"""

from __future__ import annotations

from pipe import evalsteps
from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:fp16-1a743582"
# Validated on a 4090, 2026-07-21. The old pin (e8a1a25, Dec 2021) could not
# train this student in fp16 AT ALL — guided-alignment aborted with
# "Child 1 has different type (first: float32 != child: float16)" — and
# --workspace >12000 aborted in mini-batch-fit's probe on a 2^31 overflow.
# 1a743582 fixes both, and inherits sparse guided alignment (4b51dcbd).
#
#   old pin, fp32                142k w/s
#   new pin, fp32                161k w/s   (+13.6%, sparse alignment alone)
#   new pin, fp16 ws12000        211k w/s   (1.50x over the old baseline)
#
# Guided alignment stays ON: it produces the alignment head, and when align_ensw
# degraded it the note was "chrF unaffected; bold/format transfer is what
# suffers" — a shipped app feature chrF would not have caught.
#
# ONE GPU. 2-GPU measured 1.54x throughput at 1.93x price (54-58% util per card),
# failing NOTES' gate of >80% each and 1.7-1.9x. The starvation is marian's own
# reader, not the OpusTrainer pipe — that leg ran on --train-sets directly — so
# materializing the augmented TSV would not rescue it either.
#
# Validated end to end: npz -> browsermt marian-conv -> slimt loads and
# translates, 31,561,697 B (the shipped packs are 31,561,635 B).
MARIAN_EXTRA = "--fp16 --workspace 12000 --mini-batch 4000"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=12, disk_gb=80, tries=8, geo=EU, min_cuda=11.8),
    script="train_eval.sh",
    deps=deps.TRAIN_EVAL,
    outputs={
        "model": Output(rel="model.npz.best-ce-mean-words.npz", kind=Kind.BLOB),
        "flores_hyp": Output(rel="flores.hyp", kind=Kind.LINES),
        "check_hyp": Output(rel="check.hyp", kind=Kind.LINES),
    },
)
def train_eval(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("train_tsv"), ctx.inp("vocab"), ctx.inp("valid"),
        ctx.inp("flores_src"), ctx.inp("check_src"),
        ctx.out_dir, "0", MARIAN_EXTRA,
    ]


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    trained = run.do(
        train_eval, timeout=12 * 3600,
        # The step key hashes name/image/script/inputs/args — NOT the argv the
        # step function returns (ctx.out_dir depends on the key, so argv cannot).
        # Without this, a flags-only change is invisible: tlkd and tl2gpu hashed
        # to the SAME key despite different --devices and --mini-batch-words-ref,
        # so they shared an output dir and a `done` record would have memoized the
        # new config away entirely. flows/pack.py already does this with "infix".
        args={"devices": "0", "marian_extra": MARIAN_EXTRA},
        train_tsv=a("train_tsv"), vocab=a("vocab"), valid=a("valid"),
        flores_src=a("flores_src"), check_src=a("check_src"),
    )
    scored = run.do(
        evalsteps.eval_score, timeout=3600,
        flores_hyp=trained["flores_hyp"], flores_ref=a("flores_ref"),
        flores_src=a("flores_src"),
        check_hyp=trained["check_hyp"], check_src=a("check_src"),
        check_ref=a("check_ref"),
    )
    return {
        "model": trained["model"].to_json(),
        "metrics": scored["metrics"].to_json(),
        "review": scored["review"].to_json(),
    }
