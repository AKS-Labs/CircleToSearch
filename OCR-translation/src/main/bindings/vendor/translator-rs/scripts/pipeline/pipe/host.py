from __future__ import annotations

import os
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator, Protocol


class Host(Protocol):
    name: str

    def mkdir(self, path: Path) -> None: ...
    def write_file(self, path: Path, text: str, executable: bool = False) -> None: ...
    def read_file(self, path: Path) -> str | None: ...
    def read_from(self, path: Path, offset: int) -> bytes | None: ...
    def exists(self, path: Path) -> bool: ...
    def remove(self, path: Path) -> None: ...
    def pid_alive(self, pid: int) -> bool: ...
    def capture(self, argv: list[str]) -> str: ...
    def spawn(self, argv: list[str]) -> None: ...
    def stream(self, argv: list[str]) -> Iterator[bytes]: ...


@dataclass
class LocalHost:
    name: str = "local"

    def mkdir(self, path: Path) -> None:
        path.mkdir(parents=True, exist_ok=True)

    def write_file(self, path: Path, text: str, executable: bool = False) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        if executable:
            path.chmod(0o755)

    def read_file(self, path: Path) -> str | None:
        if not path.is_file():
            return None
        return path.read_text()

    def read_from(self, path: Path, offset: int) -> bytes | None:
        if not path.is_file():
            return None
        with path.open("rb") as f:
            f.seek(offset)
            return f.read()

    def exists(self, path: Path) -> bool:
        return path.exists()

    def remove(self, path: Path) -> None:
        path.unlink(missing_ok=True)

    def pid_alive(self, pid: int) -> bool:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True

    def capture(self, argv: list[str]) -> str:
        done = subprocess.run(argv, capture_output=True, text=True)
        if done.returncode != 0:
            raise RuntimeError(f"{argv!r} exited {done.returncode}\n{done.stderr.strip()}")
        return done.stdout

    def spawn(self, argv: list[str]) -> None:
        # start_new_session + closed streams: otherwise an ssh disconnect takes the job with it.
        subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )

    def stream(self, argv: list[str]) -> Iterator[bytes]:
        proc = subprocess.Popen(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        assert proc.stdout is not None
        try:
            while chunk := proc.stdout.read1(4096):
                yield chunk
        finally:
            proc.terminate()
            proc.wait()
