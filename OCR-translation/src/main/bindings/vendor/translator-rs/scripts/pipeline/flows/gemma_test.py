"""One-box 5-shot ug->en eval of pkupie/gemma-3-4b-ug-cpt on FLORES devtest.

We discarded the Gemma-CPT teacher on chrF/COMET being ~NLLB, but those metrics
under-penalize the entity/number errors that make NLLB unreliable; an LLM with
pretraining world-knowledge may have a better error PROFILE at the same score.
This runs it 5-shot on the SAME FLORES sentences the NLLB teacher was eyeballed
on, so the comparison is head-to-head. No SFT — just the CPT base, few-shot.

    pipe --run gemmatest put ex_ug   <5 FLORES dev uig lines>   --kind lines
    pipe --run gemmatest put ex_en   <5 FLORES dev eng lines>   --kind lines
    pipe --run gemmatest put test_ug <100 FLORES devtest uig>   --kind lines
    pipe --run gemmatest run gemma_test
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
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="gemma_fewshot.sh",
    deps=deps.GEMMA,
    outputs={"hyp": Output(rel="gemma_out.en", kind=Kind.LINES)},
)
def gemma(ctx: Ctx) -> list[str]:
    return [
        ctx.script, "pkupie/gemma-3-4b-ug-cpt",
        ctx.inp("ex_ug"), ctx.inp("ex_en"), ctx.inp("test_ug"),
        ctx.out("gemma_out.en"), "5", "100",
    ]


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    hyp = run.do(gemma, timeout=2 * 3600, ex_ug=a("ex_ug"), ex_en=a("ex_en"), test_ug=a("test_ug"))["hyp"]
    return {"hyp": hyp.to_json()}
