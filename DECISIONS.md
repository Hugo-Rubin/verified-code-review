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

---

## 2026-08-30 10:05 UTC — Phase 4: advanced loop built, and three iterations on it

### Context

The full pipeline is in place: candidate → falsification question →
investigation with repository tools → fresh-context verification → a status
assigned by Rust. Four measured iterations followed, on the same three seed
cases, same model, same temperature. All four are recorded here because two of
them made things worse.

### Evidence

```text
iteration                                   P      R      F1     FP/case
E0  baseline (direct review)              1.000  0.500  0.667    0.00
A1  advanced, candidates as-is            1.000  0.500  0.667    0.00
A2  + broadened candidate generation      1.000  1.000  1.000    0.00
A3  + claimed region seeded as evidence   0.500  1.000  0.667    0.67
A4  + reachability standard               1.000  1.000  1.000    0.00
```

**A1 — the pipeline ran, but only on one case.** The full loop worked
beautifully on c01: candidate, falsification question, a `read` of pool.rs,
fresh verification returning Supports, status Verified with evidence. But c02
and c03 produced *zero candidates*, so there was nothing to investigate. Tool
calls averaged 0.33 per case. The bottleneck was not verification, it was that
the advanced reviewer had nothing to verify.

Cause: both arms shared one JSON contract that ended with "An empty result is a
correct answer when the code is sound." For the baseline that is right. For a
stage whose entire purpose is to hand a worklist to an investigator, it
suppresses exactly the uncertain candidates the pipeline exists to resolve.

**A2 — split the closing instruction.** The baseline keeps "silence is
correct"; the advanced candidate stage is told that under-proposing is the
expensive mistake, and to raise anything whose correctness depends on facts not
visible in the changed files. Recall went 0.500 to 1.000.

**A3 — seeded the claimed region as evidence, and it backfired.** In A2, c02
was withheld as `Uncertain` for a bad reason: the verifier said the evidence
was insufficient because it had never been shown `health.rs`, the file the
claim was about. Fixing that looked obviously right, so the orchestrator began
seeding the claimed region into the evidence package.

It made things worse. Precision fell 1.000 to 0.500 and c02 became a false
positive. Shown the code, the verifier confirmed the claim:

> "If `router` were to contain no shards (resulting in an empty slice), direct
> indexing at index 0 in Rust triggers an out-of-bounds panic."

That is true, and useless. The candidate was phrased as a conditional — "will
panic **if** the router contains no shards" — and a conditional about a
mechanism is true whether or not the condition can ever hold. The investigation
had already read `router.rs` and seen the non-empty invariant; the verifier
simply never asked whether the triggering state was reachable.

c03 also picked up a second false positive: a `Performance` candidate about
redundant hash lookups in `on_request`. The observation is correct but is not
in ground truth, so it scores as a false positive — which is the intended
behaviour, since it is exactly the low-value noise that costs a reviewer triage
time.

**A4 — fixed the standard rather than reverting the change.** Reverting would
have restored F1 1.000, but on a system whose only correct clearing had been
luck. Instead the verifier prompt now states that reachability is part of the
claim: confirming the mechanism settles only half, and code that misbehaves
only in a state it can never occupy is not a defect — evidence that the state
is prevented *contradicts* the claim. The candidate prompt was matched, asking
for claims about what can happen rather than conditionals.

c02 then cleared for the right reason, as `Rejected` rather than merely
uncertain:

> "The sole constructor `Router::new` rejects empty inputs with
> `RouterError::NoShards`, the field is private, and mutating operations
> preserve the element count. Because a `Router` with no shards is unreachable
> by construction, calling `summary` on a router with no shards cannot occur."
>
> decisive evidence: src/router.rs:27-32, src/router.rs:46-54

And c03 reached the ground-truth mechanism rather than guessing at it. It
searched for `touch(`, read `src/handler.rs`, and identified `on_heartbeat` at
lines 42-44 as the caller that omits the `contains` check — which is precisely
what makes the panic reachable.

### Decision

Keep A4. Keep the seeded claimed region from A3 despite its having caused a
regression: the regression was the verifier's standard, not the evidence, and
the standard is now fixed. Evidence that the claimed region is not doing the
work on its own: it is tagged `DiffHunk`, and `concrete_evidence_count`
excludes that kind, so a candidate still cannot reach `Verified` without at
least one thing the investigation actually retrieved.

Recorded as a general finding, not a benchmark artifact: **a claim phrased as a
conditional cannot be falsified.** "X will panic if Y" is true of the code
regardless of whether Y is reachable, so a verifier that checks mechanisms will
confirm it every time. Falsification only does work when the claim asserts
something that can be shown not to happen.

### Rejected alternatives

- **Reverting the seeded evidence to restore F1 1.000.** Rejected: A2's F1
  1.000 depended on c02 being withheld because the verifier could not see the
  file it was judging. That is a lucky miss, not a working falsification step,
  and it would have collapsed the moment the evidence gap closed.
- **Counting the seeded region toward the evidence gate.** Rejected: every
  candidate would clear the gate for free and the gate would stop meaning
  anything. Regression-tested in both directions.
- **Adding a rule that panics behind an invariant are safe.** Rejected as
  benchmark-specific coaching. The reachability standard is a general property
  of code review and names no pattern.
- **Removing the c03 `Performance` finding from the false-positive count.**
  Rejected: it is a true observation of low value that a human still has to
  read and dismiss. Counting it is the point of the triage metric.

### Consequence

At n=3 the comparison is baseline F1 0.667 against advanced F1 1.000, with
identical precision and recall going 0.500 to 1.000. Three cases cannot
support that as a headline number, and it will not be presented as one until
the benchmark is expanded. Phase 5 next.

---

## 2026-08-30 11:05 UTC — Benchmark frozen at 12 cases

### Context

Phase 5 complete. The benchmark is frozen before the reported sweep.

### Evidence

```text
6 RealIssue     c01 c04 c05 c06 c07 c08
4 Trap          c02 c09 c10 c11
2 Challenging   c03 c12
```

Every case is a standalone Rust crate that compiles and whose tests pass. In
each defective case the suite passes *despite* the defect, so the tests give a
reviewer no signal. Ground truth for all twelve was verified by executing it.

Three cases are deliberately paired to isolate what investigation buys:

- **c03 vs c09** — the same edit (a silent fallback replaced by a panicking
  `expect`, with a doc comment asserting that callers check first). In c03 the
  assertion is false and the panic is reachable. In c09 it is true. The two are
  indistinguishable from the diff and the changed file.
- **c11** — a path-traversal guard is deleted. The deletion is safe only
  because every caller passes a closed enum's literal.
- **c12** — the reverse of the traps: a *safe-looking* guard that is wrong,
  because `Store::len` returns capacity rather than fill. It exists so that a
  system which improves precision by rejecting everything cannot score well.

### Decision

