#!/usr/bin/env python3
"""Strip operator-identifying fields from recorded run artifacts.

`RunConfig` is embedded verbatim in every trajectory and summary so a reader
can see the exact conditions of a run. One field in it, the Vertex project id,
identifies the operator's cloud account and is not needed to reproduce
anything — whoever reproduces a run supplies their own in `.env`.

The reviewer now refuses to serialize that field at all (see
`LlmConfig::project_id`). This script cleans artifacts that were written before
that change, so historical results stay publishable without being re-run and
without losing anything that matters.

Only the `project_id` key is removed. Every measurement, prompt, tool call and
verdict is left exactly as recorded.

Usage:
    python scripts/scrub_artifacts.py [paths...]     # defaults to results/ and results-archive/
"""

import io
import json
import pathlib
import sys

FIELD = "project_id"
DEFAULT_ROOTS = ["results", "results-archive"]


def strip(node) -> bool:
    """Remove FIELD anywhere in the tree. Returns True if anything changed."""
    changed = False
    if isinstance(node, dict):
        if FIELD in node:
            del node[FIELD]
            changed = True
        for value in node.values():
            changed |= strip(value)
    elif isinstance(node, list):
        for item in node:
            changed |= strip(item)
    return changed


def main(argv: list[str]) -> int:
    roots = [pathlib.Path(p) for p in (argv or DEFAULT_ROOTS)]
    cleaned, scanned = [], 0

    for root in roots:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.json")):
            scanned += 1
            try:
                data = json.loads(io.open(path, encoding="utf-8").read())
            except (json.JSONDecodeError, OSError) as e:
                print(f"skipped {path}: {e}", file=sys.stderr)
                continue

            if strip(data):
                io.open(path, "w", encoding="utf-8", newline="\n").write(
                    json.dumps(data, indent=2) + "\n"
                )
                cleaned.append(str(path))

    print(f"scanned {scanned} artifact(s); cleaned {len(cleaned)}")
    for p in cleaned:
        print(f"  {p}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
