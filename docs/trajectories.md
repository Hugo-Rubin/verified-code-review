# Agent trajectories

Every run this project reports is recorded in full. Nothing is summarised at
write time and nothing is discarded — including candidates that were rejected,
model calls that failed, and retries.

There are **two agents**, and representative trajectories for both are below.

- **Raw records:** [`../results/trajectories/`](../results/trajectories/) —
  24 JSON files, one per (case, agent).
- **Rendered for reading:** [`trajectories/`](trajectories/) — the five
  discussed here, as Markdown.

To render any other run:

```bash
python scripts/render_trajectory.py results/trajectories/advanced/c04-*.json --out /tmp
```

---

## What a trajectory contains

| Field | |
|---|---|
| `trajectory_id`, `case_id`, `agent`, `model` | identity |
| `config` | the complete run configuration, so conditions are never guessed |
| `events[]` | every step in execution order |
| `final_findings[]` | every finding with its status, evidence, and the reason for that status |
| `usage`, `cost_usd`, `runtime_ms`, `llm_calls`, `tool_calls`, `retries` | accounting |

Event types:

| Event | |
|---|---|
| `LlmCall` | prompt version, **full system and user text**, response, tokens, latency, attempts |
| `LlmFailure` | a call that failed after all retries, with the scrubbed error |
| `CandidateProposed` | the reviewer's provisional finding |
| `FalsificationQuestion` | fixed **before** any evidence is gathered |
| `ToolCall` | tool, arguments, **verbatim response**, whether it succeeded, duration |
| `EvidenceAssembled` | the package handed to the verifier |
| `Verification` | the fresh verdict and its decisive evidence |
| `Decision` | the status the orchestrator assigned, and why |
| `HumanCheckpoint` | what is handed to a person, and confirmation the system acted on nothing |
| `Note` | orchestrator diagnostics — dropped findings, unparseable responses |

Credentials never appear. Prompts are built from case material only, and any
provider error text passes through a scrubber before it is stored.

---

## Agent 1 — Baseline reviewer

One model call. Diff plus the full contents of every changed file, in, findings
out. There is no investigation stage and no verification stage, so every
finding it produces is reported as-is. That is the point of the comparison: its
only basis is that the model said so.

### [`c01-pool-counter-leak-baseline.md`](trajectories/c01-pool-counter-leak-baseline.md) — a clean hit

The whole run is one call. The model finds the counter leak precisely and
files it under `StateManagement` where ground truth's primary category is
`ResourceManagement` — which is exactly why `also_accept` exists in the
evaluator. Ends at a human checkpoint.

### [`c12-slot-guard-capacity-baseline.md`](trajectories/c12-slot-guard-capacity-baseline.md) — the informative miss

Returns `{"findings": []}`. Read it beside the advanced run on the same case
below.

The baseline is looking at `src/api.rs`, which contains an explicit bounds
check, a doc comment promising `None` rather than a panic, and three passing
tests. Everything visible says the case is handled. The defect is that
`Store::len()` returns *capacity* rather than fill — a fact that lives in
`src/store.rs`, a file this change does not touch and the baseline never sees.

This is not the model reasoning badly. It is the model reasoning correctly
about insufficient information.

---

## Agent 2 — Advanced reviewer

Five stages: candidate → falsification question → investigation → fresh-context
verification → decision. The falsification and verification stages are separate
stateless requests, so the verifier never inherits the reviewer's reasoning.

### [`c12-slot-guard-capacity-advanced.md`](trajectories/c12-slot-guard-capacity-advanced.md) — the headline execution

The same case the baseline missed. **7 model calls, 3 tool calls, 47 s.**

1. **Candidate** — *"fetch assumes store.record_at(index) is valid and
   non-panicking for every index below store.len(), which fails if Store allows
   sparse slots or deletions."*
2. **Falsification question** — *"Does the Store implementation allow
   deletions, vacant slots, or any index below store.len() to be unoccupied?"*
   Note this asks about the thing the claim depends on, not about the
   mechanism.
3. **Investigation** — `search {"pattern": "struct Store"}` locates
   `src/store.rs:13`; two bounded `read` calls pull in the whole file. The
   agent goes to the file the change never touched.
