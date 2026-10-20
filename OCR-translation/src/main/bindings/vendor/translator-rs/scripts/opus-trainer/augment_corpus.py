#!/usr/bin/env python3
"""Materialize a SAMPLE of OpusTrainer's augmented stream, to look at it.

A DIAGNOSTIC, not a training input. Training streams the augmentation live (see
train_student.sh) — materializing the whole corpus was evaluated on 2026-07-21
and rejected:

  - `until original inf` re-rolls the modifiers every pass, so a ~28-epoch run
    sees ~28 DIFFERENT perturbations of each sentence (~87% of lines uppercased
    at least once at 7%/pass). A fixed N-pass corpus freezes that to N — at N=8,
    44%.
  - determinism did not need it: the augmentation is a pure function of (corpus
    digest, opustrainer==0.5 pinned in the image, seed 1111). The seed is the
    compressed form of the expansion.
  - the corpus is already shuffled three ways (build_kd_source's seeded shuf,
    OpusTrainer's per-pass reshuffle, marian's maxi-batch window), so
    `--shuffle data` added a fourth, and checkpoints already resume weights, so
    corpus-position restore added little.
  - cost: ~23GB artifact at N=28 plus ~55GB of shuffle temp on the box, since
    marian runs with shuffle-in-ram=false.

What it is still good for: SEEING what the modifiers do. Nothing else exposes
that — the perturbed text never touched disk before. Use a small slice, eyeball
the output, and check the rates match the config.

    augment_corpus.py <100k-line slice> sample.tsv.gz --epochs 3 --jobs 3
"""

from __future__ import annotations

import argparse
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
TEMPLATE = HERE / "configs" / "opustrainer.student.yml"


def write_config(dst: Path, tsv: Path, seed: int) -> None:
    """One pass, not an endless stream, and a per-pass seed.

    The shipped config pins `seed: 1111` and `until original inf`; both have to
    change here or every worker emits the same perturbations forever.
    """
    text = TEMPLATE.read_text(encoding="utf-8")
    text = text.replace("__TSV__", str(tsv))
    text = text.replace(
        "- until original inf   # stream forever; marian stops on early-stopping",
        "- until original 1   # ONE pass; the driver runs several, each seeded")
    text = text.replace("- until original inf", "- until original 1")
    text = text.replace("seed: 1111", f"seed: {seed}")
    if "until original 1" not in text or f"seed: {seed}" not in text:
        raise SystemExit(
            "opustrainer template changed: expected 'seed: 1111' and "
            "'- until original inf' to substitute; refusing to emit a config "
            "that streams forever or reuses one seed")
    dst.write_text(text, encoding="utf-8")


def run_pass(tsv: Path, work: Path, index: int, seed: int) -> Path:
    """One augmented pass, gzipped.

    marian reads .gz training sets natively (file_stream.h keeps a second
    streambuf "in case of a .gz file"), and the artifact store counts lines with
    `zcat -f`, so compression is transparent at both ends. ~2.6x on this corpus
    (measured: 200MB -> 76.7MB at gzip-3), turning an 8-epoch artifact from ~16GB
    into ~6GB. Level 3, not 9: the point is disk, and each worker compresses its
    own pass so this parallelises with the rest.

    Not zstd — 3.0x, but marian cannot read it, and decompressing to a temp file
    before training would spend the disk we just saved.
    """
    cfg = work / f"cfg_{index:03d}.yml"
    out = work / f"pass_{index:03d}.tsv.gz"
    write_config(cfg, tsv, seed)
    # `cat` is the trainer: OpusTrainer pipes the augmented stream into whatever
    # command it is given, so cat turns it into a writer.
    with out.open("wb") as fh:
        gz = subprocess.Popen(["gzip", "-3"], stdin=subprocess.PIPE, stdout=fh)
        assert gz.stdin is not None
        proc = subprocess.run(
            ["opustrainer-train", "--config", str(cfg), "--log-level", "WARNING", "cat"],
            stdout=gz.stdin, stderr=subprocess.PIPE,
        )
        gz.stdin.close()
        gz_rc = gz.wait()
    if proc.returncode != 0:
        raise RuntimeError(
            f"pass {index} failed ({proc.returncode}): {proc.stderr.decode()[-400:]}")
    if gz_rc != 0:
        raise RuntimeError(f"pass {index}: gzip exited {gz_rc}")
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("train_tsv", type=Path)
    ap.add_argument("out_tsv", type=Path)
    ap.add_argument("--epochs", type=int, default=8,
                    help="augmented passes to write; each gets its own seed")
    ap.add_argument("--jobs", type=int, default=8)
    ap.add_argument("--seed0", type=int, default=1111)
    args = ap.parse_args()

    work = args.out_tsv.parent / "augment_work"
    work.mkdir(parents=True, exist_ok=True)
    src_lines = sum(1 for _ in args.train_tsv.open(encoding="utf-8", errors="replace"))
    print(f"source {args.train_tsv}: {src_lines} lines; "
          f"{args.epochs} passes on {args.jobs} workers", file=sys.stderr)

    with ThreadPoolExecutor(max_workers=args.jobs) as ex:
        parts = list(ex.map(
            lambda i: run_pass(args.train_tsv, work, i, args.seed0 + i),
            range(args.epochs)))

    # Concatenated in pass order so the artifact is reproducible from the seeds.
    # Concatenated gzip MEMBERS are themselves a valid gzip stream, so this needs
    # no recompression — and zcat/marian both read the result as one file.
    with args.out_tsv.open("wb") as out:
        for p in parts:
            with p.open("rb") as fh:
                shutil.copyfileobj(fh, out, 1 << 20)
            p.unlink()

    total = int(subprocess.run(
        f"zcat -f {args.out_tsv} | wc -l", shell=True,
        capture_output=True, text=True, check=True).stdout.split()[0])
    # rmtree, not rmdir: OpusTrainer writes a `<config>.state` beside each config
    # (its resume marker), so the workdir is never empty after a pass.
    shutil.rmtree(work, ignore_errors=True)

    # NOT an equality check. The Noise modifier ADDS synthetic pairs rather than
    # perturbing existing ones (Noise: 0.0005 measured as +22 lines over 3 x 20k),
    # so the output is legitimately a little larger. A pass that died mid-write
    # would be short by ~a whole pass, which this still catches.
    expected = src_lines * args.epochs
    if not expected <= total <= expected * 1.01:
        raise SystemExit(
            f"augmented line count {total} outside [{expected}, {expected * 1.01:.0f}] "
            f"({src_lines} x {args.epochs}); a pass was truncated or duplicated")
    print(f"wrote {args.out_tsv}: {total} lines ({args.epochs} x {src_lines})",
          file=sys.stderr)


if __name__ == "__main__":
    main()
