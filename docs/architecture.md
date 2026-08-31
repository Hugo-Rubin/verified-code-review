# Architecture

One rule organises the whole codebase: **the LLM reasons, and Rust decides.**

The model generates hypotheses, chooses what to look at, interprets what comes
back, and offers a judgement. Rust supplies the evidence, enforces the
boundaries, assigns the final status, and does all the scoring. Wherever a
decision could be made either way, it is made in Rust — because that is the
half that can be tested, and 202 tests do test it.

---

## Module map

| Module | Responsibility | Why it is separate |
|---|---|---|
| `finding.rs` | Controlled `IssueType` taxonomy, locations, statuses, evidence | Predictions and ground truth share one type, so scoring cannot drift |
| `bench.rs` | Loading cases and ground truth | `Case` has no path to `GroundTruth` — a type-level guarantee, not a convention |
| `repo.rs` | Sandboxed filesystem access | The single boundary between an agent and the disk |
| `tools.rs` | `search`, `read`, `list_files` | The only source of `Evidence` in the system |
| `llm/` | Vertex client, retries, JSON extraction, offline stub | Isolates every provider quirk |
| `prompts.rs` | Every role's instructions, versioned independently | A result can always be traced to the exact instructions that produced it |
| `agent/baseline.rs` | The baseline's single reviewer role | The fair comparison point |
| `agent/advanced.rs` | Orchestration of four roles, and the decision gate | The system under test |
| `eval.rs` | Deterministic matching and metrics | No model anywhere near scoring |
| `trajectory.rs` | Full execution record | Auditability |
| `runner.rs` | Orchestration and aggregation | Keeps `main.rs` thin |
| `review.rs` | The reviewer used as a tool on a real repo and diff | A benchmark case and a real review are the *same* `Case` value; if this path behaved better, the benchmark would be measuring the wrong thing |
| `triage.rs` | Blind stopwatch session over pooled findings | Measurement, kept out of the pipeline it measures |
| `replay.rs` | Re-runs rules over recorded artifacts, calling no model | "Fired 0 times" is a claim about a run; this is how it gets checked against every run |

---

## The advanced pipeline

```
  case.json + diff.patch + changed-file contents
                     │
                     ▼
  ┌──────────────────────────────────┐
  │ 1. propose_candidates            │  advanced-review/v6
  │    "err toward proposing"        │  a wrong candidate is cheap here;
  └──────────────┬───────────────────┘  a missed one is never checked
                 │  CandidateFinding { issue_type, severity, location,
                 │                     claim, reasoning }
                 ▼
  ┌──────────────────────────────────┐
  │ 2. falsification_question        │  advanced-falsify/v2
  │    "what would show this WRONG?" │  a separate call, so the question is
  └──────────────┬───────────────────┘  on the record BEFORE any evidence
                 ▼
  ┌──────────────────────────────────┐
  │ 3. investigate (loop, bounded)   │  advanced-investigate/v2
  │                                  │
  │    model picks a tool ──────────►│  search / read / list_files
  │    Rust executes it    ◄─────────│  RepoRoot sandbox
  │    verbatim output fed back      │  refusals fed back too, so the
  │    ...until done or budget spent │  agent can correct a bad path
  └──────────────┬───────────────────┘
                 │  Vec<Evidence>  — file, lines, verbatim excerpt,
                 │                   tool_call_id.  Constructed by Rust.
                 ▼
  ┌──────────────────────────────────┐
  │ 4. verify_fresh                  │  fresh-verify/v5
  │                                  │
  │    receives: claim + evidence    │  NOT the reviewer's reasoning
  │    stateless request             │  NOT "a previous stage believed this"
  └──────────────┬───────────────────┘
                 │  Supports | Contradicts | Insufficient
                 ▼
  ┌──────────────────────────────────┐
  │ 5. decide()  — pure Rust         │
  └──────────────┬───────────────────┘
                 ▼
     Verified │ Rejected │ Uncertain
                 │
                 │  if NOTHING was reported for this case:
                 ▼
  ┌──────────────────────────────────┐
  │ 6. propose_again  (OFF by        │  advanced-second-look/v1
  │    default; --ablation           │  shown each rejected claim WITH the
  │    no-second-look, or            │  repository facts that closed it, and
  │    VCR_MAX_SECOND_LOOKS)         │  asked to look somewhere else.
  └──────────────┬───────────────────┘  Anything proposed re-enters at step 2.
                 ▼
        human reviewer decides
```

Step 6 is the only path where falsification output feeds *back into*
generation rather than only filtering it. It ships disabled: it fires on
exactly the cases that report nothing — which on both benchmarks means the
traps — and across six firings it correctly declined five times and invented a
finding once, gaining no recall anywhere for ~14% more cost. The code, its
tests and its ablation flag are kept so the measurement can be reproduced; see
the changelog.

### Why four roles rather than one

