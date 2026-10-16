"""A ghost step must not make a run un-abortable.

An aborted step can be left RUNNING in the ledger (`action: "left"`) after its box
is destroyed. salvage_run then dialled that dead address on every later abort;
paramiko raised NoValidConnectionsError, which subclasses OSError directly — not
SSHException, not ConnectionError — so it escaped both the ssh retry tuple and
salvage's `except RuntimeError`, and the abort failed before reaching the LIVE
step. That is the exact moment abort matters, because a box is burning money
(2026-07-21: had to destroy the instance by hand).
"""

import paramiko

from pipe.ssh import SshHost


def test_no_valid_connections_is_retried_not_raised_raw(monkeypatch, tmp_path) -> None:
    """The dead-box error must surface as pipe's own RuntimeError."""
    host = SshHost(host="10.255.255.1", port=22, user="root",
                   key=tmp_path / "k", name="dead-box")
    monkeypatch.setattr("time.sleep", lambda *_: None)

    def boom():
        raise paramiko.ssh_exception.NoValidConnectionsError(
            {("10.255.255.1", 22): OSError("unreachable")})

    try:
        host._retry(boom)
    except RuntimeError as e:
        assert "kept failing after" in str(e)
    except paramiko.ssh_exception.NoValidConnectionsError:
        raise AssertionError(
            "NoValidConnectionsError escaped _retry — it subclasses OSError, "
            "not SSHException, so the catch tuple must include OSError")


def test_oserror_subclasses_are_all_covered() -> None:
    """Guard the assumption the fix rests on."""
    exc = paramiko.ssh_exception.NoValidConnectionsError
    assert issubclass(exc, OSError)
    assert not issubclass(exc, paramiko.SSHException)
    assert not issubclass(exc, ConnectionError)
