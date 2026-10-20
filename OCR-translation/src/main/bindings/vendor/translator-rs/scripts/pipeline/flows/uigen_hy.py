"""uig→en pipeline-v3 with the Hy-MT2-7B-FP8 teacher (replaces NLLB-1.3B).

Same graph as uigen_v3, one teacher swap. Hy-MT2-7B-FP8 beat NLLB on the uig→en
gate (chrF++ 51.6 / COMET 87.7 vs 49 / 85.9) AND fixed the entity errors that made
NLLB gist-usable-but-not-trustworthy, while staying faithful/literal enough to
distill into a 40M student (validated 2026-07-16; see hy_kd_plan.md).

    prep ─┬─ kd_source ─┬─ split_kd ─ kd_decode ×K ─ gather ─┐
          │             │                                     │
          │             └─ bicleaner_score ─┬─ build_human ─┬─ align_ft ─┐
          └─ prep_ft ─────────────────────────────────────┘            │
                                                            └─ backward ─┤
     ─ drop_empty ─ align ─ cefilter_score ─ cefilter_cut ─ train ─ finetune ─ decode

What differs from uigen_v3, and only that:
- kd_decode runs Hy-MT2 under vLLM (hy-kd image, vllm_decode.sh), **1-best greedy**
  — extract-best is near-dead for uig (3.2% gate), so n-best buys nothing and
  1-best halves the generated tokens. The shard output IS the target directly.
- The model is pulled per-box from HF (HY_MODEL baked in the image). map_step's
  per-shard retry + `tries=8` cover a flaky HF pull; a US box's ~1KB/s EU/HF
  peering is why this stays EU-only.
- gather is a plain ordered cat (no n-best, no select_best) → kd_tgt.

Everything teacher-INDEPENDENT is byte-identical to uigen_v3 — same step names,
images, scripts and inputs — so running this under the existing `uigen` run
memoizes prep / prep_ft / kd_source / split_kd / bicleaner_score / build_human /
align_ft / backward to no-ops, and only the teacher-dependent tail re-runs:

    pipe --run uigen run uigen_hy prep   # gate + backward, before committing 5 boxes
    pipe --run uigen run uigen_hy        # the whole thing

A fresh run id instead recomputes the upstream from OPUS (the seed lines in
uigen_v3's docstring still apply: put valid + devtest_src first).
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver, Vast
from pipe.types import Artifact, Kind

TOOLS = "/opt/tools"
CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:e8a1a25"
HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
BICLEANER = "ghcr.io/davidventura/offline-translator/bicleaner:en-xx"
BMT = "marian-bmt:next"
PREP = "prep:next"
K = 5
# EU-only is load-bearing, not just this run's preference: a US box's peering to
# the EU hub / HF was ~1KB/s in validation, and the `inet_down` filter does NOT
# catch it (it measures peak to a speedtest server, not the path that matters).
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
CURATED = "TED2020,Tatoeba,QED,wikimedia"
SALVAGE_MIN = "0.8"


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    deps=deps.PREP,
    outputs={
        "pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES),
        "vocab": Output(rel="vocab.spm", kind=Kind.BLOB),
    },
)
def prep(ctx: Ctx) -> list[str]:
    return [ctx.script, "ug", "16", ctx.out_dir, "en", "train"]


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    deps=deps.PREP,
    outputs={"pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES)},
)
def prep_ft(ctx: Ctx) -> list[str]:
    return [ctx.script, "ug", "16", ctx.out_dir, "en", "reuse", CURATED]


@step(
    image=BMT,
    target=Bigserver(cpus=8),
    script="build_kd_source.sh",
    outputs={
        "kd_src": Output(rel="kd_src", kind=Kind.LINES),
        "kd_ref": Output(rel="kd_ref", kind=Kind.LINES),
    },
)
def kd_source(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("pool_tsv"), "2", ctx.out_dir]


@step(
    image=BICLEANER,
    target=Vast(gpu="RTX_4090", max_hours=5, disk_gb=40, tries=8, geo=EU, min_cuda=12.1),
    script="bicleaner_score.sh",
    outputs={"scores": Output(rel="gate_scores", kind=Kind.LINES)},
)
def bicleaner_score(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_ref"), "ug", "en", ctx.out("gate_scores")]


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="build_human_bitext.sh",
    outputs={
        "human_src": Output(rel="human_src", kind=Kind.LINES),
        "human_tgt": Output(rel="human_tgt", kind=Kind.LINES),
    },
)
def build_human(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("kd_src"), ctx.inp("kd_ref"), ctx.inp("gate_scores"), SALVAGE_MIN,
        ctx.inp("ft_pool"), ctx.out_dir,
    ]


@step(
    image=BMT,
    target=Bigserver(cpus=16),
    script="align.sh",
    outputs={"train_tsv": Output(rel="train.tsv", kind=Kind.LINES)},
)
def align_ft(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("human_src"), ctx.inp("human_tgt"), ctx.out_dir, TOOLS]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=3, disk_gb=40, tries=8, geo=EU, min_cuda=11.8),
    script="backward_step.sh",
    deps=deps.BACKWARD,
    outputs={"model": Output(rel="backward/model.npz.best-ce-mean-words.npz", kind=Kind.BLOB)},
)
def backward(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("human_tgt"), ctx.inp("human_src"), ctx.inp("vocab"), ctx.inp("valid"),
        ctx.out("backward/model.npz"),
    ]


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="split_kd.sh",
    outputs={f"shard_{i:02d}": Output(rel=f"shard_{i:02d}", kind=Kind.LINES) for i in range(K)},
)
def split_kd(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), str(K), ctx.out_dir]


@step(
    image=HYKD,
    # min_cuda 12.1: the cu129 image is CUDA 12.9, minor-version-compatible down to
    # driver ~525, so it rents on nearly the whole 4090 pool. Validated 2026-07-16 at
    # 124.5 l/s on an EU 4090 rented at this floor.
    target=Vast(gpu="RTX_4090", max_hours=4, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="vllm_decode.sh",
    outputs={"targets": Output(rel="targets", kind=Kind.LINES)},
)
def kd_decode(ctx: Ctx) -> list[str]:
    # 1-best greedy (LIMIT=0 = whole shard). vllm_decode preserves input order, so
    # gather can cat by shard index. The output IS the target — no n-best, no
    # select_best. The gather-side line-count assert is what catches a short shard.
    return [ctx.script, ctx.inp("kd_src"), ctx.out("targets"), "0"]


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
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_tgt"), ctx.out_dir]


@step(
    image=BMT,
    target=Bigserver(cpus=16),
    script="align.sh",
    outputs={"train_tsv": Output(rel="train.tsv", kind=Kind.LINES)},
)
def align(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_tgt"), ctx.out_dir, TOOLS]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=3, disk_gb=40, tries=8, geo=EU, min_cuda=11.8),
    script="cefilter_score.sh",
    outputs={"scores": Output(rel="scores", kind=Kind.LINES)},
)
def cefilter_score(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("backward_npz"), ctx.inp("vocab"),
        ctx.inp("kd_tgt"), ctx.inp("kd_src"),
        ctx.out("scores"),
    ]


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="cefilter_cut.sh",
    outputs={"train_tsv": Output(rel="train.filtered.tsv", kind=Kind.LINES)},
)
def cefilter_cut(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("scores"), ctx.inp("train_tsv"), ctx.out("train.filtered.tsv"), "5"]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=8, disk_gb=60, tries=8, geo=EU, min_cuda=11.8),
    script="train_student.sh",
    deps=deps.TRAIN_STUDENT,
    outputs={"model": Output(rel="model/model.npz.best-ce-mean-words.npz", kind=Kind.BLOB)},
)
def train(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("train_tsv"), ctx.inp("vocab"), ctx.inp("valid"),
        ctx.out("model/model.npz"), "0",
    ]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=3, disk_gb=60, tries=8, geo=EU, min_cuda=11.8),
    script="finetune_student.sh",
    deps=deps.FINETUNE,
    outputs={"model": Output(rel="model_ft/model.npz.best-ce-mean-words.npz", kind=Kind.BLOB)},
)
def finetune(ctx: Ctx) -> list[str]:
    return [
        ctx.script,
        ctx.inp("ft_tsv"), ctx.inp("vocab"), ctx.inp("pretrained"), ctx.inp("valid"),
        ctx.out("model_ft/model.npz"), "0",
    ]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=1, disk_gb=40, tries=8, geo=EU, min_cuda=11.8),
    script="decode_flores.sh",
    outputs={"hyp": Output(rel="hyp.txt", kind=Kind.LINES)},
)
def decode(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("model"), ctx.inp("vocab"), ctx.inp("devtest_src"), ctx.out("hyp.txt")]


def _blank_lines(art: Artifact) -> int:
    # vllm_decode drops whitespace-only input lines, so a blank in kd_src would
    # make the shard output shorter than its input and silently desync the KD
    # targets from the source. Caught here on the hub, before a single box rents.
    with Path(art.path).open("r", encoding="utf-8") as f:
        return sum(1 for line in f if not line.strip())


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    stage = argv[0] if argv else "all"
    if stage not in ("prep", "all"):
        raise SystemExit(f"unknown stage {stage!r}; want 'prep' or nothing")

    p = run.do(prep, timeout=4 * 3600)
    pool, vocab = p["pool_tsv"], p["vocab"]

    kd = run.do(kd_source, timeout=3600, pool_tsv=pool)
    kd_src, kd_ref = kd["kd_src"], kd["kd_ref"]
    if kd_src.lines != kd_ref.lines:
        raise RuntimeError(f"kd_src {kd_src.lines} != kd_ref {kd_ref.lines} lines")
    blanks = _blank_lines(kd_src)
    if blanks:
        raise RuntimeError(
            f"kd_src has {blanks} blank line(s): vllm_decode would drop them and "
            f"desync the KD targets. Fix build_kd_source before the decode."
        )

    shards = run.do(split_kd, timeout=1800, kd_src=kd_src)
    if sum(s.lines or 0 for s in shards.values()) != kd_src.lines:
        raise RuntimeError("split_kd lost lines across shards")

    ft_pool = run.do(prep_ft, timeout=2 * 3600)["pool_tsv"]

    # The gate and the K decode shards are independent until gather: one box each,
    # all concurrent. A failed shard reruns alone on the flow's next attempt. In
    # the prep stage the decode is deliberately not launched — the gate output is
    # inspected (distribution, salvage yield) before committing to the decode.
    with ThreadPoolExecutor(max_workers=K + 1) as ex:
        fgate = ex.submit(
            run.do, bicleaner_score, timeout=5 * 3600, kd_src=kd_src, kd_ref=kd_ref
        )
        fdec = [
            ex.submit(run.do, kd_decode, timeout=6 * 3600, kd_src=shards[f"shard_{i:02d}"])
            for i in range(K)
        ] if stage == "all" else []
        gates = fgate.result()["scores"]
        parts = {f"part_{i:02d}": f.result()["targets"] for i, f in enumerate(fdec)}
    if gates.lines != kd_src.lines:
        raise RuntimeError(f"bicleaner dropped lines: {kd_src.lines} -> {gates.lines}")

    human = run.do(
        build_human, timeout=3600,
        kd_src=kd_src, kd_ref=kd_ref, gate_scores=gates, ft_pool=ft_pool,
    )
    ft_tsv = run.do(
        align_ft, timeout=3600, human_src=human["human_src"], human_tgt=human["human_tgt"]
    )["train_tsv"]
    backward_npz = run.do(
        backward, timeout=3 * 3600,
        human_tgt=human["human_tgt"], human_src=human["human_src"],
        vocab=vocab, valid=a("valid"),
    )["model"]

    if stage == "prep":
        return {
            "pool": pool.to_json(), "vocab": vocab.to_json(),
            "kd_src": kd_src.to_json(), "gate_scores": gates.to_json(),
            "ft_tsv": ft_tsv.to_json(), "backward_npz": backward_npz.to_json(),
        }

    kd_tgt = run.do(gather, timeout=1800, **parts)["kd_tgt"]
    if kd_tgt.lines != kd_src.lines:
        raise RuntimeError(f"decode dropped lines: {kd_src.lines} -> {kd_tgt.lines}")

    before = kd_src.lines or 0
    clean = run.do(drop_empty, timeout=1800, kd_src=kd_src, kd_tgt=kd_tgt)
    kd_src, kd_tgt = clean["src"], clean["tgt"]
    if kd_src.lines != kd_tgt.lines:
        raise RuntimeError("drop_empty desynced src/tgt")
    if before - (kd_src.lines or 0) > 1000:
        raise RuntimeError(f"drop_empty removed too many pairs: {before} -> {kd_src.lines}")

    aligned = run.do(align, timeout=3600, kd_src=kd_src, kd_tgt=kd_tgt)["train_tsv"]
    if aligned.lines != kd_src.lines:
        raise RuntimeError(f"align dropped lines: {kd_src.lines} -> {aligned.lines}")

    scores = run.do(
        cefilter_score, timeout=3 * 3600,
        backward_npz=backward_npz, vocab=vocab, kd_tgt=kd_tgt, kd_src=kd_src,
    )["scores"]
    if scores.lines != kd_src.lines:
        raise RuntimeError(f"scorer dropped lines: {kd_src.lines} -> {scores.lines}")

    filtered = run.do(cefilter_cut, timeout=3600, scores=scores, train_tsv=aligned)["train_tsv"]

    model = run.do(train, timeout=8 * 3600, train_tsv=filtered, vocab=vocab, valid=a("valid"))["model"]
    model_ft = run.do(
        finetune, timeout=3 * 3600,
        ft_tsv=ft_tsv, vocab=vocab, pretrained=model, valid=a("valid"),
    )["model"]
    hyp = run.do(
        decode, timeout=3600, model=model_ft, vocab=vocab, devtest_src=a("devtest_src")
    )["hyp"]
    return {"model_ft": model_ft.to_json(), "hyp": hyp.to_json()}
