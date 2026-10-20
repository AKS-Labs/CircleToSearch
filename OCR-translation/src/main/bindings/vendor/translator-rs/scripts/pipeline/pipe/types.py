from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from enum import Enum
from pathlib import Path

_SLUG = re.compile(r"^[a-z0-9][a-z0-9._-]*$")


class Kind(Enum):
    LINES = "lines"
    BLOB = "blob"


@dataclass(frozen=True)
class RunId:
    value: str

    def __post_init__(self) -> None:
        if not _SLUG.match(self.value):
            raise ValueError(f"run id must match {_SLUG.pattern}: {self.value!r}")

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class Name:
    value: str

    def __post_init__(self) -> None:
        if not _SLUG.match(self.value):
            raise ValueError(f"name must match {_SLUG.pattern}: {self.value!r}")

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class Digest:
    value: str

    def __post_init__(self) -> None:
        if len(self.value) != 64 or not all(c in "0123456789abcdef" for c in self.value):
            raise ValueError(f"not a sha256 hex digest: {self.value!r}")

    @property
    def short(self) -> str:
        return self.value[:12]

    def __str__(self) -> str:
        return self.value


@dataclass(frozen=True)
class Artifact:
    name: Name
    path: Path
    kind: Kind
    digest: Digest
    size: int
    lines: int | None

    def __post_init__(self) -> None:
        if self.kind is Kind.LINES and self.lines is None:
            raise ValueError(f"LINES artifact {self.name} has no line count")
        if self.kind is Kind.BLOB and self.lines is not None:
            raise ValueError(f"BLOB artifact {self.name} carries a line count")

    def to_json(self) -> dict:
        return {
            "name": str(self.name),
            "path": str(self.path),
            "kind": self.kind.value,
            "digest": str(self.digest),
            "size": self.size,
            "lines": self.lines,
        }

    @staticmethod
    def from_json(d: dict) -> Artifact:
        return Artifact(
            name=Name(d["name"]),
            path=Path(d["path"]),
            kind=Kind(d["kind"]),
            digest=Digest(d["digest"]),
            size=d["size"],
            lines=d["lines"],
        )


class Status(Enum):
    RUNNING = "running"
    DONE = "done"
    FAILED = "failed"


def digest_file(path: Path) -> Digest:
    h = hashlib.sha256()
    with path.open("rb") as f:
        while chunk := f.read(1 << 20):
            h.update(chunk)
    return Digest(h.hexdigest())


def digest_text(text: str) -> Digest:
    return Digest(hashlib.sha256(text.encode()).hexdigest())


def digest_json(obj: object) -> Digest:
    return digest_text(json.dumps(obj, sort_keys=True, separators=(",", ":")))
