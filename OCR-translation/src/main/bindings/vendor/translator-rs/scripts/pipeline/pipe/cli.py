from __future__ import annotations

import argparse
import json
import re
import socket
import sys
import threading
import time
import traceback
from pathlib import Path

from . import daemon
from .artstore import ArtifactType, ArtStore, Score
from .config import Config
from .host import Host, LocalHost
from .ledger import JobRef, Ledger
from .reaper import reap
from .registry import ImageRef
from .runmodel import RunRecord, RunState, write_runner_result
from .ssh import SshHost
from .store import Store
from .step import NeedsDecision, Run
from .top import top_rows
from .types import Kind, Name, RunId, Status
from .vast import VastApi


def _emit(obj: dict) -> None:
    json.dump(obj, sys.stdout, indent=2, default=str)
    sys.stdout.write("\n")


def _open(run: str, cfg: Config) -> Run:
    rid = RunId(run)
    store = Store(cfg.root)
    store.run_dir(rid).mkdir(parents=True, exist_ok=True)
    return Run(
        id=rid,
        store=store,
        ledger=Ledger(store.ledger_path(rid)),
        scripts=cfg.scripts,
        art=ArtStore(cfg.root, cfg.store),
    )


def _art(cfg: Config) -> ArtStore:
    return ArtStore(cfg.root, cfg.store)


def _sock_path(cfg: Config) -> Path:
    return cfg.root / "piped.sock"


def _daemon_request(cfg: Config, req: dict) -> dict:
    path = _sock_path(cfg)
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        s.connect(str(path))
    except (FileNotFoundError, ConnectionRefusedError) as e:
        raise RuntimeError(
            f"piped is not running (no socket at {path}); start it via the "
            "systemd unit in pipe/daemon.py"
        ) from e
    with s, s.makefile("rw", encoding="utf-8") as f:
        f.write(json.dumps(req, default=str) + "\n")
        f.flush()
        line = f.readline()
    if not line.strip():
        raise RuntimeError("piped closed the connection without a response")
    return json.loads(line)


def cmd_adopt(a: argparse.Namespace) -> int:
    r = _open(a.run, a.cfg)
    try:
        r.ledger.adopt(a.step, a.key)
    except (KeyError, ValueError) as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit({"ok": True, "run": a.run, "step": a.step, "adopt_from": a.key,
           "note": "next run of this step reuses these outputs instead of re-running"})
    return 0


def cmd_put(a: argparse.Namespace) -> int:
    r = _open(a.run, a.cfg)
    art = r.store.put(r.id, Name(a.name), Path(a.path), Kind(a.kind))
    r.ledger.register(art)
    _emit({"ok": True, "artifact": art.to_json()})
    return 0


def cmd_status(a: argparse.Namespace) -> int:
    r = _open(a.run, a.cfg)
    _emit(
        {
            "run": a.run,
            "steps": [
                {
                    "key": s.key,
                    "step": s.step,
                    "status": s.status.value,
                    "seconds": round(s.finished - s.started, 1) if s.finished else None,
                    "log": str(s.log),
                    "outputs": sorted(s.outputs),
                }
                for s in r.ledger.steps.values()
            ],
            "artifacts": {k: v.to_json() for k, v in r.ledger.artifacts.items()},
        }
    )
    return 0


def _job_host(cfg: Config, job: JobRef) -> Host:
    if job.ssh is None:
        return LocalHost()
    return SshHost(host=job.ssh.host, port=job.ssh.port, user=job.ssh.user, key=cfg.vast_key)


def _follow(targets: list[tuple[str, Host, Path, int]]) -> int:
    """Disposable live view: tail -f each remote file from its local pumped
    offset, print until Ctrl-C. Never writes to the pumped record."""

    def watch(prefix: str, host: Host, remote: Path, offset: int) -> None:
        buf = b""
        try:
            for chunk in host.stream(["tail", "-f", "-c", f"+{offset + 1}", str(remote)]):
                buf += chunk
                *lines, buf = buf.split(b"\n")
                for ln in lines:
                    print(prefix + ln.decode(errors="replace"), flush=True)
        except Exception as e:
            print(f"{prefix}[stream ended: {e}]", file=sys.stderr, flush=True)

    threads = [
        threading.Thread(target=watch, args=t, daemon=True) for t in targets
    ]
    for t in threads:
        t.start()
    try:
        while any(t.is_alive() for t in threads):
            time.sleep(0.5)
    except KeyboardInterrupt:
        pass
    return 0