Each boundary in that diagram is a separate stateless request with its own
versioned instructions, and each exists because collapsing it would break
something specific:

| Boundary | What collapsing it would cost |
|---|---|
| Reviewer / Falsifier | A request asked to state a claim *and* to say what would refute it tends to produce a question shaped to fit the claim. |
| Falsifier / Investigator | The investigator is steered by the question, not the claim. That is what makes it hunt for the disproof rather than for confirmation. |
| Investigator / Verifier | The verifier must not see the investigation's running commentary — only what the tools actually returned. |
| Anything / Verifier | The verifier is the only role that can stop a finding. It must not know who is asking. |

This is orchestration doing work, not orchestration for its own sake — and the
distinction is measured rather than asserted. `--ablation` switches off one
role at a time so each one's contribution appears in the results table, and any
component that turns out to earn nothing is reported as such in the
[changelog](improvement-changelog.md) instead of being presented as a feature.

### Why the falsification question is its own call

If the same request produced both the question and the answer, the question
would be written to fit the answer. Splitting them fixes the question on the
record first. It is stored in the trajectory and travels with the finding.

### Why the verifier is a separate request

`LlmRequest` is deliberately stateless: every call carries its whole context
and there is no conversation object. The verifier therefore *cannot* inherit
the reviewer's reasoning — not by discipline, but because there is no channel
for it. Its prompt is written as if the reader has never seen the review, and
`prompts::tests::verifier_prompt_never_mentions_the_reviewer` fails the build
if words like "reviewer", "candidate", or "previous" appear in it.

### The decision gate

`agent::advanced::decide()` is where "I verified this" stops being enough:

| Condition | Status |
|---|---|
| Location does not resolve to a real file in the sandbox | `Rejected` |
| No verdict obtained (call failed, unparseable, unknown outcome) | `Uncertain` |
| `Contradicts` | `Rejected` |
| `Insufficient` | `Uncertain` |
| `Supports` **but zero investigation-derived evidence** | `Uncertain` |
| `Supports` **with** investigation-derived evidence | `Verified` |

The orchestrator seeds the claimed code region into the evidence package so the
verifier can see what is being discussed, but that item is tagged `DiffHunk`
and **does not count** toward the gate. If it did, every candidate would clear
the gate for free. To be `Verified`, a claim needs at least one thing the
investigation actually went and retrieved.

---

## Evidence

`Evidence` values are constructed **only** in `tools.rs`, from bytes read off
disk:

```rust
pub struct Evidence {
    pub kind: EvidenceKind,     // Search | FileRegion | DiffHunk | FileList
    pub file: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub symbol: Option<String>,
    pub excerpt: String,        // verbatim, never model-authored prose
    pub tool_call_id: String,   // cross-references the trajectory
}
```

The model may request a tool call and interpret the result. It cannot author an
evidence item. That is the mechanism behind "a model saying 'verified' is not
verification" — a claim is backed by excerpts a reader can go and check, or it
is not backed.

`FileList` never counts as evidence about a claim: a directory listing says
nothing about whether code is wrong.

---

## The sandbox

Every filesystem read on behalf of an agent goes through `RepoRoot`, rooted at
the case's `repository/` directory.

Rejected before any filesystem call: absolute paths, Windows drive prefixes,
UNC paths, and any `..` component. After canonicalisation, a path that no
longer starts with the root is rejected too, which catches symlinks pointing
outward. Any file named `ground_truth.json` is refused by name regardless of
location — belt and braces, since ground truth lives outside the sandbox
anyway.

There is no write path. The reviewer reads repositories and writes JSON to
`results/`. It cannot modify, merge, reject, or deploy anything.

---

## Deterministic evaluation

No model is involved in scoring.

A prediction matches an expected finding when the `issue_type` is acceptable
for that defect and the locations overlap within a fixed tolerance (default
±3 lines). Matching is one-to-one, closest-first by range midpoint, with both
sides sorted before matching so the result cannot depend on input order.

Two predictions on one defect produce one true positive and one false positive:
telling a reviewer the same thing twice still costs a second triage.

Only `Verified` findings are scored. `Rejected` and `Uncertain` are counted as
withheld — and withholding is not free, because a suppressed real defect still
costs a false negative. "Reject everything" scores zero.

`ExpectedFinding::also_accept` lists alternative categories that are defensible
readings of the same defect. Without it the benchmark would partly measure
agreement with our taxonomy rather than whether the bug was found. The
concession is on the category axis only; location must still overlap.

---

## Provider handling

`LlmClient` is a plain enum over two backends rather than a trait object — two
implementations do not justify the dependency or the boxing.

**Rate limiting is a first-class error.** `LlmError::RateLimited` is separate
from generic HTTP statuses because an agent making seven calls per case meets a
quota seven times sooner than one making a single call, so treating it as a
generic failure quietly penalises the more elaborate system. `Retry-After` is
honoured; otherwise quota errors back off from 4 s exponentially, capped at
60 s. A configurable minimum interval between requests keeps a sweep under
quota in the first place. This is not theoretical — it invalidated an entire
12-case comparison, which is written up in the changelog.

