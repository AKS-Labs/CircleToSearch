from __future__ import annotations

import os
import random
import shlex
import time
import uuid as uuidlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol

from .host import Host, LocalHost
from .ledger import SshInfo
from .registry import ImageRef
from .ssh import SshHost
from .vast import Instance, Lease, Offer, VastApi, VastError

IN_DIR = Path("/work/in")
OUT_DIR = Path("/work/out")
JOB_DIR = Path("/work/job")
SCRIPTS_DIR = Path("/scripts")


class Session(Protocol):
    host: Host

    def job_dir(self, key: str) -> Path: ...
    def ssh_info(self) -> SshInfo | None: ...
    def prepare(self, key: str, inputs: dict[str, Path], scripts: Path) -> None: ...
    def exec_sh(self, image: str, key: str) -> str: ...
    def collect(self, key: str, local_out: Path) -> None: ...
    def close(self, ok: bool) -> None: ...


class Target(Protocol):
    def image_id(self, image: str) -> str: ...
    def open(self, run: str, step: str, key: str, image: str, root: Path) -> Session: ...


@dataclass
class BigserverSession:
    host: Host
    jobs_root: Path
    out_dir: Path
    cpus: int
    _inputs: dict[str, Path] = field(default_factory=dict)
    _scripts: Path = Path("/nonexistent")

    def job_dir(self, key: str) -> Path:
        return self.jobs_root / key

    def ssh_info(self) -> SshInfo | None:
        return None

    def prepare(self, key: str, inputs: dict[str, Path], scripts: Path) -> None:
        self.out_dir.mkdir(parents=True, exist_ok=True)
        for name, path in inputs.items():
            if not path.is_file():
                raise FileNotFoundError(f"input {name} missing at {path}")
        self._inputs = inputs
        self._scripts = scripts

    def exec_sh(self, image: str, key: str) -> str:
        mounts: list[str] = []
        for name, path in sorted(self._inputs.items()):
            mounts += ["-v", f"{path}:{IN_DIR / name}:ro"]
        mounts += ["-v", f"{self.out_dir}:{OUT_DIR}"]
        mounts += ["-v", f"{self.job_dir(key)}:{JOB_DIR}:ro"]
        mounts += ["-v", f"{self._scripts}:{SCRIPTS_DIR}:ro"]
        argv = [
            "docker", "run", "--rm",
            "--cpus", str(self.cpus),
            "--user", f"{os.getuid()}:{os.getgid()}",
            *mounts,
            "-w", str(OUT_DIR),
            image,
            "bash", str(JOB_DIR / "cmd.sh"),
        ]
        return f"set -u\n{shlex.join(argv)}\n"

    def collect(self, key: str, local_out: Path) -> None:
        pass

    def close(self, ok: bool) -> None:
        pass


@dataclass(frozen=True)
class Bigserver:
    cpus: int

    def image_id(self, image: str) -> str:
        return LocalHost().capture(
            ["docker", "image", "inspect", "--format={{.Id}}", image]
        ).strip()

    def open(self, run: str, step: str, key: str, image: str, root: Path) -> Session:
        return BigserverSession(
            host=LocalHost(),
            jobs_root=root / "runs" / run / "jobs",
            out_dir=root / "runs" / run / "out" / key,
            cpus=self.cpus,
        )


@dataclass
class VastSession:
    host: SshHost
    vast: VastApi
    lease: Lease
    instance: Instance

    def job_dir(self, key: str) -> Path:
        return Path("/work/jobs") / key

    def ssh_info(self) -> SshInfo | None:
        return SshInfo(host=self.host.host, port=self.host.port, user=self.host.user)

    def prepare(self, key: str, inputs: dict[str, Path], scripts: Path) -> None:
        self.host.mkdir(OUT_DIR)
        self.host.mkdir(IN_DIR)
        for name, path in inputs.items():
            self.host.push(path, IN_DIR / name)
        self.host.push_dir(scripts, SCRIPTS_DIR)

    def exec_sh(self, image: str, key: str) -> str:
        # The box already IS the container -- there is no docker here.
        return f"set -u\ncd {OUT_DIR}\nbash {self.job_dir(key) / 'cmd.sh'}\n"

    def collect(self, key: str, local_out: Path) -> None:
        local_out.mkdir(parents=True, exist_ok=True)
        self.host.pull_dir(OUT_DIR, local_out)
        for remote, local in (("log", "job.log"), ("metrics.jsonl", "metrics.jsonl")):
            text = self.host.read_file(self.job_dir(key) / remote)
            if text is not None:
                (local_out / local).write_text(text)

    def close(self, ok: bool) -> None:
        self.host.close()
        if not ok:
            # Leave it up to be inspected; the lease's expiry still caps the spend.
            print(
                f"[vast] instance {self.instance.id} kept for inspection until "
                f"{time.strftime('%H:%M', time.localtime(self.lease.expires))}: "
                f"ssh -p {self.host.port} {self.host.user}@{self.host.host}",
                flush=True,
            )
            return
        self.vast.destroy(self.instance.id)
        self.vast.drop_lease(self.lease.uuid)