def cmd_log(a: argparse.Namespace) -> int:
    r = _open(a.run, a.cfg)
    matches = [s for s in r.ledger.steps.values() if s.step == a.step]
    if not matches:
        _emit({"ok": False, "error": f"no step {a.step} in run {a.run}"})
        return 1
    rec = matches[-1]
    if a.follow:
        if rec.job is None:
            _emit({"ok": False, "error": f"step {a.step} has no job to follow"})
            return 1
        offset = rec.log.stat().st_size if rec.log.is_file() else 0
        if offset:
            sys.stdout.buffer.write(rec.log.read_bytes())
            sys.stdout.flush()
        return _follow([("", _job_host(a.cfg, rec.job), rec.job.dir / "log", offset)])
    text = ""
    source = str(rec.log)
    if rec.log.is_file():
        text = rec.log.read_text()
    elif rec.job is not None and rec.job.ssh is not None:
        # Still on a rented box: read it live rather than waiting for the step to finish.
        h = SshHost(
            host=rec.job.ssh.host, port=rec.job.ssh.port, user=rec.job.ssh.user, key=a.cfg.vast_key
        )
        text = h.read_file(rec.job.dir / "log") or ""
        source = f"{rec.job.host}:{rec.job.dir / 'log'}"
        h.close()
    lines = text.splitlines()
    if a.grep:
        pat = re.compile(a.grep)
        lines = [ln for ln in lines if pat.search(ln)]
    if a.tail:
        lines = lines[-a.tail :]
    _emit({"ok": True, "log": source, "matched": len(lines), "lines": lines})
    return 0


def cmd_run(a: argparse.Namespace) -> int:
    try:
        resp = _daemon_request(
            a.cfg, {"verb": "submit", "run": a.run, "flow": a.flow, "argv": a.rest}
        )
    except RuntimeError as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit(resp)
    return 0 if resp.get("ok") else 1


def cmd_runner(a: argparse.Namespace) -> int:
    """Hidden: spawned by piped, one per run, as its own process group. Reports
    only through runner_result.json; run.json stays piped's to write."""
    cfg = a.cfg
    rid = RunId(a.run_id)
    store = Store(cfg.root)
    rec = RunRecord.load(store.run_dir(rid) / "run.json")
    result_path = store.run_dir(rid) / "runner_result.json"
    run = Run(
        id=rid,
        store=store,
        ledger=Ledger(store.ledger_path(rid)),
        scripts=cfg.scripts,
        flow=rec.flow,
        pins=dict(rec.pins),
        decisions={k: v.value for k, v in rec.decisions.items()},
        art=ArtStore(cfg.root, cfg.store),
    )
    try:
        mod = daemon.load_flow(rec.flow)
        result = mod.main(run, list(rec.argv))
    except NeedsDecision:
        # run.decide already wrote the pending_decision payload.
        return 0
    except BaseException as e:
        traceback.print_exc()
        write_runner_result(result_path, {"error": f"{type(e).__name__}: {e}"})
        return 1
    write_runner_result(result_path, {"result": result})
    return 0


def cmd_wait(a: argparse.Namespace) -> int:
    path = Store(a.cfg.root).run_dir(RunId(a.run)) / "run.json"
    while True:
        if not path.is_file():
            _emit({"ok": False, "error": f"no run record at {path}"})
            return 1
        rec = RunRecord.load(path)
        if rec.state is RunState.DONE:
            _emit({"ok": True, "run": a.run, "state": rec.state.value})
            return 0
        if rec.state is RunState.NEEDS_DECISION:
            _emit(
                {
                    "ok": True,
                    "run": a.run,
                    "state": rec.state.value,
                    "pending_decision": (
                        rec.pending_decision.to_json() if rec.pending_decision else None
                    ),
                }
            )
            return 2
        if rec.state in (RunState.FAILED, RunState.ABORTED):
            _emit({"ok": False, "run": a.run, "state": rec.state.value,
                   "reason": rec.transitions[-1].reason if rec.transitions else None})
            return 1
        time.sleep(a.poll)


def cmd_ps(a: argparse.Namespace) -> int:
    # Reads run.json directly, so it works daemon-down.
    _emit({"runs": daemon.list_runs(Store(a.cfg.root))})
    return 0


