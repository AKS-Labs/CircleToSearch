#!/usr/bin/env python3
"""Deduplicate MNN external weight blobs without JSON-roundtripping the model.

MNN's JSON -> MNN path can perturb Kokoro outputs, so this helper uses a JSON
dump only as metadata. It writes a compact `.weight` file and patches int64
external offsets directly in the original flatbuffer bytes.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path
from typing import Any, Iterable


def iter_ops(root: dict[str, Any]) -> Iterable[dict[str, Any]]:
    yield from root.get("oplists", [])
    for subgraph in root.get("subgraphs", []):
        yield from subgraph.get("nodes", [])


def external_key(op: dict[str, Any], blob: bytes, ext_tail: list[int]) -> tuple[Any, ...]:
    main = op.get("main") or {}
    common = json.dumps(main.get("common", {}), sort_keys=True, separators=(",", ":"))
    return (
        op.get("type"),
        op.get("main_type"),
        common,
        tuple(ext_tail),
        hashlib.sha256(blob).digest(),
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--json", required=True, type=Path, help="MNN JSON dump")
    parser.add_argument("--mnn-in", required=True, type=Path, help="source .mnn")
    parser.add_argument("--weight-in", required=True, type=Path, help="source .mnn.weight")
    parser.add_argument("--mnn-out", required=True, type=Path, help="patched output .mnn")
    parser.add_argument("--weight-out", required=True, type=Path, help="deduplicated output .mnn.weight")
    args = parser.parse_args()

    root = json.loads(args.json.read_text())
    weights = args.weight_in.read_bytes()

    seen: dict[tuple[Any, ...], int] = {}
    old_to_new: dict[int, int] = {}
    out_weights = bytearray()
    referenced = unique = saved = 0

    for op in iter_ops(root):
        main_obj = op.get("main") or {}
        external = main_obj.get("external")
        if not external or len(external) < 2:
            continue

        old_offset = int(external[0])
        ext_tail = [int(x) for x in external[1:]]
        size = sum(ext_tail)
        blob = weights[old_offset : old_offset + size]
        if len(blob) != size:
            raise SystemExit(f"short blob at offset {old_offset}: wanted {size}, got {len(blob)}")

        key = external_key(op, blob, ext_tail)
        referenced += size
        if key in seen:
            new_offset = seen[key]
            saved += size
        else:
            new_offset = len(out_weights)
            seen[key] = new_offset
            out_weights.extend(blob)
            unique += size

        existing = old_to_new.get(old_offset)
        if existing is not None and existing != new_offset:
            raise SystemExit(f"offset {old_offset} maps to both {existing} and {new_offset}")
        old_to_new[old_offset] = new_offset

    model = bytearray(args.mnn_in.read_bytes())
    patched_occurrences = missing_offsets = multi_offsets = 0
    for old_offset, new_offset in sorted(old_to_new.items()):
        if old_offset == new_offset:
            continue
        old_bytes = struct.pack("<q", old_offset)
        new_bytes = struct.pack("<q", new_offset)
        count = model.count(old_bytes)
        if count == 0:
            missing_offsets += 1
            print(f"missing offset in flatbuffer: {old_offset} -> {new_offset}")
            continue
        if count > 1:
            multi_offsets += 1
        model = model.replace(old_bytes, new_bytes)
        patched_occurrences += count

    args.mnn_out.write_bytes(model)
    args.weight_out.write_bytes(out_weights)

    total_out = args.mnn_out.stat().st_size + args.weight_out.stat().st_size
    print(f"referenced={referenced} unique={unique} saved={saved}")
    print(
        f"chunks={len(seen)} offsets={len(old_to_new)} "
        f"patched_occurrences={patched_occurrences} missing_offsets={missing_offsets} "
        f"multi_offsets={multi_offsets}"
    )
    print(
        f"mnn_size={args.mnn_out.stat().st_size} "
        f"weight_size={args.weight_out.stat().st_size} total_size={total_out}"
    )


if __name__ == "__main__":
    main()
