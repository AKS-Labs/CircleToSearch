"""Pack the en->tl v4 student into a slimt bucket pack: quantize + shortlist + pack.

Reuses the shared build_pack tail. Inputs come from the tlkd_v4 ledger:
`model` resolves to the proper (early-stop-6) train_eval, not the undertrained
run — the ledger tracks the latest producer of each name.

    pipe --run tlkd_v4 run tlpack
"""

from __future__ import annotations

from flows.pack import build_pack
from pipe.step import Run


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    packed = build_pack(
        run,
        model=a("model"),
        vocab=a("vocab"),
        src=a("pairs_src"),
        tgt=a("pairs_tgt"),
        devtest=a("flores_src"),
        infix="entl",
    )
    return {name: art.to_json() for name, art in packed.items()}
