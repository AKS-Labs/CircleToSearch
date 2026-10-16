"""The run record: `runs/<run>/run.json`, written ONLY by piped.

The CLI and the runner read it; the runner reports through its own
`runner_result.json`, which piped folds into the record on child exit. Every
control action is a recorded transition — nothing flips state silently.
"""

from __future__ import annotations

import json
import os
import time
from dataclasses import dataclass, replace
from enum import Enum
from pathlib import Path


class RunState(Enum):
    QUEUED = "queued"
    RUNNING = "running"
    DONE = "done"
    FAILED = "failed"
    ABORTED = "aborted"
    NEEDS_DECISION = "needs_decision"

    @property
    def live(self) -> bool:
        return self in (RunState.QUEUED, RunState.RUNNING, RunState.NEEDS_DECISION)


_ALLOWED: dict[RunState, frozenset[RunState]] = {
    RunState.QUEUED: frozenset({RunState.RUNNING, RunState.ABORTED}),
    RunState.RUNNING: frozenset(
        {RunState.DONE, RunState.FAILED, RunState.ABORTED, RunState.NEEDS_DECISION}
    ),
    RunState.NEEDS_DECISION: frozenset({RunState.RUNNING, RunState.ABORTED}),
    RunState.DONE: frozenset(),
    RunState.FAILED: frozenset(),
    RunState.ABORTED: frozenset(),
}


@dataclass(frozen=True)
class Transition:
    t: float
    src: RunState
    dst: RunState
    reason: str
    actor: str

    def to_json(self) -> dict:
        return {
            "t": self.t,
            "from": self.src.value,
            "to": self.dst.value,
            "reason": self.reason,
            "actor": self.actor,
        }

    @staticmethod
    def from_json(d: dict) -> Transition:
        return Transition(
            t=d["t"], src=RunState(d["from"]), dst=RunState(d["to"]),
            reason=d["reason"], actor=d["actor"],
        )


@dataclass(frozen=True)
class Decision:
    value: object
    t: float

    def to_json(self) -> dict:
        return {"value": self.value, "t": self.t}

    @staticmethod
    def from_json(d: dict) -> Decision:
        return Decision(value=d["value"], t=d["t"])


@dataclass(frozen=True)
class PendingDecision:
    name: str
    payload_path: str | None

    def to_json(self) -> dict:
        return {"name": self.name, "payload_path": self.payload_path}

    @staticmethod
    def from_json(d: dict) -> PendingDecision:
        return PendingDecision(name=d["name"], payload_path=d["payload_path"])


@dataclass(frozen=True)
class RunRecord:
    run: str
    flow: str
    argv: tuple[str, ...]
    state: RunState
    pins: dict[str, str]
    created: float
    runner_pid: int | None
    transitions: tuple[Transition, ...]
    decisions: dict[str, Decision]
    pending_decision: PendingDecision | None

    @staticmethod
    def new(run: str, flow: str, argv: list[str], pins: dict[str, str]) -> RunRecord:
        return RunRecord(
            run=run,
            flow=flow,
            argv=tuple(argv),
            state=RunState.QUEUED,
            pins=dict(pins),
            created=time.time(),
            runner_pid=None,
            transitions=(),
            decisions={},
            pending_decision=None,
        )

    def transition(
        self, to: RunState, reason: str, actor: str, runner_pid: int | None = None
    ) -> RunRecord:
        if to not in _ALLOWED[self.state]:
            raise ValueError(f"illegal transition {self.state.value} -> {to.value}")
        rec = Transition(t=time.time(), src=self.state, dst=to, reason=reason, actor=actor)
        return replace(
            self,
            state=to,
            runner_pid=runner_pid,
            transitions=self.transitions + (rec,),
        )

    def with_decision(self, name: str, value: object) -> RunRecord:
        return replace(
            self,
            decisions={**self.decisions, name: Decision(value=value, t=time.time())},
            pending_decision=None,
        )

    def with_pending(self, pending: PendingDecision) -> RunRecord:
        return replace(self, pending_decision=pending)

    def to_json(self) -> dict:
        return {
            "run": self.run,
            "flow": self.flow,
            "argv": list(self.argv),
            "state": self.state.value,
            "pins": self.pins,
            "created": self.created,
            "runner_pid": self.runner_pid,
            "transitions": [t.to_json() for t in self.transitions],
            "decisions": {k: v.to_json() for k, v in self.decisions.items()},
            "pending_decision": self.pending_decision.to_json() if self.pending_decision else None,
        }

    @staticmethod
    def from_json(d: dict) -> RunRecord:
        return RunRecord(
            run=d["run"],
            flow=d["flow"],
            argv=tuple(d["argv"]),
            state=RunState(d["state"]),
            pins=d["pins"],
            created=d["created"],
            runner_pid=d["runner_pid"],
            transitions=tuple(Transition.from_json(t) for t in d["transitions"]),
            decisions={k: Decision.from_json(v) for k, v in d["decisions"].items()},
            pending_decision=(
                PendingDecision.from_json(d["pending_decision"])
                if d["pending_decision"]
                else None
            ),
        )

    def save(self, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(".json.tmp")
        tmp.write_text(json.dumps(self.to_json(), indent=2) + "\n")
        os.replace(tmp, path)

    @staticmethod
    def load(path: Path) -> RunRecord:
        if not path.is_file():
            raise FileNotFoundError(f"no run record at {path}")
        return RunRecord.from_json(json.loads(path.read_text()))


@dataclass(frozen=True)
class RunnerResult:
    """What `pipe _runner` reports back: exactly one of the three."""

    result: object | None
    pending: PendingDecision | None
    error: str | None

    @staticmethod
    def load(path: Path) -> RunnerResult:
        d = json.loads(path.read_text())
        keys = {"result", "pending_decision", "error"} & d.keys()
        if len(keys) != 1:
            raise ValueError(f"runner_result at {path} must have exactly one of "
                             f"result/pending_decision/error, has {sorted(keys)}")
        return RunnerResult(
            result=d.get("result"),
            pending=(
                PendingDecision.from_json(d["pending_decision"])
                if "pending_decision" in d
                else None
            ),
            error=d.get("error"),
        )


def write_runner_result(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".json.tmp")
    tmp.write_text(json.dumps(payload, indent=2, default=str) + "\n")
    os.replace(tmp, path)
