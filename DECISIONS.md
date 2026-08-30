# DECISIONS

Append-only decision log for the Verified Code Reviewer project.
Newest entries are appended at the bottom. Do not rewrite prior entries.

---

## 2026-08-30 08:38 UTC — Deadline and reserve bookkeeping established

### Context

First UTC time check of the project, taken during the initial repository
inspection and before any implementation work.

### Evidence

```text
start_time            = 2026-08-30T08:38:01Z   (measured via `date -u`)
deadline              = 2026-08-31T18:00:00Z   (hard, given)
total_budget          = 33h 21m 59s            (deadline - start_time)
0.25 x total_budget   =  8h 20m 30s
reserve_trigger_time  = 2026-08-31T09:39:30Z   (deadline - 0.25 x total_budget)
work_window_to_reserve= 25h 01m 29s
```

### Decision

`reserve_trigger_time` is **fixed at 2026-08-31T09:39:30Z**. All later checks
compare the current wall-clock UTC time directly against this fixed timestamp.
The 25% figure is never recomputed against whatever time happens to remain.

Boundary checks are performed:

- before each major phase;
- before each optional experiment;
- after each individual tool is wired up;
- after each benchmark case is completed;
- after each experiment in the sweep concludes.

At `reserve_trigger_time`, all new feature and experiment work stops. Remaining
time goes exclusively to: clean-environment reproduction, README, improvement
changelog, results packaging, trajectory packaging, final verification, video,
and submission preparation.

### Rejected alternatives

- Recomputing a rolling 25% of remaining time at each check. Rejected: it never
  actually triggers, since 25% of a shrinking remainder shrinks with it.
- Tracking elapsed hours instead of wall-clock deadline distance. Rejected by
  the masterplan (§2, §4) and by the operating instructions.

### Consequence

Bookkeeping is recorded before any code exists. The next action is the initial
inspection report to the human, followed by Phase 1 (minimum foundation) only
after the human confirms scope and resolves the blockers recorded below.

---

## 2026-08-30 08:40 UTC — Initial repository inspection findings

### Context

Inspected the repository structure, Git status and history, `masterplan.md`,
`micro1 - First Hackathon97ce7c5.pdf`, `README.md`, `LICENSE`, and `.gitignore`
before beginning implementation.

### Evidence

Tracked files (commit `01fa1a1`, "Initial commit", the only commit):

```text
.gitignore   (Rust/Cargo template, 21 lines)
LICENSE      (Apache License 2.0, verified as intended)
README.md    (1 line: "# verified-code-review")
```

