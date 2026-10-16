"""The pair pipeline as a type. A pair supplies parts; it never writes the order.

`pipe.gate` and the eval steps were each built after a failure, documented, and
then absent from the next flow — because a flow is copied, and a copy keeps what
someone remembered to keep. Comments do not survive that; a signature does.

So the graph lives in `run_pair` ONLY. A pair implements `Pair`, which cannot be
instantiated without every part, and `run_pair` is the only thing that decides
what runs when. There is no supported way to express "pair without a teacher
gate" or "KD straight off the raw corpus" — not because it is forbidden, but
because no signature accepts it:

- `Pair.kd()` takes `Filtered`. The only source of a `Filtered` is `pipe.gate`,
  which refuses when `known_good` is absent. So KD cannot see an ungated corpus:
  uig round 1's 15-20% Kazakh becomes a type error rather than a bad run.
- `run_pair` calls `require_check_set` before renting anything, so a pair with no
  check set stops before spending. en-tl and sw-en were trained end to end on
  teachers that pass FLORES and produce unusable output; the check set is the
  requirement that would have stopped both, so it ranks with "needs a teacher"
  and "needs a GPU".
- `PairResult` is frozen and total: a run that did not evaluate the teacher, the
  KD checkpoint AND the packed student cannot be
  constructed, so "we skipped the eval" has no representation.

An implementation could still no-op its own `train()`. That is out of scope —
this makes the honest path the only *convenient* one, not the only possible one.

    class UigPair(Pair):
        lang, src = "ug", "en"
        mix = Mix(total=10_000_000, fill=Register.CRAWL, caps={
            Register.UI: 50_000, Register.HUMAN: 200_000,
            Register.DIALOGUE: 1_000_000, Register.ENTITY: 150_000,
        })
        def teacher_decode(self): return hy_decode_step
        def kd(self, run, filtered, kd_ref): ...
        def pack(self, run, model, vocab): ...

    def main(run, argv):
        return run_pair(run, UigPair()).to_json()
"""

from __future__ import annotations

import sys
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path

from pipe import deps
from pipe.artstore import Filtered
from pipe.gate import gate
from pipe.step import Ctx, Output, Run, StepDef, step
from pipe.target import Bigserver
from pipe.types import Artifact, Kind

from . import evalsteps

# registers.py is the opus-trainer library the steps run; importing it here keeps
# ONE definition of the register set, so a Mix written in a flow and the pools
# written by prep cannot disagree about what registers exist.
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "opus-trainer"))
from registers import Mix, Register  # noqa: E402


@dataclass(frozen=True)
class Evaluated:
    """One system's numbers plus the pairs a human or agent reads.

    Both, always. The metrics rank; only the review shows that a teacher renders
    *Banyo* as "Reflection" — chrF called that direction a tie, COMET ranked the
    broken model above its own median, and the mechanical checks scored it 100%
    clean.
    """

    metrics: Artifact
    review: Artifact

    def to_json(self) -> dict:
        return {"metrics": self.metrics.to_json(), "review": self.review.to_json()}


@dataclass(frozen=True)
class PairResult:
    """Total by construction: every stage that must be evaluated has a field."""

    teacher: Evaluated
    kd: Evaluated
    student: Evaluated

    def to_json(self) -> dict:
        return {
            "teacher": self.teacher.to_json(),
            "kd": self.kd.to_json(),
            "student": self.student.to_json(),
        }


class Pair(ABC):
    """The pair-specific parts, and only those.

    Everything here varies by pair for a real reason: the teacher family decodes
    differently (vLLM / transformers / Marian), and uigen_r2 merges several KD
    sources where uigen_v3 has one. Ordering does not vary, so it is not here.
    """

    lang: str
    src: str = "en"
    filter_budget: float = 0.02

    @property
    @abstractmethod
    def mix(self) -> Mix:
        """Per-register line targets for the KD draw. REQUIRED, no default.

        A default here would be the bug it exists to prevent. The old draw was
        `dedup | shuf | head` over one concatenated pool, which is proportional
        and therefore decided by corpus size: en-tl has 63.5M NLLB lines against
        23k translatewiki, so UI contributed ~4k lines of 10M and the resulting
        student passes `Emergency Exit`, `Detour` and `Cash Only` through
        untranslated. Nothing in the pipeline was positioned to notice, because
        nobody had written down what the corpus was supposed to contain.

        Stating a Mix is writing that down, and `Mix.__post_init__` refuses one
        that leaves a register unnamed — so a register can be capped at zero on
        purpose, but it cannot go missing by accident.
        """

    @abstractmethod
    def teacher_decode(self) -> StepDef:
        """Step decoding FLORES + the check set with the teacher.

        Must emit flores_hyp / flores_ref / flores_src / check_hyp.
        """

    @abstractmethod
    def kd(self, run: Run, filtered: Filtered, kd_ref: Artifact) -> Artifact:
        """Filtered corpus -> aligned train_tsv (split/decode/gather/cefilter/align).

        Takes `Filtered`, not `Artifact`: the corpus gate is upstream of KD in the
        type system, so it cannot be skipped by editing a call site.

        `kd_ref` is each KD line's own bitext target, carried through from the
        draw. extract-best and ce-filter need it, and deriving it later is
        impossible once the pairing is gone.
        """

    @abstractmethod
    def train(self) -> StepDef:
        """One box: train -> decode(FLORES) -> decode(check set).

        One step because pipe leases per step key, so splitting it rents a box per
        phase and ships a checkpoint between them; the decodes are seconds on a
        GPU already in hand.

        Finetune is deliberately NOT here. It bought +2.87 chrF on tl and +1.3 on
        sw, always on FLORES, and its effect on deployment-shaped input has never
        been measured for any pair — so it is an experiment to run against this
        baseline (train a ft variant, eval it the same way, compare check deltas),
        not a required stage. Adding it back means a second Evaluated on
        PairResult, which is the point: a stage that is not measured does not
        belong in the required shape.

        Must emit: model, flores_hyp, check_hyp.
        """

    @abstractmethod
    def pack(self, run: Run, model: Artifact, vocab: Artifact) -> dict[str, Artifact]:
        """Quantize + shortlist -> {"model", "vocab", "shortlist"}."""


