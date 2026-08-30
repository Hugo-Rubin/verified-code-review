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

---

## 2026-08-30 09:10 UTC — Seed benchmark built and ground truth verified by execution

### Context

Phase 2: three seed cases, one per required shape. All are standalone Rust
crates that compile and whose test suites pass.

### Evidence

```text
c01-pool-counter-leak       RealIssue    1 expected finding
c02-shard-index-trap        Trap         0 expected findings
c03-session-touch-context   Challenging  1 expected finding
```

Ground truth was not asserted by inspection. A throwaway harness linked all
three crates as dependencies and executed each claim:

```text
c01  2 successful acquires         -> active = 2
     2 REJECTED acquires           -> active = 4      (leak observed)
     both connections released     -> active = 2      (expected 0)
     pool permanently exhausted    -> true
c02  Router::new(vec![])           -> Err(NoShards)
     replace() then shards().len() -> 1               (count preserved)
     summary() on a 1-shard router -> ok
c03  sweep() drops the idle session, then on_heartbeat() panics at
     store.rs:54 with "called Option::unwrap() on a None value"
```

The observed panic line for c03, 54, falls inside the recorded ground-truth
range 53-55, so the location is confirmed against real behaviour rather than by
counting lines.

Each case's test suite passes *despite* the defect, which is what makes c01 and
c03 realistic: `refuses_beyond_the_limit` never asserts that the counter
recovers, and `touch_updates_last_seen` performs the `contains` check that the
buggy caller omits.

### Decision

Seed benchmark accepted. One evaluator change was made before any run, and is
recorded because it affects every number the project will report:
`ExpectedFinding` gained an `also_accept` list of alternative `issue_type`
values.

Rationale: a counter that is never decremented is defensibly
`ResourceManagement`, `StateManagement`, or `Correctness`. Without
`also_accept`, a reviewer that found the defect precisely but filed it under
the other name would be charged a false positive *and* a false negative, making
the benchmark partly a measure of agreement with our taxonomy rather than of
whether the bug was found. The concession is on the category axis only —
location overlap is still required, so it cannot turn an unrelated finding into
a true positive. Tested in both directions.

This turned out to matter immediately: the baseline filed c01 as
`StateManagement` while the ground-truth primary is `ResourceManagement`.

### Rejected alternatives

- Free-text or embedding similarity matching on the claim. Rejected as
  non-deterministic and unreproducible.
- Restricting the benchmark to defects with an unambiguous category. Rejected:
  it would bias the benchmark toward unrealistically tidy bugs.
- Putting ground truth inside `repository/`. Rejected: the agent reads that
  tree.

### Consequence

Benchmark is loadable and the baseline can run. Proceeding to the GO/NO-GO
checkpoint.

---

## 2026-08-30 09:19 UTC — Baseline GO/NO-GO: GO, with the failure mode reframed

### Context

Masterplan Phase 3. The baseline ran against all three seed cases before any
advanced work began. The question is whether the baseline actually exhibits a
verification problem worth solving.

### Evidence

Two runs, same model (`gemini-3.7-flash`, Vertex, temperature 0), same cases.

Run 1 used baseline prompt v1. While reading its output I found a
methodological flaw of my own making. The v1 prompt contained this line:

> "A pattern that merely looks risky is not a defect. `unwrap()` on a value
> that cannot be `None`, an index that cannot be out of bounds, and a lock that
> cannot deadlock are all correct code."

An `unwrap()` on a non-`None` value and an in-bounds index are near-verbatim
descriptions of c03 and c02. The baseline prompt was coaching the reviewer past
the exact situations the benchmark exists to test. That is prompt-level leakage
of case knowledge, and it had to go before the sweep was contaminated.

Run 2 used prompt v2, with those examples removed and the general principle
kept. A regression test now fails the build if a review prompt mentions
`unwrap`, `index`, `deadlock`, or any benchmark-specific noun.

Both runs produced identical scores:

```text
                      run 1 (v1 prompt)   run 2 (v2 prompt)
TP / FP / FN               1 / 0 / 1           1 / 0 / 1
precision                    1.000               1.000
recall                       0.500               0.500
F1                           0.667               0.667
findings to triage        0.33/case           0.33/case
runtime                  5241 ms/case        5849 ms/case
```

Per case, in both runs:

```text
c01 RealIssue    FOUND, precisely, at the right lines, filed as StateManagement
c02 Trap         reported nothing            -> correct, no false positive
c03 Challenging  reported nothing            -> MISSED a real, reachable panic
```

Run 1 artifacts are preserved under `results-archive/baseline-prompt-v1/`.

### Decision

**GO** — but the hypothesised failure mode was wrong, and the record says so.

The prediction was that a direct reviewer would over-report: plausible-looking
false positives on clean code, later cleared by falsification. That is not what
happened. This baseline is *conservative*. It produced zero false positives on
the trap, and on the case that mattered it stayed silent about a genuine bug.

The real failure mode is the mirror image: **the baseline accepts an
unverifiable claim made by the code itself, and goes quiet.** The c03 diff adds
a doc comment asserting "Callers check `contains` first, so the session is
known to be present." That assertion is false — `Server::on_heartbeat`, in a
file the change does not touch, calls `touch` without checking — but nothing in
the diff or in the changed file contradicts it, and the file's own unit test
appears to corroborate it. The baseline had no way to check, and trusted it.
The panic is reachable, and was demonstrated by execution.

That is still squarely a verification problem, and still exactly what
repository-aware investigation is for. Trustworthiness runs in both directions:
a reviewer that silently drops real defects is as untrustworthy as one that
invents them. What changes is the mechanism the advanced system must
demonstrate — investigation supplying evidence the reviewer could not otherwise
obtain, rather than falsification pruning overconfident guesses.

Both halves remain under test. The advanced reviewer's prompt deliberately asks
for every plausible candidate, which should raise its false-positive exposure
on c02; falsification then has to earn its place by clearing them. If it does
not, that is a reportable negative result and it will be reported.

### Rejected alternatives

- **Declaring NO-GO.** Rejected: F1 0.667 with a demonstrated, reproducible
  miss of a real defect is a meaningful problem, not a baseline performing
  extremely well.
- **Weakening the baseline prompt to manufacture false positives.** Rejected
  outright. The masterplan forbids artificially degrading the baseline and it
  would invalidate the central claim. Note that removing the v1 tells changed
  nothing, which is itself evidence that the conservatism belongs to the model
  rather than to the prompt.
- **Rewriting c02 into an easier trap.** Rejected: the case is doing its job.
  It is currently unfalsified rather than passed, and one clean case is not
  evidence about false-positive rates in either direction.
- **Keeping the v1 prompt because it scored identically.** Rejected: identical
  scores do not make case-specific coaching acceptable, and a reader auditing
  the prompts would rightly object.

### Consequence

Proceed to Phase 4 on the same three seed cases. Two limitations are now on the
record and must reach the README:

1. n=3 is far too small to conclude anything. One trap in particular is not
   evidence about false-positive rates.
2. If the baseline continues to produce no false positives as the benchmark
   grows, the falsification half of the thesis will have little to bite on, and
   the contribution will rest on investigation improving recall. That must be
   reported as measured, not narrated around.

Phase 5 expansion will therefore weight toward traps and context-dependent
cases, so that both halves of the hypothesis get a fair test.