Benchmark frozen. Ground truth, expected findings, and case difficulty are not
to change. `scripts/make_diffs.py --check` verifies that every `diff.patch`
still matches its recorded `_before/` tree, so drift is detectable rather than
assumed absent.

12 cases meets the masterplan target and clears the PDF's "ten or more" soft
target, so the 8-case floor was never needed.

### Rejected alternatives

- Freezing at 8 to save time. Not needed; the reserve trigger was 23 hours away.
- Adding more traps after seeing that the advanced arm produced false positives
  on them. Rejected outright: changing the benchmark in response to results is
  the thing the freeze exists to prevent.

### Consequence

All reported numbers come from this frozen benchmark.

---

## 2026-08-30 11:40 UTC — First 12-case sweep: the advanced system LOST, and why

### Context

First full sweep on the frozen benchmark. The n=3 seed result had the advanced
arm at F1 1.000 against a baseline of 0.667.

### Evidence

It did not generalise. Measured, on 12 cases:

```text
                 baseline    advanced
precision           1.000       0.714
recall              0.750       0.625
F1                  0.857       0.667
FP/case              0.00        0.17
```

Per case, the advanced arm won one and lost four:

```text
c03  Challenging   0 -> 1 TP    the case the thesis was built on
c04  RealIssue     1 -> 0 TP    lost
c05  RealIssue     1 -> 0 TP    lost
c10  Trap          0 -> 1 FP    gained a false positive
c11  Trap          0 -> 1 FP    gained a false positive
```

Reading the trajectories, the four losses had three distinct causes, and only
one of them was about the idea being tested.

**1. Rate limiting (c04, and a confound across the whole arm).** The Verify
call for c04 returned HTTP 429 four times and gave up, so a correctly
investigated finding was classified `Uncertain` for want of a verdict. Across
the arm: 5 hard failures and 21 retries, all in the advanced arm. The advanced
reviewer makes roughly six model calls per case where the baseline makes one,
so it reaches a per-minute quota about six times sooner. The comparison was
partly measuring the quota, and the entire penalty landed on the arm under
test.

**2. A trailing comma (c05).** The model returned a well-formed finding
followed by a second object ending `"reasoning": "...",}`. That is invalid
JSON, `serde_json` correctly rejected it, and the whole response — including a
completely correct finding — was discarded.

**3. True but immaterial claims (c10, c11).** This is the interesting one. On
both traps the falsification step worked *perfectly*. It rejected the
dangerous-looking claim in each case with excellent reasoning:

> c11: "asset_path is crate-internal (`pub(crate)`) and all call sites pass
> fixed string literals returned by `AssetKind::file_name()` ... No caller
> passes arbitrary string inputs, preventing directory traversal."

Both cases still scored a false positive, from a *second* candidate:

```text
c10  "SizeReport does not derive Clone"                     -> Verified
c11  "asset_path returns Option but can never return None"  -> Verified
```

Both claims are **true**. Neither is a defect. The verifier confirmed them
because the evidence does support them — it was asked whether a claim is
accurate, and it answered correctly.

### Decision

Fix all three, then re-run both arms under identical conditions. Specifically:

1. `LlmError::RateLimited` split out from generic statuses, `Retry-After`
   honoured, base backoff for quota errors raised to 4s exponential (capped at
   60s), default retries 3 to 5, and a configurable minimum interval between
   requests defaulting to 1500ms.
2. `extract_json` gained a last-resort repair that removes commas sitting
   directly before `}` or `]`, outside string literals. It runs only after
   every strict parse has failed, and it cannot rescue genuinely broken JSON.
3. The verifier was reframed from "does the evidence support this claim" to
   "does the evidence establish a real defect", with an explicit rule that an
   accurate description of the code that identifies nothing wrong is
   `Contradicts`, not `Supports`. The candidate prompt now requires every
   claim to name a consequence, and names missing derives and
   more-fallible-than-necessary signatures as non-defects.

Fixes 1 and 2 are plumbing defects in the harness, not tuning: they affect
both arms identically and would have been bugs whatever the results said.
Fix 3 is a genuine design change and is recorded as such.

### Rejected alternatives

- **Reporting the n=3 result as the headline.** Rejected. It is exactly the
  overclaim the 12-case sweep exists to prevent, and the sweep disproved it.
- **Dropping c10 and c11, or narrowing their ground truth to exclude
  nitpicks.** Rejected as benchmark tampering after seeing results. The traps
  did their job; the system was wrong.
- **Suppressing low-severity findings at the decision stage.** Rejected as a
  metric hack: severity is self-reported by the model, so it would let the
  system hide its own noise by relabelling it, and the same trick would
  suppress genuine low-severity defects.
- **Leaving the rate limiting in place as "realistic".** Rejected. It is an
  artefact of a free-tier quota, it degrades only one arm, and reporting it as
  a property of the design would be misleading.

### Consequence

Run 1 is preserved at `results-archive/n12-run1-advanced-regression/`. It is
not the reported result, and the improvement changelog carries it as a stage
with its own evidence.

---

## 2026-08-30 12:05 UTC — Reported result: advanced 0.933 vs baseline 0.857

### Context

Both arms re-run on the frozen 12-case benchmark after the three fixes, same
model, same temperature, same session.

### Evidence

```text
| Metric                   | Baseline | Advanced |  Change |
|--------------------------|---------:|---------:|--------:|
| Precision                |    1.000 |    1.000 |  +0.000 |
| Recall                   |    0.750 |    0.875 |  +0.125 |
| F1                       |    0.857 |    0.933 |  +0.076 |
| False positives/case     |     0.00 |     0.00 |   +0.00 |
| Findings to triage/case  |     0.50 |     0.58 |   +0.08 |
| Runtime/case (ms)        |    17389 |    33324 |  +15936 |

By category            baseline          advanced
RealIssue   n=6        6/0/0  F1 1.000   6/0/0  F1 1.000
Trap        n=4        0/0/0  F1 0.000   0/0/0  F1 0.000
Challenging n=2        0/0/2  F1 0.000   1/0/1  F1 0.667
```

Run health: zero hard failures, 6 retries, all recovered. The rate-limiting
confound is gone.

The whole gain is on the challenging cases, which is precisely where the
hypothesis said it should be: both arms are perfect on the six real defects
that are visible in the diff, both are clean on the four traps, and the
advanced arm additionally resolves c03, which requires finding an unchecked
caller in a file the change does not touch.

Precision held at 1.000 while candidate generation was deliberately broadened.
That is falsification doing its job: five candidates were investigated and
`Rejected` with repository evidence, one on each of the four traps plus one
overbroad candidate on c03.

### Decision

This is the reported result. It supports a narrower claim than the seed run
suggested, and the narrower claim is the one that goes in the README:
repository-grounded investigation buys recall on defects whose evidence lives
outside the diff, and fresh-context falsification is what makes it safe to go
looking, by keeping precision at 1.000 while the candidate stage is opened up.

### Rejected alternatives

