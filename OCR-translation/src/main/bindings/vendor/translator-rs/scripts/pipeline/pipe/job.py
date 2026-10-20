from __future__ import annotations

import json
import shlex
import time
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

from .host import Host
from .ledger import JobRef
from .types import Name

# exit_code is the only completion signal: a log marker gets scrolled past, and the
# tail-for-"Training finished" approach has already lost us a run.
WRAPPER = """#!/bin/bash
set -u
JOB=$1
cd "$JOB"
[ -f exit_code ] && exit 0
if [ -f pid ] && kill -0 "$(cat pid)" 2>/dev/null; then exit 0; fi
echo $$ > pid
: > log
if [ -f sampler.sh ]; then
  setsid bash sampler.sh "$JOB" </dev/null >/dev/null 2>&1 &
fi
bash exec.sh >> log 2>&1
echo $? > exit_code
"""

# The box ships a collector, never a server: one JSON line to metrics.jsonl every
# 10s, exit when exit_code appears. Pure bash + coreutils (+ nvidia-smi where it
# exists) because the trimmed images have no python. Every field is best-effort —
# omitted rather than failing — and nothing here may ever touch the job or its
# exit_code: observation only, hence set +e and the discarded output above.
SAMPLER = """#!/bin/bash
set +e
JOB=$1
OUT=${2:-/work/out}
INTERVAL=${SAMPLE_INTERVAL:-10}

num() { case "$1" in ''|*[!0-9.]*) return 1;; *) return 0;; esac }

while :; do
  [ -f "$JOB/exit_code" ] && exit 0
  JPID=$(cat "$JOB/pid" 2>/dev/null)
  if [ -n "$JPID" ] && ! kill -0 "$JPID" 2>/dev/null; then exit 0; fi

  line="{\\"t\\": $(date +%s)"

  v=$(cut -d' ' -f1 /proc/loadavg 2>/dev/null)
  num "$v" && line="$line, \\"load1\\": $v"

  v=$(awk '/MemAvailable/ {print int($2/1024)}' /proc/meminfo 2>/dev/null)
  num "$v" && line="$line, \\"mem_avail_mb\\": $v"

  df_target=$JOB
  [ -d "$OUT" ] && df_target=$OUT
  v=$(df -P "$df_target" 2>/dev/null | awk 'NR==2 {sub(/%/, "", $5); print $5}')
  num "$v" && line="$line, \\"disk_used_pct\\": $v"

  gpu=$(nvidia-smi --query-gpu=utilization.gpu,power.draw,memory.used,temperature.gpu \\
          --format=csv,noheader,nounits 2>/dev/null \\
        | awk -F', *' 'NF>=4 && $1==$1+0 && $2==$2+0 && $3==$3+0 && $4==$4+0 {
            printf "%s{\\"util\\": %d, \\"power_w\\": %.1f, \\"mem_mb\\": %d, \\"temp\\": %d}", s, $1, $2, $3, $4
            s=", "
          }')
  [ -n "$gpu" ] && line="$line, \\"gpu\\": [$gpu]"

  if [ -n "$JPID" ]; then
    v=$(ps -eo pid=,ppid=,rss= 2>/dev/null | awk -v root="$JPID" '
      {ppid[$1]=$2; rss[$1]=$3}
      END {
        t=0
        for (p in ppid) {
          q=p
          while (q in ppid) { if (q==root) {t+=rss[p]; break}; q=ppid[q] }
        }
        printf "%d", t/1024
      }')
    num "$v" && line="$line, \\"job_rss_mb\\": $v"
  fi

  phase=$(head -c 64 "$OUT/.phase" 2>/dev/null | tr -cd 'a-z0-9_-')
  [ -n "$phase" ] && line="$line, \\"phase\\": \\"$phase\\""

  echo "$line}" >> "$JOB/metrics.jsonl"
  sleep "$INTERVAL"
done
"""


class JobState(Enum):
    ABSENT = "absent"
    RUNNING = "running"
    EXITED = "exited"


@dataclass(frozen=True)
class JobStatus:
    state: JobState
    exit_code: int | None

    @property
    def ok(self) -> bool:
        return self.state is JobState.EXITED and self.exit_code == 0


@dataclass
class LogPump:
    """Incremental remote-log mirror: each pump transfers only bytes past the
    tracked offset, so `pipe log` reads a live local file without the poll loop
    ever re-reading the whole remote log."""

    host: Host
    src: Path
    dest: Path
    offset: int = 0

    def __post_init__(self) -> None:
        # Resume from what already landed locally, so an orchestrator restart
        # does not re-pull the log from byte 0.
        if self.dest.is_file():
            self.offset = self.dest.stat().st_size

    def pump(self) -> None:
        chunk = self.host.read_from(self.src, self.offset)
        if not chunk:
            return
        self.dest.parent.mkdir(parents=True, exist_ok=True)
        with self.dest.open("ab") as f:
            f.write(chunk)
        self.offset += len(chunk)


@dataclass(frozen=True)
class Job:
    host: Host
    ref: JobRef
    name: Name

    @property
    def dir(self) -> Path:
        return self.ref.dir

    def status(self) -> JobStatus:
        code = self.host.read_file(self.dir / "exit_code")
        if code is not None:
            return JobStatus(JobState.EXITED, int(code.strip()))
        pid = self.host.read_file(self.dir / "pid")
        if pid is None:
            return JobStatus(JobState.ABSENT, None)
        if self.host.pid_alive(int(pid.strip())):
            return JobStatus(JobState.RUNNING, None)
        return JobStatus(JobState.ABSENT, None)

    def log(self) -> str:
        return self.host.read_file(self.dir / "log") or ""

    def reset(self) -> None:
        for leftover in ("exit_code", "pid", "log"):
            self.host.remove(self.dir / leftover)

    def launch(self, exec_sh: str, payload: list[str], args: dict) -> None:
        self.host.mkdir(self.dir)
        self.host.write_file(self.dir / "args.json", json.dumps(args, indent=2))
        self.host.write_file(self.dir / "cmd.sh", f"set -euo pipefail\n{shlex.join(payload)}\n")
        self.host.write_file(self.dir / "exec.sh", exec_sh)
        self.host.write_file(self.dir / "sampler.sh", SAMPLER, executable=True)
        self.host.write_file(self.dir / "wrapper.sh", WRAPPER, executable=True)
        self.host.spawn(["setsid", "bash", str(self.dir / "wrapper.sh"), str(self.dir)])

    def wait(
        self,
        timeout: float,
        poll: float = 5.0,
        log_dest: Path | None = None,
        metrics_dest: Path | None = None,
    ) -> JobStatus:
        pumps = []
        if log_dest is not None:
            pumps.append(LogPump(self.host, self.dir / "log", log_dest))
        if metrics_dest is not None:
            pumps.append(LogPump(self.host, self.dir / "metrics.jsonl", metrics_dest))
        deadline = time.monotonic() + timeout
        while True:
            st = self.status()
            for pump in pumps:
                pump.pump()
            if st.state is JobState.EXITED:
                return st
            if time.monotonic() > deadline:
                raise TimeoutError(f"job {self.name} still {st.state.value} after {timeout}s")
            time.sleep(poll)
