#!/usr/bin/env python3
"""Structural verification of a benchmark directory.

Benchmark cases in this project are increasingly written by agents rather than
by hand, and an agent's report that it verified its own work is exactly the
standard this project refuses to accept. This runs the checks that do not
depend on anyone's say-so:

  1. every case has the four required artifacts
  2. the crate builds with **zero warnings**
  3. the test suite **passes** -- the defect must survive its own tests, or the
     case is not a review problem
  4. no `ground_truth.json` anywhere inside `repository/`, so the sandbox
     cannot serve answers to the reviewer
  5. every `diff.patch` matches its `_before/` and `repository/` trees
  6. `vcr check` reports no anchoring or description-neutrality warnings

What it deliberately does NOT check is whether the ground truth is *true*. That
requires executing the defect and reading the output, which is a human judgement
and is recorded per case in `notes` under "Verified by execution:". This script
tells you a case is well-formed; it cannot tell you it is correct.

    python scripts/verify_benchmark.py benchmark/holdout
    python scripts/verify_benchmark.py benchmark/holdout2 --language python

Exit code is non-zero if any check fails.
"""

import argparse
import json
import pathlib
import subprocess
import sys

REQUIRED = ["case.json", "diff.patch", "ground_truth.json", "repository"]


def run(cmd, cwd=None):
    return subprocess.run(
        cmd, cwd=cwd, capture_output=True, text=True, shell=isinstance(cmd, str)
    )


def check_case(case_dir: pathlib.Path, language: str) -> list[str]:
    problems = []
    name = case_dir.name

    for artifact in REQUIRED:
        if not (case_dir / artifact).exists():
            problems.append(f"{name}: missing {artifact}")
    if problems:
        return problems

    # The case_id must match the directory, or result tables become ambiguous.
    try:
        manifest = json.loads((case_dir / "case.json").read_text(encoding="utf-8"))
        if manifest.get("case_id") != name:
            problems.append(
                f"{name}: case_id {manifest.get('case_id')!r} != directory name"
            )
    except Exception as e:  # noqa: BLE001 - report, do not crash the sweep
        problems.append(f"{name}: case.json unreadable: {e}")
        return problems

    # Ground truth must never be reachable through the sandbox.
    leaked = list((case_dir / "repository").rglob("ground_truth.json"))
    if leaked:
        problems.append(f"{name}: ground_truth.json inside repository/: {leaked}")

    # A trap must have no expected findings; anything else must have some.
    try:
        gt = json.loads((case_dir / "ground_truth.json").read_text(encoding="utf-8"))
        n = len(gt.get("expected_findings", []))
        category = manifest.get("category")
        if category == "Trap" and n != 0:
            problems.append(f"{name}: Trap with {n} expected finding(s)")
        if category != "Trap" and n == 0:
            problems.append(f"{name}: {category} with no expected findings")
        if "Verified by execution" not in gt.get("notes", ""):
            problems.append(
                f"{name}: notes do not record an execution check "
                '("Verified by execution")'
            )
    except Exception as e:  # noqa: BLE001
        problems.append(f"{name}: ground_truth.json unreadable: {e}")

    repo = case_dir / "repository"
    if language == "rust":
        manifest_path = repo / "Cargo.toml"
        if not manifest_path.exists():
            problems.append(f"{name}: no Cargo.toml in repository/")
            return problems
        build = run(["cargo", "build", "--manifest-path", str(manifest_path)])
        warnings = [
            l for l in build.stderr.splitlines() if l.startswith("warning:")
        ]
        if build.returncode != 0:
            problems.append(f"{name}: does not build")
        elif warnings:
            problems.append(f"{name}: builds with {len(warnings)} warning(s)")
        tests = run(["cargo", "test", "--manifest-path", str(manifest_path)])
        if tests.returncode != 0:
            problems.append(
                f"{name}: TEST SUITE FAILS -- the suite must pass despite the defect"
            )
    else:
        tests = run("python -m pytest -q", cwd=repo)
        if tests.returncode != 0:
            problems.append(
                f"{name}: TEST SUITE FAILS -- the suite must pass despite the defect"
            )

    return problems


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("benchmark")
    ap.add_argument("--language", choices=["rust", "python"], default="rust")
    args = ap.parse_args()

    root = pathlib.Path(args.benchmark)
    if not root.is_dir():
        print(f"no such benchmark directory: {root}")
        return 1

    cases = sorted(d for d in root.iterdir() if d.is_dir() and (d / "case.json").exists())
    if not cases:
        print(f"no cases found under {root}")
        return 1

    print(f"verifying {len(cases)} case(s) in {root}\n")
    problems: list[str] = []
    for case_dir in cases:
        found = check_case(case_dir, args.language)
        status = "FAIL" if found else "ok"
        print(f"  {case_dir.name:<38} {status}")
        for p in found:
            print(f"      {p}")
        problems += found

    print("\n-- diffs match their trees --")
    diffs = run(
        [sys.executable, "scripts/make_diffs.py", "--root", str(root), "--check"]
    )
    out = (diffs.stdout + diffs.stderr).strip()
    print("  " + (out.splitlines()[-1] if out else "(no output)"))
    if diffs.returncode != 0:
        problems.append("diff.patch does not match the before/after trees")

    print("\n-- vcr check (anchoring + description neutrality) --")
    vcr = run(
        ["cargo", "run", "--release", "--quiet", "--bin", "vcr", "--",
         "check", "--benchmark", str(root)]
    )
    warns = [l.strip() for l in vcr.stdout.splitlines() if "WARNING" in l]
    if warns:
        for w in warns:
            print(f"  {w}")
        problems.append(f"{len(warns)} vcr check warning(s)")
    else:
        print("  no warnings")

    print()
    if problems:
        print(f"FAILED: {len(problems)} problem(s)")
        return 1
    print("All structural checks passed.")
    print(
        "This does NOT establish that the ground truth is correct. Each case's\n"
        "defect still has to be executed and its output read by a person."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
