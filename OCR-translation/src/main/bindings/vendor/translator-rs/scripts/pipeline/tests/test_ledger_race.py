"""A `pipe put` during a running step must survive that step's save().

The runner loads the ledger once and holds it for a step that can last hours, so
the old save() — a full write of its in-memory copy — erased anything another
process registered meanwhile. `put` reported success and returned a digest, then
the artifact vanished, surfacing much later as "no artifact vocab in this run"
(2026-07-21, lost vocab/valid/flores_src during a 19-minute align).
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

from pipe.ledger import Ledger
from pipe.types import Artifact, Kind, Name

REPO = Path(__file__).resolve().parents[1]


def _art(name: str, digest_char: str) -> Artifact:
    return Artifact(name=Name(name), path=Path("/x"), kind=Kind.LINES,
                    digest=digest_char * 64, size=1, lines=1)


def test_concurrent_put_survives_step_save() -> None:
    path = Path(tempfile.mkdtemp()) / "ledger.json"

    # A runner loads the ledger and holds it across a long step.
    runner = Ledger(path)
    runner.register(_art("early", "a"))

    # A separate process registers an artifact while the runner is mid-step.
    subprocess.run(
        [sys.executable, "-c",
         "import sys; sys.path.insert(0, %r)\n" % str(REPO) +
         "from pathlib import Path\n"
         "from pipe.ledger import Ledger\n"
         "from pipe.types import Artifact, Kind, Name\n"
         "Ledger(Path(%r)).register(Artifact(name=Name('from_other_process'), "
         "path=Path('/y'), kind=Kind.LINES, digest='b'*64, size=1, lines=1))"
         % str(path)],
        check=True,
    )

    # The step finishes and saves.
    runner.register(_art("late", "c"))

    on_disk = set(json.loads(path.read_text())["artifacts"])
    assert "from_other_process" in on_disk, "concurrent put was clobbered"
    assert {"early", "late"} <= on_disk, "the runner's own writes were lost"
    # and the runner sees the merged state, not just its own
    assert "from_other_process" in runner.artifacts
