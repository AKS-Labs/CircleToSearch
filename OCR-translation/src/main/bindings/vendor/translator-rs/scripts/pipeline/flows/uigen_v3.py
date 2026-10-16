"""uig→en pipeline-v3, end to end from OPUS.

Uyghur is a NEW pair, so this flow builds everything the sw→en template took as
seeded: the joint SPM vocab (prep --vocab train), the human bitext, and the
backward RNN. Only FLORES (valid/devtest) is seeded.

    prep ─┬─ kd_source ─┬─ split_kd ─ kd_decode ×K ─ gather ─┐
          │             │                                     ├─ select_best ─
          │             └─ bicleaner_score ─┬─────────────────┘
          │                                 └─ build_human ─┬─ align_ft ─┐
          └─ prep_ft ───────────────────────────────────────┘            │
                                                            └─ backward ─┤
     ─ drop_empty ─ align ─ cefilter_score ─ cefilter_cut ─ train ─ finetune ─ decode

The gate, the decode shards, and the backward RNN are independent until
selection/ce-filter, so they run concurrently — each on its own box with its own
lease, and a dead box retries that shard alone.

ONE bicleaner run serves two consumers at two thresholds: >=0.5 gates
extract-best (below it a misaligned reference would make "closest to reference"
mean "closest to noise"), >=0.8 salvages the finetune/backward human bitext.
en-ug curated supply is ~6.2k raw pairs — thinner than sw's 9k — so the salvage
is not optional here.

Teacher = NLLB-200-distilled-1.3B: the checkpoint that gated uig→en at 49.7
chrF++ (600M: 47.3). This deliberately retries the 1.3B that failed sw's raw
re-KD; the v3 filters (gated extract-best + ce-filter) exist to handle a richer
teacher's output. en→uig stays on HOLD — nobody clears the ~50 bar into Uyghur.

Seed once, then run:

    pipe --run uigen put valid       <2-col ug\\ten FLORES dev>
    pipe --run uigen put devtest_src <FLORES devtest uig side>
    pipe --run uigen run uigen_v3 prep    # through the gate + backward RNN
    pipe --run uigen run uigen_v3         # the whole thing
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver, Vast
from pipe.types import Kind

TOOLS = "/opt/tools"
CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:e8a1a25"
NLLB = "ghcr.io/davidventura/offline-translator/nllb-ct2:1.3b"
BICLEANER = "ghcr.io/davidventura/offline-translator/bicleaner:en-xx"
BMT = "marian-bmt:next"
PREP = "prep:next"
K = 5
# EU hosts have empirically behaved better than the cheapest US/SE offers
# (2026-07-15: dud Quebec pair, SE box stuck in a pull-retry loop). Not policy,
# just this run's preference — drop if it thins the offer pool too much.
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")
# Human corpora that exist for en-ug on OPUS. No tico-19/bible-uedin (absent).
# Tanzil (93.5k Quran pairs) is deliberately excluded: it would swamp the 6.2k
# curated ~15:1 with archaic religious register — the shape that made the sw
# back-translation regress (228k Leipzig over 9k curated → forgetting).
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
    # A NEW pair has no joint vocab and cannot borrow another pair's, so this
    # prep trains it (32k unigram, both sides) and emits it alongside the pool.
    return [ctx.script, "ug", "16", ctx.out_dir, "en", "train"]


@step(
    image=PREP,
    target=Bigserver(cpus=16),
    script="prep_step.sh",
    deps=deps.PREP,
    outputs={"pool_tsv": Output(rel="pool.tsv", kind=Kind.LINES)},
)
def prep_ft(ctx: Ctx) -> list[str]:
    # Curated/human-only pool: both sides are used as-is at finetune, so mined
    # corpora (semantic misalignment) are excluded. Separate prep because the
    # dedup pool carries no provenance.
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
    # pool is en\tug; column 2 (ug) becomes the uig→en KD source, column 1 the ref
    return [ctx.script, ctx.inp("pool_tsv"), "2", ctx.out_dir]


@step(
    image=BICLEANER,
    target=Vast(gpu="RTX_4090", max_hours=5, disk_gb=40, tries=8, geo=EU, min_cuda=12.1),
    script="bicleaner_score.sh",
    outputs={"scores": Output(rel="gate_scores", kind=Kind.LINES)},
)
def bicleaner_score(ctx: Ctx) -> list[str]:
    # bicleaner-ai-full-en-xx is one multilingual XLM-R-large; ug is in XLM-R, so
    # no dedicated en-ug model exists or is needed.
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
    # The ce-filter judge: en→uig, the REVERSE of the student. Human pairs only,
    # so it is independent of the teacher — a teacher-descended scorer shares the
    # teacher's errors and cannot see them.
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
    image=NLLB,
    target=Vast(gpu="RTX_4090", max_hours=6, disk_gb=60, tries=8, geo=EU, min_cuda=12.1),
    script="kd_decode.sh",
    deps=deps.KD_DECODE,
    outputs={"nbest": Output(rel="nbest.tsv.gz", kind=Kind.BLOB)},
)
def kd_decode(ctx: Ctx) -> list[str]:
    # beam 4 / n-best 4 are the script's defaults (the standing rule). They stay
    # out of argv on purpose: the step key covers the script digest but NOT argv,
    # so a beam change must be a script edit to invalidate the memoized shards.
    return [ctx.script, ctx.inp("kd_src"), "uig_Arab", "eng_Latn", ctx.out("nbest.tsv.gz")]


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
    # A zero-token pair sends fast_align's EM to nan and empties the ENTIRE
    # reverse pass (the align_ensw defect) — cheaper to drop than to debug.
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

    shards = run.do(split_kd, timeout=1800, kd_src=kd_src)
    if sum(s.lines or 0 for s in shards.values()) != kd_src.lines:
        raise RuntimeError("split_kd lost lines across shards")

    ft_pool = run.do(prep_ft, timeout=2 * 3600)["pool_tsv"]

    # The gate and the K decode shards are independent until selection: one box
    # each, all concurrent. A failed shard reruns alone on the flow's next attempt.
    # In the prep stage the decode is deliberately not launched — the gate output
    # is inspected (distribution, salvage yield) before committing to the decode.
    with ThreadPoolExecutor(max_workers=K + 1) as ex:
        fgate = ex.submit(
            run.do, bicleaner_score, timeout=5 * 3600, kd_src=kd_src, kd_ref=kd_ref
        )
        fdec = [
            ex.submit(run.do, kd_decode, timeout=6 * 3600, kd_src=shards[f"shard_{i:02d}"])
            for i in range(K)
        ] if stage == "all" else []
        gates = fgate.result()["scores"]
        parts = {f"part_{i:02d}": f.result()["nbest"] for i, f in enumerate(fdec)}
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

    gathered = run.do(gather_nbest, timeout=1800, **parts)["nbest"]
    kd_tgt = run.do(
        select_best, timeout=4 * 3600, nbest=gathered, kd_ref=kd_ref, gate_scores=gates
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
