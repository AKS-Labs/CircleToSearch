"""Cross-run artifact store: content-addressed identity, typed wrappers, pins.

Per-run working artifacts (`runs/<run>/artifacts/`, step memoization) stay named
and untouched — this store is only for artifacts that outlive the run that built
them. Identity is the content digest, derived not declared, so "OPUS-FILTERED-v2"
cannot be forgotten or go stale; runs bind to those digests via pins resolved at
pin time, never via a mutable alias.
"""

from __future__ import annotations

import inspect
import json
import os
import re
import shutil
import typing
from dataclasses import dataclass
from datetime import datetime, timezone
from enum import Enum
from functools import wraps
from pathlib import Path
from typing import Callable, ClassVar

from .config import StoreDests
from .store import count_lines
from .types import Digest, Kind, digest_file

_HEX = re.compile(r"^[0-9a-f]+$")


class ArtifactType(Enum):
    RAW = "dataset/raw"
    FILTERED = "dataset/filtered"
    DECODED = "dataset/decoded"
    ALIGNED = "dataset/aligned"
    HUMAN_BITEXT = "dataset/human_bitext"
    TRAINED = "model/trained"
    FINETUNED = "model/finetuned"
    BACKWARD = "model/backward"
    # Its own family: derived from a corpus like a model (parents = the corpus it
    # was trained on) but carries no Scores, so it must not fall under the
    # publish_model gate; reuse across rounds is a pin licensed by fertility.
    VOCAB = "vocab/spm"
    EVALSET = "eval/evalset"
    SCORE = "eval/score"

    @property
    def family(self) -> str:
        return self.value.partition("/")[0]


@dataclass(frozen=True)
class Producer:
    run: str
    step_key: str

    def to_json(self) -> dict:
        return {"run": self.run, "step_key": self.step_key}

    @staticmethod
    def from_json(d: dict) -> Producer:
        return Producer(run=d["run"], step_key=d["step_key"])


@dataclass(frozen=True)
class Stored:
    digest: Digest
    type: ArtifactType
    path: Path
    size: int
    lines: int | None
    produced: str
    parents: tuple[Digest, ...]
    producer: Producer | None
    label: str | None

    def to_json(self) -> dict:
        return {
            "digest": str(self.digest),
            "type": self.type.value,
            "path": str(self.path),
            "size": self.size,
            "lines": self.lines,
            "produced": self.produced,
            "parents": [str(p) for p in self.parents],
            "producer": self.producer.to_json() if self.producer else None,
            "label": self.label,
        }

    @staticmethod
    def from_json(d: dict) -> Stored:
        return Stored(
            digest=Digest(d["digest"]),
            type=ArtifactType(d["type"]),
            path=Path(d["path"]),
            size=d["size"],
            lines=d["lines"],
            produced=d["produced"],
            parents=tuple(Digest(p) for p in d["parents"]),
            producer=Producer.from_json(d["producer"]) if d["producer"] else None,
            label=d["label"],
        )


@dataclass(frozen=True)
class StoredWrapper:
    stored: Stored

    TYPE: ClassVar[ArtifactType]

    def __post_init__(self) -> None:
        if not hasattr(type(self), "TYPE"):
            raise TypeError("StoredWrapper is abstract; use the per-type wrapper classes")
        if self.stored.type is not self.TYPE:
            raise ValueError(
                f"{type(self).__name__} cannot wrap a {self.stored.type.value} artifact"
            )

    @property
    def digest(self) -> Digest:
        return self.stored.digest

    @property
    def path(self) -> Path:
        return self.stored.path

    @property
    def size(self) -> int:
        return self.stored.size

    @property
    def lines(self) -> int | None:
        return self.stored.lines

    @property
    def label(self) -> str | None:
        return self.stored.label


class Raw(StoredWrapper):
    TYPE = ArtifactType.RAW


class Filtered(StoredWrapper):
    TYPE = ArtifactType.FILTERED


