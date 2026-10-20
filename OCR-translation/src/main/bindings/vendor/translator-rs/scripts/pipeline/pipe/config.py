from __future__ import annotations

import tomllib
from dataclasses import dataclass
from pathlib import Path

DEFAULT_PATH = Path("~/.config/pipe/config.toml").expanduser()


@dataclass(frozen=True)
class StoreDests:
    raw_dest: Path
    default_dest: Path


@dataclass(frozen=True)
class Config:
    root: Path
    scripts: Path
    vast_key: Path
    # None when config.toml has no [store] table; erroring is deferred to the
    # point of use so configs written before the store existed keep working
    # for every non-store verb.
    store: StoreDests | None

    @staticmethod
    def load(path: Path = DEFAULT_PATH) -> Config:
        if not path.is_file():
            raise FileNotFoundError(f"no config at {path} -- run sync.sh from the repo")
        raw = tomllib.loads(path.read_text())
        missing = {"root", "scripts", "vast_key"} - raw.keys()
        if missing:
            raise ValueError(f"{path} is missing {sorted(missing)}")
        store_raw = raw.get("store")
        store = None
        if store_raw is not None:
            store_missing = {"raw_dest", "default_dest"} - store_raw.keys()
            if store_missing:
                raise ValueError(f"{path} [store] is missing {sorted(store_missing)}")
            store = StoreDests(
                raw_dest=Path(store_raw["raw_dest"]).expanduser(),
                default_dest=Path(store_raw["default_dest"]).expanduser(),
            )
        return Config(
            root=Path(raw["root"]).expanduser(),
            scripts=Path(raw["scripts"]).expanduser(),
            vast_key=Path(raw["vast_key"]).expanduser(),
            store=store,
        )

    def synced_at(self) -> str:
        stamp = self.root / "code" / "pipeline" / "SYNCED"
        if not stamp.is_file():
            return "unknown (no SYNCED stamp)"
        return stamp.read_text().strip()
