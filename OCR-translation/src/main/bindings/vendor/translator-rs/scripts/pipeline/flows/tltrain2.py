"""en->tl training on TWO GPUs, raced against the single-GPU run.

The earlier 2-GPU measurement (1.54x, 54-58% util per card) used a 200k-line head
slice, and that slice is NOT representative: it gives 65% util on one GPU where
the real corpus gives 91%. Shorter, less varied lines fit smaller batches, so the
probe was more launch-bound and more reader-sensitive than reality — precisely
the conditions that make a second card look bad. So the 1.54x is probably
pessimistic, and this re-tests on the real corpus.

COST IS NOT THE POINT and never was: a 2-GPU box is ~1.93x the price, so even
perfect linear scaling is only ~3% better value. The point is WALL TIME — 6.5h
against ~3.6h at 1.8x, for about the same $2.50 — which doubles how many
experiments fit in a day.

Reader headroom, from the live single-GPU run: marian ~3 cores and OpusTrainer
22.5% of one at 215k w/s, on a box with 255 cores. Feeding ~430k w/s wants ~6
cores and ~45%. Nothing there predicts starvation.

Separate run id from `tlkd` on purpose: the ledger delta-merge fix is synced but
the running single-GPU runner still holds the OLD code in memory, so sharing a
ledger would re-run the lost-update race it fixes.

    pipe --run tl2gpu run tltrain2
"""

from __future__ import annotations

from pipe import deps, evalsteps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:fp16-1a743582"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")

# --sync-sgd is what makes the second card data-parallel rather than idle; the
# rest matches the single-GPU run so the only variable is the GPU count.
# --mini-batch-words-ref corrects a DOUBLE penalty measured on the first 2-GPU
# attempt (2026-07-21). At equal data (~29M sentences) it reached ce 4.095 where
# the 1-GPU run reached 2.382, because sync-sgd doubles the effective batch, so
# at equal data the run had taken HALF the updates AND sat at HALF the learning
# rate — lr-warmup is indexed in UPDATES (16000), so it was only halfway through
# a ramp the 1-GPU run had twice completed. Fewer steps x smaller steps is ~4x
# less parameter movement per sentence, which cancelled the 1.96x throughput.
#
# marian scales learn-rate/optimizer-params/exponential-smoothing by
# mbSize/refMBWords (OptimizerBase::update), where mbSize is words per UPDATE —
# accumulated across optimizer-delay AND GPUs. 53752 is the 1-GPU measurement
# (212,157 w/s x 253.36s / 1000 updates), so the 2-GPU batch scores a ratio of 2
# and gets 2x lr, including during warmup since the ramp targets the scaled rate.
#
# DO NOT verify the correction by the logged L.r. — it does NOT show the scaling.
# marian logs the SCHEDULED rate (warmup x decay x base = 0.0003 x 1000/16000 =
# 1.875e-05 at Up.1000) and applies mbSize/refMBWords INSIDE the optimizer,
# invisible to the log. The successful tl2gpu run and the 1-GPU control both
# logged 1.8750e-05 at Up.1000; an earlier comment here claimed this run "must
# show 3.75e-05", which is FALSE and cost a false alarm on the v4 launch.
# The correction shows up in the CE trajectory, not the lr: uncorrected 2-GPU hit
# ce 4.095 at Up.5000, corrected hit 3.126. Verify there, or just confirm the
# marian command carries `--mini-batch-words-ref 53752 --sync-sgd`.
MARIAN_EXTRA = ("--fp16 --workspace 12000 --mini-batch 4000 --sync-sgd "
                "--mini-batch-words-ref 53752")
DEVICES = "0 1"


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", num_gpus=2, max_hours=12, disk_gb=80,
                tries=8, geo=EU, min_cuda=11.8),
    script="train_eval.sh",
    # The training behaviour lives in train_student.sh and the three configs, not
    # in the six-line train_eval.sh wrapper. Without them in the key, the
    # early-stopping-epsilon fix (which aborted the v4 launch) would re-run from
    # the memo with the broken config; and any future config edit would be
    # invisible. This is the config-not-in-key gap, closed for this step.
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
        ctx.out_dir, DEVICES, MARIAN_EXTRA,
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
        args={"devices": DEVICES, "marian_extra": MARIAN_EXTRA},
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
