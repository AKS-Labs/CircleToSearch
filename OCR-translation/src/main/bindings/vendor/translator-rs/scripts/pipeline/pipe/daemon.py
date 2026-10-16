# piped — the one honest daemon on the hub. It owns run lifecycle (submit /
# abort / decide / ps over $PIPE_ROOT/piped.sock), spawns `pipe _runner <run>`
# per run, folds runner_result.json into run.json on child exit, and absorbs the
# reaper: the 60s tick reaps expired leases, detects dead runners, and salvages
# finished remote jobs so a paid decode is never re-bought.
#
# systemd unit (installed by hand, like the rest of the DESIGN.md
# Prerequisites; not templated by sync.sh):
#
#   # ~/.config/systemd/user/piped.service
#   [Unit]
#   Description=pipe run daemon
#
#   [Service]
#   ExecStart=%h/.local/bin/pipe piped
#   Restart=on-failure
#   RestartSec=5
#
#   [Install]
#   WantedBy=default.target
#
#   systemctl --user enable --now piped   (plus `loginctl enable-linger`)

from __future__ import annotations

import importlib.util
import json
import os
import signal
import socket
import subprocess
import sys
import time
import traceback
from pathlib import Path
from types import ModuleType

from .artstore import read_pins
from .config import Config
from .job import Job, JobState
from .ledger import Ledger
from .reaper import reap
from .runmodel import RunnerResult, RunRecord, RunState
from .ssh import SshHost
from .step import SALVAGED_MARKER
from .store import Store
from .target import OUT_DIR
from .types import Name, RunId, Status
from .vast import VastApi, VastError

FLOWS = Path(__file__).resolve().parents[1] / "flows"

TICK_SECONDS = 60.0


def flow_path(flow: str) -> Path:
    path = FLOWS / f"{flow}.py"
    if not path.is_file():
        known = sorted(f.stem for f in FLOWS.glob("*.py") if f.stem != "__init__")
        raise FileNotFoundError(f"no flow {flow!r}; known: {known}")
    return path


