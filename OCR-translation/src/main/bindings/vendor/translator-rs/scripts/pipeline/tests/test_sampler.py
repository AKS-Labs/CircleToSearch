from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

import pytest

from pipe.job import SAMPLER, WRAPPER


@pytest.mark.parametrize("script", [WRAPPER, SAMPLER], ids=["wrapper", "sampler"])
def test_shell_syntax(tmp_path: Path, script: str) -> None:
    path = tmp_path / "script.sh"
    path.write_text(script)
    subprocess.run(["bash", "-n", str(path)], check=True)


def _wait_for_lines(path: Path, n: int, timeout: float = 10.0) -> list[str]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            lines = [ln for ln in path.read_text().splitlines() if ln.strip()]
            if len(lines) >= n:
                return lines
        time.sleep(0.05)
    raise AssertionError(f"{path} never reached {n} lines")


def test_sampler_emits_valid_json_and_exits_on_exit_code(tmp_path: Path) -> None:
    job = tmp_path / "job"
    out = tmp_path / "out"
    job.mkdir()
    out.mkdir()
    (job / "pid").write_text(str(os.getpid()))
    (out / ".phase").write_text("train\n")
    sampler = tmp_path / "sampler.sh"
    sampler.write_text(SAMPLER)

    proc = subprocess.Popen(
        ["bash", str(sampler), str(job), str(out)],
        env={**os.environ, "SAMPLE_INTERVAL": "0.05"},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        lines = _wait_for_lines(job / "metrics.jsonl", 2)
        (job / "exit_code").write_text("0\n")
        proc.wait(timeout=5)
    finally:
        proc.kill()

    for line in lines:
        sample = json.loads(line)
        assert isinstance(sample["t"], int)
        assert isinstance(sample["load1"], float)
        assert isinstance(sample["mem_avail_mb"], int)
        assert 0 <= sample["disk_used_pct"] <= 100
        assert sample["phase"] == "train"
        if "job_rss_mb" in sample:
            assert isinstance(sample["job_rss_mb"], int) and sample["job_rss_mb"] >= 0
        if "gpu" in sample:
            for g in sample["gpu"]:
                assert {"util", "power_w", "mem_mb", "temp"} <= g.keys()


def test_sampler_omits_phase_when_absent(tmp_path: Path) -> None:
    job = tmp_path / "job"
    job.mkdir()
    (job / "pid").write_text(str(os.getpid()))
    sampler = tmp_path / "sampler.sh"
    sampler.write_text(SAMPLER)

    proc = subprocess.Popen(
        ["bash", str(sampler), str(job), str(tmp_path / "no-such-out")],
        env={**os.environ, "SAMPLE_INTERVAL": "0.05"},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        lines = _wait_for_lines(job / "metrics.jsonl", 1)
        (job / "exit_code").write_text("0\n")
        proc.wait(timeout=5)
    finally:
        proc.kill()

    sample = json.loads(lines[0])
    assert "phase" not in sample
    assert "t" in sample