- Claiming the improvement is driven by falsification reducing false
  positives. The data does not show that: the baseline had zero false
  positives to remove. Falsification's measured contribution is *protecting*
  precision under broadened candidate generation, which is a different and
  smaller claim.

### Consequence

Remaining known failure: c12 is missed by both arms. Both accept a bounds
check that calls `Store::len`, and neither reads far enough to discover that
`len` returns capacity rather than fill. That is the main failure mode and it
goes in the README as measured, not as a footnote.

---

## 2026-08-30 12:30 UTC — Correction to the 12:05 entry

### Context

This log is append-only, so the earlier entry stands as written and is
corrected here.

### Evidence

The 12:05 entry ("Reported result: advanced 0.933 vs baseline 0.857") says the
advanced arm "additionally resolves c03" and that "c12 is missed by both arms".
Both statements are **wrong**, and reversed. Checking the per-case evaluation
for that run:

```text
c03-session-touch-context   Challenging   TP=0 FP=0 FN=1  (withheld 1)
c12-slot-guard-capacity     Challenging   TP=1 FP=0 FN=0
```

The advanced arm resolved **c12** and missed **c03**. The aggregate figures in
that entry (P 1.000, R 0.875, F1 0.933, one challenging case solved) were
correct; only the attribution of *which* case was wrong.

### Decision

Correction recorded. The mistake matters because the two cases fail for
opposite reasons, and the wrong attribution would have pointed the next
investigation at the wrong thing. Reading the c03 trajectory is what produced
the `fresh-verify/v4` experiment below.

### Consequence

The claim "the main failure mode is c12" in that entry is withdrawn. The
failure mode in that run was c03, and it is analysed in the next entry.

---

## 2026-08-30 13:10 UTC — Verifier v4 and v5: what counts as evidence

### Context

Run 2 (`fresh-verify/v3`, F1 0.933) still missed c03. The reason turned out to
be the most on-thesis failure in the project.

### Evidence

The c03 trajectory shows the reviewer proposing the correct claim, then having
it rejected:

> **Candidate:** "Existing or third-party callers that pass an expired or
> missing session ID to `touch` without first checking `contains` will cause an
> unhandled panic."
>
> **Falsification question:** "Does `touch` in `src/store.rs` panic when called
> with a missing or expired session ID?"
>
> **Verdict — Contradicts:** "The `touch` method explicitly documents its
> precondition: 'Callers check `contains` first, so the session is known to be
> present.' Panicking on `.unwrap()` when a caller violates this documented
> precondition is expected behavior rather than a defect in the method."

Two compounding faults. The falsification question asked about the *mechanism*
(does it panic?) rather than about the thing the claim depends on (does any
caller violate the precondition?), so the investigation only read `store.rs`
and never enumerated callers. And the verifier then accepted the function's own
doc comment as settling the question — the comment that c03 exists to make
false.

**`fresh-verify/v4`** was the direct fix: the code's own claims about itself
are not evidence, and when a claim depends on callers only the call sites
settle it. The falsification prompt was matched, pushing questions toward call
sites and stating that a comment must never be the answer.

Measured on the frozen benchmark:

```text
                     run 2 (v3)   run 3 (v4)
precision                 1.000        1.000
recall                    0.875        0.750
F1                        0.933        0.857
RealIssue    n=6      6/0/0        4/0/2
Trap         n=4      0/0/0        0/0/0
Challenging  n=2      1/0/1        2/0/0
```

v4 did exactly what it was designed to do — **both** challenging cases now
resolve — and regressed overall by rejecting two genuine defects:

```text
c06  "the only mention of batch sizes reaching 100,000 to 500,000 elements
      appears in a module doc comment rather than in concrete call sites"
c08  "the evidence does not include the database schema or insert queries to
      confirm that orders.name has a 64-character limit"
```

Told to distrust comments, it distrusted facts the repository has no way to
state anywhere else. A repository can settle whether callers honour a
precondition; it cannot contain the database schema of the service it talks to.

**`fresh-verify/v5`** keeps the distinction the repository can actually make: a
comment asserting something the repo can check is a claim, so go read the call
sites; a comment stating a fact from outside the repo is the best evidence
available and is reasoned from, not dismissed.

```text
                     run 4 (v5)  — FINAL
precision                 0.889
recall                    1.000
F1                        0.941
RealIssue    n=6      6/1/0
Trap         n=4      0/0/0
Challenging  n=2      2/0/0
```

### Decision

v5 is the reported configuration. It is the only version that resolves both
challenging cases *and* keeps all six real defects, and it clears all four
traps. The single false positive is a design observation about the notes length
limit in c08.

The baseline was re-run in the same session afterwards so both arms come from
identical conditions. It scored P 1.000, R 0.750, F1 0.857 — unchanged from the
previous run, as expected given its prompt and configuration did not change.

### Rejected alternatives

- **Reporting v3 (F1 0.933) and not attempting v4.** Rejected: c03 was failing
  for the exact reason the project exists to address, and leaving it unexamined
  to protect a number would have been the wrong trade.
- **Reverting to v3 after v4 regressed.** Rejected: v4's regression was
  informative rather than fatal. The rule was right and too broad, and
  narrowing it produced the best configuration measured.
- **Keeping v4 because it scored 2/2 on the challenging cases.** Rejected:
  headline F1 fell, and trading two real defects for two challenging ones is
  not an improvement.
- **Tuning the c06 and c08 ground truth so the facts live in code rather than
  comments.** Rejected outright as benchmark tampering after seeing results.
  The benchmark was frozen; the system was wrong.

### Consequence

Five configurations of the advanced arm have now been measured on the frozen
benchmark (A4, v3, v4, v5) plus the seed-phase runs, and every one is preserved
under `results-archive/`. The improvement changelog carries them all, including
the three that made things worse.

Remaining known failure: one false positive on c08, a true but immaterial
observation that survived the materiality rule. Falsification filters for truth
and, with the v5 wording, for consequence — but the boundary is a judgement the
model still makes, and it will not always draw it where a reviewer would.

---

## 2026-08-30 13:45 UTC — Scope framing: Rust MVP of a language-independent architecture

### Context

Positioning decision from the human: the hackathon deliverable is a Rust
reviewer reviewing Rust changes, and the verification architecture is
deliberately built so that language-specific tooling can be added later. This
is a documentation change only — no code was written to support other
languages.

### Evidence

Checked the claim against the actual module map rather than accepting it.
Language knowledge turns out to sit in exactly two places:

```text
language-independent
  repo.rs            path containment, no knowledge of file contents
  tools.rs           literal substring search, bounded line reads, listing
  finding.rs         evidence is (file, line range, excerpt); the nine
                     IssueType categories are defect classes, not syntax
  agent/advanced.rs  falsification, fresh-context verification, decide().
                     None of the three verifier rules - reachability,
                     materiality, comment checkability - names a language
                     construct
  eval.rs            category plus location overlap
  trajectory.rs      recording and accounting

language-specific
  prompts.rs         opens "You are an experienced Rust reviewer"
  benchmark/cases/   twelve Rust crates

language-specific if extended
  test execution     cargo test / pytest / npm test / go test / mvn test
  AST + call graph   where the literal-search blind spots would be fixed
```

