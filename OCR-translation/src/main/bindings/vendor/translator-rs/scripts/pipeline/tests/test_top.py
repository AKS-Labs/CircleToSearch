from __future__ import annotations

import json
from pathlib import Path

from pipe.ledger import Ledger
from pipe.store import Store
from pipe.top import latest_sample, top_rows, warnings_for
from pipe.types import RunId, Status

NOW = 1_800_000_000.0


def make_running_step(
    root: Path, run: str, key: str, step: str, samples: list[dict], log_lines: list[str]
) -> Path:
    store = Store(root)
    rid = RunId(run)
    local_out = store.run_dir(rid) / "out" / key
    local_out.mkdir(parents=True)
    ledger = Ledger(store.ledger_path(rid))
    ledger.begin(key, step, local_out / "job.log", None)
    if samples:
        (local_out / "metrics.jsonl").write_text(
            "".join(json.dumps(s) + "\n" for s in samples)
        )
    if log_lines:
        (local_out / "job.log").write_text("".join(ln + "\n" for ln in log_lines))
    return local_out


def test_top_row_with_both_warnings_and_staleness(tmp_path: Path) -> None:
    sample = {
        "t": int(NOW - 30),
        "load1": 4.2,
        "mem_avail_mb": 1200,
        "disk_used_pct": 95,
        "gpu": [{"util": 98, "power_w": 120.0, "mem_mb": 20000, "temp": 61}],
        "job_rss_mb": 9000,
        "phase": "train",
    }
    make_running_step(
        tmp_path, "uigr2", "train@abc123", "train",
        samples=[{"t": int(NOW - 300)}, sample],
        log_lines=["Ep. 1 : Up. 500 : cost 3.2", "Ep. 1 : Up. 1000 : cost 2.9"],
    )

    rows = top_rows(tmp_path, now=NOW)
    assert len(rows) == 1
    row = rows[0]
    assert (row["run"], row["step"], row["key"]) == ("uigr2", "train", "train@abc123")
    assert row["phase"] == "train"
    assert row["age_s"] == 30.0
    assert row["sample"] == sample
    assert row["log"] == "Ep. 1 : Up. 1000 : cost 2.9"
    assert any(w.startswith("disk 95%") for w in row["warnings"])
    assert any(w.startswith("starvation?") for w in row["warnings"])


def test_top_no_warnings_when_healthy(tmp_path: Path) -> None:
    sample = {
        "t": int(NOW - 5),
        "disk_used_pct": 50,
        "gpu": [{"util": 99, "power_w": 400.0, "mem_mb": 20000, "temp": 70}],
        "phase": "train",
    }
    make_running_step(tmp_path, "r1", "train@k1", "train", [sample], [])
    (row,) = top_rows(tmp_path, now=NOW)
    assert row["warnings"] == []


def test_low_power_outside_train_phase_is_not_starvation(tmp_path: Path) -> None:
    sample = {
        "t": int(NOW - 5),
        "gpu": [{"util": 10, "power_w": 80.0, "mem_mb": 2000, "temp": 40}],
        "phase": "pack",
    }
    assert warnings_for(sample) == []


def test_done_steps_are_not_listed(tmp_path: Path) -> None:
    local_out = make_running_step(tmp_path, "r1", "align@k2", "align", [], [])
    store = Store(tmp_path)
    ledger = Ledger(store.ledger_path(RunId("r1")))
    ledger.finish("align@k2", Status.DONE, {})
    assert top_rows(tmp_path, now=NOW) == []
    assert local_out.is_dir()


def test_missing_metrics_gives_empty_sample(tmp_path: Path) -> None:
    make_running_step(tmp_path, "r1", "align@k3", "align", [], ["aligning..."])
    (row,) = top_rows(tmp_path, now=NOW)
    assert row["sample"] is None
    assert row["phase"] is None
    assert row["age_s"] is None
    assert row["warnings"] == []
    assert row["log"] == "aligning..."


def test_latest_sample_skips_partial_trailing_line(tmp_path: Path) -> None:
    path = tmp_path / "metrics.jsonl"
    path.write_text(json.dumps({"t": 100, "load1": 1.0}) + "\n" + '{"t": 12')
    assert latest_sample(path) == {"t": 100, "load1": 1.0}