def cmd_decide(a: argparse.Namespace) -> int:
    try:
        value: object = json.loads(a.value)
    except json.JSONDecodeError:
        value = a.value
    try:
        resp = _daemon_request(
            a.cfg, {"verb": "decide", "run": a.run, "name": a.name, "value": value}
        )
    except RuntimeError as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit(resp)
    return 0 if resp.get("ok") else 1


def cmd_abort(a: argparse.Namespace) -> int:
    try:
        resp = _daemon_request(a.cfg, {"verb": "abort", "run": a.run})
    except RuntimeError as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit(resp)
    return 0 if resp.get("ok") else 1


def cmd_piped(a: argparse.Namespace) -> int:
    daemon.serve(a.cfg)
    return 0


def cmd_top(a: argparse.Namespace) -> int:
    if not a.follow:
        _emit({"steps": top_rows(a.cfg.root)})
        return 0
    runs_dir = a.cfg.root / "runs"
    targets: list[tuple[str, Host, Path, int]] = []
    if runs_dir.is_dir():
        for ledger_path in sorted(runs_dir.glob("*/ledger.json")):
            run = ledger_path.parent.name
            for rec in Ledger(ledger_path).steps.values():
                if rec.status is not Status.RUNNING or rec.job is None:
                    continue
                pumped = rec.log.parent / "metrics.jsonl"
                offset = pumped.stat().st_size if pumped.is_file() else 0
                targets.append(
                    (
                        f"[{run}/{rec.step}] ",
                        _job_host(a.cfg, rec.job),
                        rec.job.dir / "metrics.jsonl",
                        offset,
                    )
                )
    if not targets:
        _emit({"ok": False, "error": "no running steps with a live job to follow"})
        return 1
    return _follow(targets)


