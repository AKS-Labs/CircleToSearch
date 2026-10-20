"""NLLB-600M + NLLB-1.3B on en<->tl: FLORES gate AND probe decode, one box.

Closes two gaps at once. (1) The NLLB tl rows in NOTES (600M 58.4/66.4, 1.3B
60.3/60.6) are chrF++ only, from July, never re-decoded — so they carry unknown
harness drift against the OPUS-MT and Hy-MT2 numbers measured 2026-07-20, and
tl->en is exactly where the three contenders sit within ~2 chrF of each other.
(2) tl->en's SHIPPED pack is distilled from NLLB-600M, not OPUS-MT, so the probe
comparison run on 2026-07-20 (OPUS vs Hy) said nothing about the incumbent in
that direction. NLLB was never probed.

Both sizes on one box, and the 1.3B weights prefetch in the background during the
600M compute — the download is the serial cost, not the decode (600 FLORES + 204
probe lines is minutes).

    pipe --run tlnllb put probes_en <probes/probes_tl.en> --kind lines
    pipe --run tlnllb put probes_tl <probes/probes_en.tl> --kind lines
    pipe --run tlnllb run tlnllb
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Vast
from pipe.types import Kind

NLLB = "ghcr.io/davidventura/offline-translator/nllb-ct2:1.3b"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


def _outputs() -> dict[str, Output]:
    out: dict[str, Output] = {}
    for tag in ("600m", "1.3b"):
        for direction, pair in (("en2tl", "eng_Latn-tgl_Latn"), ("tl2en", "tgl_Latn-eng_Latn")):
            out[f"probe_{tag}_{direction}"] = Output(
                rel=f"probe_{tag}.{direction}", kind=Kind.LINES)
            out[f"flores_{tag}_{direction}"] = Output(
                rel=f"flores_{tag}/{pair}.hyp", kind=Kind.LINES)
    return out


@step(
    image=NLLB,
    target=Vast(gpu="RTX_4090", max_hours=3, disk_gb=80, tries=8, geo=EU, min_cuda=12.1),
    script="nllb_all.sh",
    deps=deps.NLLB_ALL,
    outputs=_outputs(),
)
def nllb(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("probes_en"), ctx.inp("probes_tl"), ctx.out_dir]


def main(run: Run, argv: list[str]) -> dict:
    out = run.do(
        nllb,
        timeout=3 * 3600,
        probes_en=run.ledger.artifact("probes_en"),
        probes_tl=run.ledger.artifact("probes_tl"),
    )
    return {name: art.to_json() for name, art in out.items()}
