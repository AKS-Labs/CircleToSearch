"""Direct is preferred over vast's proxy, always.

Every byte through the proxy is relayed via their US-west gateway, so an EU hub
talking to an EU box crosses the Atlantic twice — 1.15 MB/s on a 1.96GB upload,
measured 2026-07-21, with the rented GPU idle throughout. It is also what drops
connections mid-transfer (hence --partial and 8 retries in _rsync).
"""

from pipe.vast import Instance

BASE = {"id": 1, "actual_status": "running", "ssh_host": "ssh4.vast.ai",
        "ssh_port": 26512, "gpu_name": "RTX 4090"}


def test_prefers_direct_when_offer_exposes_port_22() -> None:
    inst = Instance.from_json(
        BASE | {"public_ipaddr": "203.0.113.7", "ports": {"22/tcp": [{"HostPort": "41022"}]}})
    ep = inst.endpoint
    assert ep == ("203.0.113.7", 41022, True)
    assert inst.reachable


def test_falls_back_to_proxy_when_no_direct_port() -> None:
    inst = Instance.from_json(BASE | {"public_ipaddr": "203.0.113.7", "ports": {}})
    ep = inst.endpoint
    assert ep is not None and not ep.direct
    assert (ep.host, ep.port) == ("ssh4.vast.ai", 26512)


def test_unreachable_without_either_route() -> None:
    inst = Instance.from_json({"id": 2, "actual_status": "loading", "gpu_name": "RTX 4090"})
    assert inst.endpoint is None
    assert not inst.reachable


def test_malformed_port_does_not_crash_and_uses_proxy() -> None:
    inst = Instance.from_json(
        BASE | {"public_ipaddr": "203.0.113.7", "ports": {"22/tcp": [{"HostPort": "n/a"}]}})
    assert inst.endpoint is not None and not inst.endpoint.direct
