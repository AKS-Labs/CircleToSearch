from __future__ import annotations

from pathlib import Path

from pipe.host import LocalHost
from pipe.job import Job, JobState, LogPump
from pipe.ledger import JobRef
from pipe.types import Name


def test_read_from_offsets(tmp_path: Path) -> None:
    host = LocalHost()
    path = tmp_path / "log"
    assert host.read_from(path, 0) is None
    path.write_bytes(b"abc")
    assert host.read_from(path, 0) == b"abc"
    assert host.read_from(path, 3) == b""
    with path.open("ab") as f:
        f.write(b"def")
    assert host.read_from(path, 3) == b"def"


def test_pump_is_incremental(tmp_path: Path) -> None:
    src = tmp_path / "remote.log"
    dest = tmp_path / "local.log"
    pump = LogPump(LocalHost(), src, dest)

    pump.pump()
    assert not dest.exists() and pump.offset == 0

    src.write_bytes(b"one\n")
    pump.pump()
    assert dest.read_bytes() == b"one\n" and pump.offset == 4

    with src.open("ab") as f:
        f.write(b"two\n")
    pump.pump()
    assert dest.read_bytes() == b"one\ntwo\n" and pump.offset == 8


def test_pump_resumes_from_existing_dest(tmp_path: Path) -> None:
    src = tmp_path / "remote.log"
    dest = tmp_path / "local.log"
    src.write_bytes(b"abcdef")
    dest.write_bytes(b"abc")
    pump = LogPump(LocalHost(), src, dest)
    assert pump.offset == 3
    pump.pump()
    assert dest.read_bytes() == b"abcdef"


def test_job_wait_pumps_log_and_metrics(tmp_path: Path) -> None:
    job_dir = tmp_path / "jobs" / "k1"
    job_dir.mkdir(parents=True)
    (job_dir / "log").write_text("hello\n")
    (job_dir / "metrics.jsonl").write_text('{"t": 1}\n{"t": 2}\n')
    (job_dir / "exit_code").write_text("0\n")
    job = Job(host=LocalHost(), ref=JobRef("local", job_dir, None), name=Name("t"))

    log_dest = tmp_path / "out" / "job.log"
    metrics_dest = tmp_path / "out" / "metrics.jsonl"
    status = job.wait(timeout=5, poll=0.01, log_dest=log_dest, metrics_dest=metrics_dest)
    assert status.state is JobState.EXITED and status.ok
    assert log_dest.read_text() == "hello\n"
    assert metrics_dest.read_text() == '{"t": 1}\n{"t": 2}\n'


def test_metrics_pump_resumes_from_pumped_offset(tmp_path: Path) -> None:
    src = tmp_path / "metrics.jsonl"
    dest = tmp_path / "local" / "metrics.jsonl"
    dest.parent.mkdir()
    src.write_text('{"t": 1}\n{"t": 2}\n')
    dest.write_text('{"t": 1}\n')
    pump = LogPump(LocalHost(), src, dest)
    assert pump.offset == len('{"t": 1}\n')
    pump.pump()
    assert dest.read_text() == '{"t": 1}\n{"t": 2}\n'


def test_localhost_stream(tmp_path: Path) -> None:
    got = b"".join(LocalHost().stream(["printf", "a\\nb\\n"]))
    assert got == b"a\nb\n"
