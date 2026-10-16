#!/usr/bin/env python3
"""Render probe decodes side by side for offline judgment by a human or an agent.

This is the artifact the whole probe apparatus exists to produce. No metric
separates a teacher that says "Reflection" for *Banyo* from one that says
"Bathroom" — measured 2026-07-20, chrF called them a tie, COMET ranked the broken
one above its own median, and the mechanical checks scored it 100% clean. Reading
the pairs is what worked, so the pipeline's job is to always put them in front of
someone, not to grade them.

Several hypothesis files can be passed at once; side-by-side is what makes a
regression obvious, since a lone output usually reads as plausible.

Mechanical flags from probe_check are shown inline as attention hints. They are
NOT a verdict — a flagged line is often fine ("Dapa!" -> "Down on the ground!" is
a length blowup and correct) and an unflagged line is often catastrophic.

    probe_review.py SRC OUT.txt --hyp shipped hyp_a.txt --hyp candidate hyp_b.txt
"""

import argparse
from pathlib import Path

from probe_check import check, load_sources


def render(src_path: Path, out: Path, named: list[tuple[str, list[str]]],
           direction: str = "en2tl") -> int:
    """Write the side-by-side review; return how many lines carry a flag.

    A function, not just a CLI, so an eval entry point can produce metrics AND
    review in ONE invocation. Two separate programs in a step wrapper means
    "metrics without review" is a runnable partial, and a partial that looks
    like success is how the tl corpus lost its src/tgt pairing.
    """
    probes = load_sources(src_path, direction)
    for label, lines in named:
        if len(lines) != len(probes):
            raise SystemExit(f"{label}: {len(lines)} hypotheses for {len(probes)} probes")

    width = max(len(label) for label, _ in named)
    flagged = 0
    with out.open("w", encoding="utf-8") as f:
        for i, (src, category, max_ratio) in enumerate(probes):
            notes = []
            for label, lines in named:
                for fail in check(src, lines[i], max_ratio):
                    notes.append(f"{label}: {fail.kind} ({fail.detail})")
            flagged += bool(notes)
            f.write(f"--- [{i:3d}] {category}\n")
            f.write(f"{'SRC'.ljust(width)}  {src}\n")
            for label, lines in named:
                f.write(f"{label.ljust(width)}  {lines[i]}\n")
            for n in notes:
                f.write(f"{'!!'.ljust(width)}  {n}\n")
            f.write("\n")
    print(f"{out}: {len(probes)} probes x {len(named)} systems, {flagged} with "
          f"mechanical flags -- READ THIS FILE, the flags are hints not a verdict")
    return flagged


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("src", type=Path, help="probes .jsonl or plain source .txt")
    ap.add_argument("out", type=Path)
    ap.add_argument("--direction", default="en2tl", help="only used for a .jsonl source")
    ap.add_argument("--hyp", nargs=2, action="append", required=True,
                    metavar=("LABEL", "FILE"))
    args = ap.parse_args()

    named = [(label, Path(f).read_text(encoding="utf-8").splitlines())
             for label, f in args.hyp]
    render(args.src, args.out, named, args.direction)


if __name__ == "__main__":
    main()
