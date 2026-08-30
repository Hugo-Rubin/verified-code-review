#!/usr/bin/env python3
"""Render a trajectory JSON file as readable Markdown.

The JSON is the record of authority — it holds the complete prompts, every
tool response verbatim, and all accounting. This renders the same content in
reading order so a person can follow a run from the agent's instructions to
its final result without scrolling through escaped JSON.

Nothing is summarised or paraphrased. Long blocks are truncated with an
explicit marker naming how much was cut and where to read the rest.

Usage:
    python scripts/render_trajectory.py results/trajectories/advanced/c12-*.json
    python scripts/render_trajectory.py <file> --out docs/trajectories/
    python scripts/render_trajectory.py <file> --full     # no truncation
"""

import argparse
import io
import json
import pathlib
import sys

PROMPT_LIMIT = 2400
RESPONSE_LIMIT = 1800
TOOL_LIMIT = 1600


def clip(text: str, limit: int, label: str) -> str:
    text = text.rstrip()
    if limit <= 0 or len(text) <= limit:
        return text
    cut = len(text) - limit
    return f"{text[:limit].rstrip()}\n\n[... {cut} more characters of {label}; full text in the JSON ...]"


def fence(text: str, lang: str = "") -> str:
    # Avoid closing an outer fence early if the content contains backticks.
    guard = "```"
    while guard in text:
        guard += "`"
    return f"{guard}{lang}\n{text}\n{guard}"


