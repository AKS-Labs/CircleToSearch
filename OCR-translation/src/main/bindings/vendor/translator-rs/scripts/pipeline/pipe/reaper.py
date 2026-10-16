from __future__ import annotations

import time
from dataclasses import dataclass
from pathlib import Path

from .vast import Instance, VastApi

# An instance we labelled but have no lease for is unmanaged by definition. It still
# gets a grace window, because a lease is written moments after create returns and a
# reaper tick can land in between.
ORPHAN_GRACE = 3600.0


@dataclass(frozen=True)
class Reaping:
    instance_id: int
    reason: str
    destroyed: bool


def reap(vast: VastApi, now: float | None = None, dry_run: bool = False) -> list[Reaping]:
    now = now or time.time()
    leases = vast.leases()
    out: list[Reaping] = []

    ours: dict[str, Instance] = {}
    for inst in vast.instances():
        if inst.label and inst.label.startswith("pipe:"):
            ours[inst.label.removeprefix("pipe:")] = inst

    for uid, inst in ours.items():
        lease = leases.get(uid)
        if lease is None:
            out.append(Reaping(inst.id, "labelled pipe: but no lease on disk", False))
            continue
        if now > lease.expires:
            if not dry_run:
                vast.destroy(inst.id)
                vast.drop_lease(uid)
            out.append(
                Reaping(inst.id, f"lease expired {int(now - lease.expires)}s ago", not dry_run)
            )

    for uid, lease in leases.items():
        if uid not in ours:
            if not dry_run:
                vast.drop_lease(uid)
            out.append(Reaping(lease.instance_id, "lease with no live instance", False))

    return out