Untracked, present only in the main checkout
`D:\Dev\Verified Code Review\`:

```text
masterplan.md                        (25398 bytes)
micro1 - First Hackathon97ce7c5.pdf  (648125 bytes)
```

Neither authoritative document is committed to Git, and therefore neither is
present in this worktree
(`.claude/worktrees/verified-code-reviewer-setup-44a0bd`).

Environment probe:

```text
rustc   : not found (Windows PATH and Git Bash PATH)
cargo   : not found; %USERPROFILE%\.cargo\bin\cargo.exe does not exist
git     : 2.38.1.windows.1
ANTHROPIC_API_KEY / OPENAI_API_KEY / CLAUDE_API_KEY : all unset
```

Working tree is clean. No Rust source, no `Cargo.toml`, no benchmark, no
`DECISIONS.md` prior to this file, no `docs/`.

### Decision

Record the findings and report to the human before implementing. Two hard
blockers are surfaced rather than worked around:

1. **No Rust toolchain installed.** The project language is locked to Rust by
   the masterplan (§3) and by the operating instructions. Nothing can be built,
   tested, `cargo fmt`-ed, or `cargo clippy`-ed until Rust is installed.
2. **No LLM API key present in the environment.** Neither the baseline nor the
   advanced system can run without one, and cost/token accounting is a required
   metric.

Both are human-action items. Neither is a reason to change the Rust decision.

### Rejected alternatives

- Switching the project language away from Rust because the toolchain is
  missing. Rejected: a missing toolchain is an install step, not a concrete
  technical blocker. The language decision stands.
- Committing `masterplan.md` and the PDF without asking. Deferred to the human:
  the PDF is third-party hackathon material and committing it is a
  distribution decision, not an engineering one.

### Consequence

Awaiting human resolution of the two blockers. Phase 1 (minimum foundation)
begins after that.

---

## 2026-08-30 08:40 UTC — PDF vs masterplan conflict review

### Context

The operating instructions require that any conflict between the hackathon PDF
(authoritative) and `masterplan.md` (our strategy) be recorded here, with the
actual hackathon requirement taking priority.

### Evidence

Requirement-by-requirement comparison of the PDF's deliverables, judging
rubric, and ground rules against the masterplan.

### Decision

**No substantive conflict found.** The masterplan is a strict superset of the
PDF's requirements and is consistent with them. Specifically:

- PDF requires a fair baseline on the same cases; masterplan §4 Phase 2-3 and
  principle 6 match.
- PDF requires an improvement changelog with an entry per meaningful
  experiment, including removed ones; masterplan §19 matches.
- PDF requires one primary metric plus human time per task and cost per task;
  masterplan §9 matches and names F1 as the headline.
- PDF targets "ten or more cases ... when the task allows it" and one
  challenging case; masterplan §4 Phase 5 targets 12 with a documented floor of
  8 and 2 challenging cases. The floor of 8 sits below the PDF's soft target of
  10. This is a **tension, not a conflict** — the PDF phrases 10+ as a target,
  not a requirement. Resolution: aim for 12, treat 10 as the preferred floor,
  and use 8 only if the fixed reserve trigger is imminent. Whatever the final
  count, it is recorded here and in the results.
- PDF ground rule 04 (consequential actions sandboxed, human approval before
  the action) and 05 (qualified human reviewer in the loop); masterplan §15 and
  principle 10 match.
- PDF ground rule 02 (make clear what existed before the competition); the
  pre-existing state is recorded in the inspection entry above.
- PDF ground rule 08 (no credentials in the submission); masterplan §17 matches.

### Rejected alternatives

None; nothing had to be chosen between.

### Consequence

The masterplan can be followed as written. The 8-case floor is flagged as the
one place where following the masterplan could underperform the PDF's stated
target, and is to be avoided unless the reserve trigger forces it.

---

## 2026-08-30 09:00 UTC — Phase 1 foundation built

### Context

Rust 1.98.0 and cargo 1.98.0 confirmed installed. Provider chosen: Gemini 3.7
Flash via Google Vertex AI. Credentials will be supplied by the human in a
local `.env`; the agent never sees them.

Phase 1 per masterplan §4: build only enough infrastructure to run an
experiment. Explicitly *not* the full tool suite.

### Evidence

Delivered and verified:

```text
Cargo project            verified-code-reviewer 0.1.0, binary `vcr`
configuration            src/config.rs, loaded from .env
LLM abstraction          src/llm/ (vertex + deterministic mock)
finding schema           src/finding.rs, controlled IssueType enum
CLI                      src/main.rs: check | run | evaluate | report
trajectory logging       src/trajectory.rs
baseline reviewer        src/agent/baseline.rs (functional)
advanced skeleton        src/agent/advanced.rs (candidates only, by design)
deterministic evaluator  src/eval.rs
sandbox                  src/repo.rs
benchmark loader         src/bench.rs