### Decision

Documented in `README.md` ("Scope: a Rust demonstration of a
language-independent design") and `docs/architecture.md` ("Where the language
boundary sits"), both stating plainly that this is a claim about the design and
**not** a measured result: nothing in this project has been run against a
non-Rust codebase.

The narrow scope is also recorded as what made the evaluation affordable — one
toolchain, one build system, one test runner, no sandbox variation — which is
why a 12-case benchmark with execution-verified ground truth fit inside the
deadline.

### Rejected alternatives

- Claiming multi-language support or implying the results generalise. Rejected:
  it would be unevidenced, and the project's whole argument is about not
  reporting unverified claims.
- Adding a language abstraction layer now. Rejected: no experiment demanded it,
  the masterplan forbids building features without evidence they are needed,
  and the human explicitly asked for documentation only.

### Consequence

Framing is recorded. Two measurement improvements are deferred to a later
session by agreement with the human, and are named here so they are not lost:

1. **Multiple trials per arm** for variance. Currently one run per arm. Real
   nondeterminism was observed: the advanced arm's handling of c03 and c12
   swapped completely between two consecutive runs.
2. **A stopwatch measurement of human review time**, replacing the currently
   labelled findings-to-triage proxy.

Token pricing will be supplied by the human, after which `vcr evaluate` fills
in cost per case from the already-recorded token counts with no further model
spend.

---

## 2026-08-30 15:50 UTC — Sprint 2 design decisions: measurement instruments

### Context

With pricing supplied by the human, cost per case is now reported. This entry
records the design of four measurement instruments built in this sprint,
before any of them produced numbers, so the designs cannot be read as having
been chosen to fit results.

### Decision 1 — Evidence accuracy is checked, not asserted

The system claims every evidence excerpt is verbatim repository content at a
cited location. That claim is itself checkable, so `eval::audit_evidence` now
checks it: for each item it re-reads the cited file and compares the excerpt
line by line against the stated line numbers, stripping the `"  123 | "`
gutter that bounded reads prepend.

Deterministic, no model involved, no spend. Items with no file or no excerpt
count toward `total` but not `checkable`, and accuracy is defined as 1.0 when
nothing was checkable — a run that gathered no evidence has not misquoted
anything. `checkable` is reported alongside so that 1.0 cannot be read as
strong when it is vacuous.

Rejected: asking a model whether the evidence looked right. That is the exact
failure mode this project argues against.

### Decision 2 — Ablations, so "which change mattered" is measured

`--ablation` switches off one stage at a time:

```text
none              the complete pipeline
no-falsification  investigation still runs; nothing adjudicates it. Any
                  candidate with evidence is reported. Isolates what
                  falsification is worth on top of investigation.
no-followup       falsification stays, but an "Insufficient" verdict never
                  triggers a second look. Isolates the feedback loop.
candidates-only   no investigation, no verification. Isolates the prompt
                  from the machinery.
```

Each writes to `summary-<agent>-<ablation>.json` and its own trajectory
directory, so an ablation can never overwrite a full run, and every ablation
trajectory opens with a Note marking it as a deliberately crippled run.

Rejected: arguing from the trajectories about what each stage contributed.
The A2 result showed our intuitions about this pipeline are unreliable —
candidate generation, not verification, was the bottleneck — so the
contributions get measured.

### Decision 3 — The stopwatch harness is blind and deliberately conservative

`vcr triage` replaces the findings-to-triage proxy with a real measurement.
Two design choices matter and both cost the advanced arm:

**Blind.** Findings from every arm are pooled, shuffled with a recorded seed,
and presented with no indication of which system produced them. A reviewer who
knows a finding came from "the advanced system" spends different effort on it,
and that difference would land directly in the number being measured.

**Claim only, no evidence.** The reviewer sees the claim and its location, not
the gathered evidence and not the verifier's verdict. This understates the
advanced system's benefit — a reviewer handed a cited argument plausibly
decides faster than one handed a bare assertion. Showing evidence would
measure the whole product, but it would also make the arms instantly
distinguishable and destroy the blinding.

Measuring the conservative quantity honestly beats measuring the flattering
one badly. The limitation is written into every session file rather than left
for a reader to notice.

Rejected: an unblinded session with full output. More realistic, and
uninterpretable.

### Decision 4 — Repeated trials, because one run is a sample

`vcr variance` aggregates `<root>/<trial>/evaluation-<arm>.json` and reports
mean, min, max and sample standard deviation per metric, plus the specific
cases whose true-positive count was not identical across trials.

Naming the unstable cases matters more than the standard deviation: it is the
difference between "F1 moved by 0.04" and "c03 and c12 trade places between
runs", and only the second tells you anything.

### Decision 5 — A Python pilot, in a separate benchmark

Three Python cases live in `benchmark/pilot-python/`, deliberately **not**
merged into the frozen Rust benchmark — mixing them would break the freeze and
contaminate every headline figure.

Two of the three are direct analogues of Rust cases (p02 of c02, p03 of c12),
because the question the pilot exists to answer is whether the *same
investigation behaviour* transfers to a language with different failure modes,
not whether the reviewer can find Python bugs in general.

Ground truth was verified by execution, as for the Rust cases: p01's silent
`None` was observed propagating into an `AttributeError` in a different
module, p02's cluster invariant was probed and held, p03's `len()` guard was
observed admitting an index that then raised `IndexError`.

p02's notes record an honest wrinkle with no Rust equivalent: Python privacy
is a convention, so `cluster._nodes.clear()` would work from outside. A
reviewer flagging that has made a real argument, and it still scores as a
false positive under our ground truth. That scoring decision is recorded
rather than hidden.

### Consequence

Instruments built. Results follow in the next entries, and every one of these
designs was fixed before its first number existed.


---

## 2026-08-30 17:10 UTC — Sprint 2 results: three trials, one ablation, two corrections

### Context

The instruments designed in the previous entry were run. Three trials of each
arm on the frozen 12-case benchmark, plus a falsification ablation, plus the
Python pilot.

### Evidence

```text
arm                          P              R              F1             FP/case  $/case
baseline                  1.000 ± 0.000  0.750 ± 0.000  0.857 ± 0.000     0.00    0.0032
advanced                  0.921 ± 0.069  0.917 ± 0.072  0.917 ± 0.036     0.06    0.0147
advanced-no-falsification 0.619 ± 0.031  0.875 ± 0.000  0.725 ± 0.021     0.36    0.0108
```

Evidence accuracy: **1.000 in every run of every arm**, across 48–60 cited
excerpts per advanced run and 17/17 on Python. Zero mismatches ever observed.

Follow-up loop: **fired zero times**. Across 36 verifications the verifier
returned `Supports` 24 times and `Contradicts` 12 times, never `Insufficient`.

Python pilot (1 run, 3 cases): baseline F1 **0.000** — reported nothing on any
case; advanced F1 **0.500**, trap cleared by rejecting both candidates on
repository evidence.

### Decision

Three findings, two of which correct things previously written down.

**1. The advanced arm beat the baseline in every trial.** Its worst F1 (0.875)
exceeds the baseline's (0.857). The baseline was *perfectly stable* — identical
on all twelve cases in all three trials, σ = 0.000 on every metric — and all of
the advanced arm's variance comes from one case, `c12`, found in 1 trial of 3.
Reported headline figures are now means over three trials, not the single run
that produced 0.941. That single run was the best of the three, and saying so
matters.

**2. CORRECTION — "which change contributed most" was answered wrong.** The
changelog credited broadened candidate generation (A2). The ablation disproves
it: with falsification removed and broadening intact, F1 falls to 0.725, which
is **below the plain baseline**, and all four traps become false positives in
every trial. Broadening alone makes the system worse than doing nothing clever.

The corrected claim is that the question was malformed. Broadening and
falsification are one mechanism, not two rankable changes: each half alone
scores below the baseline, together they score 0.060 above it. The old answer
was plausible, consistent with every number available at the time, and wrong —
which is the argument for ablations over introspection.

**3. NEGATIVE RESULT — the follow-up loop is inert.** The self-correction step
added this sprint never executed, because the verifier never returns
`Insufficient` on this benchmark. Kept in the code, reported as inert. It costs
nothing when it does not fire and would plausibly matter where evidence is
thinner, but it contributed nothing here and is not claimed as an improvement.

### Rejected alternatives

- **Reporting the 0.941 single run as the headline.** Rejected: three trials
  exist and their mean is 0.917. Quoting the best sample would be exactly the
  overclaim the trials were run to prevent.
- **Nudging the verifier toward `Insufficient` so the follow-up loop would have
  work to do.** Rejected outright: that is tuning the measurement to justify
  the code.
- **Deleting the follow-up loop.** Rejected: it is correct, costs nothing when
  idle, and deleting it would also delete the finding. Reported rather than
  removed.
- **Quietly updating the "contributed most" claim.** Rejected: the correction
  is stated as a correction in both the changelog and the README, because being
  publicly wrong about your own system and then measuring it is the more useful
  story.
- **Running the `no-followup` ablation.** Not needed: with zero `Insufficient`
  verdicts the branch is unreachable, so that ablation is provably identical to
  the full system. Recorded rather than spent on.

### Consequence

One bug found and fixed along the way: ablation trajectories were named from
the agent rather than the arm, so a `no-falsification` run wrote
`<case>-advanced.json` while the evaluator looked for
`<case>-advanced-no-falsification.json` and scored nothing. The runs had
succeeded, so renaming recovered them without re-spending.

Still outstanding: the blind stopwatch harness is built and documented but no
session has been run, so human review time in the headline table remains a
labelled proxy.

---

## 2026-08-30 20:35 UTC — Sprint 3: the full ablation ladder, and three features worth nothing

### Context

Five items were requested: fix the c12 instability, complete the ablation
ladder, add within-case memory, add candidate deduplication, and check a second
model without provisioning another API.

### Evidence

**c12's root cause was not verification.** The advanced reviewer proposed *zero*
candidates on it in 2 of 3 trials, so nothing was ever investigated. Its Python
twin p03 failed identically, and so did the c10 trap — meaning "0 false
positives on traps" had partly been earned by never challenging them.

`advanced-review/v6` adds a general rule: where the change calls something whose
definition is not visible, state what it must do for the code to be right and
raise that as a candidate. No benchmark noun appears in it, and a test enforces
that. Recall went 0.750 to **1.000 with σ = 0.000**.

**The complete ladder**, 3 trials each, all at the final configuration:

```text
                                   P       R       F1      $/case
baseline                        1.000   0.750   0.857     0.0032
advanced prompt alone           0.607   0.958   0.742     0.0038
  + investigation               0.707   1.000   0.828     0.0112
  + falsification (full)        0.963   1.000   0.980     0.0159
```

Both intermediate configurations score **below the baseline**.

**Cross-model.** The baseline's exact prompts were exported from the recorded
trajectories and given byte-identical to Claude Sonnet 5, whose answers were
scored by the same deterministic evaluator. Result: **12/12 per-case agreement**
with Gemini, same F1 0.857, same `issue_type` on all six matches.

**Three features earned nothing.**

```text
follow-up on Insufficient   fired 0 times in 36 verifications
candidate deduplication     fired 0 times in 3 trials
within-case memory          used on all 12 cases; -3% calls, -1% cost
```

### Decision

Keep v6. Keep all three inert features, reported as inert.

Explicitly **not** attributing the precision improvement (0.926 → 0.963 between
the v6 and final sweeps) to deduplication. Deduplication never ran. With
σ = 0.064 on precision across three trials, 0.037 is inside noise, and the two
sweeps are indistinguishable on that metric. Writing "deduplication improved
precision" would have been a plausible, checkable, false claim — and it is
exactly the kind this project exists to argue against.

Two tooling bugs fixed, both of which had been hiding real information:

1. `variance` compared only true positives, so a run whose precision moved
   between trials reported "every case scored identically". That was false.
   Corrected, it immediately surfaced the duplicate-finding instability on c03
   and c08 that motivated deduplication in the first place.
2. Cost was computed as tokens × price with no check that tokens were ever
   recorded, so the externally produced Sonnet run reported `$0.00000` — a
   fabricated-looking measurement of "free" where the truth is "not measured".
   A run with no recorded tokens now reports cost as unavailable.

### Rejected alternatives

- **Crediting dedup or memory for the precision gain.** Rejected: dedup never
  executed, and memory's effect is inside noise.
- **Removing the three inert features.** Rejected: they are correct, tested,
  cost nothing idle, and deleting them would delete the finding. That three
  well-motivated components all earned nothing is more useful than the
  components.
- **Tuning the benchmark so dedup would fire.** Benchmark tampering with extra
  steps.
- **Running the advanced arm on Sonnet.** Rejected on validity grounds:
  reproducing a multi-turn tool loop outside the Rust orchestrator means
  re-implementing the thing under test, so any difference could come from the
  re-implementation. Scope is stated as "the problem generalises across models;
  the solution is measured on one".
- **Putting the Sonnet cost estimate in a results table.** Rejected: its tokens
  were never measured. The estimate lives in prose that labels every line
  MEASURED or ESTIMATE.

### Consequence

Final reported configuration: `advanced-review/v6`, `advanced-falsify/v2`,
`advanced-investigate/v2`, `fresh-verify/v5`, with deduplication and within-case
memory enabled but measured as inert. F1 0.980 ± 0.034 against a baseline of
0.857 ± 0.000, recall 1.000 with zero variance.

Outstanding: the blind stopwatch session has still not been run, so human review
time in the headline table remains a labelled proxy.

---

## 2026-08-31 00:20 UTC — Sprint 4a: the Python pilot, expanded and corrected

### Context

The pilot was three cases, all ports of Rust cases. It could confirm that the
architecture transfers; it could not discover a Python failure mode we had not
already seen in Rust. Trials 4 and 5 of the headline arms also completed.

### Decisions

1. **Three new pilot cases, chosen for defect classes Rust cannot express.**
   `p04` a mutable default argument (Rust has none), `p06` a generator consumed
   twice (Rust's iterator semantics cannot fail this silently), and `p05` a
   shared-module-state trap that is safe only because the accessor copies *and*
   every value is a scalar. Ground truth for all three verified by execution and
   re-verified before documenting; all six suites pass with their defects in
   place.

2. **`p06`'s ground-truth anchor corrected, and both figures reported.** It was
   anchored at the consumer (24-28) rather than the changed lines (15-19), the
   only 1 of 18 findings in the project outside its case's changed hunk. The
   advanced arm reported the defect at the change with a fully correct
   diagnosis and was charged a false positive *and* a false negative. Corrected
   to 15-19. Advanced pilot F1 **0.571 → 0.857**; baseline **0.667 either way**,
   because its location at 22-27 overlaps both anchors. Both numbers appear in
   `docs/pilot-python.md`. The as-authored figure was not recomputed by hand:
   the anchor was temporarily reverted and the evaluator re-run, then restored.

3. **The convention is enforced, not promised.** This defect was found because
   a *result* looked wrong, which is the direction that ends in a tuned
   benchmark. `bench::findings_outside_the_diff` parses each case's own diff and
   reports any expected finding outside the changed ranges; `vcr check` prints
   it as a warning; four unit tests cover single-line hunks, added files and
   deleted files. Both benchmarks are clean under it, which is how we know 17
   of 18 already followed the convention rather than believing it.

4. **`v6` did not transfer to Python, and is not being patched.** `p03` is
   `c12`'s twin, and the v6 rule took `c12` from 1-of-3 to 5-of-5. `p03` was
   missed again. The trajectory shows candidate generation never proposed the
   defect — the two candidates it did propose were investigated and correctly
   rejected — so verification was not the failure.

### Rejected alternatives

- **Editing the v6 prompt until `p03` is found.** Rejected. A prompt change
  aimed at one named case is precisely the overfitting the ablation ladder
  exists to detect, and the pilot is not a headline figure, so there is nothing
  to gain but a nicer sentence.
- **Reporting only the corrected pilot figure.** Rejected. The correction moves
  the advanced arm and leaves the baseline untouched, which is exactly the
  shape a self-serving benchmark edit would have. The reader gets both numbers
  and the reason the baseline does not move.
- **Silently re-running `p01`–`p03` and comparing to the old three-case
  numbers.** Rejected. They were re-executed, so it is a new sample, not a
  re-scoring; the changelog says so, and the disappeared `p01` duplicate false
  positive is called out rather than absorbed into a "precision improved" line.
- **Rewriting the sprint-2 changelog row that says "three Python cases".**
  Rejected: it was true when written. The expansion is a new entry.

### Consequence

Python pilot: 6 cases, baseline F1 0.667, advanced F1 0.857, precision 1.000 in
both arms, zero false positives, both traps cleared on repository evidence,
evidence accuracy 1.000 (51/51). Still a pilot, still not a headline figure.

Headline arms now at 5 trials: advanced F1 **0.988 ± 0.026**, recall
**1.000 ± 0.000**, precision 0.978; baseline identical on all 12 cases in all 5
trials, σ = 0.000 on every metric.

---

## 2026-08-31 00:45 UTC — Sprint 4b/4c: measuring the inert features, and a held-out benchmark

### Context

Sprint 3 reported three components as contributing nothing and kept all three,
which was honest about what had been measured and wrong about what it meant.
"Fired 0 times" measures the trigger, not the feature, and the trigger had only
ever been observed on runs where it did not occur. Separately, the frozen
benchmark's author bias needed cases from someone who cannot see the system.

### Decisions

1. **Deduplication reused the evaluator's matching tolerance, and that was a
   category error.** `vcr replay-dedup` replays the rule over every recorded
   run without calling a model. Across all 19 runs the trigger fires 6 times
   and **none of the 6 is a duplicate**: all are `c08`'s `order.name` at 26-28
   against `order.notes` at 30-32, two distinct ground-truth defects joined
   only because 28 + 3 >= 30. Each merge would have cost a true positive.
   Overlap is now strict. Two things had hidden it: `v6` stopped producing that
   pair, and the unit test written to prove the feature worked used that exact
   geometry and asserted the merge was correct.

2. **The follow-up loop's trigger was unreachable, so it was replaced rather
   than nudged.** `Insufficient` is 0 of 70; the evidence gate's other dead end
   is also 0. Both were measured before anything was built, which is what
   stopped a fourth inert feature being written. The new trigger — a case that
   finishes with nothing to report — fires.

3. **The second look ships off, on its own measurement.** It fired on exactly
   the four frozen-benchmark traps and declined on all four; on the pilot it
   fired on both traps, declined on one, and on the other proposed a
   true-but-immaterial claim that became a false positive. Six firings, five
   correct declines, no recall gained on either benchmark, ~14% more cost.
   Default 0, guarded by a test.

4. **A six-case held-out benchmark, authored by an agent denied the prompts,
   the pipeline, the docs and every result.** Baseline F1 0.750, advanced
   0.889. Every ground-truth claim re-derived by execution here rather than
   taken from the authoring agent's report.

5. **"Zero false positives on traps" is revised, not defended.** Both arms
   produced a false positive on `h04`. The advanced trajectory failed in this
   project's own documented main failure mode: the falsification question
   targeted the mechanism instead of the precondition, and the investigation
   ran `list_files`, saw `src/graph.rs` — which holds the constructor that
   rejects the graph shape the claim needs — and stopped without opening it,
   with four of eight tool calls unspent.

6. **All 40 headline true positives audited by hand**, raw claim text
   published. 7 of 8 defects described exactly in all five trials; `c12` hedged
   and still counted, with the hedge stated.

### Rejected alternatives

- **Widening deduplication to merge across `issue_type`.** Rejected, and this
  is the important one. The single false positive in the whole 5-trial headline
  run (`t3`, `c03`) is a cross-category overlap, so widening would have taken
  precision from 0.978 to 1.000. But those two claims are not duplicates: one
  is the real defect, the other is "two hash lookups per update" — a true
  statement that is not a defect. Merging them would have deleted a false
  positive **for the wrong reason** and would suppress genuine second defects
  elsewhere. Improving a headline number by accident is still tampering.
- **Patching the prompt so `h04` passes.** Rejected. A prompt change written
  against a case we just watched fail is the overfitting the ablation ladder
  exists to catch, and it would destroy the only property that makes a held-out
  set worth having. Reported instead.
- **Patching `v6` so the Python `p03` is found.** Rejected for the same reason.
- **Claiming the second look's single 12-case F1 of 1.000.** Rejected: inside
  the noise of 0.988 +- 0.026, one run against five, and this project has
  already been flattered by a single run once.
- **Deleting the inert features.** Still rejected — but the reason has changed.
  Deleting deduplication would have deleted the finding that it was wrong.
- **Publishing the held-out result only if it agreed with the headline.** Never
  on the table; it is written down because a held-out set used that way is not
  one.

### Consequence

Headline, 5 trials: advanced F1 **0.988 +- 0.026**, precision 0.978, recall
**1.000 +- 0.000**; baseline 0.857 +- 0.000, identical on all 12 cases in all 5
trials. Held-out, 1 run: baseline 0.750, advanced **0.889**. Python pilot, 1
run: baseline 0.667, advanced 0.857.

Shipped configuration: `advanced-review/v6`, `advanced-falsify/v2`,
`advanced-investigate/v2`, `fresh-verify/v5`, strict-overlap deduplication,
within-case memory, second look **off**.

Outstanding and unfixed, stated rather than resolved: the blind stopwatch
session was not run, so human review time remains a labelled proxy; the `h04`
trap failure is not patched; `p03` has been missed three times; within-case
memory has 15/156 tool calls of measured headroom that was not built.

---

## 2026-08-31 02:45 UTC — Sprint 4d: making the checks reproducible

### Context

Three claims in the documentation rested on analysis I had done in an ad-hoc
script that was not in the repository: the deduplication replay, the by-hand
match audit, and the neutrality of the case descriptions. A claim a reader
cannot re-derive is an assertion.

### Decisions

1. **`vcr audit-matches`** pairs every scored true positive with the ground
   truth it was credited for and prints both. It computes **no verdict** — the
   evaluator's location-plus-category match is a proxy for "found the defect",
   no deterministic matcher can check the reason, and a model judge is
   forbidden here. The command puts both texts in front of a person.

2. **`vcr check` now flags case descriptions that reveal their category.** The
   word list is narrow — verdict words only — and was calibrated against the 24
   existing cases before being written down, which produced two findings that
   are now encoded as tests: matching must be on whole words (a substring
   version flagged "fix" inside "fixed protocol chunk size"), and "mistake" is
   deliberately excluded because it appears in both a trap and a real-issue
   description and therefore separates nothing. A further test runs the check
   over all 24 shipped cases, so an edit that turns a description into a hint
   fails the build.

3. **Held-out benchmark extended to 3 trials, with alternating arm order.**
   Baseline 0.750 +- 0.000, advanced 0.926 +- 0.064, recall 1.000 +- 0.000.
   Alternating the arm order is a partial answer to "the arms were not run
   interleaved".

4. **The two held-out trajectories are rendered and discussed.** The `h04`
   failure was previously prose; it is now inspectable next to the `h06`
   success, which isolates the variable: same prompts, same budgets, and the
   falsification question it chose decided the outcome.

### A finding that needed two benchmarks

Auditing the held-out matches showed `h06` hedging in exactly the way `c12`
does — naming the divergence and the boundary but leaving the direction open.
Those are the only two boundary/off-by-one defects in either benchmark and they
were written by different authors. One hedge is a quirk of a case; two, on
independently authored cases of the same defect class, is a property of the
system. It will say where to look and that something is off by one, and it will
not commit to which side.

### Rejected alternatives

- **Adding "mistake" and "fix" to the tell list because they sound like
  tells.** Rejected on the data: both flagged existing neutral descriptions,
  and "mistake" appears in both categories. A lint calibrated on intuition
  rather than on the corpus would have forced three cases to be reworded for
  no gain.
- **Having `audit-matches` output a verdict.** Rejected. Deciding whether a
  claim describes a defect is exactly the judgement this project refuses to
  delegate to a model, and a hand-rolled heuristic would be the same mistake
  wearing a deterministic hat.
- **Re-rendering the five frozen-benchmark trajectories.** Not needed: all five
  already trace to `results-final/t1`, verified by matching trajectory ids.

### Consequence

237 tests. Every documented analysis is now reproducible from a committed
command or test: `vcr replay-dedup`, `vcr audit-matches`, `vcr check`, and
`scripts/extract_narration.py --check`.

---

## 2026-08-31 04:00 UTC — Sprint 4f: from harness to tool, and dogfooding it

### Context

Every figure in this project came from benchmarks we wrote. That measures a
change in behaviour; it does not show the thing is usable. There was no way to
point the reviewer at a real repository and a real diff.

### Decisions

1. **`vcr review --repo <dir> --diff <file>`.** A benchmark case is a directory
   holding `case.json`, `diff.patch` and `repository/`; a real review is the
   same `Case` value assembled in memory. Same prompts, same sandbox, same
   evidence gate, same roles. Deliberately not a special path: if reviewing a
   real diff behaved better than the benchmark harness, the benchmark would be
   measuring the wrong thing.

2. **The report shows what was cleared, not only what was found.** A reviewer
   who disagrees with a rejection needs the claim and the repository evidence
   that closed it, or the system is asking to be trusted rather than checked. A
   test asserts the model's internal reasoning never reaches the reader: claims
   and evidence are checkable, narration is not.

3. **Dogfooded on this repository, and both runs reported whether or not they
   flattered us.** Reviewing its own newest code proposed nothing. Reviewing
   commit `70315f1`, which introduced the deduplication defect later found by
   replay, it **did not find the bug** — it proposed one candidate, gathered
   evidence from two files outside the diff, and correctly cleared it.

### The finding that came out of it

The dogfood run caught a real bug in **our own output**. With zero candidates
proposed, the renderer printed "every candidate below was investigated against
the repository and ruled out" — a stronger claim about the system's diligence
than the run supported, which is precisely the failure mode this project exists
to prevent, in our own report. The two silences are now distinguished and two
tests pin them.

And the miss sharpened the stated failure mode. The deduplication defect is not
wrong in the code — the comparison is sound, the constant is real, the tests
pass. It is wrong in what the constant *means*. So:

> The system finds defects that are wrong in the code. It does not find defects
> that are wrong in the intent behind a name.

That is a limit of falsification itself, not of this implementation:
falsification needs a claim repository evidence can settle, and "this constant
is used for a purpose its definition does not support" is a claim about design
intent. The repository contains text, not intent. What actually caught that bug
was `vcr replay-dedup` asking what the rule had done across every recorded run.

### Rejected alternatives

- **Tuning a prompt so the dogfood case would pass.** Rejected. A rule written
  to catch "a constant borrowed from another subsystem", authored after
  watching this exact miss, is overfitting to one observation and would fire on
  every shared constant in every codebase. The miss stands, like `h04`.
- **Choosing a dogfood diff more likely to succeed.** Rejected before running:
  the commit was picked because it contains the worst defect this project has
  made, which is the strongest available test and the one whose failure is most
  worth publishing.
- **Omitting the run that found nothing.** Rejected. It is the run that caught
  the false claim in our own renderer.

### Consequence

246 tests. `vcr review` ships. Two dogfood runs are recorded in full under
`results-selfreview/` and `results-dogfood/`, with the write-up in
`docs/dogfood.md`, including the defect it failed to find.

---

## 2026-08-31 01:30 UTC — Correction: the deduplication replay counts

### Context

The dedup replay figure was recorded as "6 firings across 19 recorded runs".
Adding the held-out trials, the second-look runs and the dogfood runs took the
repository to 26 recorded runs, and the documented number went stale.

### Evidence

`vcr replay-dedup --root .` now reports **7 firings, 0 of them strict
overlaps**, across 26 runs. The seventh is new and did not come from `c08`:

```text
p02-primary-node-trap  [ApiContract]  x1 run(s)
  src/status.py:14-14  cluster.unhealthy() returns an integer count ...
  src/status.py:9-13   cluster.nodes() returns an iterator or set ...
    seen in results-pilot-secondlook
```

Two different methods, two different claims, joined only by the ±3 tolerance.

### Decision

Counts corrected in README.md, docs/improvement-changelog.md,
docs/architecture.md and the doc comment on `deduplicate_candidates`.

The finding is **stronger**, not weaker: the wrong merge now has an instance in
a second language, on a case written months apart from `c08` and by a different
process, arising from a run nobody designed to test deduplication. Seven
firings, zero duplicates, two languages.

### A note on how this was caught

By auditing every countable claim in the documentation against the artifacts,
after finding an invented figure ("six months of recorded behaviour") in
docs/dogfood.md. This project's whole argument is that assertions should be
checked against evidence; unchecked numbers in its own prose are the cheapest
possible way to undermine it. Verified in the same pass: 285 evidence citations
across five trials, 70 findings with Insufficient at 0, 24 shipped benchmark
cases, 86 adjudicated candidates.

---

## 2026-08-31 05:10 UTC — Headline to n=15, held-out to n=6

### Context

Trials are cheap and case authoring is not, so the fastest way to firm up the
evidence was more trials of what already exists. Two batches were run: headline
arms t6-t15 on the frozen benchmark, and held-out arms t4-t6.

### Evidence

Shipped configuration throughout, second look off, temperature 0. Every trial
verified complete before use.

```text
frozen benchmark, n=15
  advanced   F1 0.9922 +- 0.0207   P 0.9852   R 1.0000 +- 0.0000
  baseline   F1 0.8571 +- 0.0000   identical on all 12 cases in all 15 trials

held-out benchmark, n=6
  advanced   F1 0.9444 +- 0.0609   P 0.9000   R 1.0000 +- 0.0000
  baseline   F1 0.7500 +- 0.0000   identical on all 6 cases in all 6 trials
```

Recall is 1.000 with zero variance on both benchmarks across 21 trials. The
baseline does not move at all on either. Every point of the advanced arm's
spread is precision, and every false positive is on a trap.

### The finding

**A stated conclusion did not survive the larger sample.** At n=5 the README
and this changelog both said the advanced arm's remaining spread "comes from a
single case, `c03`". At n=15 a second case, `c08`, produced a false positive in
one trial. The claim was an artefact of the sample that produced it — which is
the same mistake this document already records making once, when a single run
of 0.941 was quoted before three trials averaged 0.917.

The corrected statement is narrower and better: 13 of 15 trials score a perfect
1.000, and the two that do not each contribute one false positive, on two
different cases.

**The `c12` hedge is now confirmed at n=15.** All fifteen claims for that case
state the cause conditionally and not one asserts flatly that `Store::len`
returns capacity. Ten extra trials produced no crisp phrasing, so this is a
property of how the system reports boundary-condition defects rather than a
small-sample artefact. It matches `h06` on the held-out set, the only other
off-by-one defect in either benchmark.

### Rejected alternatives

- **Claiming the hand audit covers all 120 matches.** Rejected: 40 were read in
  full, plus all 15 `c12` claims. The docs now say exactly that, and
  `vcr audit-matches` prints all 120 for anyone who wants to check the rest.
- **Leaving the ablation rows at 3 trials unmarked.** Rejected: the ladder table
  now carries a per-row trial count, since the headline rows are 15 and the two
  ablation rows are 3, and presenting them as comparable would be misleading.

### Consequence

Headline: advanced **F1 0.992 +- 0.021** against a baseline of **0.857 +-
0.000**, recall **1.000 +- 0.000**. Held-out: **0.944 +- 0.061** against
**0.750 +- 0.000**. Evidence accuracy 1.000 across 909 citations in 15 trials.

Not updated: docs/video-script.md still quotes the n=5 figures. The script is
rewritten when the video is made and is deliberately left alone until then.

---

## 2026-08-31 05:40 UTC — The follow-up loop is idle, not useless

### Context

Three components had been reported as contributing nothing. Two of those
verdicts were later corrected: deduplication was actively wrong, and the second
look fires but does not help. The third, the follow-up loop on an
`Insufficient` verdict, had never been measurable at all, because across 70
findings the verifier returned `Insufficient` zero times.

"Fired 0 times" cannot distinguish **useless** from **never needed**, and those
have different consequences for anyone deploying this.

### Decision

Separate them by changing the **operating point**, not the feature. The
verifier returns `Insufficient` when evidence is too thin to settle a claim, so
cut the investigation budget from 8 tool calls per candidate to 1 and run the
loop against its own ablation on the same twelve cases, 3 trials each.

```text
tool budget = 1 per candidate, 3 trials, 36 candidates per arm

  follow-up DISABLED   Insufficient 9    recall 0.750 +- 0.217   F1 0.844 +- 0.154
  follow-up ENABLED    Insufficient 3    recall 0.917 +- 0.072   F1 0.956 +- 0.039
                       (loop fired 6 times)
```

Precision is 1.000 in both arms, so the recall is not bought with false
positives, and the variance falls fourfold.

### Consequence

The earlier verdict is corrected. The loop is not worthless; the shipped tool
budget is generous enough that it never gets a chance to help. A deployment
that tightened the budget for cost would want it on. This is a fact about the
operating point rather than about the code, and it is exactly what "fired 0
times" concealed.

### Rejected alternatives

- **Nudging the verifier to return `Insufficient` more often.** Rejected here
  for the same reason it was rejected in Sprint 3: that is tuning the
  measurement to justify the code. Nothing about the verifier was touched; only
  how much the investigator was allowed to look, which is a knob a real
  deployment might turn.
- **Reporting the starved F1 alongside the headline.** Rejected. It is measured
  at a different operating point and would be read as a comparable number.
  Every appearance of it is labelled "tool budget = 1".
- **Claiming this as an improvement to the shipped system.** Rejected: at the
  shipped budget the loop still contributes nothing, and the headline figures
  are unchanged.
