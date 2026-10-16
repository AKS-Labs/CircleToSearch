from __future__ import annotations

import shlex
import stat
import subprocess
import time
from dataclasses import dataclass, field
from typing import ClassVar
from pathlib import Path
from typing import Callable, TypeVar

import paramiko

T = TypeVar("T")


@dataclass
class SshHost:
    host: str
    port: int
    user: str
    key: Path
    name: str = field(default="")
    TRIES: ClassVar[int] = 6
    _client: paramiko.SSHClient | None = field(default=None, repr=False)

    def __post_init__(self) -> None:
        if not self.name:
            self.name = f"{self.user}@{self.host}:{self.port}"

    def _conn(self) -> paramiko.SSHClient:
        if self._client is not None:
            return self._client
        c = paramiko.SSHClient()
        c.set_missing_host_key_policy(paramiko.AutoAddPolicy())
        c.connect(
            hostname=self.host,
            port=self.port,
            username=self.user,
            key_filename=str(self.key),
            timeout=20,
            banner_timeout=30,
            auth_timeout=30,
            look_for_keys=False,
            allow_agent=False,
        )
        self._client = c
        return c

    def close(self) -> None:
        if self._client is not None:
            self._client.close()
            self._client = None

    def _sftp(self) -> paramiko.SFTPClient:
        return self._conn().open_sftp()

    def _retry(self, fn: Callable[[], T]) -> T:
        # vast's ssh gateways drop sessions mid-poll (paramiko surfaces EOFError);
        # a transient drop must reconnect, not kill a flow with a live 3h job.
        # FileNotFoundError never reaches here: the verbs handle it inside fn.
        #
        # 6 not 4: the linear backoff means 4 tries wait only 5+10+15 = 30s, and a
        # freshly rented box needs longer than that for vast to propagate the
        # attached key — AuthenticationException (an SSHException subclass, so it
        # IS retried here) rejected 3 boxes on 2026-07-20, all of them in NL. 6
        # tries buys 75s, which is the cheap half of the fix; the proper one is a
        # patient policy for first contact and a fast-failing one for mid-job
        # drops, since one policy cannot serve both.
        last: Exception | None = None
        for attempt in range(self.TRIES):
            try:
                return fn()
            # OSError is load-bearing: paramiko's NoValidConnectionsError (a dead
            # box, a closed port) subclasses socket.error/OSError DIRECTLY, not
            # SSHException and not ConnectionError, so it escaped this tuple and
            # propagated raw out of salvage — making a run un-abortable
            # (2026-07-21).
            except (EOFError, paramiko.SSHException, OSError, TimeoutError) as e:
                last = e
                self.close()
                time.sleep(5 * (attempt + 1))
        raise RuntimeError(
            f"ssh to {self.name} kept failing after {self.TRIES} tries: {last!r}"
        ) from last

    def mkdir(self, path: Path) -> None:
        def once() -> None:
            sftp = self._sftp()
            parts = Path(path).parts
            cur = Path(parts[0])
            for p in parts[1:]:
                cur = cur / p
                try:
                    sftp.stat(str(cur))
                except FileNotFoundError:
                    sftp.mkdir(str(cur))

        self._retry(once)

    def write_file(self, path: Path, text: str, executable: bool = False) -> None:
        def once() -> None:
            self.mkdir(path.parent)
            sftp = self._sftp()
            with sftp.open(str(path), "w") as f:
                f.write(text)
            sftp.chmod(str(path), 0o755 if executable else 0o644)

        self._retry(once)

    def read_file(self, path: Path) -> str | None:
        def once() -> str | None:
            try:
                with self._sftp().open(str(path), "r") as f:
                    return f.read().decode()
            except FileNotFoundError:
                return None

        return self._retry(once)

    def read_from(self, path: Path, offset: int) -> bytes | None:
        # SFTP files support seek, so a log pump only ever transfers the tail
        # it has not seen — never the whole file per poll.
        def once() -> bytes | None:
            try:
                with self._sftp().open(str(path), "rb") as f:
                    f.seek(offset)
                    return f.read()
            except FileNotFoundError:
                return None

        return self._retry(once)

    def exists(self, path: Path) -> bool:
        def once() -> bool:
            try:
                self._sftp().stat(str(path))
            except FileNotFoundError:
                return False
            return True

        return self._retry(once)

    def remove(self, path: Path) -> None:
        def once() -> None:
            try:
                self._sftp().remove(str(path))
            except FileNotFoundError:
                pass

        self._retry(once)

    def pid_alive(self, pid: int) -> bool:
        # /proc via sftp: no shell, so there is nothing to quote or inject.
        return self.exists(Path(f"/proc/{int(pid)}"))

    def capture(self, argv: list[str]) -> str:
        def once() -> str:
            cmd = shlex.join(argv)
            _, out, err = self._conn().exec_command(cmd, timeout=120)
            code = out.channel.recv_exit_status()
            if code != 0:
                raise RuntimeError(f"{cmd} exited {code}\n{err.read().decode().strip()}")
            return out.read().decode()

        return self._retry(once)

    def spawn(self, argv: list[str]) -> None:
        def once() -> None:
            cmd = shlex.join(argv)
            self._conn().exec_command(f"nohup setsid {cmd} </dev/null >/dev/null 2>&1 &", timeout=30)

        self._retry(once)

    def stream(self, argv: list[str]):
        # A live view is disposable — no retry, no reconnect: the pumped record
        # is the authoritative copy, this channel only exists while a human watches.
        transport = self._conn().get_transport()
        assert transport is not None
        chan = transport.open_session()
        chan.exec_command(shlex.join(argv))
        try:
            while True:
                data = chan.recv(4096)
                if not data:
                    return
                yield data
        finally:
            chan.close()

    def push(self, src: Path, dst: Path) -> None:
        self.mkdir(dst.parent)
        self._rsync(str(src), f"{self.user}@{self.host}:{dst}")

    def push_dir(self, src: Path, dst: Path) -> None:
        self.mkdir(dst)
        self._rsync(f"{src}/", f"{self.user}@{self.host}:{dst}/")

    def pull(self, src: Path, dst: Path) -> None:
        dst.parent.mkdir(parents=True, exist_ok=True)
        self._rsync(f"{self.user}@{self.host}:{src}", str(dst))

    def pull_dir(self, src: Path, dst: Path) -> None:
        dst.mkdir(parents=True, exist_ok=True)
        self._rsync(f"{self.user}@{self.host}:{src}/", f"{dst}/")

    def _rsync(self, src: str, dst: str) -> None:
        ssh = (
            f"ssh -p {int(self.port)} -i {shlex.quote(str(self.key))} "
            "-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null "
            "-o ServerAliveInterval=20"
        )
        # Flaky vast gateways drop connections mid-transfer; --partial keeps what
        # landed, so each retry resumes rather than restarts. 8 tries rode out a
        # box that dropped every ~15MB of a 125MB model pull (2026-07-16).
        #
        # -z is not optional at these speeds: transfers go over vast's US-west ssh
        # gateway, so an EU hub -> EU box round-trips through California. Measured
        # 2026-07-21: a 1.96GB train.tsv uploading at ~1.15 MB/s, ~28 min, with the
        # rented GPU idle at 17W the whole time. train.tsv is 3.0x zstd-compressible
        # (200MB -> 66.8MB), so this is ~9 min instead. rsync 3.2+ NEGOTIATES the
        # compressor for plain -z (zstd > lz4 > zlib), so no --compress-choice here:
        # naming zstd explicitly would fail against an older remote rsync instead of
        # degrading to zlib.
        last = ""
        for attempt in range(8):
            done = subprocess.run(
                ["rsync", "-az", "--partial", "--timeout=120", "-e", ssh, src, dst],
                capture_output=True,
                text=True,
            )
            if done.returncode == 0:
                return
            last = done.stderr.strip()
            time.sleep(5 * (attempt + 1))
        raise RuntimeError(f"rsync {src} -> {dst} failed after 8 tries: {last}")
