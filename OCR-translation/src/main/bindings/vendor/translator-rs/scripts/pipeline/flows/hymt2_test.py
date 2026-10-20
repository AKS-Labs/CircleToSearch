"""One-box zero-shot ug->en eval of tencent/Hy-MT2-7B on FLORES devtest.

Hy-MT2-7B was the only model to BEAT NLLB-1.3B on the uig->en gate (51.6 vs
49.7 chrF++), so unlike Gemma its higher score may reflect real fidelity gain
rather than surface. This runs it on the SAME 100 FLORES sentences the NLLB
teacher was eyeballed on, saving outputs for a head-to-head.

    pipe --run hymt2 put test_ug <100 FLORES devtest uig>  --kind lines
    pipe --run hymt2 run hymt2_test
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
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=80, tries=8, geo=EU, min_cuda=12.1),
    script="hy_mt2.sh",
    deps=deps.HY_MT2,
    outputs={"hyp": Output(rel="hymt2_out.en", kind=Kind.LINES)},
)
def hymt2(ctx: Ctx) -> list[str]:
    return [ctx.script, "tencent/Hy-MT2-7B", ctx.inp("test_ug"), ctx.out("hymt2_out.en"), "100"]


def main(run: Run, argv: list[str]) -> dict:
    hyp = run.do(hymt2, timeout=2 * 3600, test_ug=run.ledger.artifact("test_ug"))["hyp"]
    return {"hyp": hyp.to_json()}