class Decoded(StoredWrapper):
    TYPE = ArtifactType.DECODED


class Aligned(StoredWrapper):
    TYPE = ArtifactType.ALIGNED


class HumanBitext(StoredWrapper):
    TYPE = ArtifactType.HUMAN_BITEXT


class Trained(StoredWrapper):
    TYPE = ArtifactType.TRAINED


class Finetuned(StoredWrapper):
    TYPE = ArtifactType.FINETUNED


class Backward(StoredWrapper):
    TYPE = ArtifactType.BACKWARD


class Vocab(StoredWrapper):
    TYPE = ArtifactType.VOCAB


class EvalSet(StoredWrapper):
    TYPE = ArtifactType.EVALSET


class Score(StoredWrapper):
    TYPE = ArtifactType.SCORE


WRAPPERS: dict[ArtifactType, type[StoredWrapper]] = {
    cls.TYPE: cls
    for cls in (
        Raw, Filtered, Decoded, Aligned, HumanBitext,
        Trained, Finetuned, Backward, Vocab, EvalSet, Score,
    )
}


def wrap(stored: Stored) -> StoredWrapper:
    return WRAPPERS[stored.type](stored)


def typed_stage(fn: Callable) -> Callable:
    """Reject wrong wrapper types at call time, before any box is rented.

    `decode(Raw)` must be a type error, not a forgotten checklist item: an ABC
    makes you *write* filter_corpus(), a type makes you *have* filtered.
    """
    sig = inspect.signature(fn)
    hints = typing.get_type_hints(fn)

    @wraps(fn)
    def checked(*args, **kwargs):
        bound = sig.bind(*args, **kwargs)
        for pname, val in bound.arguments.items():
            ann = hints.get(pname)
            if ann is None:
                continue
            options = typing.get_args(ann) or (ann,)
            if not any(
                isinstance(o, type) and issubclass(o, StoredWrapper) for o in options
            ):
                continue
            accepted = tuple(o for o in options if isinstance(o, type) and o is not type(None))
            if val is None and type(None) in options:
                continue
            if not isinstance(val, accepted):
                names = " | ".join(c.__name__ for c in accepted)
                raise TypeError(
                    f"stage {fn.__name__} takes {pname}: {names}, got {type(val).__name__}"
                )
        return fn(*args, **kwargs)

    return checked


def _slug(label: str) -> str:
    return re.sub(r"[^a-z0-9._-]+", "-", label.lower()).strip("-")


def _write_json(path: Path, obj: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(obj, indent=2) + "\n")
    os.replace(tmp, path)


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def read_pins(root: Path, flow: str) -> dict[str, str]:
    path = root / "pins" / f"{flow}.json"
    if not path.is_file():
        return {}
    return json.loads(path.read_text())


