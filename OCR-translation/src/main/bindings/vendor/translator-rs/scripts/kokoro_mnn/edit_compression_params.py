"""Edit MNN converter compression params for selected op weight settings."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--infile", required=True)
    parser.add_argument("--outfile", required=True)
    parser.add_argument("--op", action="append", default=[])
    parser.add_argument("--op-prefix", action="append", default=[])
    parser.add_argument("--exclude-prefix", action="append", default=[])
    parser.add_argument("--bits", type=int, required=True)
    args = parser.parse_args()

    data = json.loads(Path(args.infile).read_text())
    wanted = set(args.op)
    prefixes = tuple(args.op_prefix)
    exclude_prefixes = tuple(args.exclude_prefix)
    if not wanted and not prefixes:
        raise SystemExit("provide at least one --op or --op-prefix")
    changed: list[str] = []
    for algo in data.get("algo", []):
        quant = algo.get("quantParams", {})
        for layer in quant.get("layer", []):
            op_name = layer.get("opName")
            if op_name not in wanted and not op_name.startswith(prefixes):
                continue
            if exclude_prefixes and op_name.startswith(exclude_prefixes):
                continue
            for weight in layer.get("weight", []):
                weight["bits"] = args.bits
            changed.append(op_name)

    missing = sorted(wanted - set(changed))
    if missing:
        raise SystemExit(f"missing op(s): {missing}")

    Path(args.outfile).write_text(json.dumps(data, indent=2) + "\n")
    print(f"wrote {args.outfile}; set bits={args.bits} for {len(changed)} op(s)")
    for op_name in changed:
        print(f"  {op_name}")


if __name__ == "__main__":
    main()
