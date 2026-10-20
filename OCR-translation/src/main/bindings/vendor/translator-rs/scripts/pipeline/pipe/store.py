from __future__ import annotations

import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from .types import Artifact, Kind, Name, RunId, digest_file


def count_lines(path: Path) -> int:
    # zcat -f reads plain files too, so callers never branch on the extension.
    zcat = subprocess.Popen(["zcat", "-f", str(path)], stdout=subprocess.PIPE)
    assert zcat.stdout is not None
    wc = subprocess.Popen(["wc", "-l"], stdin=zcat.stdout, stdout=subprocess.PIPE)
    zcat.stdout.close()
    out, _ = wc.communicate()
    if zcat.wait() != 0:
        raise RuntimeError(f"zcat failed on {path}")
    return int(out.split()[0])


@dataclass(frozen=True)
class Store:
    """The artifact hub on the CPU host. Nothing of value lives only on a rented box.

    Layout under `root` (PIPE_ROOT -- no default, so no one's filesystem layout is
    baked into the repo):
        code/                         the orchestrator, rsynced from the repo
        runs/<run>/artifacts/<name>   the artifacts themselves
        runs/<run>/jobs/<key>/        job dirs (cmd.sh, args.json, log, exit_code)
        runs/<run>/ledger.json        step memoization + artifact index
        leases/<uuid>.json            vast lease -> run/step/expiry mapping
    """

    root: Path

    def run_dir(self, run: RunId) -> Path:
        return self.root / "runs" / str(run)

    def artifacts_dir(self, run: RunId) -> Path:
        return self.run_dir(run) / "artifacts"

    def jobs_dir(self, run: RunId) -> Path:
        return self.run_dir(run) / "jobs"

    def ledger_path(self, run: RunId) -> Path:
        return self.run_dir(run) / "ledger.json"

    def leases_dir(self) -> Path:
        return self.root / "leases"

    def path_for(self, run: RunId, name: Name) -> Path:
        return self.artifacts_dir(run) / str(name)

    def describe(self, name: Name, path: Path, kind: Kind) -> Artifact:
        if not path.is_file():
            raise FileNotFoundError(f"artifact {name} not at {path}")
        return Artifact(
            name=name,
            path=path,
            kind=kind,
            digest=digest_file(path),
            size=path.stat().st_size,
            lines=count_lines(path) if kind is Kind.LINES else None,
        )

    def put(self, run: RunId, name: Name, src: Path, kind: Kind) -> Artifact:
        dst = self.path_for(run, name)
        dst.parent.mkdir(parents=True, exist_ok=True)
        if src.resolve() != dst.resolve():
            shutil.copyfile(src, dst)
        return self.describe(name, dst, kind)
