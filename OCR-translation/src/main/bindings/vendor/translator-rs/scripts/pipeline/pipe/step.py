from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from .artstore import ArtStore, StoredWrapper
from .job import Job, JobState
from .ledger import JobRef, Ledger
from .runmodel import PendingDecision, write_runner_result
from .store import Store
from .target import IN_DIR, OUT_DIR, SCRIPTS_DIR, Target
from .types import Artifact, Kind, Name, RunId, Status, digest_file, digest_json

# Written by piped's salvage: the remote job finished but the flow process died
# before collecting, and the outputs were pulled into local_out on our behalf.
SALVAGED_MARKER = ".salvaged"


class NeedsDecision(Exception):
    """A flow parked at a human gate; the runner catches this to exit cleanly."""

    def __init__(self, name: str, payload_path: Path | None) -> None:
        super().__init__(f"decision {name!r} pending")
        self.name = name
        self.payload_path = payload_path


@dataclass(frozen=True)
class Ctx:
    script: Path
    out_dir: Path

    def inp(self, name: str) -> Path:
        return IN_DIR / name

    def out(self, rel: str) -> Path:
        return self.out_dir / rel


@dataclass(frozen=True)
class Output:
    rel: str
    kind: Kind


@dataclass(frozen=True)
class StepDef:
    name: str
    image: str
    target: Target
    script: str
    outputs: dict[str, Output]
    fn: Callable[[Ctx], list[str]]
    deps: tuple[str, ...] = ()


def step(
    *,
    image: str,
    target: Target,
    script: str,
    outputs: dict[str, Output],
    deps: tuple[str, ...] = (),
) -> Callable[[Callable[[Ctx], list[str]]], StepDef]:
    """Declare a step.

    `deps` names the OTHER files in the scripts dir this step's behaviour depends
    on — the libraries its wrapper invokes. Only the wrapper is digested into the
    key otherwise, so `prep_step.sh` calling `prep_data.py` hashes the six-line
    adapter and not the thousand lines that decide what the corpus contains. On
    2026-07-21 a placeholder-regex bug corrupted the UI register; the fix changed
    no `.sh`, so the step would have been served from the memo with the corrupt
    pool still in it.
    """

    def deco(fn: Callable[[Ctx], list[str]]) -> StepDef:
        return StepDef(
            name=fn.__name__,
            image=image,
            target=target,
            script=script,
            outputs=outputs,
            fn=fn,
            deps=tuple(deps),
        )

    return deco


def _as_artifact(name: str, value: Artifact | StoredWrapper) -> Artifact:
    if isinstance(value, Artifact):
        return value
    s = value.stored
    return Artifact(
        name=Name(name),
        path=s.path,
        kind=Kind.LINES if s.lines is not None else Kind.BLOB,
        digest=s.digest,
        size=s.size,
        lines=s.lines,
    )