@dataclass(frozen=True)
class Vast:
    gpu: str
    max_hours: float
    disk_gb: int = 100
    key: Path = Path("~/.ssh/vast_pipe").expanduser()
    min_reliability: float = 0.98
    min_inet_down: float = 300.0
    max_dph: float = 1.0
    ready_timeout: float = 900.0
    tries: int = 3
    geo: tuple[str, ...] = ()
    min_cuda: float = 0.0
    # Data-parallel on ONE box (marian --devices 0 1 --sync-sgd), never across
    # machines — that needs MPI and an interconnect. The image is unchanged:
    # GPU count is a runtime flag, so one marian-cuda serves 1, 2 or CPU.
    num_gpus: int = 1

    def image_id(self, image: str) -> str:
        return ImageRef.parse(image).digest()

    def open(self, run: str, step: str, key: str, image: str, root: Path) -> Session:
        v = VastApi(root / "leases")
        # Leases are identified by the step KEY, not the step name: parallel shards
        # of one step must never resume each other's box.
        resumed = self._resume(v, run, key)
        if resumed is not None:
            return resumed
        return self._rent(v, run, key, image)

    def _resume(self, v: VastApi, run: str, step: str) -> Session | None:
        for lease in v.leases().values():
            if (lease.run, lease.step) != (run, step):
                continue
            try:
                inst = v.instance(lease.instance_id)
            except VastError:
                v.drop_lease(lease.uuid)
                continue
            if not inst.reachable:
                continue
            print(f"[vast] resuming instance {inst.id} for {step}", flush=True)
            return VastSession(host=self._ssh(inst), vast=v, lease=lease, instance=inst)
        return None

    def _rent(self, v: VastApi, run: str, step: str, image: str) -> Session:
        offers = v.search(
            gpu_name=self.gpu,
            min_reliability=self.min_reliability,
            min_inet_down=self.min_inet_down,
            disk_gb=self.disk_gb,
            max_dph=self.max_dph,
            geo_in=self.geo,
            min_cuda=self.min_cuda,
            num_gpus=self.num_gpus,
        )
        if not offers:
            raise VastError(
                f"no {self.gpu} x{self.num_gpus} offers matching the filters")
        # Concurrent renters (shard fan-out) all see the same price-sorted list;
        # sampling the affordable head spreads them across machines instead of
        # racing for the single cheapest offer.
        head = offers[: max(self.tries * 2, 8)]
        random.shuffle(head)
        failures: list[str] = []
        for offer in head[: self.tries]:
            try:
                return self._try_offer(v, offer, run, step, image)
            except (VastError, TimeoutError, RuntimeError) as e:
                failures.append(f"offer {offer.id} ({offer.geolocation}): {e}")
                print(f"[vast] rejected {failures[-1]}", flush=True)
        raise VastError("no offer survived the acceptance probe:\n  " + "\n  ".join(failures))

    def _try_offer(self, v: VastApi, offer: Offer, run: str, step: str, image: str) -> Session:
        uid = uuidlib.uuid4().hex[:12]
        iid = v.create(offer, image=image, disk_gb=self.disk_gb, label=f"pipe:{uid}")
        now = time.time()
        lease = Lease(
            uuid=uid,
            instance_id=iid,
            run=run,
            step=step,
            created=now,
            expires=now + self.max_hours * 3600,
        )
        v.write_lease(lease)
        print(f"[vast] rented {iid} ({offer.gpu_name} @ ${offer.dph_total}/h, {offer.geolocation})",
              flush=True)
        try:
            inst = v.wait_ready(iid, timeout=self.ready_timeout)
            host = self._ssh(inst)
            self._probe(host, offer)
        except Exception:
            v.destroy(iid)
            v.drop_lease(uid)
            raise
        return VastSession(host=host, vast=v, lease=lease, instance=inst)

    def _ssh(self, inst: Instance) -> SshHost:
        ep = inst.endpoint
        assert ep is not None, f"instance {inst.id} has no reachable endpoint"
        if not ep.direct:
            print(f"[vast] {inst.id}: no direct port, falling back to the proxy "
                  f"({ep.host}) — expect ~1 MB/s transfers", flush=True)
        return SshHost(host=ep.host, port=ep.port, user="root", key=self.key)

    def _probe(self, host: SshHost, offer: Offer) -> None:
        """Confirm we got what we paid for before handing the box a 6-hour job."""
        # 600s: vast reports "running" while the image is still pulling, and sshd
        # in a custom image only comes up after that — 180s rejected honest boxes.
        deadline = time.monotonic() + 600
        while True:
            try:
                host.capture(["true"])
                break
            except Exception as e:
                if time.monotonic() > deadline:
                    raise RuntimeError(f"ssh never came up: {e}") from e
                time.sleep(10)
        gpus = host.capture(["nvidia-smi", "--query-gpu=name", "--format=csv,noheader"]).split("\n")
        got = [g.strip() for g in gpus if g.strip()]
        want = self.gpu.replace("_", " ")
        if not any(want in g for g in got):
            raise RuntimeError(f"wanted {want}, box reports {got}")
