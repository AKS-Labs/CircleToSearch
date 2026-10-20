"""One-box NLLB-1.3B FLORES devtest teacher decode (uig->en).

The KD floor/ceiling reference: the NLLB-1.3B teacher was never FLORES-decoded and
saved (its uig run crashed at train), so this produces its devtest hypothesis for a
head-to-head chrF++/spBLEU/COMET against the Hy teacher + student. GPU, not the hub
CPU (int8 NLLB-1.3B on CPU is too slow to be worth it).

    pipe --run nllbt put src <FLORES devtest uig side> --kind lines
    pipe --run nllbt run nllb_teacher
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
    target=Vast(gpu="RTX_4090", max_hours=1, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="nllb_flores.sh",
    deps=deps.KD_DECODE,
    outputs={"hyp": Output(rel="nllb_teacher.en", kind=Kind.LINES)},
)
def teacher(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("src"), ctx.out("nllb_teacher.en")]


def main(run: Run, argv: list[str]) -> dict:
    hyp = run.do(teacher, timeout=3600, src=run.ledger.artifact("src"))["hyp"]
    return {"hyp": hyp.to_json()}
