from __future__ import annotations

import json
import urllib.request
from dataclasses import dataclass

MANIFEST_TYPES = ",".join(
    [
        "application/vnd.docker.distribution.manifest.v2+json",
        "application/vnd.docker.distribution.manifest.list.v2+json",
        "application/vnd.oci.image.manifest.v1+json",
        "application/vnd.oci.image.index.v1+json",
    ]
)


@dataclass(frozen=True)
class ImageRef:
    registry: str
    repo: str
    tag: str

    @staticmethod
    def parse(image: str) -> ImageRef:
        name, _, tag = image.rpartition(":")
        if not name or "/" not in image:
            raise ValueError(f"need a fully qualified registry/repo:tag image, got {image!r}")
        registry, _, repo = name.partition("/")
        if "." not in registry:
            raise ValueError(f"{image!r} has no registry host; refusing to guess docker.io")
        return ImageRef(registry=registry, repo=repo, tag=tag)

    def digest(self) -> str:
        """Resolve tag -> content digest anonymously, the same way a rented box will."""
        tok = json.loads(
            urllib.request.urlopen(
                f"https://{self.registry}/token?scope=repository:{self.repo}:pull", timeout=30
            ).read()
        )["token"]
        req = urllib.request.Request(
            f"https://{self.registry}/v2/{self.repo}/manifests/{self.tag}",
            method="HEAD",
            headers={"Authorization": f"Bearer {tok}", "Accept": MANIFEST_TYPES},
        )
        with urllib.request.urlopen(req, timeout=30) as r:
            digest = r.headers.get("Docker-Content-Digest")
        if not digest:
            raise RuntimeError(f"{self} returned no Docker-Content-Digest")
        return digest
