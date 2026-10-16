"""sw→en pipeline-v3, end to end from the cleaned pool:

kd_source → [bicleaner gate ∥ 5-way sharded KD decode] → gather → extract-best →
align → ce-filter → train → finetune → decode. The gate and the decode shards
run concurrently (selection needs the gate only after decode), each shard on its
own box with its own lease, so a dead box retries that shard alone.

Seed once, then run:

    pipe --run swenv3 put pool_tsv <2-col en\\tsw cleaned pool>
    pipe --run swenv3 put vocab    <joint .spm>            --kind blob
    pipe --run swenv3 put valid    <2-col src\\ttrg tsv>
    pipe --run swenv3 put ft_tsv   <3-col finetune tsv>
    pipe --run swenv3 put backward_npz <en→sw backward RNN> --kind blob
    pipe --run swenv3 put devtest_src  <FLORES devtest source side>
    pipe --run swenv3 run swen_v3
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver, Vast
from pipe.types import Kind

TOOLS = "/opt/tools"
CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:e8a1a25"
NLLB = "ghcr.io/davidventura/offline-translator/nllb-ct2:600m"
BICLEANER = "ghcr.io/davidventura/offline-translator/bicleaner:en-xx"
BMT = "marian-bmt:next"
K = 5
# EU hosts have empirically behaved better than the cheapest US/SE offers
# (2026-07-15: dud Quebec pair, SE box stuck in a pull-retry loop). Not policy,
# just this run's preference — drop if it thins the offer pool too much.
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
PREP = "prep:next"


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    deps=deps.PREP,
    outputs={"pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES)},
)
def prep(ctx: Ctx) -> list[str]:
    return [ctx.script, "sw", "16", ctx.out_dir, "en"]


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
    # pool is en\tsw; column 2 (sw) becomes the sw→en KD source, column 1 the ref
    return [ctx.script, ctx.inp("pool_tsv"), "2", ctx.out_dir]


@step(
    image=BICLEANER,
    target=Vast(gpu="RTX_4090", max_hours=4, disk_gb=40, tries=8, geo=EU, min_cuda=12.1),
    script="bicleaner_score.sh",
    outputs={"scores": Output(rel="gate_scores", kind=Kind.LINES)},
)
def bicleaner_score(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), ctx.inp("kd_ref"), "sw", "en", ctx.out("gate_scores")]


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="split_kd.sh",
    outputs={f"shard_{i:02d}": Output(rel=f"shard_{i:02d}", kind=Kind.LINES) for i in range(K)},
)
def split_kd(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), str(K), ctx.out_dir]


@step(
    image=NLLB,
    target=Vast(gpu="RTX_4090", max_hours=5, disk_gb=40, tries=8, geo=EU, min_cuda=12.1),
    script="kd_decode.sh",
    deps=deps.KD_DECODE,
    outputs={"nbest": Output(rel="nbest.tsv.gz", kind=Kind.BLOB)},
)
def kd_decode(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), "swh_Latn", "eng_Latn", ctx.out("nbest.tsv.gz")]


@step(
    image=BMT,
    target=Bigserver(cpus=2),
    script="gather_cat.sh",
    outputs={"nbest": Output(rel="nbest.tsv.gz", kind=Kind.BLOB)},
)
def gather_nbest(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.out("nbest.tsv.gz")] + [ctx.inp(f"part_{i:02d}") for i in range(K)]


@step(
    image=NLLB,
    target=Bigserver(cpus=16),
    script="select_best.sh",
    deps=deps.SELECT_BEST,
    outputs={"kd_sel": Output(rel="kd_sel", kind=Kind.LINES)},
)
def select_best(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("nbest"), ctx.inp("kd_ref"), ctx.inp("gate_scores"), ctx.out("kd_sel"), "16"]


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
    target=Vast(gpu="RTX_4090", max_hours=2, disk_gb=40, tries=8, geo=EU, min_cuda=11.8),
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


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact

    # A seeded pool (the swenv3 run) wins; a fresh run preps its own.
    try:
        pool = a("pool_tsv")
    except KeyError:
        pool = run.do(prep, timeout=3 * 3600)["pool_tsv"]

    kd = run.do(kd_source, timeout=3600, pool_tsv=pool)
    kd_src, kd_ref = kd["kd_src"], kd["kd_ref"]
    if kd_src.lines != kd_ref.lines:
        raise RuntimeError(f"kd_src {kd_src.lines} != kd_ref {kd_ref.lines} lines")

    shards = run.do(split_kd, timeout=1800, kd_src=kd_src)
    if sum(s.lines or 0 for s in shards.values()) != kd_src.lines:
        raise RuntimeError("split_kd lost lines across shards")

    # The gate and the K decode shards are independent until selection: one box
    # each, all concurrent. A failed shard reruns alone on the flow's next attempt.
    with ThreadPoolExecutor(max_workers=K + 1) as ex:
        fgate = ex.submit(
            run.do, bicleaner_score, timeout=4 * 3600, kd_src=kd_src, kd_ref=kd_ref
        )
        fdec = [
            ex.submit(run.do, kd_decode, timeout=4 * 3600, kd_src=shards[f"shard_{i:02d}"])
            for i in range(K)
        ]
        gates = fgate.result()["scores"]
        parts = {f"part_{i:02d}": f.result()["nbest"] for i, f in enumerate(fdec)}
    if gates.lines != kd_src.lines:
        raise RuntimeError(f"bicleaner dropped lines: {kd_src.lines} -> {gates.lines}")

    gathered = run.do(gather_nbest, timeout=1800, **parts)["nbest"]
    kd_tgt = run.do(
        select_best, timeout=3 * 3600, nbest=gathered, kd_ref=kd_ref, gate_scores=gates
    )["kd_sel"]
    if kd_tgt.lines != kd_src.lines:
        raise RuntimeError(f"selection dropped lines: {kd_src.lines} -> {kd_tgt.lines}")

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
        cefilter_score, timeout=2 * 3600,
        backward_npz=a("backward_npz"), vocab=a("vocab"), kd_tgt=kd_tgt, kd_src=kd_src,
    )["scores"]
    if scores.lines != kd_src.lines:
        raise RuntimeError(f"scorer dropped lines: {kd_src.lines} -> {scores.lines}")

    filtered = run.do(cefilter_cut, timeout=3600, scores=scores, train_tsv=aligned)["train_tsv"]

    model = run.do(
        train, timeout=8 * 3600, train_tsv=filtered, vocab=a("vocab"), valid=a("valid")
    )["model"]
    model_ft = run.do(
        finetune, timeout=3 * 3600,
        ft_tsv=a("ft_tsv"), vocab=a("vocab"), pretrained=model, valid=a("valid"),
    )["model"]
    hyp = run.do(
        decode, timeout=3600, model=model_ft, vocab=a("vocab"), devtest_src=a("devtest_src")
    )["hyp"]
    return {"model_ft": model_ft.to_json(), "hyp": hyp.to_json()}
