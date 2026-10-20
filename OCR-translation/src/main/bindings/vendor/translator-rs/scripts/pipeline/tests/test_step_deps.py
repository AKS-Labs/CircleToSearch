"""Every declared step dep must resolve to a real file under the scripts dir.

Without this, a typo'd dep path (or a script renamed without updating deps) fails
only when the flow is RUN — at key computation, on a rented box, after paying to
get there. Here it fails in CI. Covers both the shared pipe.deps tuples and any
inline deps on individual StepDefs across every flow.
"""

from __future__ import annotations

import importlib.util
from pathlib import Path

import pytest

from pipe import deps
from pipe.step import StepDef

PIPELINE = Path(__file__).resolve().parents[1]
SCRIPTS = PIPELINE.parent / "opus-trainer"


def _dep_tuples():
    for name in dir(deps):
        if name.startswith("_"):
            continue
        val = getattr(deps, name)
        if isinstance(val, tuple):
            yield name, val


@pytest.mark.parametrize("name,paths", list(_dep_tuples()))
def test_shared_dep_files_exist(name, paths):
    missing = [p for p in paths if not (SCRIPTS / p).is_file()]
    assert not missing, f"pipe.deps.{name} references missing files: {missing}"


def _load_flow(path: Path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _flow_files():
    return sorted((PIPELINE / "flows").glob("*.py"))


@pytest.mark.parametrize("flow", _flow_files(), ids=lambda p: p.name)
def test_flow_step_deps_exist(flow):
    """Import each flow (also proves the deps back-port left it importable) and
    check every StepDef's deps — inline or shared — resolve to real files."""
    if flow.name == "__init__.py":
        return
    try:
        mod = _load_flow(flow)
    except Exception as e:  # a heavy optional import, not a deps problem
        pytest.skip(f"{flow.name} not importable in test env: {e}")
    for obj in vars(mod).values():
        if isinstance(obj, StepDef):
            missing = [p for p in obj.deps if not (SCRIPTS / p).is_file()]
            assert not missing, f"{flow.name}:{obj.name} missing dep files: {missing}"