@dataclass
class Run:
    id: RunId
    store: Store
    ledger: Ledger
    scripts: Path
    flow: str | None = None
    # Snapshotted from run.json at start, so provenance survives repinning.
    pins: dict[str, str] = field(default_factory=dict)
    decisions: dict[str, object] = field(default_factory=dict)
    art: ArtStore | None = None

    def pinned(self, name: str) -> StoredWrapper:
        flow = self.flow or "<flow>"
        if self.art is None:
            raise RuntimeError(f"run {self.id} has no artifact store; pins cannot resolve")
        if name not in self.pins:
            raise KeyError(
                f"no pin {name!r} for flow {flow}; "
                f"run: pipe pin {flow} {name} <alias|digest>"
            )
        return self.art.get(self.pins[name])

    def decide(self, name: str, payload: Path | None = None) -> object:
        if name in self.decisions:
            return self.decisions[name]
        pending = PendingDecision(name=name, payload_path=str(payload) if payload else None)
        write_runner_result(
            self.store.run_dir(self.id) / "runner_result.json",
            {"pending_decision": pending.to_json()},
        )
        raise NeedsDecision(name, payload)

    def do(
        self,
        defn: StepDef,
        timeout: float,
        args: dict | None = None,
        **inputs: Artifact | StoredWrapper,
    ) -> dict[str, Artifact]:
        args = args or {}
        arts = {name: _as_artifact(name, v) for name, v in inputs.items()}
        # Before open(): a memoized step must not rent a box to find out it has nothing to do.
        key = self._key(defn, arts, args)

        cached = self.ledger.get(key)
        if cached is not None and cached.status is Status.DONE:
            return cached.outputs

        # An operator asserted (via `pipe adopt`) that the edit which changed this
        # key cannot change the output, so reuse the previous run's artifacts rather
        # than pay for the step again. Recorded with provenance, never silent.
        inherited = self.ledger.take_adoption(defn.name)
        if inherited is not None:
            self.ledger.begin(key, defn.name, inherited.log, inherited.job)
            self.ledger.finish(key, Status.DONE, inherited.outputs, adopted_from=inherited.key)
            return inherited.outputs

        local_out = self.store.run_dir(self.id) / "out" / key

        # piped already pulled the finished job's outputs and destroyed the box,
        # so opening the target would rent a fresh one for nothing.
        if (local_out / SALVAGED_MARKER).is_file():
            produced = self._collect(defn, local_out)
            self.ledger.finish(key, Status.DONE, produced)
            return produced

        in_paths = {name: art.path for name, art in arts.items()}

        session = defn.target.open(
            run=str(self.id), step=defn.name, key=key, image=defn.image, root=self.store.root
        )
        ok = False
        try:
            job = Job(
                host=session.host,
                ref=JobRef(session.host.name, session.job_dir(key), session.ssh_info()),
                name=Name(defn.name),
            )
            status = job.status()
            if status.state is JobState.EXITED and not status.ok:
                # An EXITED-ok job here means we died before collecting; that must not relaunch.
                job.reset()
                status = job.status()
            if status.state is JobState.ABSENT:
                session.prepare(key, in_paths, self.scripts)
                payload = defn.fn(Ctx(script=SCRIPTS_DIR / defn.script, out_dir=OUT_DIR))
                self.ledger.begin(key, defn.name, local_out / "job.log", job.ref)
                job.launch(session.exec_sh(defn.image, key), [str(p) for p in payload], args)

            status = job.wait(
                timeout=timeout,
                log_dest=local_out / "job.log",
                metrics_dest=local_out / "metrics.jsonl",
            )
            session.collect(key, local_out)
            if not status.ok:
                self.ledger.finish(key, Status.FAILED, {})
                raise RuntimeError(
                    f"step {defn.name} exited {status.exit_code}; log: {local_out / 'job.log'}"
                )
            produced = self._collect(defn, local_out)
            ok = True
        finally:
            session.close(ok)

        self.ledger.finish(key, Status.DONE, produced)
        return produced

    def producing_step(self, art: Artifact) -> str | None:
        for key, rec in self.ledger.steps.items():
            if any(o.digest == art.digest for o in rec.outputs.values()):
                return key
        return None

    def _collect(self, defn: StepDef, out_dir: Path) -> dict[str, Artifact]:
        produced: dict[str, Artifact] = {}
        for name, spec in defn.outputs.items():
            path = out_dir / spec.rel
            if not path.is_file():
                raise RuntimeError(
                    f"step {defn.name} exited 0 but produced no {spec.rel} at {path}"
                )
            produced[name] = self.store.describe(Name(name), path, spec.kind)
        return produced

    def _key(self, defn: StepDef, inputs: dict[str, Artifact], args: dict) -> str:
        script_path = self.scripts / defn.script
        if not script_path.is_file():
            raise FileNotFoundError(f"step {defn.name} names a missing script: {script_path}")
        deps = {}
        for rel in sorted(defn.deps):
            dep_path = self.scripts / rel
            if not dep_path.is_file():
                raise FileNotFoundError(
                    f"step {defn.name} declares a missing dep: {dep_path}"
                )
            deps[rel] = str(digest_file(dep_path))
        payload = {
            "step": defn.name,
            "image": defn.target.image_id(defn.image),
            "script": str(digest_file(script_path)),
            "deps": deps,
            "inputs": {k: str(v.digest) for k, v in sorted(inputs.items())},
            "args": args,
        }
        return f"{defn.name}@{digest_json(payload).short}"
