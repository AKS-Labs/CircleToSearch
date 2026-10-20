"""Decode the en<->tl PROBE set with Hy-MT2-7B-FP8 on one box.

FLORES said en->tl was a tie (chrF++ 59.69 OPUS vs 59.58 Hy) while COMET said it
was not (82.62 vs 86.42). Neither settles the question that decides a teacher
swap: does the model mangle entities, numbers and units, and does it hold register
on text that is not clean news. FLORES is clean news, so it cannot answer that —
the Swahili brittleness thread found the same thing (chrF 59.7 hid "rain of rain"
for rainbow) and needed a hand-built probe set to see it.

So: ~100 short/subtitle/dialogue/informal/technical/sign/menu lines per direction,
seeded with entities, dosages, pressures, times and prices. No references — the
output is READ, not scored.

    pipe --run tlprobe put probes_en <probes/probes_tl.en> --kind lines
    pipe --run tlprobe put probes_tl <probes/probes_en.tl> --kind lines
    pipe --run tlprobe run tlprobe
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
MODEL = "tencent/Hy-MT2-7B-FP8"


@step(
    image=HYKD,
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="probe_gate.sh",
    deps=deps.PROBE_GATE,
    outputs={
        "en2tl": Output(rel="probe/en2tl.hyp", kind=Kind.LINES),
        "tl2en": Output(rel="probe/tl2en.hyp", kind=Kind.LINES),
    },
)
def probe(ctx: Ctx) -> list[str]:
    return [ctx.script, MODEL, ctx.inp("probes_en"), ctx.inp("probes_tl"), ctx.out("probe")]


def main(run: Run, argv: list[str]) -> dict:
    out = run.do(
        probe,
        timeout=2 * 3600,
        probes_en=run.ledger.artifact("probes_en"),
        probes_tl=run.ledger.artifact("probes_tl"),
    )
    return {name: art.to_json() for name, art in out.items()}