def _kd_source_step(mix: Mix, kd_col: int, seed: int):
    """The KD draw, closed over the pair's Mix.

    A factory for the same reason `_student_step` is one: a step function only
    receives `ctx`, and the mix is a per-pair constant that cannot come from the
    environment (the job protocol passes an args file and a script path, never
    env).

    `deps` names the libraries the wrapper invokes. Without them only the six-line
    `kd_mix_step.sh` is digested into the key, so a change to the sampler or the
    register filters would be served from the memo — which is exactly how a
    placeholder-regex bug survived a re-run on 2026-07-21.
    """

    @step(
        image="prep:next",
        target=Bigserver(cpus=8),
        script="kd_mix_step.sh",
        deps=deps.KD_MIX,
        outputs={
            "kd_src": Output(rel="kd_src", kind=Kind.LINES),
            "kd_ref": Output(rel="kd_ref", kind=Kind.LINES),
            "mix": Output(rel="mix.json", kind=Kind.BLOB),
        },
    )
    def kd_source(ctx: Ctx) -> list[str]:
        return [
            ctx.script, ctx.out_dir, str(kd_col), str(mix.total), mix.spec, str(seed),
            *(ctx.inp(f"pool_{r.value}") for r in Register),
        ]

    return kd_source


def _score(run: Run, flores_hyp: Artifact, check_hyp: Artifact,
           flores_ref: Artifact, flores_src: Artifact,
           check_src: Artifact, check_ref: Artifact) -> Evaluated:
    """Scoring is shared and always on Bigserver: identical code per pair is what
    makes numbers comparable across pairs and rounds. Decoding stays on the
    rented box, which is model-specific and already paid for."""
    out = run.do(
        evalsteps.eval_score, timeout=3600,
        flores_hyp=flores_hyp, flores_ref=flores_ref, flores_src=flores_src,
        check_hyp=check_hyp, check_src=check_src, check_ref=check_ref,
    )
    return Evaluated(metrics=out["metrics"], review=out["review"])


def run_pair(run: Run, pair: Pair) -> PairResult:
    """The graph. The only place it exists."""
    a = run.ledger.artifact
    check_src, check_ref = evalsteps.require_check_set(run)
    flores_ref, flores_src = a("flores_ref"), a("flores_src")

    # The draw comes BEFORE the gate, and `raw` is its output rather than a
    # pre-existing artifact: a corpus someone built by hand and dropped in the
    # ledger has no recorded mix, which is the state every pair was in until now.
    kd_col = 1 if pair.src == "en" else 2
    drawn = run.do(
        _kd_source_step(pair.mix, kd_col, seed=42), timeout=2 * 3600,
        # argv is not hashed, so the mix must ride in args or a changed mix would
        # silently reuse a corpus drawn under the old one.
        args={"mix": pair.mix.spec, "total": pair.mix.total, "kd_col": kd_col},
        **{f"pool_{r.value}": a(f"pool_{r.value}") for r in Register},
    )

    filtered = gate(
        run,
        raw=drawn["kd_src"],
        vocab=a("vocab"),
        gold=a("gold"),
        known_good=a("known_good"),
        budget=pair.filter_budget,
        image="prep:next",
        target=Bigserver(cpus=16),
        label=f"{pair.src}{pair.lang}",
    )

    # Before the KD rental, not after: a student cannot exceed its teacher, so
    # every GPU-hour past a bad gate distils a known-bad model.
    decoded = run.do(
        pair.teacher_decode(), timeout=2 * 3600,
        filtered=filtered, check_src=check_src, check_ref=check_ref,
    )
    teacher = _score(run, decoded["flores_hyp"], decoded["check_hyp"],
                     flores_ref, flores_src, check_src, check_ref)

    train_tsv = pair.kd(run, filtered, drawn["kd_ref"])

    trained = run.do(
        pair.train(), timeout=12 * 3600,
        train_tsv=train_tsv, vocab=a("vocab"), valid=a("valid"),
        flores_src=flores_src, check_src=check_src,
    )
    kd_eval = _score(run, trained["flores_hyp"], trained["check_hyp"],
                     flores_ref, flores_src, check_src, check_ref)

    packed = pair.pack(run, trained["model"], a("vocab"))
    student_out = evalsteps.student_eval(
        run, pair.lang, pair.src,
        packed["model"], packed["vocab"], packed["shortlist"],
    )
    student = Evaluated(metrics=student_out["metrics"], review=student_out["review"])

    return PairResult(teacher=teacher, kd=kd_eval, student=student)