**JSON extraction is layered.** Strict parse, then code-fence stripping, then
the outermost balanced span (ignoring braces inside string literals), and
finally a repair pass that removes commas sitting directly before `}` or `]`.
The repair runs only after every strict parse has failed and cannot rescue
genuinely broken JSON. It exists because a real run discarded a correct finding
over one trailing comma.

**The mock provider reports zero tokens** so it cannot contribute
plausible-looking numbers to a cost table, and every run records its provider
so `report` can refuse to present stub output as a measurement.

---

## Cost accounting

Token usage is recorded per request and aggregated per case. Dollar cost is
computed **only** from operator-supplied rates: pricing must be fully
configured or fully absent, and absent pricing is reported as "unavailable"
rather than as zero.

Cost is recomputed at evaluation time from the token counts already stored, so
rates can be supplied after a run without spending the model again.

---

## Where the language boundary sits

The implementation is Rust-only by choice. The architecture is not, and the
split is visible in the module map: language knowledge is concentrated in two
places, and neither is load-bearing for the verification logic.

**Language-independent — the whole core:**

- `repo.rs` — path containment. Knows nothing about file contents.
- `tools.rs` — literal substring search, bounded line-range reads, path
  listing. Operates on bytes and line numbers.
- `finding.rs` — the evidence model is `(file, line range, verbatim excerpt)`,
  and the nine `IssueType` categories (Correctness, ErrorHandling, Validation,
  StateManagement, ResourceManagement, Concurrency, ApiContract, Testing,
  Performance) describe defect classes, not syntax.
- `agent/advanced.rs` — the falsification question, the fresh-context request,
  and `decide()` reason about claims and evidence. None of the three rules the
  verifier applies (reachability, materiality, comment checkability) mentions a
  language construct.
- `eval.rs` — matching is category plus location overlap.
- `trajectory.rs`, `runner.rs` — recording and aggregation.

**Language-specific — currently two things:**

- `prompts.rs` opens with "You are an experienced Rust reviewer". A Python
  variant is a prompt change, not an architecture change.
- `benchmark/cases/` is twelve Rust crates.

**Language-specific if extended:**

Test execution is the obvious next capability, and it is exactly the part that
does not generalise: `cargo test`, `pytest`, `npm test`, `go test`, `mvn test`.
So is AST and call-graph analysis, which is where the blind spots of literal
search — dynamic dispatch, trait objects, macro-generated call paths — would be
addressed properly rather than documented as limitations.

The natural shape of an extension is therefore a per-language adapter behind
the existing tool interface, leaving the pipeline untouched. That is a design
observation, not a demonstrated capability: nothing here has been run against a
non-Rust codebase, and doing so would need a prompt variant and a benchmark
with execution-verified ground truth before any claim could be made.

---

## Checks that read artifacts and call no model

Two commands exist because a claim in the README needed to be *checkable* by a
reader rather than believed, and neither may use a model to do it.

**`vcr replay-dedup`** re-runs the candidate-deduplication rule over every
recorded trajectory in the repository, separating merges that rest on genuinely
overlapping line ranges from those that rest only on the evaluator's ±3
matching tolerance. It exists because "fired 0 times in 3 trials" is a
statement about *those three runs*, not about the rule — and replaying it over
all 19 recorded runs showed the rule fires 6 times and is wrong every time.

**`vcr audit-matches`** pairs every scored true positive with the ground truth
it was credited for and prints both. The evaluator matches on location and
category, which is a proxy for "found the defect": a claim landing on the right
lines under an accepted category scores a true positive whether or not it
describes the real bug. No deterministic matcher can tell the difference, and
asking a model to judge would reintroduce precisely the standard this project
rejects — so the command computes **no verdict**. It puts both texts in front
of a person.

Both are in `replay.rs`, both finish instantly, and both are free.

---

## What was deliberately not built

- **AST or call-graph analysis.** The masterplan said not to build one without
  evidence that simpler tools were insufficient. Literal search plus bounded
  reads resolved every case in this benchmark, including all four traps. The
  limitation this leaves — dynamic dispatch, trait objects, macro-generated
  call paths — is stated in the README rather than papered over.
- **A multi-agent verifier.** The fresh-context request already provides
  independence without a second agent architecture. Nothing measured suggested
  a second agent would add more than it cost.
- **Sandboxed test execution.** Never needed: no failure in any run was of the
  kind that running the tests would have settled. In this benchmark the tests
  pass *despite* the defects by construction, so executing them would have been
  actively misleading.
- **Planner, critic, manager, editor roles.** No experiment suggested the
  orchestration was the bottleneck. The one time the pipeline underperformed,
  the cause was a single instruction telling the candidate stage that silence
  was acceptable.
