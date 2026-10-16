"""uig→en round 2: Kazakh purge + HPLT mono enrichment, Hy-MT2-7B-FP8 teacher.

Round 1 (`uigen`) shipped a 37.53 chrF++ student off a kd_src that was ~15-20%
Arabic-script (töte) Kazakh: fastText lid.176 only knows Cyrillic Kazakh, so töte
scored as Uyghur and passed every LID gate. The teacher then hallucinated on it
(one measured case: Kazakh "جون" jön/"proper" decoded to the name "John"), and the
student trained on the result.

Round 2 = purge + enrich, in one readout by explicit decision (2026-07-17): with no
further data to add, purity-vs-volume attribution has no lever attached to it, so a
random-drop control was dropped in favour of spending the wall time on data.

    existing_src/tgt (round 1, PURGED — no decode, targets are sunk) ─┐
                                                                       │
    hplt_src ─ split_hplt ─ decode ×6 ─ gather_hplt ─── hplt_tgt ──────┤
                                                                       │
                              merge ─ drop_empty ─ align ─ train

Deliberate differences from uigen_hy:

- PER-SOURCE decode artifacts, never gather_cat'd into one blob: hplt_tgt is kept
  alongside the merge, so ablating a source later costs one train and ZERO
  re-decode. Only HPLT is decoded here — MADLAD-400's `ug` split was fetched,
  segmented and measured, then DROPPED (2026-07-17): it is 56.6% töte Kazakh, and
  94% of the lines surviving the filter were already in HPLT, leaving 209k unique
  (3.8%) for a 7th box. The measurement is the finding; the data was not worth it.
- Shard count is sized to the box count, not to a global K: one shard per box, so
  no box queues behind another shard and wall time is one shard, not six.
- NO cefilter. Measured 2026-07-17: its bottom-5% was 95% Kazakh with fine
  targets, and the ce-7.01 judge overfit at the first validation — it ranks
  register, not quality. The step stays in the codebase; this flow bypasses it.
- NO bicleaner / build_human / backward: nothing here needs them. The round-1
  artifacts remain valid for the step-3 arm sweep off this checkpoint.
- Inputs arrive via the cross-run store + pins, NOT as a flow edge back to
  `uigen`. Round-1's align@f7c38cfd85e5 stays untouched, so there is no cascade
  risk on a memoized upstream and no dependency on the round-1 script digests
  still matching.

Vocab is the round-1 joint SPM, reused unchanged: a fresh vocab would invalidate
everything downstream, and the enrichment measured 1.81 pieces/word against it vs
the existing corpus's 1.78 — same distribution, no re-vocab warranted.

Prerequisites (hub, one time) — publish into the cross-run store, then pin the
digests each publish prints for this flow:
    pipe publish /nvme2/prom/uig/r2/existing.src --type dataset/filtered --label uig-r2-existing-src
    pipe publish /nvme2/prom/uig/r2/existing.tgt --type dataset/filtered --label uig-r2-existing-tgt
    pipe publish /nvme2/prom/uig/r2/hplt.src     --type dataset/filtered --label uig-r2-hplt-src
    pipe publish /nvme2/prom/uig/r2/vocab.spm    --type vocab/spm --label uig-r1-vocab
    pipe publish /nvme2/prom/uig/valid.ugen.tsv  --type eval/evalset --kind blob --label uig-valid
    pipe publish /nvme2/prom/flores/flores200_dataset/devtest/uig_Arab.devtest \
        --type eval/evalset --label flores-devtest-uig

    pipe pin uigen_r2 existing_src <digest>
    pipe pin uigen_r2 existing_tgt <digest>
    pipe pin uigen_r2 hplt_src     <digest>
    pipe pin uigen_r2 vocab        <digest>
    pipe pin uigen_r2 valid        <digest>
    pipe pin uigen_r2 devtest      <digest>

    pipe --run uigr2 run uigen_r2 decode        # decode only, before committing to train
    pipe --run uigr2 run uigen_r2               # up to the trained student
    pipe --run uigr2 run uigen_r2 pack          # + quantize/shortlist/pack -> slimt bucket pack
"""

from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from flows.pack import build_pack
from pipe import deps
from pipe.artstore import StoredWrapper
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver, Vast
from pipe.types import Artifact, Kind

TOOLS = "/opt/tools"
CUDA = "ghcr.io/davidventura/offline-translator/marian-cuda:e8a1a25"
HYKD = "ghcr.io/davidventura/offline-translator/hy-kd:cu129p"
BMT = "marian-bmt:next"

# One shard per box: every shard is the whole of one box's work, so no box queues.
# 5.29M / 6 ≈ 881k ≈ 1.6h at the validated 124.5 l/s.
K_HPLT = 6

# EU-only is load-bearing, not a preference: a US box's peering to the EU hub / HF
# was ~1KB/s in validation, and the `inet_down` filter does NOT catch it (it
# measures peak to a speedtest server, not the path that matters).
EU = ("FR", "HU", "PL", "UA", "DE", "NL", "CZ", "AT", "RO", "BG", "IT", "ES", "PT")


@step(
    image=BMT,
    target=Bigserver(cpus=4),
    script="split_kd.sh",
    outputs={f"shard_{i:02d}": Output(rel=f"shard_{i:02d}", kind=Kind.LINES) for i in range(K_HPLT)},
)
def split_hplt(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.inp("kd_src"), str(K_HPLT), ctx.out_dir]


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
def gather_hplt(ctx: Ctx) -> list[str]:
    return [ctx.script, ctx.out("kd_tgt")] + [ctx.inp(f"part_{i:02d}") for i in range(K_HPLT)]


