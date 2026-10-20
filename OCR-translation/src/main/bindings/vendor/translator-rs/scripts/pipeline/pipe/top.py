"""`pipe top`: latest telemetry per RUNNING step across runs, from the locally
pumped files only — never ssh. Warnings are presentation-side; nothing anywhere
branches on phase or on a sample."""

from __future__ import annotations

import json
import time
from pathlib import Path

from .ledger import Ledger, StepRecord
from .types import Status

DISK_WARN_PCT = 90
# power draw is the starvation signal util% hides; 450W is the 4090 board power
# this pipeline rents. Refine per-GPU only when another card enters the pool.
TRAIN_TDP_W = 450.0
STARVATION_FRACTION = 0.4


def latest_sample(metrics_path: Path) -> dict | None:
    if not metrics_path.is_file():
        return None
    # The pump can land mid-line, so the last line may be a partial JSON object;
    # walk back to the newest complete sample.
    for line in reversed(metrics_path.read_text().splitlines()):
        if not line.strip():
            continue
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
    return None


def tail_line(log_path: Path, window: int = 4096) -> str | None:
    if not log_path.is_file():
        return None
    size = log_path.stat().st_size
    with log_path.open("rb") as f:
        f.seek(max(0, size - window))
        chunk = f.read()
    lines = [ln for ln in chunk.decode(errors="replace").splitlines() if ln.strip()]
    return lines[-1] if lines else None


def warnings_for(sample: dict) -> list[str]:
    out: list[str] = []
    disk = sample.get("disk_used_pct")
    if isinstance(disk, (int, float)) and disk > DISK_WARN_PCT:
        out.append(f"disk {disk}% used")
    gpus = sample.get("gpu")
    if sample.get("phase") == "train" and gpus:
        power = max((g.get("power_w", 0.0) for g in gpus), default=0.0)
        floor = STARVATION_FRACTION * TRAIN_TDP_W
        if power < floor:
            out.append(
                f"starvation? {power:.0f}W while training "
                f"(< {STARVATION_FRACTION:.0%} of {TRAIN_TDP_W:.0f}W TDP)"
            )
    return out


def _row(run: str, key: str, rec: StepRecord, now: float) -> dict:
    local_out = rec.log.parent
    sample = latest_sample(local_out / "metrics.jsonl")
    return {
        "run": run,
        "step": rec.step,
        "key": key,
        "phase": sample.get("phase") if sample else None,
        "sample": sample,
        "age_s": round(now - sample["t"], 1) if sample and "t" in sample else None,
        "log": tail_line(rec.log),
        "warnings": warnings_for(sample) if sample else [],
    }


def top_rows(root: Path, now: float | None = None) -> list[dict]:
    now = now if now is not None else time.time()
    runs_dir = root / "runs"
    if not runs_dir.is_dir():
        return []
    rows = []
    for ledger_path in sorted(runs_dir.glob("*/ledger.json")):
        run = ledger_path.parent.name
        ledger = Ledger(ledger_path)
        for key, rec in ledger.steps.items():
            if rec.status is Status.RUNNING:
                rows.append(_row(run, key, rec, now))
    return rows
