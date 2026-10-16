"""The corpus gate: the typed `Raw -> Filtered` stage no round may skip.

Round 1 of uig trained on a corpus that was ~15-20% Arabic-script (töte) Kazakh
because lid.176 only knows Cyrillic Kazakh; a corpus defect is silent by
construction — training succeeds, only the scores are bad. The type system closes
that: decode accepts `Filtered`, never `Raw`, and this module is the only way to
mint a `Filtered`.

Every filter rule is a hypothesis until measured against gold + known_good, and
known_good (the corpus that trained the last student that worked) is the
load-bearing reference: gold is 1012 lines, and a rule costing 0.55% of real text
scored 0/1012 on it. All four hand-asserted rules from 2026-07-17 were wrong or
incomplete on first write; every mined rule was right. The gate is what makes a
plausible rule a measured one, so it is concrete and unskippable, and it refuses
when known_good is absent rather than warning.

Gold is READ by the gate to measure keep-rate; it is never rewritten by it — the
`EvalSet` wrapper never becomes a `Filtered`, so a filtered benchmark cannot leak
into use.
"""

from __future__ import annotations

from dataclasses import dataclass

from .artstore import (
    Aligned, ArtifactType, EvalSet, Filtered, Producer, Raw, Vocab, typed_stage,
)
from .step import Ctx, Output, Run, StepDef, step
from .target import Target
from .types import Artifact, Kind

FILTER_SCRIPT = "filter_corpus.py"
# The pair-specific plugin seam (script_lid.py-shaped): the generic LID is the
# thing that fails, so each pair supplies its own separator.
LID_SCRIPT = "script_lid.py"


@dataclass(frozen=True)
class GateReport:
    rule: str
    fires_on_target: float   # share of the corpus this would drop
    fp_on_gold: float        # share of gold (small; sanity floor only)
    fp_on_known_good: float  # share of the last working corpus — THE number

    @property
    def ratio(self) -> float:
        return self.fires_on_target / max(self.fp_on_known_good, 1e-9)


def gate_rule(rule_name: str, fires: float, gold_fp: float, kg_fp: float | None,
              budget: float) -> GateReport:
    """Refuse a filter rule whose cost on real text exceeds its budget.

    Deliberately concrete: this is the step no round may skip, because it is the
    only one that turned a plausible rule into a measured one every time it ran.
    """
    if kg_fp is None:
        raise ValueError(
            f"{rule_name}: no known_good corpus, so its false-positive rate is "
            f"UNMEASURED. gold is {gold_fp:.3%} but gold is too small to bound it "
            f"(a rule costing 0.55% scored 0/1012 on FLORES). Supply known_good, "
            f"or state explicitly that this round accepts an unmeasured filter."
        )
    report = GateReport(rule_name, fires, gold_fp, kg_fp)
    if kg_fp > budget:
        raise ValueError(
            f"{rule_name} drops {kg_fp:.3%} of known-good text to catch "
            f"{fires:.2%} of the target (ratio {report.ratio:.1f}:1); budget is "
            f"{budget:.3%}. This is the -دىڭ shape: plausible rule, real cost."
        )
    return report


def _filter_step(image: str, target: Target, latin_source: bool) -> StepDef:
    @step(
        image=image,
        target=target,
        script=FILTER_SCRIPT,
        outputs={"kept": Output(rel="kept", kind=Kind.LINES)},
    )
    def gate_filter(ctx: Ctx) -> list[str]:
        argv = [
            "python3", str(ctx.script),
            "--vocab", str(ctx.inp("vocab")),
            "--in", str(ctx.inp("corpus")),
            "--out", str(ctx.out("kept")),
        ]
        if latin_source:
            argv.append("--latin-source")
        return argv

    return gate_filter


def _lid_step(image: str, target: Target, script: str, extra: tuple[str, ...]) -> StepDef:
    @step(
        image=image,
        target=target,
        script=script,
        outputs={"kept": Output(rel="kept", kind=Kind.LINES)},
    )
    def gate_lid(ctx: Ctx) -> list[str]:
        return [
            "python3", str(ctx.script),
            "--in", str(ctx.inp("corpus")),
            "--out", str(ctx.out("kept")),
            *extra,
        ]

    return gate_lid


def _drop_share(total: int | None, kept: int | None, what: str) -> float:
    if not total:
        raise ValueError(f"{what} has no line count; the gate cannot measure against it")
    if kept is None:
        raise ValueError(f"filtered {what} has no line count")
    return (total - kept) / total


@typed_stage
def gate(
    run: Run,
    raw: Raw,
    *,
    vocab: Artifact | Aligned | Raw | Vocab,
    gold: EvalSet,
    known_good: Filtered | Aligned | None,
    budget: float,
    image: str,
    target: Target,
    timeout: float = 3600.0,
    latin_source: bool = False,
    lid: str | None = None,
    lid_args: tuple[str, ...] = (),
    label: str | None = None,
) -> Filtered:
    """Measure the filter against gold and known_good, refuse over-budget rules,
    then filter the raw corpus and publish the result as `dataset/filtered`."""
    if run.art is None:
        raise RuntimeError("gate publishes into the cross-run store; run has no ArtStore")
    fstep = _filter_step(image, target, latin_source)

    # Gold is measured first and only measured: 1012 lines through the filter to
    # read the keep-rate, output discarded (2026-07-17 precedent, script_lid over
    # devtest.ug — never filtered for use).
    gold_kept = run.do(fstep, timeout=timeout, corpus=gold, vocab=vocab)["kept"]
    gold_fp = _drop_share(gold.lines, gold_kept.lines, "gold")

    if known_good is None:
        gate_rule(FILTER_SCRIPT, 0.0, gold_fp, None, budget)
        raise AssertionError("unreachable: gate_rule refuses when known_good is None")

    kg_kept = run.do(fstep, timeout=timeout, corpus=known_good, vocab=vocab)["kept"]
    kg_fp = _drop_share(known_good.lines, kg_kept.lines, "known_good")

    kept = run.do(fstep, timeout=timeout, corpus=raw, vocab=vocab)["kept"]
    fires = _drop_share(raw.lines, kept.lines, "raw corpus")
    report = gate_rule(FILTER_SCRIPT, fires, gold_fp, kg_fp, budget)
    print(
        f"[gate] {report.rule}: fires {report.fires_on_target:.2%}, "
        f"gold fp {report.fp_on_gold:.3%}, known_good fp {report.fp_on_known_good:.3%}",
        flush=True,
    )

    if lid is not None:
        kept = run.do(
            _lid_step(image, target, lid, tuple(lid_args)), timeout=timeout, corpus=kept
        )["kept"]

    parents = (raw.digest,)
    if isinstance(vocab, (Aligned, Raw, Vocab)):
        parents += (vocab.digest,)
    step_key = run.producing_step(kept)
    published = run.art.publish(
        kept.path,
        ArtifactType.FILTERED,
        parents=parents,
        label=label,
        producer=Producer(run=str(run.id), step_key=step_key) if step_key else None,
    )
    assert isinstance(published, Filtered)
    return published