@step(
    image=BMT,
    target=Bigserver(cpus=2),
    script="merge_sources.sh",
    outputs={
        "src": Output(rel="src", kind=Kind.LINES),
        "tgt": Output(rel="tgt", kind=Kind.LINES),
    },
)
def merge(ctx: Ctx) -> list[str]:
    # Source order is fixed (existing, hplt) and each side is cat'd in the
    # same order, so src line N still pairs with tgt line N. merge_sources.sh
    # asserts per-source src/tgt line counts before concatenating — a short decode
    # that slipped past the gather assert would otherwise desync every pair after it.
    return [
        ctx.script, ctx.out_dir,
        ctx.inp("existing_src"), ctx.inp("existing_tgt"),
        ctx.inp("hplt_src"), ctx.inp("hplt_tgt"),
    ]


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
    # fast_align is unsupervised and trains on whatever corpus it is handed, so the
    # MERGED corpus is aligned once. Aligning per source would fit a different
    # alignment model per source and make the guided-alignment column inconsistent.
    return [ctx.script, ctx.inp("src"), ctx.inp("tgt"), ctx.out_dir, TOOLS]


@step(
    image=CUDA,
    target=Vast(gpu="RTX_4090", max_hours=10, disk_gb=60, tries=8, geo=EU, min_cuda=11.8),
    script="train_student.sh",
    deps=deps.TRAIN_STUDENT,
    outputs={"model": Output(rel="model/model.npz.best-ce-mean-words.npz", kind=Kind.BLOB)},
)
def train(ctx: Ctx) -> list[str]:
    # ctx.out("model/model.npz"), not ctx.out("model"): marian validates the --model
    # extension and aborts on a path without .npz/.bin. The trailing "0" is DEVICES
    # (GPU 0); train_student.sh treats a missing devices arg as CPU, which would run
    # for days on 8M lines.
    return [
        ctx.script, ctx.inp("train_tsv"), ctx.inp("vocab"), ctx.inp("valid"),
        ctx.out("model/model.npz"), "0",
    ]


def _decode_source(
    run: Run, name: str, split_step, gather_step, src: Artifact | StoredWrapper, k: int
) -> Artifact:
    shards = run.do(split_step, timeout=1800, kd_src=src)
    if sum(s.lines or 0 for s in shards.values()) != src.lines:
        raise RuntimeError(f"split_{name} lost lines across shards")

    # One box per shard, all concurrent: wall time is one shard, not k of them.
    # A failed shard reruns alone on the flow's next attempt. Step keys are derived
    # from (step, image, script, inputs), so each shard's distinct input already
    # gives it a distinct key — nothing to disambiguate by hand.
    with ThreadPoolExecutor(max_workers=k) as ex:
        futures = [
            ex.submit(run.do, kd_decode, timeout=6 * 3600, kd_src=shards[f"shard_{i:02d}"])
            for i in range(k)
        ]
        parts = [f.result() for f in futures]

    for i, part in enumerate(parts):
        got, want = part["targets"].lines, shards[f"shard_{i:02d}"].lines
        if got != want:
            raise RuntimeError(f"{name} shard {i:02d} decoded {got} lines for {want} inputs")

    gathered = run.do(
        gather_step, timeout=1800,
        **{f"part_{i:02d}": p["targets"] for i, p in enumerate(parts)},
    )["kd_tgt"]
    if gathered.lines != src.lines:
        raise RuntimeError(f"gather_{name}: {gathered.lines} targets for {src.lines} sources")
    return gathered


def main(run: Run, argv: list[str]) -> dict:
    stage = argv[0] if argv else "all"
    if stage not in ("decode", "all", "pack"):
        raise SystemExit(f"unknown stage {stage!r}; want 'decode', 'pack' or nothing")

    existing_src, existing_tgt = run.pinned("existing_src"), run.pinned("existing_tgt")
    hplt_src = run.pinned("hplt_src")
    if existing_src.lines != existing_tgt.lines:
        raise RuntimeError(
            f"round-1 carry-over is desynced: {existing_src.lines} src vs {existing_tgt.lines} tgt"
        )

    hplt_tgt = _decode_source(run, "hplt", split_hplt, gather_hplt, hplt_src, K_HPLT)

    # The per-source target is kept as a named artifact, not just as a merge input:
    # that is what makes a later source ablation one train and zero re-decode.
    if stage == "decode":
        return {"hplt_tgt": hplt_tgt.to_json()}

    merged = run.do(
        merge, timeout=3600,
        existing_src=existing_src, existing_tgt=existing_tgt,
        hplt_src=hplt_src, hplt_tgt=hplt_tgt,
    )
    pairs = run.do(drop_empty, timeout=3600, kd_src=merged["src"], kd_tgt=merged["tgt"])
    aligned = run.do(align, timeout=2 * 3600, src=pairs["src"], tgt=pairs["tgt"])["train_tsv"]
    model = run.do(
        train, timeout=10 * 3600,
        train_tsv=aligned, vocab=run.pinned("vocab"), valid=run.pinned("valid"),
    )["model"]
    result = {
        "hplt_tgt": hplt_tgt.to_json(),
        "train_tsv": aligned.to_json(), "model": model.to_json(),
    }

    if stage == "pack":
        packed = build_pack(
            run, model=model, vocab=run.pinned("vocab"),
            src=pairs["src"], tgt=pairs["tgt"], devtest=run.pinned("devtest"), infix="ugen",
        )
        result["pack"] = {name: art.to_json() for name, art in packed.items()}

    return result
