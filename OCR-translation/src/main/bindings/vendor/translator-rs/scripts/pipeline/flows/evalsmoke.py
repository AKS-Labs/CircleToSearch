"""Smoke the shared eval_score step on artifacts that already exist.

The eval steps, the eval:next image and eval_pair.py had never executed under
pipe. Wiring alone already caught one defect (eval_score pointed at prep:next,
which carries no torch, so it would have died at import), and this session has
produced three more of that class — a missing chmod +x, a torch<2.6 guard, and a
wrapper reading env where the job protocol passes args.

So the whole path runs here on Bigserver, for free, on the Hy-MT2 tl hypotheses
already on the hub, BEFORE a multi-box KD depends on it.

    pipe --run evalsmoke put flores_hyp <...> --kind lines   (x6, see below)
    pipe --run evalsmoke run evalsmoke
"""

from __future__ import annotations

from pipe.step import Run

from pipe import evalsteps


def main(run: Run, argv: list[str]) -> dict:
    a = run.ledger.artifact
    out = run.do(
        evalsteps.eval_score,
        timeout=3600,
        flores_hyp=a("flores_hyp"), flores_ref=a("flores_ref"), flores_src=a("flores_src"),
        check_hyp=a("check_hyp"), check_src=a("check_src"), check_ref=a("check_ref"),
    )
    return {name: art.to_json() for name, art in out.items()}
