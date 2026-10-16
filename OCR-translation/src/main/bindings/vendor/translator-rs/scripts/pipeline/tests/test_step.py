from __future__ import annotations

import json
from pathlib import Path

import pytest

from pipe.ledger import Ledger
from pipe.runmodel import RunnerResult
from pipe.step import SALVAGED_MARKER, Ctx, NeedsDecision, Output, Run, step
from pipe.store import Store
from pipe.types import Kind, RunId, Status


class NeverOpenTarget:
    """A target whose open() must never run: the salvage marker means the
    outputs are already local and the box is already destroyed."""

    def image_id(self, image: str) -> str:
        return "fake-image-id"

    def open(self, run: str, step: str, key: str, image: str, root: Path):
        raise AssertionError("target.open must not be called")


def make_step():
    @step(
        image="fake:img",
        target=NeverOpenTarget(),
        script="noop.sh",
        outputs={"result": Output(rel="result", kind=Kind.LINES)},
    )
    def salvage_me(ctx: Ctx) -> list[str]:
        return [str(ctx.script)]

    return salvage_me


@pytest.fixture
def run(tmp_path: Path) -> Run:
    scripts = tmp_path / "scripts"
    scripts.mkdir()
    (scripts / "noop.sh").write_text("true\n")
    root = tmp_path / "root"
    store = Store(root)
    rid = RunId("t1")
    store.run_dir(rid).mkdir(parents=True)
    return Run(id=rid, store=store, ledger=Ledger(store.ledger_path(rid)), scripts=scripts)


def test_salvaged_marker_collects_without_opening_the_target(run: Run) -> None:
    defn = make_step()
    key = run._key(defn, {}, {})
    local_out = run.store.run_dir(run.id) / "out" / key
    local_out.mkdir(parents=True)
    (local_out / "result").write_text("a\nb\n")
    (local_out / SALVAGED_MARKER).touch()
    run.ledger.begin(key, defn.name, local_out / "job.log", None)

    produced = run.do(defn, timeout=1)
    assert produced["result"].lines == 2
    assert run.ledger.get(key).status is Status.DONE

    # And the memoized path serves it from the ledger from now on.
    assert run.do(defn, timeout=1) == produced


def test_no_marker_means_the_target_is_opened(run: Run) -> None:
    defn = make_step()
    with pytest.raises(AssertionError, match="must not be called"):
        run.do(defn, timeout=1)


def test_decide_returns_recorded_value(run: Run) -> None:
    run.decisions["train_go"] = "arm_b"
    assert run.decide("train_go") == "arm_b"
    assert not (run.store.run_dir(run.id) / "runner_result.json").is_file()


def test_decide_writes_pending_and_raises(run: Run, tmp_path: Path) -> None:
    payload = tmp_path / "scores.json"
    with pytest.raises(NeedsDecision) as exc:
        run.decide("train_go", payload=payload)
    assert exc.value.name == "train_go"
    result_path = run.store.run_dir(run.id) / "runner_result.json"
    rr = RunnerResult.load(result_path)
    assert rr.pending is not None
    assert rr.pending.name == "train_go"
    assert rr.pending.payload_path == str(payload)
    raw = json.loads(result_path.read_text())
    assert set(raw) == {"pending_decision"}
