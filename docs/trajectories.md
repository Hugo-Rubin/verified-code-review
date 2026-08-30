# Agent trajectories

Every run this project reports is recorded in full. Nothing is summarised at
write time and nothing is discarded — including candidates that were rejected,
model calls that failed, and retries.

There are **two systems** under comparison, and **five agent roles** between
them. Representative trajectories covering every role are below.

| System | Roles | Prompt |
|---|---|---|
| Baseline | Reviewer | `baseline-review/v2` |
| Advanced | Reviewer | `advanced-review/v6` |
| | Falsifier | `advanced-falsify/v2` |
| | Investigator | `advanced-investigate/v2` |
| | Fresh verifier | `fresh-verify/v5` |

Each role is a separate stateless request with its own instructions. In a
trajectory they are distinguishable by the `stage` and `prompt_version` fields
on every `LlmCall`, so you can read any single role's contribution in
isolation:

```bash
python -c "import json,sys; t=json.load(open(sys.argv[1])); [print(e['stage'], e['prompt_version']) for e in t['events'] if e['event']=='LlmCall']" results-final/t1/trajectories/advanced/c12-slot-guard-capacity-advanced.json
```

- **Raw records:** [`../results-final/t1/trajectories/`](../results-final/t1/trajectories/)
  — every case for all four arms of the reported run.
- **Rendered for reading:** [`trajectories/`](trajectories/) — the five
  discussed here, as Markdown.

To render any other run:

```bash
python scripts/render_trajectory.py results-final/t1/trajectories/advanced/c04-*.json --out /tmp
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

## System 1 — Baseline: a single reviewer role

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

## System 2 — Advanced: four roles, orchestrated by Rust

Reviewer → falsifier → investigator → fresh verifier, then a decision made in
Rust. Each role is a separate stateless request, which is what stops the
verifier inheriting the reviewer's reasoning.

In the trajectories below, watch the `stage` on each model call change from
`Review` to `Falsify` to `Investigate` to `Verify`. Those are four different
sets of instructions, not one agent talking to itself: nothing carries between
them except the artifacts the orchestrator chooses to pass on — the claim, the
question, and evidence built by Rust from bytes on disk.

### [`c12-slot-guard-capacity-advanced.md`](trajectories/c12-slot-guard-capacity-advanced.md) — the headline execution

The same case the baseline missed. **6 model calls, 2 tool calls.**

1. **Candidate** — *"Callers passing an index for an unoccupied or deleted slot
   in `Store` will cause `store.record_at(index)` to panic despite the index
   being below `store.len()`."* Note it names a consequence and asserts
   something that can happen, rather than a conditional.
2. **Falsification question** — *"Does `Store::record_at(index)` return an
   `Option` or otherwise handle unoccupied slots without panicking for any
   `index < store.len()`?"* This asks about the thing the claim depends on, not
   about the mechanism.
3. **Investigation** — `search {"pattern": "fn record_at"}` finds the
   definition, then one bounded `read` of `src/store.rs`. The agent goes
   straight to the file the change never touched.
4. **Fresh verification** — Supports:

   > In `Store`, `store.len()` returns `self.capacity`, while `store.records`
   > only holds the pushed records. When a store has unoccupied slots, an index
   > below `len()` passes the guard and panics.

5. **Decision** — `Verified`, backed by investigation-derived evidence.

The verifier reached the ground-truth mechanism independently. This is also the
case that motivated `advanced-review/v6`: under the previous prompt the
reviewer proposed *nothing at all* here in 2 of 3 trials, so none of the above
happened.

### [`c11-asset-path-check-trap-advanced.md`](trajectories/c11-asset-path-check-trap-advanced.md) — falsification doing its job

A trap. The change deletes a path-traversal guard, which is the shape of a real
vulnerability, and the reviewer duly proposes it.

The falsification question goes straight at usage rather than at the mechanism:
*"Does every call site of `asset_path` pass only static string literals or
values returned by `AssetKind::file_name()`?"* Three tool calls later — a
`search` for the function, then reads of both `src/assets.rs` and
`src/serve.rs`:

> `asset_path` is crate-internal (`pub(crate)`), and all call sites in the
> repository pass only static file names produced by `AssetKind::file_name()`.
> Because the set of inputs is closed, no caller-supplied path reaches the
> join.

`Contradicts` → **Rejected**. The reviewer never sees it.

This is the case for the whole design. A plausible, scary, *wrong* finding was
generated and then killed by evidence rather than by a judgement call — and the
argument is on the record for anyone who wants to check it.

### [`c03-session-touch-context-advanced.md`](trajectories/c03-session-touch-context-advanced.md) — a rejection worth reading

c03 is c11's twin with the opposite ground truth: a doc comment asserts callers
check first, and this time it is false. This run shows both halves of the
decision working on a single case.

Two candidates were proposed and they ended differently:

> *"Existing callers that invoke `touch` with an unknown or expired session ID
> will panic at runtime on `.unwrap()`."*

The investigation searched for `touch(`, read `src/handler.rs`, and the
verifier worked past the doc comment:

> `touch` unconditionally calls `.unwrap()` on `self.sessions.get_mut(id)`.
> While `on_request` checks `contains(id)` beforehand, `on_heartbeat` does not.

**Verified.** The comment claiming callers check first is true of one caller
and false of the other, and only reading the call sites separates those.

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
