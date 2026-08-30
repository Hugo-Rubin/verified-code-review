#!/usr/bin/env python3
"""Turn a second model's baseline answers into a scorable run.

The baseline is one stateless call per case, so another model can be given the
byte-identical prompts exported by `export_baseline_prompts.py` and its answers
scored by the same deterministic evaluator. That is what makes this a
cross-model comparison rather than a re-implementation: nothing about the task,
the schema, the input, or the scoring changes — only the model.

    python scripts/import_external_baseline.py \
        --responses /tmp/sonnet-responses \
        --out results-sonnet \
        --model claude-sonnet-5

Then score it exactly like any other run:

    cargo run --bin vcr -- evaluate --agent baseline --out results-sonnet

Parsing deliberately mirrors `agent::mod::parse_review`: unknown issue types,
line 0, and empty claims are dropped with a recorded warning rather than
coerced, so a model that ignores the schema is penalised the same way in both
arms.
"""

import argparse
import datetime
import io
import json
import pathlib
import sys
import uuid

ISSUE_TYPES = {
    "Correctness", "ErrorHandling", "Validation", "StateManagement",
    "ResourceManagement", "Concurrency", "ApiContract", "Testing", "Performance",
}
SEVERITIES = {"Low", "Medium", "High"}


def normalize_path(p: str) -> str:
    p = p.replace("\\", "/")
    if p.startswith("./"):
        p = p[2:]
    return p.lstrip("/")


def parse_findings(payload, case_id):
    """Mirror of the Rust parser. Returns (findings, warnings)."""
    findings, warnings = [], []
    items = payload.get("findings") if isinstance(payload, dict) else None
    if items is None:
        return findings, ["response had no `findings` array"]

    for i, raw in enumerate(items):
        if not isinstance(raw, dict):
            warnings.append(f"finding[{i}] dropped: not an object")
            continue

        issue_type = str(raw.get("issue_type", "")).strip()
        if issue_type not in ISSUE_TYPES:
            warnings.append(
                f"finding[{i}] dropped: issue_type {issue_type!r} is not in the controlled taxonomy"
            )
            continue

        try:
            start = int(raw.get("start_line", 0))
        except (TypeError, ValueError):
            warnings.append(f"finding[{i}] dropped: start_line is not an integer")
            continue
        if start == 0:
            warnings.append(f"finding[{i}] dropped: start_line 0 (lines are 1-based)")
            continue

        claim = str(raw.get("claim", "")).strip()
        if not claim:
            warnings.append(f"finding[{i}] dropped: empty claim")
            continue

        try:
            end = int(raw.get("end_line", start))
        except (TypeError, ValueError):
            end = start
        end = max(end, start)

        severity = str(raw.get("severity", "")).strip()
        if severity not in SEVERITIES:
            severity = "Medium"

        findings.append({
            "id": f"{case_id}-base-{i + 1}",
            "issue_type": issue_type,
            "severity": severity,
            "location": {
                "file": normalize_path(str(raw.get("file", ""))),
                "start_line": start,
                "end_line": end,
            },
            "claim": claim,
            "reasoning": str(raw.get("reasoning", "")).strip(),
            "falsification_question": "",
            "evidence": [],
            "status": "Verified",
            "status_reason": "baseline: reported as produced, no verification stage",
        })

    return findings, warnings


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--responses", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--benchmark", default="benchmark/cases")
    args = ap.parse_args()

    resp_dir = pathlib.Path(args.responses)
    if not resp_dir.is_dir():
        print(f"no responses directory at {resp_dir}", file=sys.stderr)
        return 2

    out = pathlib.Path(args.out)
    traj_dir = out / "trajectories" / "baseline"
    traj_dir.mkdir(parents=True, exist_ok=True)

    now = datetime.datetime.now(datetime.timezone.utc).isoformat()

    # Token counts are unknown for an externally produced run, and inventing
    # them would put fabricated numbers in a cost column. Zero, with the run
    # marked, is honest.
    config = {
        "llm": {
            "provider": "Vertex",
            "model": args.model,
            "location": "external",
            "auth": "ApiKey",
            "temperature": 0.0,
            "max_output_tokens": 8192,
            "timeout_secs": 180,
            "max_retries": 0,
            "min_request_interval_ms": 0,
        },
        "match_line_tolerance": 3,
        "max_tool_calls_per_finding": 8,
        "max_followup_investigations": 1,
        "max_read_lines": 200,
        "max_search_results": 40,
        "ablation": "None",
    }

    stats, imported = [], 0
    for path in sorted(resp_dir.glob("*.json")):
        case_id = path.stem
        text = io.open(path, encoding="utf-8").read().strip()

        # Tolerate a fenced answer, as the Rust extractor does.
        if text.startswith("```"):
            body = text.split("\n", 1)[1] if "\n" in text else ""
            text = body.rsplit("```", 1)[0]

        try:
            payload = json.loads(text)
        except json.JSONDecodeError as e:
            print(f"{case_id}: unparseable response ({e}); recording zero findings", file=sys.stderr)
            payload = {}

        findings, warnings = parse_findings(payload, case_id)
        traj_id = str(uuid.uuid4())

        events = [{"event": "Note", "note": f"externally produced baseline run: model {args.model}"}]
        events += [{"event": "Note", "note": w} for w in warnings]
        events += [
            {"event": "CandidateProposed", "candidate": {
                "id": f["id"], "issue_type": f["issue_type"], "severity": f["severity"],
                "location": f["location"], "claim": f["claim"], "reasoning": f["reasoning"],
            }} for f in findings
        ]
        events.append({
            "event": "HumanCheckpoint",
            "note": (
                f"{len(findings)} finding(s) reported for human review. The system takes no "
                "action on the code: it does not merge, reject, or modify anything."
            ),
        })

        traj = {
            "trajectory_id": traj_id,
            "case_id": case_id,
            "agent": "Baseline",
            "model": args.model,
            "started_at": now,
            "finished_at": now,
            "config": config,
            "events": events,
            "final_findings": findings,
            "usage": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
            "runtime_ms": 0,
            "llm_calls": 1,
            "tool_calls": 0,
            "retries": 0,
        }
        io.open(traj_dir / f"{case_id}-baseline.json", "w", encoding="utf-8", newline="\n").write(
            json.dumps(traj, indent=2) + "\n"
        )

        stats.append({
            "case_id": case_id, "trajectory_id": traj_id, "runtime_ms": 0,
            "llm_calls": 1, "tool_calls": 0, "retries": 0,
            "input_tokens": 0, "output_tokens": 0,
            "reported_findings": len(findings), "withheld_findings": 0,
        })
        imported += 1
        if warnings:
            print(f"{case_id}: {len(findings)} finding(s), {len(warnings)} warning(s)")

    summary = {
        "agent": "Baseline", "model": args.model, "provider": "Vertex",
        "started_at": now, "finished_at": now,
        "benchmark_dir": args.benchmark, "case_count": imported,
        "config": config, "stats": stats,
    }
    io.open(out / "summary-baseline.json", "w", encoding="utf-8", newline="\n").write(
        json.dumps(summary, indent=2) + "\n"
    )

    total = sum(s["reported_findings"] for s in stats)
    print(f"imported {imported} case(s), {total} finding(s) total, into {out}")
    print("token counts are zero: an externally produced run has none to record")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