def cmd_publish(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    typ = ArtifactType(a.type)
    try:
        parents = tuple(art.resolve(p) for p in a.parent)
        if typ.family == "model":
            scores = []
            for ref in a.score:
                s = art.get(art.resolve(ref))
                if not isinstance(s, Score):
                    raise ValueError(f"{ref} is {s.stored.type.value}, not eval/score")
                scores.append(s)
            published = art.publish_model(
                Path(a.path), typ, scores, parents=parents, label=a.label
            )
        else:
            kind = Kind(a.kind) if a.kind else None
            published = art.publish(
                Path(a.path), typ, parents=parents, label=a.label, kind=kind
            )
    except (ValueError, KeyError, FileNotFoundError) as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit({"ok": True, "artifact": published.stored.to_json()})
    return 0


def cmd_alias(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    try:
        if a.alias_cmd == "set":
            digest = art.resolve(a.ref)
            art.alias_set(a.alias, digest)
            _emit({"ok": True, "alias": a.alias, "digest": str(digest)})
        else:
            _emit({"ok": True, "aliases": art.alias_list()})
    except (ValueError, KeyError) as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    return 0


def cmd_pin(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    try:
        digest = art.pin(a.flow, a.name, a.ref)
    except (ValueError, KeyError) as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit({"ok": True, "flow": a.flow, "name": a.name, "digest": str(digest)})
    return 0


def cmd_pins(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    pins = art.pins(a.flow)
    rows = {}
    for name, digest in sorted(pins.items()):
        try:
            stored = art.get(digest).stored
            rows[name] = {"digest": digest, "type": stored.type.value, "label": stored.label}
        except KeyError:
            rows[name] = {"digest": digest, "type": None, "label": None}
    _emit({"ok": True, "flow": a.flow, "pins": rows})
    return 0


def cmd_artifacts(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    typ = ArtifactType(a.type) if a.type else None
    _emit({"artifacts": [s.to_json() for s in art.list(typ)]})
    return 0


def cmd_scores(a: argparse.Namespace) -> int:
    art = _art(a.cfg)
    try:
        model = art.resolve(a.model) if a.model else None
        rows = art.score_table(model=model, pair=a.pair)
    except (ValueError, KeyError) as e:
        _emit({"ok": False, "error": str(e)})
        return 1
    _emit({"scores": rows})
    return 0


def cmd_offers(a: argparse.Namespace) -> int:
    v = VastApi(Store(a.cfg.root).leases_dir())
    offers = v.search(
        gpu_name=a.gpu,
        min_reliability=a.min_reliability,
        min_inet_down=a.min_inet_down,
        disk_gb=a.disk,
        max_dph=a.max_dph,
    )
    _emit(
        {
            "matched": len(offers),
            "offers": [
                {
                    "id": o.id,
                    "gpu": o.gpu_name,
                    "dph": o.dph_total,
                    "reliability": round(o.reliability, 4),
                    "inet_down": o.inet_down,
                    "disk": o.disk_space,
                    "where": o.geolocation,
                }
                for o in offers[: a.limit]
            ],
        }
    )
    return 0


def cmd_version(a: argparse.Namespace) -> int:
    _emit({"synced": a.cfg.synced_at(), "root": str(a.cfg.root), "scripts": str(a.cfg.scripts)})
    return 0


def cmd_image(a: argparse.Namespace) -> int:
    _emit({"image": a.image, "digest": ImageRef.parse(a.image).digest()})
    return 0


def cmd_boxes(a: argparse.Namespace) -> int:
    v = VastApi(Store(a.cfg.root).leases_dir())
    leases = v.leases()
    rows = []
    for i in v.instances():
        uid = i.label.removeprefix("pipe:") if i.label and i.label.startswith("pipe:") else None
        rows.append(
            {
                "id": i.id,
                "status": i.status,
                "gpu": i.gpu_name,
                "label": i.label,
                "ssh": (f"{i.endpoint.host}:{i.endpoint.port}"
                        f"{'' if i.endpoint.direct else ' (proxy)'}")
                       if i.endpoint else None,
                "lease": leases[uid].to_json() if uid and uid in leases else None,
            }
        )
    _emit({"instances": rows})
    return 0


def cmd_destroy(a: argparse.Namespace) -> int:
    v = VastApi(Store(a.cfg.root).leases_dir())
    v.destroy(a.instance_id)
    for uid, lease in v.leases().items():
        if lease.instance_id == a.instance_id:
            v.drop_lease(uid)
    _emit({"ok": True, "destroyed": a.instance_id})
    return 0


def cmd_reap(a: argparse.Namespace) -> int:
    v = VastApi(Store(a.cfg.root).leases_dir())
    actions = reap(v, dry_run=a.dry_run)
    _emit(
        {
            "ok": True,
            "dry_run": a.dry_run,
            "actions": [
                {"instance": r.instance_id, "reason": r.reason, "destroyed": r.destroyed}
                for r in actions
            ],
        }
    )
    return 0


def main() -> int:
    p = argparse.ArgumentParser(prog="pipe")
    p.add_argument("--run", default="adhoc")
    sub = p.add_subparsers(dest="cmd", required=True)

    sp = sub.add_parser(
        "adopt",
        help="reuse a done step's outputs under its next key (for a no-op script edit)",
    )
    sp.add_argument("step", help="step name, e.g. kd_decode")
    sp.add_argument("key", help="existing done key whose outputs to reuse, e.g. kd_decode@1f2d60292ddb")
    sp.set_defaults(fn=cmd_adopt)

    sp = sub.add_parser("put", help="promote a file into the per-run store under a name")
    sp.add_argument("name")
    sp.add_argument("path")
    sp.add_argument("--kind", choices=[k.value for k in Kind], default=Kind.LINES.value)
    sp.set_defaults(fn=cmd_put)

    sp = sub.add_parser("status", help="ledger view of the run")
    sp.set_defaults(fn=cmd_status)

    sp = sub.add_parser("log", help="read a step's log; live from the box if still running")
    sp.add_argument("step")
    sp.add_argument("--grep")
    sp.add_argument("--tail", type=int)
    sp.add_argument("-f", "--follow", action="store_true",
                    help="stream the remote log live from the pumped offset until Ctrl-C")
    sp.set_defaults(fn=cmd_log)

    sp = sub.add_parser(
        "top", help="latest telemetry per running step, from the pumped files"
    )
    sp.add_argument("-f", "--follow", action="store_true",
                    help="stream each running step's metrics.jsonl live until Ctrl-C")
    sp.set_defaults(fn=cmd_top)

    sp = sub.add_parser("run", help="submit a flow to piped and print the run id")
    sp.add_argument("flow", help="flow name, e.g. align_only")
    sp.add_argument("rest", nargs="*")
    sp.set_defaults(fn=cmd_run)

    sp = sub.add_parser("_runner")
    sp.add_argument("run_id")
    sp.set_defaults(fn=cmd_runner)

    sp = sub.add_parser(
        "wait", help="poll run.json; exit 0 done, 2 needs_decision, 1 failed/aborted"
    )
    sp.add_argument("--poll", type=float, default=5.0)
    sp.set_defaults(fn=cmd_wait)

    sp = sub.add_parser("ps", help="list runs with state (reads files; works daemon-down)")
    sp.set_defaults(fn=cmd_ps)

    sp = sub.add_parser("decide", help="record a decision and respawn the parked run")
    sp.add_argument("name")
    sp.add_argument("value", help="JSON if it parses, else a plain string")
    sp.set_defaults(fn=cmd_decide)

    sp = sub.add_parser(
        "abort", help="kill a run's runner, salvage finished jobs, destroy its boxes"
    )
    sp.set_defaults(fn=cmd_abort)

    sp = sub.add_parser("piped", help="run the pipe daemon (normally under systemd)")
    sp.set_defaults(fn=cmd_piped)

    sp = sub.add_parser("publish", help="publish a file into the cross-run store")
    sp.add_argument("path")
    sp.add_argument("--type", required=True, choices=[t.value for t in ArtifactType])
    sp.add_argument("--parent", action="append", default=[],
                    help="parent digest/alias; repeatable")
    sp.add_argument("--label")
    sp.add_argument("--kind", choices=[k.value for k in Kind],
                    help="override the per-family default line counting")
    sp.add_argument("--score", action="append", default=[],
                    help="score digest/alias; required (repeatable) for model/*")
    sp.set_defaults(fn=cmd_publish)

    sp = sub.add_parser("alias", help="mutable names, resolved to a digest only at pin time")
    alias_sub = sp.add_subparsers(dest="alias_cmd", required=True)
    ap_set = alias_sub.add_parser("set")
    ap_set.add_argument("alias", help="ns/name, e.g. uig/latest")
    ap_set.add_argument("ref", help="digest, digest prefix, or existing alias")
    ap_set.set_defaults(fn=cmd_alias)
    ap_list = alias_sub.add_parser("list")
    ap_list.set_defaults(fn=cmd_alias)

    sp = sub.add_parser("pin", help="resolve a ref now and pin it for a flow")
    sp.add_argument("flow")
    sp.add_argument("name")
    sp.add_argument("ref", help="alias or digest; resolved to a digest immediately")
    sp.set_defaults(fn=cmd_pin)

    sp = sub.add_parser("pins", help="a flow's pinned inputs")
    sp.add_argument("flow")
    sp.set_defaults(fn=cmd_pins)

    sp = sub.add_parser("artifacts", help="list cross-run store meta")
    sp.add_argument("--type", choices=[t.value for t in ArtifactType])
    sp.set_defaults(fn=cmd_artifacts)

    sp = sub.add_parser(
        "scores", help="every (model, evalset, metric) cell; no aggregation, no best"
    )
    sp.add_argument("--model", help="digest/alias to filter on")
    sp.add_argument("--pair", help="alias namespace to filter on")
    sp.set_defaults(fn=cmd_scores)

    sp = sub.add_parser("offers", help="vast offers matching the rental filters")
    sp.add_argument("--gpu", default="RTX_4090")
    sp.add_argument("--disk", type=int, default=100)
    sp.add_argument("--min-reliability", type=float, default=0.98)
    sp.add_argument("--min-inet-down", type=float, default=300.0)
    sp.add_argument("--max-dph", type=float, default=1.0)
    sp.add_argument("--limit", type=int, default=8)
    sp.set_defaults(fn=cmd_offers)

    sp = sub.add_parser("version", help="when the deployed code was last synced")
    sp.set_defaults(fn=cmd_version)

    sp = sub.add_parser("image", help="resolve an image tag to its registry digest")
    sp.add_argument("image")
    sp.set_defaults(fn=cmd_image)

    sp = sub.add_parser("boxes", help="live vast instances and their leases")
    sp.set_defaults(fn=cmd_boxes)

    sp = sub.add_parser("destroy", help="destroy an instance and drop its lease")
    sp.add_argument("instance_id", type=int)
    sp.set_defaults(fn=cmd_destroy)

    sp = sub.add_parser("reap", help="standalone emergency reap for daemon-down")
    sp.add_argument("--dry-run", action="store_true")
    sp.set_defaults(fn=cmd_reap)

    a = p.parse_args()
    a.cfg = Config.load()
    return a.fn(a)


if __name__ == "__main__":
    raise SystemExit(main())