def load_flow(flow: str) -> ModuleType:
    path = flow_path(flow)
    spec = importlib.util.spec_from_file_location("flow", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load flow {path}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def salvage_run(cfg: Config, store: Store, run: RunId) -> list[dict]:
    """For each RUNNING ledger step whose remote job has exited, pull the
    outputs and log into the step's local_out, mark it salvaged, and destroy the
    box. A job still running is left alone — the lease expiry caps its spend and
    a resubmit can resume it."""
    ledger = Ledger(store.ledger_path(run))
    vast = VastApi(store.leases_dir())
    leased = {(l.run, l.step) for l in vast.leases().values()}
    actions: list[dict] = []
    for key, rec in ledger.steps.items():
        if rec.status is not Status.RUNNING or rec.job is None or rec.job.ssh is None:
            continue
        # A RUNNING step with no lease is a GHOST: its box was destroyed but an
        # earlier abort left the ledger entry behind (action "left"). Dialling its
        # long-dead address is what made every later abort of the run fail before
        # it reached the live step — the exact moment you most need abort to work,
        # because a box is burning money. Mark it and move on.
        if (str(run), key) not in leased:
            ledger.finish(key, Status.FAILED, {})
            actions.append({"step": key, "action": "orphaned",
                            "note": "no lease; box already gone, marked failed"})
            continue
        host = SshHost(
            host=rec.job.ssh.host, port=rec.job.ssh.port, user=rec.job.ssh.user,
            key=cfg.vast_key,
        )
        try:
            job = Job(host=host, ref=rec.job, name=Name(rec.step))
            status = job.status()
            if status.state is not JobState.EXITED:
                actions.append({"step": key, "action": "left", "state": status.state.value})
                continue
            local_out = store.run_dir(run) / "out" / key
            host.pull_dir(OUT_DIR, local_out)
            log = host.read_file(rec.job.dir / "log")
            if log is not None:
                (local_out / "job.log").write_text(log)
            (local_out / SALVAGED_MARKER).touch()
            actions.append({"step": key, "action": "salvaged", "exit_code": status.exit_code})
        except (RuntimeError, OSError) as e:
            # Best-effort by design: a step we cannot reach must not stop the loop,
            # because the steps AFTER it may still have boxes to destroy.
            actions.append({"step": key, "action": "unreachable", "error": str(e)})
            continue
        finally:
            host.close()
        for uid, lease in vast.leases().items():
            if (lease.run, lease.step) != (str(run), key):
                continue
            try:
                vast.destroy(lease.instance_id)
                actions.append({"step": key, "action": "destroyed", "instance": lease.instance_id})
            except VastError as e:
                actions.append({"step": key, "action": "destroy_failed", "error": str(e)})
            vast.drop_lease(uid)
    return actions


class Piped:
    def __init__(self, cfg: Config) -> None:
        self.cfg = cfg
        self.store = Store(cfg.root)
        self.procs: dict[int, tuple[str, subprocess.Popen]] = {}

    # ---- run.json is written HERE and nowhere else ----

    def _record_path(self, run: str) -> Path:
        return self.store.run_dir(RunId(run)) / "run.json"

    def _save(self, rec: RunRecord) -> None:
        rec.save(self._record_path(rec.run))

    def _spawn(self, run: str) -> int:
        run_dir = self.store.run_dir(RunId(run))
        # A stale result from the previous attempt must never be folded as fresh.
        (run_dir / "runner_result.json").unlink(missing_ok=True)
        with (run_dir / "runner.log").open("ab") as log:
            proc = subprocess.Popen(
                [sys.executable, "-m", "pipe.cli", "_runner", run],
                stdin=subprocess.DEVNULL,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        self.procs[proc.pid] = (run, proc)
        return proc.pid

    # ---- verbs ----

    def submit(self, run: str, flow: str, argv: list[str]) -> dict:
        RunId(run)
        flow_path(flow)
        path = self._record_path(run)
        if path.is_file():
            old = RunRecord.load(path)
            if old.state.live:
                raise ValueError(f"run {run!r} is {old.state.value}; abort or decide it first")
        pins = read_pins(self.cfg.root, flow)
        rec = RunRecord.new(run, flow, argv, pins)
        self._save(rec)
        pid = self._spawn(run)
        self._save(rec.transition(RunState.RUNNING, "submit", actor="cli", runner_pid=pid))
        return {"ok": True, "run": run, "runner_pid": pid, "pins": pins}

    def abort(self, run: str) -> dict:
        rec = RunRecord.load(self._record_path(run))
        if not rec.state.live:
            raise ValueError(f"run {run!r} is already {rec.state.value}")
        killed = None
        if rec.runner_pid is not None:
            entry = self.procs.pop(rec.runner_pid, None)
            try:
                os.killpg(rec.runner_pid, signal.SIGTERM)
                killed = rec.runner_pid
            except ProcessLookupError:
                pass
            if entry is not None:
                try:
                    entry[1].wait(timeout=15)
                except subprocess.TimeoutExpired:
                    os.killpg(rec.runner_pid, signal.SIGKILL)
                    entry[1].wait()
        salvaged = salvage_run(self.cfg, self.store, RunId(run))
        vast = VastApi(self.store.leases_dir())
        torn_down = []
        for uid, lease in vast.leases().items():
            if lease.run != run:
                continue
            try:
                vast.destroy(lease.instance_id)
                torn_down.append({"instance": lease.instance_id, "step": lease.step})
            except VastError as e:
                torn_down.append(
                    {"instance": lease.instance_id, "step": lease.step, "error": str(e)}
                )
            vast.drop_lease(uid)
        self._save(rec.transition(RunState.ABORTED, "abort", actor="cli"))
        return {"ok": True, "run": run, "runner_pid_killed": killed,
                "salvaged": salvaged, "instances": torn_down}

    def decide(self, run: str, name: str, value: object) -> dict:
        rec = RunRecord.load(self._record_path(run))
        if rec.state is not RunState.NEEDS_DECISION:
            raise ValueError(f"run {run!r} is {rec.state.value}, not parked on a decision")
        if rec.pending_decision is None or rec.pending_decision.name != name:
            pending = rec.pending_decision.name if rec.pending_decision else None
            raise ValueError(f"run {run!r} is waiting on decision {pending!r}, not {name!r}")
        rec = rec.with_decision(name, value)
        self._save(rec)
        pid = self._spawn(run)
        self._save(
            rec.transition(RunState.RUNNING, f"decide {name}", actor="cli", runner_pid=pid)
        )
        return {"ok": True, "run": run, "decision": name, "runner_pid": pid}

    def ps(self) -> dict:
        return {"ok": True, "runs": list_runs(self.store)}

    # ---- lifecycle ----

    def _fold(self, run: str, exit_code: int | None) -> None:
        rec = RunRecord.load(self._record_path(run))
        if rec.state is not RunState.RUNNING:
            return
        result_path = self.store.run_dir(RunId(run)) / "runner_result.json"
        if not result_path.is_file():
            self._save(rec.transition(
                RunState.FAILED, f"runner exited {exit_code} with no result", actor="piped"
            ))
            return
        rr = RunnerResult.load(result_path)
        if rr.error is not None:
            self._save(rec.transition(RunState.FAILED, rr.error, actor="piped"))
        elif rr.pending is not None:
            self._save(
                rec.with_pending(rr.pending).transition(
                    RunState.NEEDS_DECISION,
                    f"decision {rr.pending.name!r} pending",
                    actor="piped",
                )
            )
        else:
            self._save(rec.transition(RunState.DONE, "runner result", actor="piped"))

    def _poll_children(self) -> None:
        for pid in list(self.procs):
            run, proc = self.procs[pid]
            code = proc.poll()
            if code is None:
                continue
            del self.procs[pid]
            self._fold(run, code)

    def _tick(self) -> None:
        try:
            reap(VastApi(self.store.leases_dir()))
        except Exception:
            traceback.print_exc()
        runs_dir = self.store.root / "runs"
        if not runs_dir.is_dir():
            return
        for path in runs_dir.glob("*/run.json"):
            try:
                rec = RunRecord.load(path)
                if rec.state is not RunState.RUNNING or rec.runner_pid is None:
                    continue
                if rec.runner_pid in self.procs or _pid_alive(rec.runner_pid):
                    continue
                # Not our child (daemon restarted) or already reaped: if the
                # runner managed to report, fold that; otherwise it died.
                result_path = path.parent / "runner_result.json"
                if result_path.is_file():
                    self._fold(rec.run, None)
                    continue
                salvage_run(self.cfg, self.store, RunId(rec.run))
                self._save(rec.transition(RunState.FAILED, "runner died", actor="piped"))
            except Exception:
                traceback.print_exc()

    # ---- socket ----

    def _dispatch(self, req: dict) -> dict:
        verb = req.get("verb")
        if verb == "submit":
            return self.submit(req["run"], req["flow"], req.get("argv", []))
        if verb == "abort":
            return self.abort(req["run"])
        if verb == "decide":
            return self.decide(req["run"], req["name"], req["value"])
        if verb == "ps":
            return self.ps()
        raise ValueError(f"unknown verb {verb!r}")

    def _handle(self, conn: socket.socket) -> None:
        try:
            with conn, conn.makefile("rw", encoding="utf-8") as f:
                line = f.readline()
                if not line.strip():
                    return
                try:
                    resp = self._dispatch(json.loads(line))
                except Exception as e:
                    resp = {"ok": False, "error": str(e)}
                f.write(json.dumps(resp, default=str) + "\n")
                f.flush()
        except OSError:
            pass

    def serve(self) -> None:
        sock_path = self.cfg.root / "piped.sock"
        if sock_path.exists():
            probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            try:
                probe.connect(str(sock_path))
            except (ConnectionRefusedError, FileNotFoundError):
                sock_path.unlink()
            else:
                probe.close()
                raise RuntimeError(f"another piped is already serving {sock_path}")
        srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        srv.bind(str(sock_path))
        srv.listen()
        srv.settimeout(1.0)
        print(f"[piped] serving {sock_path}", flush=True)
        last_tick = 0.0
        try:
            while True:
                try:
                    conn, _ = srv.accept()
                except socket.timeout:
                    pass
                else:
                    self._handle(conn)
                self._poll_children()
                if time.monotonic() - last_tick >= TICK_SECONDS:
                    self._tick()
                    last_tick = time.monotonic()
        finally:
            srv.close()
            sock_path.unlink(missing_ok=True)


def _pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def list_runs(store: Store) -> list[dict]:
    runs_dir = store.root / "runs"
    if not runs_dir.is_dir():
        return []
    rows = []
    for path in sorted(runs_dir.glob("*/run.json")):
        rec = RunRecord.load(path)
        rows.append(
            {
                "run": rec.run,
                "flow": rec.flow,
                "state": rec.state.value,
                "created": rec.created,
                "runner_pid": rec.runner_pid,
                "pending_decision": (
                    rec.pending_decision.to_json() if rec.pending_decision else None
                ),
            }
        )
    return rows


def serve(cfg: Config) -> None:
    Piped(cfg).serve()
