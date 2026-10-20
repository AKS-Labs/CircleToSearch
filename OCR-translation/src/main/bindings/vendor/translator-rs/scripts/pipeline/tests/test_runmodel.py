from __future__ import annotations

from pathlib import Path

import pytest

from pipe.runmodel import PendingDecision, RunnerResult, RunRecord, RunState, write_runner_result


def fresh() -> RunRecord:
    return RunRecord.new("uigr2", "uigen_r2", ["decode"], {"hplt_src": "ab" * 32})


def test_new_record_is_queued() -> None:
    rec = fresh()
    assert rec.state is RunState.QUEUED
    assert rec.transitions == ()
    assert rec.runner_pid is None


def test_full_lifecycle_with_decide(tmp_path: Path) -> None:
    rec = fresh().transition(RunState.RUNNING, "submit", actor="cli", runner_pid=41)
    assert rec.runner_pid == 41

    pending = PendingDecision(name="train_go", payload_path="/x/scores.json")
    rec = rec.with_pending(pending).transition(
        RunState.NEEDS_DECISION, "decision 'train_go' pending", actor="piped"
    )
    assert rec.pending_decision == pending

    rec = rec.with_decision("train_go", True)
    assert rec.pending_decision is None
    assert rec.decisions["train_go"].value is True

    rec = rec.transition(RunState.RUNNING, "decide train_go", actor="cli", runner_pid=42)
    rec = rec.transition(RunState.DONE, "runner result", actor="piped")
    assert [t.dst for t in rec.transitions] == [
        RunState.RUNNING, RunState.NEEDS_DECISION, RunState.RUNNING, RunState.DONE
    ]

    path = tmp_path / "run.json"
    rec.save(path)
    assert RunRecord.load(path) == rec


def test_illegal_transitions_refused() -> None:
    rec = fresh()
    with pytest.raises(ValueError, match="illegal transition queued -> done"):
        rec.transition(RunState.DONE, "nope", actor="cli")
    done = rec.transition(RunState.RUNNING, "submit", actor="cli", runner_pid=1).transition(
        RunState.DONE, "runner result", actor="piped"
    )
    with pytest.raises(ValueError, match="illegal transition done -> running"):
        done.transition(RunState.RUNNING, "nope", actor="cli")


def test_abort_allowed_from_every_live_state() -> None:
    assert fresh().transition(RunState.ABORTED, "abort", actor="cli").state is RunState.ABORTED
    running = fresh().transition(RunState.RUNNING, "submit", actor="cli", runner_pid=1)
    assert running.transition(RunState.ABORTED, "abort", actor="cli").state is RunState.ABORTED
    parked = running.transition(RunState.NEEDS_DECISION, "pending", actor="piped")
    assert parked.transition(RunState.ABORTED, "abort", actor="cli").state is RunState.ABORTED


def test_json_schema_shape(tmp_path: Path) -> None:
    rec = fresh().transition(RunState.RUNNING, "submit", actor="cli", runner_pid=7)
    d = rec.to_json()
    assert set(d) == {
        "run", "flow", "argv", "state", "pins", "created", "runner_pid",
        "transitions", "decisions", "pending_decision",
    }
    assert set(d["transitions"][0]) == {"t", "from", "to", "reason", "actor"}
    assert d["pending_decision"] is None


def test_runner_result_exactly_one_field(tmp_path: Path) -> None:
    path = tmp_path / "runner_result.json"
    write_runner_result(path, {"result": {"ok": 1}})
    rr = RunnerResult.load(path)
    assert rr.result == {"ok": 1} and rr.pending is None and rr.error is None

    write_runner_result(path, {"pending_decision": {"name": "go", "payload_path": None}})
    rr = RunnerResult.load(path)
    assert rr.pending == PendingDecision(name="go", payload_path=None)

    path.write_text('{"result": 1, "error": "x"}')
    with pytest.raises(ValueError, match="exactly one"):
        RunnerResult.load(path)
    path.write_text("{}")
    with pytest.raises(ValueError, match="exactly one"):
        RunnerResult.load(path)