cargo test    118 passed, 0 failed
cargo clippy  clean under -D warnings
cargo fmt     applied
```

Reserve check at 2026-08-30T09:00:37Z: 24h 39m until the fixed trigger. Build
phase continues.

### Decision

Foundation accepted as built. Several design choices are load-bearing for
evaluation validity and are recorded here rather than left in code comments:

1. **Ground truth is unreachable by construction, not by convention.** `Case`
   (what an agent sees) has no field pointing at `GroundTruth`, they are loaded
   by separate functions, and `RepoRoot` refuses any path named
   `ground_truth.json` as well as all parent traversal. Tested.

2. **Only `Verified` findings are scored as predictions.** `Uncertain` and
   `Rejected` are withheld and counted separately. Withholding is not free: a
   withheld true defect still costs a false negative, so suppression cannot
   game the metric. Tested both directions.

3. **Cost is never estimated.** Pricing must be supplied in `.env` as both
   input and output rates or neither; half-configured pricing is a startup
   error. Absent pricing, cost is reported as "unavailable", never as zero.
   The mock provider reports zero tokens so it cannot contribute plausible
   numbers to a cost table.

4. **Mock runs are marked in the data.** `RunSummary.provider` and
   `Aggregate.mock_run` propagate to the report, which prints a refusal banner
   rather than presenting stub output as a measurement.

5. **The advanced skeleton cannot report findings.** It classifies every
   candidate `Uncertain` with an explicit reason. If it could return
   `Verified`, the advanced arm would silently be a second baseline with a
   different prompt.

6. **Matching is one-to-one, closest-first.** Two predictions on one real
   defect yield one TP and one FP, because a reviewer still triages both.
   Prediction and expected orderings are sorted before matching, so the result
   does not depend on input order. Tested.

### Rejected alternatives

- A trait-object LLM abstraction with `async_trait`. Rejected: two backends do
  not justify the dependency or the boxing; a plain enum dispatches fine.
- A `gcp_auth`/`yup-oauth2` dependency for Vertex service-account flows.
  Rejected for build time and failure surface near a deadline. Three auth
  modes are supported instead — API key, a supplied access token, and shelling
  out to `gcloud auth print-access-token`.
- Defaulting precision to 1.0 when a system reports nothing. Rejected: it would
  let a reviewer that says nothing top the precision column.
- Storing ground truth inside the case's `repository/` directory. Rejected:
  the agent reads that tree.

### Consequence

Phase 2 next: 2–3 seed benchmark cases (real bug, false-positive trap,
context-dependent), then the baseline run and the GO/NO-GO checkpoint.

---

## 2026-08-30 09:00 UTC — E0 and E1 merged into a single baseline

### Context

Masterplan §8 lists E0 as "single LLM review" and E1 as "baseline + controlled
schema" — two separate experiments.

### Evidence

The deterministic evaluator matches a prediction to ground truth on
`issue_type` + location overlap. A free-form E0 response has neither a
controlled `issue_type` nor a machine-readable location, so it cannot be
scored by the evaluator at all. Scoring it would require either an LLM judge
(forbidden by masterplan §6 and by the operating instructions) or hand-coding
a free-text parser whose behaviour would itself become an uncontrolled
variable in the comparison.

### Decision

E0 and E1 are merged. The baseline is a single direct review pass that emits
the same controlled schema as the advanced arm. Both arms therefore share: the
same model, the same temperature, the same output contract, and the same view
of the diff and the changed files. The only difference is repository
investigation and fresh-context falsification — which is exactly the variable
under test.

The improvement changelog will carry one baseline row rather than two, and will
state this merge explicitly.

### Rejected alternatives

- Keeping a free-form E0 and scoring it with an LLM judge. Rejected outright:
  "LLM decides whether LLM was correct" is the failure mode this project
  exists to argue against.
- Keeping a free-form E0 and scoring it by hand. Rejected: not reproducible by
  a third party from a clean environment, which is 15 points of the rubric.
- Handicapping the baseline (weaker prompt, no file context) to widen the gap.
  Rejected as dishonest; it would also invalidate the central claim.

### Consequence

The baseline is deliberately a *strong* direct reviewer: same model, an
explicit instruction to avoid speculation, and the full contents of every
changed file. If the hypothesis survives against that, the result means
something.