def render(traj: dict, limits: bool) -> str:
    pl = PROMPT_LIMIT if limits else 0
    rl = RESPONSE_LIMIT if limits else 0
    tl = TOOL_LIMIT if limits else 0

    cfg = traj.get("config", {})
    llm = cfg.get("llm", {})
    out: list[str] = []
    w = out.append

    agent = traj.get("agent", "?")
    w(f"# Trajectory — `{traj.get('case_id')}` · {agent}\n")

    w("| | |")
    w("|---|---|")
    w(f"| Agent | {agent} |")
    w(f"| Case | `{traj.get('case_id')}` |")
    w(f"| Model | `{traj.get('model')}` |")
    w(f"| Provider | {llm.get('provider')} |")
    w(f"| Temperature | {llm.get('temperature')} |")
    w(f"| Trajectory id | `{traj.get('trajectory_id')}` |")
    w(f"| Started | {traj.get('started_at')} |")
    w(f"| Runtime | {traj.get('runtime_ms')} ms |")
    w(f"| Model calls | {traj.get('llm_calls')} |")
    w(f"| Tool calls | {traj.get('tool_calls')} |")
    w(f"| Retries | {traj.get('retries')} |")
    usage = traj.get("usage", {})
    w(f"| Tokens | {usage.get('input_tokens')} in / {usage.get('output_tokens')} out |")
    cost = traj.get("cost_usd")
    w(f"| Cost | {'$%.6f' % cost if cost is not None else 'not configured'} |")
    w(f"| Match tolerance | ±{cfg.get('match_line_tolerance')} lines |")
    w(f"| Tool-call budget | {cfg.get('max_tool_calls_per_finding')} per candidate |")
    w("")

    w("---\n")
    w("## Steps\n")

    for i, ev in enumerate(traj.get("events", []), start=1):
        kind = ev.get("event")

        if kind == "LlmCall":
            w(f"### {i}. Model call — {ev.get('stage')}\n")
            w(f"Prompt version `{ev.get('prompt_version')}` · "
              f"{ev['usage']['input_tokens']} in / {ev['usage']['output_tokens']} out · "
              f"{ev.get('latency_ms')} ms · attempt(s) {ev.get('attempts')}\n")
            if ev.get("attempts", 1) > 1:
                w(f"> Retried {ev['attempts'] - 1} time(s) before succeeding.\n")
            w("<details><summary>System instructions</summary>\n")
            w(fence(clip(ev.get("system", ""), pl, "system prompt")))
            w("\n</details>\n")
            w("<details><summary>User message</summary>\n")
            w(fence(clip(ev.get("user", ""), pl, "user message")))
            w("\n</details>\n")
            w("**Response**\n")
            w(fence(clip(ev.get("response_text", ""), rl, "response"), "json"))
            w("")

        elif kind == "LlmFailure":
            w(f"### {i}. Model call FAILED — {ev.get('stage')}\n")
            w(f"Prompt version `{ev.get('prompt_version')}` · "
              f"{ev.get('attempts')} attempt(s), all unsuccessful\n")
            w(fence(clip(ev.get("error", ""), rl, "error")))
            w("")

        elif kind == "CandidateProposed":
            c = ev["candidate"]
            loc = c["location"]
            w(f"### {i}. Candidate proposed — `{c['id']}`\n")
            w(f"**{c['issue_type']}** · severity {c['severity']} · "
              f"`{loc['file']}:{loc['start_line']}-{loc['end_line']}`\n")
            w(f"> {c['claim']}\n")
            if c.get("reasoning"):
                w(f"Reasoning: {c['reasoning']}\n")

        elif kind == "FalsificationQuestion":
            w(f"### {i}. Falsification question — `{ev['candidate_id']}`\n")
            w("Fixed before any evidence is gathered, so it cannot be written "
              "to fit the verdict.\n")
            w(f"> **{ev['question']}**\n")

        elif kind == "ToolCall":
            status = "ok" if ev.get("ok") else "refused / no result"
            w(f"### {i}. Tool call — `{ev['tool']}` ({status})\n")
            w(f"For candidate `{ev['candidate_id']}` · call id `{ev['tool_call_id']}` · "
              f"{ev.get('duration_ms')} ms\n")
            w("**Arguments**\n")
            w(fence(json.dumps(ev.get("arguments"), indent=2), "json"))
            w("\n**Tool response** (verbatim, this is what the agent saw next)\n")
            w(fence(clip(ev.get("response", ""), tl, "tool output")))
            w("")

        elif kind == "EvidenceAssembled":
            items = ev.get("evidence", [])
            w(f"### {i}. Evidence package — `{ev['candidate_id']}`\n")
            w(f"{len(items)} item(s) handed to the fresh verifier. Every one was "
              "produced by a Rust tool from bytes on disk; the model cannot "
              "author an evidence item.\n")
            if items:
                w("| # | Kind | Location | Excerpt (first line) |")
                w("|---|---|---|---|")
                for n, e in enumerate(items, start=1):
                    where = e.get("file") or "(repository)"
                    if e.get("start_line"):
                        where += f":{e['start_line']}"
                        if e.get("end_line") and e["end_line"] != e["start_line"]:
                            where += f"-{e['end_line']}"
                    first = (e.get("excerpt", "").strip().splitlines() or [""])[0]
                    first = first.replace("|", "\\|")[:90]
                    w(f"| {n} | {e.get('kind')} | `{where}` | `{first}` |")
                w("")

        elif kind == "Verification":
            r = ev["result"]
            w(f"### {i}. Fresh-context verification — `{ev['candidate_id']}`\n")
            w("A separate stateless request. It received the claim and the "
              "evidence and nothing else — not the reviewer's reasoning, and no "
              "indication that an earlier stage believed the claim.\n")
            w(f"**Verdict: {r['outcome']}**\n")
            w(f"> {r['rationale']}\n")
            if r.get("decisive_evidence"):
                w("Decisive evidence:\n")
                for d in r["decisive_evidence"]:
                    w(f"- `{d}`")
                w("")

        elif kind == "Decision":
            w(f"### {i}. Decision — `{ev['candidate_id']}`\n")
            w(f"**{ev['status']}**\n")
            w(f"Assigned by the orchestrator, not the model: {ev['reason']}\n")

        elif kind == "HumanCheckpoint":
            w(f"### {i}. Human checkpoint\n")
            w(f"> {ev['note']}\n")

        elif kind == "Note":
            w(f"### {i}. Orchestrator note\n")
            w(f"> {ev['note']}\n")

    findings = traj.get("final_findings", [])
    w("---\n")
    w("## Final findings\n")
    if not findings:
        w("None. The run produced no findings for this case.\n")
    else:
        for f in findings:
            c = f["candidate"] if "candidate" in f else f
            loc = c["location"]
            shown = "shown to the human" if f["status"] == "Verified" else "withheld"
            w(f"### `{c['id']}` — {f['status']} ({shown})\n")
            w(f"**{c['issue_type']}** at `{loc['file']}:{loc['start_line']}-{loc['end_line']}`\n")
            w(f"> {c['claim']}\n")
            if f.get("falsification_question"):
                w(f"Falsification question: *{f['falsification_question']}*\n")
            w(f"Status reason: {f['status_reason']}\n")
            w(f"Evidence items: {len(f.get('evidence', []))}\n")

    return "\n".join(out) + "\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("--out", help="directory to write .md files into")
    ap.add_argument("--full", action="store_true", help="do not truncate long blocks")
    args = ap.parse_args()

    out_dir = pathlib.Path(args.out) if args.out else None
    if out_dir:
        out_dir.mkdir(parents=True, exist_ok=True)

    for pattern in args.files:
        matches = sorted(pathlib.Path().glob(pattern)) or [pathlib.Path(pattern)]
        for path in matches:
            if not path.is_file():
                print(f"not a file: {path}", file=sys.stderr)
                continue
            traj = json.loads(io.open(path, encoding="utf-8").read())
            md = render(traj, limits=not args.full)
            if out_dir:
                target = out_dir / (path.stem + ".md")
                io.open(target, "w", encoding="utf-8", newline="\n").write(md)
                print(f"wrote {target}")
            else:
                print(md)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