4. **Fresh verification** — Supports:

   > `Store::len()` returns the configured `capacity`, whereas
   > `Store::record_at(index)` indexes directly into `self.records`. When a
   > store is not completely filled, any index where
   > `store.filled() <= index < store.len()` passes the guard in `fetch` and
   > panics on the vector index operation.

   Decisive evidence: `src/api.rs:9-12`, `src/store.rs:30-32`,
   `src/store.rs:53-55`.
5. **Decision** — `Verified`, backed by 3 investigation-derived evidence items.

The verifier reached the ground-truth mechanism independently, and cited the
exact lines.

### [`c11-asset-path-check-trap-advanced.md`](trajectories/c11-asset-path-check-trap-advanced.md) — falsification doing its job

A trap. The change deletes a path-traversal guard, which is the shape of a real
vulnerability, and the reviewer duly proposes it.

The falsification question goes straight at usage rather than at the mechanism:
*"Do any callers of `asset_path` pass dynamic, user-controlled inputs rather
than hardcoded string literals?"* Three tool calls later:

> `asset_path` is crate-internal (`pub(crate)`) and all call sites pass fixed
> string literals returned by `AssetKind::file_name()`. In `src/serve.rs:23`
> the only production caller invokes `asset_path(&self.root,
> kind.file_name())`, where `kind` is mapped from a closed enum with hardcoded
> filenames. No caller passes arbitrary string inputs, preventing directory
> traversal.

`Contradicts` → **Rejected**. The reviewer never sees it.

This is the case for the whole design. A plausible, scary, *wrong* finding was
generated and then killed by evidence rather than by a judgement call — and the
argument is on the record for anyone who wants to check it.

### [`c03-session-touch-context-advanced.md`](trajectories/c03-session-touch-context-advanced.md) — a rejection worth reading

c03 is c11's twin with the opposite ground truth: a doc comment asserts callers
check first, and this time it is false. This run shows both halves of the
decision working on a single case.

Two candidates were proposed and they ended differently:

- *"Existing callers or consumers passing an unverified or expired session ID
  to `touch` will trigger an unexpected panic."* → **Verified**. The verifier
  worked past the doc comment: `touch` unconditionally unwraps
  `get_mut(id)`, and while `on_request` does check, another caller does not.
- *"Requiring callers to call `contains` before `touch` causes redundant hash
  map lookups on every session touch."* → **Rejected**. True, and not a defect.

One case, one finding shown and one suppressed, both decided on evidence. This
is the behaviour the materiality rule was added to produce, and it is why the
false-positive count stayed at one across the whole benchmark while candidate
generation was deliberately broadened.

Worth reading beside the archived run at
[`../results-archive/n12-run2-verify-v3/`](../results-archive/n12-run2-verify-v3/),
where an earlier verifier rejected the *correct* c03 finding on the grounds
that the doc comment documented the precondition. That single trajectory drove
two prompt revisions and is written up in the changelog.

---

## Retries, failures, and feedback

The tool layer feeds refusals back to the agent rather than swallowing them: a
bad path, an inverted line range, or a search with no matches returns
explanatory text as the tool response, and the agent gets another turn within
its budget. `ToolCall.ok = false` marks these.

Failed model calls are recorded rather than hidden. `LlmCall.attempts > 1`
means the request was retried; `LlmFailure` means every attempt failed and the
run continued without that stage's answer — which downgrades the finding to
`Uncertain`, never to `Verified`.

For a worked example of failure handling changing an outcome, see
[`../results-archive/n12-run1-advanced-regression/`](../results-archive/n12-run1-advanced-regression/):
`c04`'s Verify call returned HTTP 429 four times, so a correctly investigated
finding was classified `Uncertain` for want of a verdict. That run has 5 hard
failures and 21 retries across the advanced arm, and diagnosing it produced the
request-pacing fix.

## Human checkpoints

Every run ends with a `HumanCheckpoint` event stating what is handed over and
confirming the system acted on nothing:

> 1 verified finding(s) reported for human review; 0 investigated and cleared;
> 0 left uncertain. All findings, including cleared and uncertain ones, remain
> in this trajectory. The system takes no action on the code: it does not
> merge, reject, or modify anything.

The reviewer has no write path to any repository under review. All access goes
through a sandbox that rejects absolute paths, parent traversal, and symlinks
escaping the case root, and refuses any file named `ground_truth.json`.
