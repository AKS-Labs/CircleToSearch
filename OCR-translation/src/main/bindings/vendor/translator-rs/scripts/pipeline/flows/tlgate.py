"""Gate Hy-MT2-7B-FP8 as an en<->tl teacher on FLORES-200 devtest.

The shipped Tagalog packs distilled OPUS-MT (en->tl) and NLLB-600M (tl->en), and
the standing tl question is whether a better teacher moves the student — tl->en
sits ~5.7 chrF under its NLLB teacher, and NOTES' ranked levers put "a better
teacher" second only to human finetune supply. Hy-MT2-7B is the only model that
beat NLLB-1.3B on a gate so far (uig->en 51.6 vs 49.7), so this asks the same
question for tl before anything gets retrained.

FP8 (not the bf16 checkpoint hy_mt2_gate.py defaults to) because that is the
weight format the KD decode would actually run, so the gate measures the teacher
we would distill rather than a stronger one we would not.

The OPUS-MT half of the comparison is CPU-cheap and runs off-pipeline
(`opus_gate.py --pairs eng_Latn-tgl_Latn,tgl_Latn-eng_Latn`); both harnesses emit
the same chrF++/spBLEU line, so the numbers sit in one table.

The step DECODES; it does not score COMET. The hypotheses come back as artifacts
and chrf_score.py scores chrF++/spBLEU/COMET22 off-box, so both teachers are
measured by one scorer and unbabel-comet never enters the vLLM image.

    pipe --run tlgate run tlgate
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
PAIRS = "eng_Latn-tgl_Latn,tgl_Latn-eng_Latn"
MODEL = "tencent/Hy-MT2-7B-FP8"
LIMIT = "300"


@step(
    image=HYKD,
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="hy_gate.sh",
    deps=deps.HY_GATE,
    outputs={
        "scores": Output(rel="hy_tl_gate.txt", kind=Kind.LINES),
        "en_tl_hyp": Output(rel="hyp/eng_Latn-tgl_Latn.hyp", kind=Kind.LINES),
        "en_tl_src": Output(rel="hyp/eng_Latn-tgl_Latn.src", kind=Kind.LINES),
        "en_tl_ref": Output(rel="hyp/eng_Latn-tgl_Latn.ref", kind=Kind.LINES),
        "tl_en_hyp": Output(rel="hyp/tgl_Latn-eng_Latn.hyp", kind=Kind.LINES),
        "tl_en_src": Output(rel="hyp/tgl_Latn-eng_Latn.src", kind=Kind.LINES),
        "tl_en_ref": Output(rel="hyp/tgl_Latn-eng_Latn.ref", kind=Kind.LINES),
    },
)
def hy_gate(ctx: Ctx) -> list[str]:
    return [ctx.script, PAIRS, MODEL, LIMIT, ctx.out("hyp"), ctx.out("hy_tl_gate.txt")]


def main(run: Run, argv: list[str]) -> dict:
    out = run.do(hy_gate, timeout=2 * 3600)
    return {name: art.to_json() for name, art in out.items()}