class ArtStore:
    """`$PIPE_ROOT/store`: meta is the source of truth, bytes live under the
    per-stage destination roots from config.toml `[store]`."""

    def __init__(self, root: Path, dests: StoreDests | None) -> None:
        self.root = root
        self.dests = dests
        self.meta_dir = root / "store" / "meta"
        self.aliases_dir = root / "store" / "aliases"
        self.pins_dir = root / "pins"

    # ---- publish ----

    def publish(
        self,
        path: Path,
        typ: ArtifactType,
        parents: tuple[Digest, ...] = (),
        label: str | None = None,
        producer: Producer | None = None,
        kind: Kind | None = None,
    ) -> StoredWrapper:
        if typ.family == "model":
            raise ValueError(
                f"{typ.value} must go through publish_model with its scores; "
                "a model is not registered without them"
            )
        return self._publish(path, typ, parents, label, producer, kind)

    def publish_model(
        self,
        path: Path,
        typ: ArtifactType,
        scores: list[Score],
        parents: tuple[Digest, ...] = (),
        label: str | None = None,
        producer: Producer | None = None,
    ) -> StoredWrapper:
        if typ.family != "model":
            raise ValueError(f"publish_model is for model/* artifacts, not {typ.value}")
        if not scores:
            raise ValueError(
                "refusing to register a model without Scores: eval is minutes of "
                "decode, and an unevaluated model in the store is how the table "
                "goes incomplete"
            )
        digest = digest_file(path)
        for s in scores:
            if digest not in s.stored.parents:
                raise ValueError(
                    f"score {s.digest.short} does not descend from this model "
                    f"({digest.short}); its parents are "
                    f"{[p.short for p in s.stored.parents]}"
                )
        return self._publish(path, typ, parents, label, producer, kind=None)

    def publish_score(
        self,
        model: Digest,
        evalset: EvalSet,
        metric: str,
        value: float,
        label: str | None = None,
        producer: Producer | None = None,
    ) -> Score:
        blob = json.dumps(
            {"model": str(model), "evalset": str(evalset.digest), "metric": metric,
             "value": value},
            sort_keys=True,
        ) + "\n"
        tmp = self.root / "store" / "tmp" / f"score-{model.short}-{metric}.json"
        tmp.parent.mkdir(parents=True, exist_ok=True)
        tmp.write_text(blob)
        try:
            published = self.publish(
                tmp, ArtifactType.SCORE,
                parents=(model, evalset.digest), label=label, producer=producer,
            )
        finally:
            tmp.unlink(missing_ok=True)
        assert isinstance(published, Score)
        return published

    def _publish(
        self,
        path: Path,
        typ: ArtifactType,
        parents: tuple[Digest, ...],
        label: str | None,
        producer: Producer | None,
        kind: Kind | None,
    ) -> StoredWrapper:
        if not path.is_file():
            raise FileNotFoundError(f"nothing to publish at {path}")
        digest = digest_file(path)
        meta_path = self.meta_dir / f"{digest}.json"
        if meta_path.is_file():
            existing = Stored.from_json(json.loads(meta_path.read_text()))
            same = (existing.type, existing.parents, existing.label) == (typ, parents, label)
            if not same:
                raise ValueError(
                    f"digest {digest.short} is already published with different "
                    f"metadata ({existing.type.value}, label={existing.label!r}); "
                    "refusing to overwrite"
                )
            return wrap(existing)
        if self.dests is None:
            raise ValueError(
                "config.toml has no [store] table (raw_dest, default_dest); "
                "publishing needs it"
            )
        dest_root = self.dests.raw_dest if typ is ArtifactType.RAW else self.dests.default_dest
        name = digest.short + (f"-{_slug(label)}" if label else "")
        obj = dest_root / typ.value / name
        obj.parent.mkdir(parents=True, exist_ok=True)
        tmp = obj.with_name(obj.name + ".tmp")
        shutil.copyfile(path, tmp)
        os.replace(tmp, obj)
        obj.chmod(0o444)
        if kind is None:
            kind = Kind.LINES if (typ.family == "dataset" or typ is ArtifactType.EVALSET) else Kind.BLOB
        stored = Stored(
            digest=digest,
            type=typ,
            path=obj,
            size=obj.stat().st_size,
            lines=count_lines(obj) if kind is Kind.LINES else None,
            produced=_now(),
            parents=parents,
            producer=producer,
            label=label,
        )
        _write_json(meta_path, stored.to_json())
        return wrap(stored)

    # ---- read ----

    def get(self, digest: Digest | str) -> StoredWrapper:
        d = digest if isinstance(digest, Digest) else Digest(digest)
        meta_path = self.meta_dir / f"{d}.json"
        if not meta_path.is_file():
            raise KeyError(f"no artifact {d.short} in the store")
        return wrap(Stored.from_json(json.loads(meta_path.read_text())))

    def list(self, typ: ArtifactType | None = None) -> list[Stored]:
        if not self.meta_dir.is_dir():
            return []
        out = [
            Stored.from_json(json.loads(p.read_text()))
            for p in sorted(self.meta_dir.glob("*.json"))
        ]
        if typ is not None:
            out = [s for s in out if s.type is typ]
        return sorted(out, key=lambda s: s.produced)

    def resolve(self, ref: str) -> Digest:
        """A ref is a full digest, a unique digest prefix (>= 8 hex), or an
        `ns/name` alias. Aliases resolve to a digest HERE, at pin time — runs
        only ever hold hashes."""
        if "/" in ref:
            return self.alias_resolve(ref)
        if len(ref) == 64 and _HEX.match(ref):
            d = Digest(ref)
            self.get(d)
            return d
        if len(ref) >= 8 and _HEX.match(ref):
            hits = [p.stem for p in self.meta_dir.glob("*.json") if p.stem.startswith(ref)]
            if len(hits) > 1:
                raise ValueError(f"{ref!r} is ambiguous: {[h[:12] for h in hits]}")
            if not hits:
                raise KeyError(f"no artifact matching {ref!r} in the store")
            return Digest(hits[0])
        raise ValueError(f"{ref!r} is neither a digest, a digest prefix, nor an ns/name alias")

    # ---- aliases ----

    def alias_set(self, alias: str, digest: Digest) -> None:
        ns, _, name = alias.partition("/")
        if not ns or not name:
            raise ValueError(f"alias must be ns/name, got {alias!r}")
        self.get(digest)
        _write_json(self.aliases_dir / ns / name, {"digest": str(digest), "updated": _now()})

    def alias_resolve(self, alias: str) -> Digest:
        ns, _, name = alias.partition("/")
        path = self.aliases_dir / ns / name
        if not path.is_file():
            raise KeyError(f"no alias {alias!r}")
        return Digest(json.loads(path.read_text())["digest"])

    def alias_list(self) -> list[dict]:
        if not self.aliases_dir.is_dir():
            return []
        out = []
        for ns_dir in sorted(self.aliases_dir.iterdir()):
            for f in sorted(ns_dir.iterdir()):
                raw = json.loads(f.read_text())
                out.append(
                    {"alias": f"{ns_dir.name}/{f.name}",
                     "digest": raw["digest"], "updated": raw["updated"]}
                )
        return out

    def aliased_under(self, ns: str) -> set[str]:
        ns_dir = self.aliases_dir / ns
        if not ns_dir.is_dir():
            return set()
        return {json.loads(f.read_text())["digest"] for f in ns_dir.iterdir()}

    # ---- pins ----

    def pin(self, flow: str, name: str, ref: str) -> Digest:
        digest = self.resolve(ref)
        pins = read_pins(self.root, flow)
        pins[name] = str(digest)
        _write_json(self.pins_dir / f"{flow}.json", pins)
        return digest

    def pins(self, flow: str) -> dict[str, str]:
        return read_pins(self.root, flow)

    # ---- scores ----

    def score_table(self, model: Digest | None = None, pair: str | None = None) -> list[dict]:
        """Every (model, evalset, metric) cell joinable from the store. No
        aggregation, no `best` — the axes disagree by design and the
        disagreement is the signal."""
        in_pair = self.aliased_under(pair) if pair is not None else None
        rows = []
        for s in self.list(ArtifactType.SCORE):
            blob = json.loads(s.path.read_text())
            if model is not None and blob["model"] != str(model):
                continue
            if in_pair is not None and not {blob["model"], blob["evalset"]} & in_pair:
                continue
            rows.append(
                {
                    "model": blob["model"][:12],
                    "model_label": self._label_of(blob["model"]),
                    "evalset": blob["evalset"][:12],
                    "evalset_label": self._label_of(blob["evalset"]),
                    "metric": blob["metric"],
                    "value": blob["value"],
                }
            )
        return sorted(rows, key=lambda r: (r["model"], r["evalset"], r["metric"]))

    def _label_of(self, digest: str) -> str | None:
        try:
            return self.get(digest).label
        except KeyError:
            return None
