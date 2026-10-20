"""en->tl corpus v4: register-split prep + a KD draw to absolute per-register targets.

WHY A NEW CORPUS RATHER THAN A RETRAIN
The v3 student matches its teacher on FLORES (59.91 vs 59.58 chrF++ at n=300) and
still copies `Emergency Exit`, `Detour`, `Pull` and `Cash Only` through
untranslated. Root cause is coverage, not labels and not the teacher: `Right`,
`Pull` and `Free` occur ZERO times in the 10M lines it trained on, while `Detour`
and `Mind the gap` occur ONCE each and the TEACHER translated both correctly
(Paglihis / Ingatan ang agwat). The short band it did get was ~1.1M XLEnt entity
pairs — names, where passing through is right — so short input taught exactly the
wrong rule.

Filtering cannot fix that, and deleting the identity pairs would be actively
harmful: 50.8% of them are usernames and codes (SilkyCat3795) that MUST survive
verbatim. The fix is supply, and supply is what a proportional draw destroys.
en-tl has 63.5M NLLB lines against 23k translatewiki, so a uniform 10M sample
takes ~4k UI lines. Absolute targets take all 13k.

    pipe --run tlprep_v4 run tlprep_v4
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver
from pipe.types import Kind

PREP = "prep:next"
KD_LINES = "10000000"
SEED = "42"

# Caps, not ratios: `min(cap, available)` means Tagalog's 13k UI lines all pass
# and Swahili's 6k all pass, where a ratio would scale both to nothing. ENTITY is
# capped rather than dropped — XLEnt is what stops the student free-running on
# short input ("hallo" -> "Hello in the hello"), it was just never bounded, so it
# took the whole short band. CRAWL fills the remainder because it is the only
# register with more data than we can use.
MIX = "ui=50000,human=200000,dialogue=1000000,entity=150000,crawl=fill"

REGISTERS = ("human", "ui", "dialogue", "entity", "crawl")


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    # Without these only the six-line wrapper is digested, so a fix to the
    # cleaning chain or the register filters is served from the memo. That is not
    # hypothetical: the first v4 launch built a UI pool with a placeholder-regex
    # bug that corrupted `%language%` into "nguage%", and the fix touched no .sh.
    deps=deps.PREP,
    outputs={
        "pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES),
        "vocab": Output(rel="vocab.spm", kind=Kind.BLOB),
        **{f"pool_{r}": Output(rel=f"pool.{r}.tsv", kind=Kind.LINES) for r in REGISTERS},
    },
)
def prep(ctx: Ctx) -> list[str]:
    # train: a NEW joint vocab. The corpus changed — UI corpora are now included
    # and the entity register is filtered — so the v3 vocab was not trained on
    # the text this student will see.
    return [ctx.script, "tl", "16", ctx.out_dir, "en", "train"]


@step(
    image=PREP,
    target=Bigserver(cpus=8),
    script="kd_mix_step.sh",
    deps=deps.KD_MIX,
    outputs={
        "kd_src": Output(rel="kd_src", kind=Kind.LINES),
        "kd_ref": Output(rel="kd_ref", kind=Kind.LINES),
        "mix": Output(rel="mix.json", kind=Kind.BLOB),
    },
)
def kd_mix(ctx: Ctx) -> list[str]:
    # column 1 (en) is the en->tl KD source, column 2 the per-line reference.
    return [
        ctx.script, ctx.out_dir, "1", KD_LINES, MIX, SEED,
        *(ctx.inp(f"pool_{r}") for r in REGISTERS),
    ]


def main(run: Run, argv: list[str]) -> dict:
    prepped = run.do(prep, timeout=6 * 3600)
    drawn = run.do(
        kd_mix, timeout=2 * 3600,
        # The mix rides in the step key: it is argv, and argv is not hashed, so
        # without this a changed mix would reuse a corpus drawn under the old one
        # (the tlkd/tl2gpu collision, which cost a whole training run).
        args={"mix": MIX, "total": KD_LINES, "seed": SEED},
        **{f"pool_{r}": prepped[f"pool_{r}"] for r in REGISTERS},
    )
    return {
        "pool_tsv": prepped["pool_tsv"].to_json(),
        "vocab": prepped["vocab"].to_json(),
        "kd_src": drawn["kd_src"].to_json(),
        "kd_ref": drawn["kd_ref"].to_json(),
        "mix": drawn["mix"].to_json(),
        **{f"pool_{r}": prepped[f"pool_{r}"].to_json() for r in REGISTERS},
    }
