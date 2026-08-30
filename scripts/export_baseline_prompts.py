#!/usr/bin/env python3
"""Export the exact baseline prompts from a completed run.

The baseline is a single stateless call per case, and every trajectory records
the full system and user text that produced it. Exporting them lets a second
model be given *byte-identical* input, which is what makes a cross-model
comparison a comparison rather than a re-implementation.

    python scripts/export_baseline_prompts.py --run results --out /tmp/prompts

Writes one `<case-id>.system.txt` and `<case-id>.user.txt` per case, plus a
`manifest.json` listing them.
"""

import argparse
import io
import json
import pathlib
import sys


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", default="results", help="directory holding trajectories/baseline/")
    ap.add_argument("--out", required=True, help="directory to write prompt files into")
    args = ap.parse_args()

    traj_dir = pathlib.Path(args.run) / "trajectories" / "baseline"
    if not traj_dir.is_dir():
        print(f"no baseline trajectories at {traj_dir}", file=sys.stderr)
        return 2

    out = pathlib.Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    manifest = []
    for path in sorted(traj_dir.glob("*.json")):
        traj = json.loads(io.open(path, encoding="utf-8").read())

        call = next(
            (e for e in traj["events"] if e["event"] == "LlmCall" and e["stage"] == "Review"),
            None,
        )
        if call is None:
            print(f"skipped {path.name}: no Review call recorded", file=sys.stderr)
            continue

        case_id = traj["case_id"]
        sys_path = out / f"{case_id}.system.txt"
        usr_path = out / f"{case_id}.user.txt"
        io.open(sys_path, "w", encoding="utf-8", newline="\n").write(call["system"])
        io.open(usr_path, "w", encoding="utf-8", newline="\n").write(call["user"])

        manifest.append(
            {
                "case_id": case_id,
                "system": sys_path.name,
                "user": usr_path.name,
                "prompt_version": call["prompt_version"],
                "reference_model": traj["model"],
            }
        )

    io.open(out / "manifest.json", "w", encoding="utf-8", newline="\n").write(
        json.dumps(manifest, indent=2) + "\n"
    )
    print(f"exported {len(manifest)} case prompt pair(s) to {out}")
    if manifest:
        print(f"prompt version: {manifest[0]['prompt_version']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
