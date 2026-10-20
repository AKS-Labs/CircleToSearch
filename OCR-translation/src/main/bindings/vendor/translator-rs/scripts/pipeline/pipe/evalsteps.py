"""Eval steps shared by every pair flow. Import these; do not hand-roll per pair.

LIVES IN `pipe/`, NOT `flows/`, and that is load-bearing: daemon.load_flow uses
spec_from_file_location, so a flow is executed as a standalone module with no
package — `from . import evalsteps` inside flows/ raises ImportError. flows/ is
entrypoints; shared machinery is library code and belongs beside pipe.gate.
(Caught by the evalsmoke run, 2026-07-20, before any KD depended on it.)

THE CHECK SET IS A REQUIREMENT, LIKE A TEACHER OR A GPU.
A pair with no check set does not train. The flow refuses, and that refusal is
the feature — the en-tl and sw-en rounds were run end to end on OPUS-MT and
NLLB teachers and produced models that are unusable on real input, which nothing
in the pipeline was positioned to notice. There is deliberately no derived or
default check set to fall back to: an auto-derived one would be drawn from the
teachers' own training corpora, would score well, and would hide exactly the
failure it exists to catch.

WHAT THE tl ROUND SHOWED (2026-07-20)
- FLORES called en->tl a four-way tie inside 3.3 chrF, with the shipped OPUS-MT
  teacher nominally winning. On short/sign/informal input the same four models
  spread over 11.68 chrF and the ranking inverted.
- The teacher gate lived in ad-hoc scripts OUTSIDE pipe, so its numbers were
  chrF-only and undated, and drifted up to 2 points when re-measured.
- The graph (see uigen_v3) ended at `decode` with no eval step at all.

NOTHING HERE FAILS A BUILD ON QUALITY
Missing inputs refuse; bad scores do not. A teacher gate is a choice — you
cannot fix NLLB, only pick differently. And the mechanical checks must never
gate: on tl they scored OPUS-MT 100% clean in the direction where it renders
*Banyo* as "Reflection", ranking it above every better model. They are a defect
list, not a score. Judgment happens by reading `review.txt`, offline, by a human
or an agent.

CHECK SET CONTENT
- en->X: `probes/adversarial.en`, hand-authored, shared by every pair, forever.
- X->en: authored per pair. This is the part that costs someone real work, and
  it is the price of training the pair.

REQUIRED ORDER IN A PAIR FLOW
    teacher_eval  ->  (prep / kd / filter / vocab / train / finetune)  ->  student_eval
Gate the teacher BEFORE renting anything for KD: a student cannot exceed its
teacher, so a teacher that fails here has already decided the outcome, and every
GPU-hour after that point is spent distilling a known-bad model.
"""

from __future__ import annotations

from pipe import deps
from pipe.step import Ctx, Output, Run, step
from pipe.target import Bigserver
from pipe.types import Artifact, Kind

EVAL = "eval:next"


def require_check_set(run: Run) -> tuple[Artifact, Artifact]:
    """The check set, or a refusal naming what is missing.

    Shaped after pipe.gate's known_good rule: refuse rather than warn, because a
    warning at this point is a warning nobody reads until the model ships.
    """
    missing = [n for n in ("check_src", "check_ref") if n not in run.ledger.artifacts]
    if missing:
        raise SystemExit(
            f"no check set for this pair: missing {missing}.\n"
            f"A pair does not train without one — OPUS-MT and NLLB both passed "
            f"FLORES and produced unusable models on real input.\n"
            f"  pipe --run <run> put check_src <X-side probe sources> --kind lines\n"
            f"  pipe --run <run> put check_ref <English references> --kind lines\n"
            f"For en->X, scripts/opus-trainer/probes/adversarial.en is the shared set."
        )
    return run.ledger.artifact("check_src"), run.ledger.artifact("check_ref")


@step(
    image=EVAL,
    target=Bigserver(cpus=16),
    script="eval_score.sh",
    deps=deps.EVAL_SCORE,
    outputs={
        "metrics": Output(rel="metrics.json", kind=Kind.BLOB),
        "review": Output(rel="review.txt", kind=Kind.BLOB),
    },
)
def eval_score(ctx: Ctx) -> list[str]:
    """Score FLORES + check set and render the side-by-side review.

    Split from decoding on purpose: decoding is model-specific and belongs to the
    pair flow, scoring is not and belongs here, so every pair's numbers come from
    identical code and are therefore comparable across pairs and across rounds.
    """
    return [
        ctx.script,
        ctx.inp("flores_hyp"), ctx.inp("flores_ref"), ctx.inp("flores_src"),
        ctx.inp("check_hyp"), ctx.inp("check_src"), ctx.inp("check_ref"),
        ctx.out("metrics.json"), ctx.out("review.txt"),
    ]


def teacher_eval(run: Run, decode_step, **decode_inputs) -> dict[str, Artifact]:
    """Gate a teacher: decode FLORES + check set, score, render. Call FIRST.

    `decode_step` comes from the pair flow because the decoder differs per teacher
    family (vLLM for Hy-MT2, transformers for NLLB, Marian for OPUS-MT).
    Everything downstream of the hypotheses is shared.
    """
    check_src, check_ref = require_check_set(run)
    decoded = run.do(decode_step, timeout=2 * 3600,
                     check_src=check_src, check_ref=check_ref, **decode_inputs)
    return run.do(
        eval_score,
        timeout=3600,
        flores_hyp=decoded["flores_hyp"],
        flores_ref=decoded["flores_ref"],
        flores_src=decoded["flores_src"],
        check_hyp=decoded["check_hyp"],
        check_src=check_src,
        check_ref=check_ref,
    )


def _student_step(lang: str, src: str):
    """Build the student bench step for one pair+direction.

    A factory because `lang`/`src` are per-pair constants and a step function only
    receives `ctx` — the same reason uigen_v3's steps close over "ug"/"en". They
    cannot come from the environment: the job protocol passes an args file and a
    script path, never env, so a step reading os.environ fails on the box.
    """

    @step(
        image=EVAL,
        target=Bigserver(cpus=16),
        script="student_eval.sh",
        deps=deps.STUDENT_EVAL,
        outputs={
            "metrics": Output(rel="metrics.json", kind=Kind.BLOB),
            "check_hyp": Output(rel="check.hyp", kind=Kind.LINES),
            "review": Output(rel="review.txt", kind=Kind.BLOB),
        },
    )
    def student_eval_step(ctx: Ctx) -> list[str]:
        return [
            ctx.script, lang, src,
            ctx.inp("model"), ctx.inp("vocab"), ctx.inp("shortlist"),
            ctx.inp("check_src"), ctx.inp("check_ref"),
            ctx.out("check.hyp"), ctx.out("metrics.json"), ctx.out("review.txt"),
        ]

    return student_eval_step


def student_eval(run: Run, lang: str, src: str, model: Artifact, vocab: Artifact,
                 shortlist: Artifact) -> dict[str, Artifact]:
    """Bench the packed student on FLORES AND the check set. Call LAST.

    benchmark_slimt.py takes --probes/--probe-out as REQUIRED arguments, so the
    check decode cannot be dropped by forgetting a flag.
    """
    check_src, check_ref = require_check_set(run)
    return run.do(_student_step(lang, src), timeout=3600, model=model, vocab=vocab,
                  shortlist=shortlist, check_src=check_src, check_ref=check_ref)
