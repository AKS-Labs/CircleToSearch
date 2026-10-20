"""en->tl KD decode with Hy-MT2-7B-FP8: split -> K boxes -> gather -> drop_empty.

The teacher was chosen on measured evidence, not on FLORES: Hy-MT2 beats the
shipped OPUS-MT teacher by +3.80 COMET on FLORES and by +8.40 on the check set,
and it is the only model of the four gated that does not degrade on
deployment-shaped input (chrF +0.64 FLORES->check, against -11.15 for OPUS-MT).

K=5 shards: the sweet spot is 1-6 boxes, because per-box setup (dead hosts, HF
pulls, slow uploads) stops paying past that.

EU-only is load-bearing rather than a preference — a US box's peering to the EU
hub / HF measured ~1KB/s, and the inet_down filter does not catch it because it
measures peak to a speedtest server, not the path that matters.

    pipe --run tlkd run tlkd
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver, Vast
from pipe.types import Kind

HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
BMT = "marian-bmt:next"
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
K = 5
TARGET = "Filipino"


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="split_kd.sh",
    outputs={f"shard_{i:02d}": Output(rel=f"shard_{i:02d}", kind=Kind.LINES)
             for i in range(K)},
)
def split_kd(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), str(K), ctx.out_dir]


@step(
    image=HYKD,
    target=Vast(gpu="RTX_4090", max_hours=6, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="vllm_kd.sh",
    # vllm_kd.sh is an argv adapter; vllm_kd.py is what decides how the teacher is
    # prompted and batched. Without it in the key, a change to the decode would be
    # served from the memo (see pipe.step.step docstring).
    deps=deps.VLLM_KD,
    outputs={"targets": Output(rel="targets", kind=Kind.LINES)},
)
def kd_decode(ctx: Ctx) -> list[str]:
    # LIMIT=0 = the whole shard. Order is preserved, so gather cats by index and
    # the gather-side line-count assert is what catches a short shard.
    return [ctx.script, ctx.inp("kd_src"), ctx.out("targets"), TARGET, "0"]


@step(
    image=BMT,
    target=Bigserver(cpus=2),
    script="gather_cat.sh",
    outputs={"kd_tgt": Output(rel="kd_tgt", kind=Kind.LINES)},
)
def gather(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.out("kd_tgt")] + [ctx.inp(f"part_{i:02d}") for i in range(K)]


@step(
    image=BMT,
    target=Bigserver(cpus=2),
    script="drop_empty_pairs.sh",
    outputs={
        "src": Output(rel="src", kind=Kind.LINES),
        "tgt": Output(rel="tgt", kind=Kind.LINES),
    },
)
def drop_empty(ctx: Ctx) -> list[str]:
    # Zero-token pairs send fast_align's EM to nan, which silently empties the
    # ENTIRE reverse pass (the align_ensw defect). Cheap, and not optional.
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_tgt"), ctx.out_dir]


def main(run: Run, argv: list[str]) -> dict:
    kd_src = run.ledger.artifact("kd_src")

    shards = run.do(split_kd, timeout=3600, kd_src=kd_src)
    total = sum(shards[f"shard_{i:02d}"].lines for i in range(K))
    if total != kd_src.lines:
        raise RuntimeError(f"split_kd lost lines: {kd_src.lines} -> {total}")

    # K boxes concurrently; a failed shard reruns alone on the next attempt.
    with ThreadPoolExecutor(max_workers=K) as ex:
        futures = [
            ex.submit(run.do, kd_decode, timeout=6 * 3600,
                      kd_src=shards[f"shard_{i:02d}"])
            for i in range(K)
        ]
        parts = {f"part_{i:02d}": f.result()["targets"] for i, f in enumerate(futures)}

    kd_tgt = run.do(gather, timeout=3600, **parts)["kd_tgt"]
    if kd_tgt.lines != kd_src.lines:
        raise RuntimeError(f"gather line mismatch: {kd_src.lines} -> {kd_tgt.lines}")

    pairs = run.do(drop_empty, timeout=3600, kd_src=kd_src, kd_tgt=kd_tgt)
    return {"kd_tgt": kd_tgt.to_json(),
            "src": pairs["src"].to_json(), "tgt": pairs["tgt"].to_json()}
