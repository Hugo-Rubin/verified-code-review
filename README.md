# Verified Code Reviewer

An LLM code reviewer for Rust changes that investigates its own candidate
findings against the repository, then tries to **disprove** them in a fresh
reasoning context before deciding whether a human should ever see them.

On a frozen 12-case benchmark it recovers the defects a direct reviewer misses
— the ones whose deciding evidence lives in files the change does not touch —
while staying clean on all four false-positive traps in every trial. On a
held-out benchmark it had never seen, it recovers the same class of defect and
is **not** clean on the traps; both numbers are below.

Mean of **15 trials per arm**, same model (`gemini-3.7-flash`), temperature 0.

| Metric | Simple baseline | Agent solution | Change |
|---|---:|---:|---:|
| **Primary outcome — finding F1** | 0.857 | **0.992** | **+0.135** |
| Precision | 1.000 | 0.985 | −0.015 |
| Recall | 0.750 | **1.000** | **+0.250** |
| Human time per task (proxy) ¹ | 0.50 findings/case | 0.68 findings/case | +0.18 |
| **Cost per task** | **$0.0032** | **$0.0157** | ×4.9 |
| Runtime per task | 8.3 s | 39.3 s | +31.0 s |
| Evidence accuracy ² | n/a — gathers none | **1.000** | — |

The advanced arm found **every real defect in every trial** (recall
1.000 ± 0.000 over 15 trials) and beat the baseline in all fifteen — its worst
F1 (0.941) still exceeds the baseline's (0.857, identical in all fifteen). Two
false positives occurred across the entire 15-trial run, on two different cases
(`c03` and `c08`), one trial each.

**Replicated on cases this project's author never saw.** A six-case held-out
benchmark, written by a separate agent denied access to the prompts, the
pipeline, the docs and every result, reproduces the direction over 6 trials:
baseline F1 **0.750 ± 0.000**, advanced **0.944 ± 0.061**, recall again
**1.000 ± 0.000**, separating on the one case whose evidence lives outside the
diff. It also produced false positives on both its traps — which the frozen
benchmark's traps never do — so trap performance on *unseen* traps is weaker
and less stable than the headline suggests. See
[`docs/holdout.md`](docs/holdout.md).

**And two negative results that bound the claim.** Two *further* agent-authored
sets (10 cases, same blocklist) produced defects legible in the diff itself, and
there the **baseline scores a perfect 1.000** while the advanced arm ties on one
and loses on the other. So a fourth set was commissioned, specified to contain
only defects whose changed line reads as *correct*. On it the baseline collapses
to **0.400** — the shape really is what makes a case hard — and the advantage
returns, **0.517 vs 0.400**.

But look at that number. Recall there is **0.417**, against **1.000 ± 0.000** on
the frozen benchmark. **The perfect recall in the table above is a property of
those twelve cases, not of the method.** On hard cases written by someone else,
this system finds fewer than half the defects. That qualification exists only
because we had somebody else write the cases, and it is the most important
sentence in this README. [`docs/benchmarks.md`](docs/benchmarks.md).

¹ Manual-triage proxy — findings a human must read and judge. **Not** a direct
measurement of human review time. A blind stopwatch harness for the real
measurement ships as `vcr triage`; see [Cost and human time](#cost-and-human-time).
² Fraction of cited excerpts that really appear at the lines they cite, checked
deterministically against the repository, 41–83 citations per run (909 across the fifteen trials, all correct).

### The whole ladder, measured

Every stage switched off in turn, same benchmark. Headline arms are means of
15 trials; the two ablation rows are means of 3.

| Configuration | Trials | F1 | Precision | Recall | Cost/case |
|---|---:|---:|---:|---:|---:|
| Simple baseline | 15 | 0.857 | 1.000 | 0.750 | $0.0032 |
| Advanced prompt alone (no investigation, no falsification) | 3 | 0.742 | 0.607 | 0.958 | $0.0038 |
| **+ investigation**, no falsification | 3 | 0.828 | 0.707 | 1.000 | $0.0112 |
| **+ falsification** — the full system | 15 | **0.992** | 0.985 | 1.000 | $0.0157 |

Read the middle two rows carefully, because they are the result.

**Neither half beats the baseline on its own.** The advanced prompt alone scores
0.742 — *worse than doing nothing clever*. Adding repository investigation
lifts it to 0.828, which is still **below** the baseline's 0.857. Only when
falsification is added does the system reach 0.992.

Investigation supplies the recall (0.750 → 1.000). Falsification is what makes
that recall affordable, taking precision from 0.707 to 0.985. Remove either and
you have something worse than the simple prompt you started with.

Full numbers: [`results-trials/`](results-trials/) and [`results/`](results-final/).
Full history, including four changes that made things worse and one feature
that did nothing at all:
[`docs/improvement-changelog.md`](docs/improvement-changelog.md).

### Using it on your own code

It is a tool, not only a benchmark harness. Point it at a working tree and a
diff and it runs the same pipeline, prompts, sandbox and evidence gate that
produced every number above:

```bash
cargo run --quiet --bin vcr -- review --repo . --diff my-change.patch --out results-review
```

No ground truth, no score — a report for a person, listing what was found, the
falsification question each claim was checked against, the repository lines
actually read, and what was investigated and **cleared**, because a reviewer who
disagrees with a rejection needs to see the evidence that closed it.

We ran it on this repository, including on the commit that introduced this
project's own worst bug. It did not find that bug, and it did find a false claim
in our own report renderer. Both runs are written up in
[`docs/dogfood.md`](docs/dogfood.md).

<details>
<summary><b>What a review actually looks like</b> — real output, reproducible with the command below</summary>

```bash
cargo run --quiet --bin vcr -- review \
  --repo benchmark/cases/c12-slot-guard-capacity/repository \
  --diff benchmark/cases/c12-slot-guard-capacity/diff.patch \
  --title "Add read endpoints over the record store" \
  --description "A new \`api\` module exposes \`fetch\` for a single slot and \`fetch_many\` for several. Both are bounds-checked and return \`None\` or skip the entry rather than panicking on an out-of-range index." \
  --out results-review-demo
```

```text
1 finding(s) for review · 0 investigated and cleared · 0 uncertain
6 model call(s), 2 tool call(s), 35057 ms
cost: $0.01586

──────────────────────────────────────────────────────────────
src/api.rs:8-13 · Medium · Correctness
  Callers accessing a valid index in a slot-based store will trigger a panic in
  `record_at` if slots can be vacant or if `store.len()` measures active record
  count rather than slot capacity.

  Checked by asking: Does `Store` store records densely such that every index
  from `0` to `store.len() - 1` is guaranteed to be occupied and valid for
  `record_at`?
  Independent verdict: Supports
    `Store::len` returns `self.capacity` rather than the count of populated
    records in `self.records`. As a result, `fetch` only bounds-checks
    `index >= store.len()` before calling `store.record_at(index)`, which
    indexes directly into `&self.records[index]` and panics whenever `index` is
    within capacity but beyond `self.records.len()`.
  Evidence read: src/api.rs:1-28, src/store.rs:13, src/store.rs:1-80

──────────────────────────────────────────────────────────────
This system does not merge, reject, approve or modify anything. A
human decides. Findings are evidence-backed claims, not verdicts.
```

Note what the reader is given: the claim, **the question it was checked
against**, an independent verdict reached in a fresh context, and the exact
repository lines that settle it — including `src/store.rs`, which the diff never
touches. The change looks correct in isolation; `Store::len` returning capacity
is what makes it wrong, and that fact is nowhere in the diff.

The demo uses a benchmark case as its input so the command above reproduces
byte-for-byte from committed files. Output saved in
[`results-review-demo/`](results-review-demo/).

</details>

---

## Where everything is

| Deliverable | Where |
|---|---|
| **Solution code** | [`src/`](src/) — `vcr` CLI. Every agent instruction is in [`src/prompts.rs`](src/prompts.rs), versioned per role |
| **Improvement changelog** | [`docs/improvement-changelog.md`](docs/improvement-changelog.md) — every iteration, including the five that made things worse and the one that was actively harmful |
| **Reproduction guide** | [`docs/reproduction.md`](docs/reproduction.md) — clean-environment setup, exact commands for solution, baseline and evaluation, expected output, runtime and cost |
| **Agent trajectories** | [`docs/trajectories.md`](docs/trajectories.md) — guided reading of every role; raw records in [`results-final/t1/trajectories/`](results-final/t1/trajectories/) |
| **Main failure mode & hot take** | [below](#main-failure-mode) |
| Decision log | [`DECISIONS.md`](DECISIONS.md) — append-only, including every rejected alternative |
| Architecture | [`docs/architecture.md`](docs/architecture.md) |
| Benchmarks | [`docs/benchmarks.md`](docs/benchmarks.md) — five case sets, what each is for, and where the advantage stops |
| Held-out benchmark | [`docs/holdout.md`](docs/holdout.md) — cases written without sight of this system |
| Dogfooding | [`docs/dogfood.md`](docs/dogfood.md) — including the defect of ours it failed to find |

Run it on your own change in one command: [Using it on your own code](#using-it-on-your-own-code).

## Problem

A reviewer approving a pull request has to answer a question the diff cannot
answer on its own: **is this actually broken?**

Most of what makes a change dangerous is not in the change. Whether an
`unwrap()` can fire depends on callers in files the diff never touches. Whether
an unchecked index is safe depends on whether a constructor rejects the empty
case. Whether a bounds check is correct depends on what the function it calls
actually returns. The diff shows you the suspicious line; the repository holds
the verdict.

LLM reviewers are good at spotting the suspicious line and bad at reaching the
verdict, because by default they are reasoning about a diff rather than
investigating a codebase. They resolve that gap by guessing — and the guess is
plausible either way, which is what makes it expensive.

## Intended user

A developer reviewing a Rust pull request in a codebase they did not entirely
write — the common case on any team past its first few months.

They are not looking for more findings. They are looking for findings they can
trust enough to act on without re-deriving the reasoning themselves.

## The bottleneck, and why solving it is valuable

Two failure modes cost this reviewer time, and they pull in opposite
directions:

- **A false positive** costs a full investigation to dismiss. The reviewer
  reads the finding, opens the file, traces the callers, and concludes it was
  fine. That is the whole review's worth of effort spent on nothing.
- **A false negative** costs whatever the defect costs in production.

A tool that reduces one by trading against the other has not helped. Turn the
sensitivity up and the reviewer starts skimming, then ignoring it. Turn it down
and it stops catching the things that motivated installing it.

What is worth building is a reviewer that raises its recall **without** paying
for it in noise — and can show its work, so the human's job is checking a cited
argument rather than reconstructing one.

## Why existing AI review falls short

The default setup gives the model a diff and asks what is wrong with it. Two
consequences follow:

1. **It cannot check anything.** A claim about callers is unverifiable from a
   diff, so the model either asserts it or stays quiet. Our baseline stays
   quiet: it missed both context-dependent defects while reporting nothing
   false. That is a defensible policy and it still means real bugs ship.
2. **"I verified this" is not verification.** Asking a model to double-check
   its own finding, in the same context that produced it, mostly produces
   agreement. Nothing forced it to go and look.

## Solution

Five stages. The model reasons; **Rust decides**.

```
                    diff + changed files
                             │
                             ▼
                  ┌──────────────────────┐
                  │ 1. Candidate finding │  propose broadly — a wrong
                  └──────────┬───────────┘  candidate is cheap here
                             ▼
                  ┌──────────────────────┐
                  │ 2. Falsification     │  "what evidence would show
                  │    question          │   this is WRONG?"  fixed on
                  └──────────┬───────────┘  the record before any lookup
                             ▼
                  ┌──────────────────────┐
                  │ 3. Investigation     │  search · read · list_files
                  └──────────┬───────────┘  bounded, sandboxed
                             ▼
                    ┌────────────────┐
                    │ Evidence       │  file · lines · verbatim excerpt
                    │ package        │  constructed by Rust, never by
                    └───────┬────────┘  the model
                            ▼
                  ┌──────────────────────┐
                  │ 4. FRESH-CONTEXT     │  a separate stateless request
                  │    verification      │  that never sees the reviewer's
                  └──────────┬───────────┘  reasoning
                             ▼
                  ┌──────────────────────┐
                  │ 5. Decision (Rust)   │
                  └──────────┬───────────┘
                 ┌───────────┼───────────┐
                 ▼           ▼           ▼
            VERIFIED     REJECTED    UNCERTAIN
          shown to a   "investigated  withheld,
            human      and cleared"   kept on record
                 │
                 ▼
        ══════════════════════
         HUMAN DECIDES
        ══════════════════════
```

### The four roles

The advanced system is not one agent with a long prompt. It is four distinct
roles, each a **separate stateless request** with its own instructions and its
own versioned prompt, orchestrated by Rust:

| Role | Prompt | Sees | Job |
|---|---|---|---|
| **Reviewer** | `advanced-review/v6` | diff + changed files | Propose candidates, erring toward proposing |
| **Falsifier** | `advanced-falsify/v2` | the claim | Write the one question whose answer would show the claim is **wrong** |
| **Investigator** | `advanced-investigate/v2` | claim, question, results so far | Choose the next `search` / `read` / `list_files` call, or stop |
| **Fresh verifier** | `fresh-verify/v5` | claim + evidence, **nothing else** | Decide whether the evidence establishes a real defect |

The split is not decoration. Each boundary exists because merging it back
would break something specific:

- **Falsifier separate from Reviewer** — asking the same request to state a
  claim and then to say what would refute it invites a question shaped to fit
  the claim. As its own call, the question is fixed on the record before any
  evidence exists.
- **Investigator separate from Falsifier** — the investigator is steered by the
  question rather than by the claim, which is what makes it look for the
  disproof instead of for confirmation.
- **Verifier separate from everything** — see below. This is the one that
  earns its keep.

Rust sits between all four: it executes the tools, constructs every piece of
evidence, and assigns the final status. No role can promote its own finding.

The baseline is a fifth, separate system — one role, one call — and it is what
the advanced arm is measured against.

A sixth prompt, `advanced-second-look/v1`, exists and **ships disabled**. It is
the one path that feeds falsification output back into generation, and it was
measured as not helping; see the anti-take below.

### Three properties are load-bearing

**The falsification question is fixed before any evidence is gathered.** A
question written after the verdict would only rationalise it.

**The verifier runs in a genuinely fresh context.** It carries the claim and
the collected excerpts and *nothing else* — not the reviewer's reasoning, not
the investigator's running commentary, not the fact that a previous role
believed the claim.

This is a property of the architecture rather than of the prompt. `LlmRequest`
is stateless by construction: every call carries its whole context and there is
no conversation object, so there is no channel through which the anchor could
leak even by accident. The prompt is written as though the reader has never
seen a review, and
`prompts::tests::verifier_prompt_never_mentions_the_reviewer` fails the build
if words like "reviewer", "candidate", or "previous" appear in it.

That isolation is what makes this orchestration rather than a pipeline of
prompts. The verifier is the only role that can stop a finding, and it is the
only one that cannot see who is asking.

**Rust assigns the final status, not the model.** The verifier returns a
judgement; the orchestrator decides what it is worth. `Supports` without
repository-grounded evidence becomes `Uncertain`, because "the model said so"
is the standard this project exists to reject.

## Scope: a Rust demonstration of a language-independent design

The deliverable here is **a Rust reviewer reviewing Rust changes**, and every
number in this README was measured on Rust. That narrowness is deliberate — one
toolchain, one build system, one test runner, no sandbox variation — and it is
what made a 12-case benchmark with execution-verified ground truth affordable
inside the deadline.

What is being demonstrated, though, is a **verification architecture**, and
most of it never touches the language:

```
                    Verified Code Reviewer
                             │
                    language-independent core
                             │
   ┌───────────┬─────────────┼─────────────┬───────────┐
   ▼           ▼             ▼             ▼           ▼
  Rust      Python        TS / JS         Go         Java
   │           │             │             │           │
 tools       tools         tools         tools       tools
```

The pipeline — candidate finding → repository search and read → evidence →
falsification question → fresh-context verification → verified / rejected /
uncertain — operates on text and file positions. So do the pieces that enforce
it:

| Component | Language-specific? |
|---|---|
| Sandbox (`repo.rs`) | No — path containment |
| Tools (`search`, `read`, `list_files`) | No — literal substring and line ranges |
| Evidence model | No — file, line range, verbatim excerpt |
| Falsification and fresh-context verification | No |
| Decision gate (`decide()`) | No |
| `IssueType` taxonomy | No — the nine categories are general |
| Deterministic evaluator | No — type plus location overlap |
| Trajectory and cost accounting | No |
| **Review prompts** | **Yes** — currently say "Rust reviewer" |
| **Benchmark** | **Yes** — twelve Rust crates |

The genuinely language-bound work sits at the edges, and is the natural next
extension:

```
cargo test   → Rust        pytest  → Python      npm test → JS / TS
go test      → Go          mvn test → Java
```

plus deeper static analysis — AST and call-graph work — which is where the
[blind spots](#limitations) of literal search would actually be addressed.

**This is a claim about the design, not a measured result.** Nothing in this
project has been run against a non-Rust codebase, and the honest expectation is
that each new language costs a prompt variant and a benchmark before any claim
could be made about it. What the Rust results support is that the verification
loop itself does useful work; what they do not yet support is that it does so
in Python.

## Baseline

A reasonable direct-review setup, deliberately made strong. Same model, same
temperature, same JSON output contract, same view of the diff and the full
contents of every changed file, and an explicit instruction to avoid
speculation. The only thing withheld is repository tools.

The two arms differ in exactly one instruction: the baseline is told an empty
answer is correct when the code is sound, while the advanced candidate stage is
told to err toward proposing. That is not a resource advantage, it is the
design under test — the baseline's output *is* its report, so confidence is the
right bar; the advanced reviewer's output is a worklist for a stage that can
settle uncertainty against the repository. Both are scored on the same thing:
what a human is finally shown.

While reading the first baseline run we found our own prompt had named
`unwrap()` on a non-`None` value and an in-bounds index as examples of correct
code — near-verbatim descriptions of two benchmark cases. It was removed. The
scores were identical, and a regression test now fails the build if a review
prompt mentions a benchmark noun.

## Verification and falsification

A finding reaches a human only if the fresh verifier says the evidence
establishes a real defect **and** the investigation actually retrieved
something. The verifier applies three rules learned the hard way, each from a
measured regression:

1. **Reachability is part of the claim.** "X panics if Y" is true of the
   mechanism whether or not Y can happen, so it cannot be falsified. Evidence
   that the triggering state is prevented *contradicts* the claim.
2. **A true statement is not automatically a defect.** *"SizeReport does not
   derive Clone"* is accurate and is not a bug. A verifier that checks whether
   claims are accurate confirms these forever.
3. **Weigh comments by whether the repository could check them.** A comment
   asserting something the repo can settle — which callers exist, what they
   pass — is a claim: go read the call sites. A comment stating a fact from
   outside the repo — a database column width, production input sizes — is the
   best evidence available and is reasoned from.

## Benchmark

Twelve cases, frozen before the reported sweep. Each is a standalone Rust crate
that compiles with a passing test suite — and in every defective case **the
suite passes despite the defect**, so the tests give a reviewer no signal.

| | Count | Purpose |
|---|---:|---|
| Real defects | 6 | Recall on genuine bugs |
| Traps | 4 | Every reported finding is a false positive |
| Challenging | 2 | Deciding evidence lives outside the changed files |

Three cases are deliberately paired to isolate what investigation buys:

- **c03 vs c09** — the *same edit*: a silent fallback replaced by a panicking
  `expect`, with a doc comment asserting callers check first. In c03 the
  assertion is false and the panic is reachable. In c09 it is true. The two are
  indistinguishable from the diff and the changed file alone.
- **c11** — a path-traversal guard is deleted. Safe only because every caller
  passes a closed enum's literal.
- **c12** — the reverse of a trap: a *safe-looking* guard that is wrong,
  because `Store::len` returns capacity rather than fill. It exists so a system
  that improves precision by rejecting everything cannot score well.

Ground truth for all twelve was **verified by executing it**, not by
inspection: the c04 lost update was observed (4,267 of 16,000 increments
survived), c06 measured at 4.0× per doubling, c12 panicked at `store.rs:54`,
and the traps were probed and held.

## Evaluation method

Deterministic. **No LLM scores anything.**

A prediction matches an expected finding when its `issue_type` is acceptable
for that defect and its location overlaps the expected range within ±3 lines.
Matching is one-to-one and closest-first: two predictions on one defect yield
one true positive and one false positive, because telling a reviewer the same
thing twice still costs a second triage.

Only `Verified` findings are scored. `Rejected` and `Uncertain` are withheld
and counted separately — and withholding is not free: suppressing a real defect
still costs a false negative, so "reject everything" cannot game the metric.

Ground truth is unreachable by construction rather than by convention. The
agent-visible `Case` type has no field pointing at `GroundTruth`, they are
loaded by separate functions, and the sandbox refuses any path named
`ground_truth.json` along with all parent traversal.

`ExpectedFinding` carries an `also_accept` list of alternative categories,
because a counter that is never decremented is defensibly
`ResourceManagement`, `StateManagement`, or `Correctness`. Without it the
benchmark would partly measure agreement with our taxonomy. The concession is
on the category axis only — location must still overlap.

### The known weakness of this matcher, and what we found when we checked

Location plus category is a **proxy for "found the defect"**, and it can be
fooled: a claim that lands on the right lines under an accepted category scores
a true positive even if it describes something else entirely. Nothing in a
deterministic matcher can tell those apart, and using a model to judge would
reintroduce the thing this project refuses.

So it was checked by hand. The headline run now contains **120 matched
findings** (8 defects × 15 trials). Forty of them — every match in trials 1–5 —
were read in full against their ground-truth descriptions, and all fifteen
claims for `c12` were read as well. The full text of all 120 is printed by
`vcr audit-matches`, so the reading can be disagreed with; the transcript of
the hand-read subset is in
[`docs/matching-audit.md`](docs/matching-audit.md).

**Result: 7 of 8 defects are described exactly.** The exception is `c12`, where
the claims name the right failure (an index below `len()` reaches a vacant slot
and panics) but state the cause as a conditional about the store's semantics —
*"if slots can be vacant"*, *"if the store is sparse"* — rather than flatly
identifying that `Store::len` returns capacity. We count those as true
positives, since a human handed that claim goes and reads `Store::len` and
finds the bug, and flag the hedge rather than let a reader assume every match
is crisp.

Extending to 15 trials strengthened that observation rather than diluting it:
**the hedge appears in all 15 of 15 `c12` claims**, never once resolving into a
flat assertion. It is a stable property of how this system reports
boundary-condition defects, not a phrasing accident in a small sample.

**The matcher did produce one spurious match, on the Python pilot.** In one run
`p03` scored a true positive for *"non-integer float indices bypass the bounds
check"* — a different claim from the real defect, landing on the same three
lines under a category in `also_accept`. It is reported in
[`docs/pilot-python.md`](docs/pilot-python.md) rather than quietly kept, and it
is the concrete reason the audit above exists.

## Results

All arms, `gemini-3.7-flash` via Vertex AI, temperature 0, frozen benchmark,
Headline arms **15 trials**, ablations **3 trials**. Mean ± sample standard
deviation.

| Metric | Baseline | Prompt alone | + investigation | **Advanced (full)** |
|---|---:|---:|---:|---:|
| Precision | 1.000 ± 0.000 | 0.607 ± 0.052 | 0.707 ± 0.035 | **0.985 ± 0.039** |
| Recall | 0.750 ± 0.000 | 0.958 ± 0.072 | 1.000 ± 0.000 | **1.000 ± 0.000** |
| **F1** | 0.857 ± 0.000 | 0.742 ± 0.052 | 0.828 ± 0.024 | **0.992 ± 0.021** |
| False positives/case | 0.00 | 0.42 | 0.28 | 0.03 |
| Findings to triage/case | 0.50 | 1.06 | 0.94 | 0.69 |
| Evidence accuracy | n/a | 1.000 | 1.000 | 1.000 ± 0.000 |
| Cost/case | $0.0032 | $0.0038 | $0.0112 | $0.0157 |
| Runtime/case | 11.1 s | 8.9 s | 34.0 s | 38.9 s |

By category, full system, per trial (out of 3):

| Category | n | Baseline TP/FP/FN | Advanced TP/FP/FN |
|---|---:|---|---|
| RealIssue | 6 | 6 / 0 / 0 | 6 / 0 / 0 |
| Trap | 4 | 0 / 0 / 0 | 0 / 0–1 / 0 |
| Challenging | 2 | 0 / 0 / 2 | **2 / 0 / 0** |

Four things are worth reading carefully.

**The gain is entirely on the challenging cases.** Both arms find all six
defects visible in the diff. The difference is the two whose deciding evidence
sits in a file the change never touches — the baseline misses both, in all
three runs, and the advanced arm now finds both, in all three runs.

**Neither half of the design works alone.** The advanced prompt by itself
scores 0.742, *below* the plain baseline. Adding investigation reaches 0.828,
still below the baseline. Only the combination clears it. This is the most
important row in the table and the one we got wrong when reasoning without it.

**The baseline is perfectly stable; the advanced arm nearly is.** The baseline
scored identically on all twelve cases in all fifteen trials, σ = 0.000 on every
metric. The advanced arm's recall is also σ = 0.000 — it found every defect
every time. All of its remaining variance is one case, `c03`, which produced
one extra false positive in one trial out of three.

**Precision costs more than recall here.** Getting recall to 1.000 was a
candidate-generation problem and was solved by an instruction. Getting
precision back to 0.985 afterwards took the entire falsification apparatus —
four roles, ~6.3 model calls and ~2.2 tool calls per case, and 5× the cost of
the baseline.

## Improvement changelog

[`docs/improvement-changelog.md`](docs/improvement-changelog.md) — every
meaningful iteration with its evidence, including the four changes that made
the system worse, the one that did nothing at all, and what each taught us.
Nothing has been removed from it.

The short version: the advanced arm **lost** to the baseline on its first
12-case sweep (F1 0.667 vs 0.857), and the seed-phase result that had looked
like a clean win did not generalise.

## Example review

The `c11` trap deletes a path-traversal guard — the shape of a real
vulnerability. The advanced reviewer proposed it, then disproved it:

```
CANDIDATE  Callers elsewhere in the crate can pass arbitrary or untrusted
           string paths to asset_path, allowing directory traversal.

QUESTION   Do any callers of asset_path pass dynamic, user-controlled inputs
           rather than hardcoded string literals?

TOOL       search {"pattern": "asset_path"}
TOOL       read   {"file": "src/serve.rs", "start_line": 1, "end_line": 35}
TOOL       read   {"file": "src/assets.rs", "start_line": 1, "end_line": 50}

VERDICT    Contradicts
           asset_path is crate-internal (pub(crate)) and all call sites pass
           fixed string literals returned by AssetKind::file_name(). In
           src/serve.rs:23 the only production caller invokes
           asset_path(&self.root, kind.file_name()), where kind is mapped
           from a closed enum with hardcoded filenames. No caller passes
           arbitrary string inputs, preventing directory traversal.

DECISION   Rejected — never shown as a finding
```

## Investigated and cleared

In the final run five candidates were investigated and rejected on repository
evidence — one on each of the four traps, plus an overbroad candidate on c03.
This is where the falsification step earns its cost: each of these is a
plausible finding the reviewer never had to triage.

```
c02  "Calling summary on a router with no shards panics on shards[0]"
     Rejected — sole constructor rejects empty input, field is private,
     mutations preserve the element count.  src/router.rs:27-32, 46-54

c09  "Callers can call pop_front without checking is_empty"
     Rejected — Queue is pub(crate), the only call site is guarded by
     `while !self.queue.is_empty()`.

c10  "total_bytes multiplies chunk count by CHUNK_SIZE and can overflow"
     Rejected — chunk_count is bounded to MAX_CHUNKS at construction;
     4096 × 65536 = 2.68e8, far below usize::MAX.

c11  "asset_path allows directory traversal"
     Rejected — see Example review above.
```

Every rejected finding remains in its trajectory. Nothing is deleted, only
withheld.

## Reproduction

[`docs/reproduction.md`](docs/reproduction.md) — clean-environment setup, exact
commands, required configuration, expected output, runtime, and cost.

## Limitations

- **Twelve cases is small.** One finding moves F1 by roughly 0.03–0.06. Treat
  the direction as the result, not the third decimal place. Sixteen further
  Rust cases across three independently authored sets, and six Python pilot
  cases, exist and are reported separately; **none is folded into the
  headline**, and two of those sets show no advantage at all.
- **Fifteen trials, not thirty.** Enough to show the baseline is perfectly
  stable (identical on all 12 cases in all 15 trials, σ = 0.000 on every
  metric) and that the advanced arm's remaining spread is two rare false
  positives, but still short of a proper confidence interval. **Trial count
  changed a stated conclusion**: at n=5 we wrote that the spread came from a
  single case, `c03`; at n=15 a second case, `c08`, produced a false positive
  too. The claim was an artefact of the sample. The headline arms were not run
  interleaved, so a drift in provider behaviour between arms would be invisible
  there. The held-out trials alternate which arm runs first, which detects a
  consistent one-directional drift but is still not true interleaving.
- **Synthetic benchmark.** The cases are realistic in shape and every
  ground-truth claim was verified by execution, but they are small crates
  written for this project, not harvested from real pull requests.
- **Author bias, partly addressed — and the check found a real boundary.**
  The frozen benchmark was written by the same person who built the reviewer.
  Sixteen further cases were authored by three separate agents, each denied the
  prompts, the pipeline, the docs, every result, and each other's work
  ([`docs/benchmarks.md`](docs/benchmarks.md)).
  On [`holdout`](docs/holdout.md) the advantage replicates over 6 trials
  (baseline F1 0.750 ± 0.000, advanced **0.944 ± 0.061**) — though both of its
  traps produced false positives, `h04` once in six trials and `h05` twice,
  which the frozen benchmark's four traps never do.
  On `holdout2` and `holdout3` it **does not replicate at all**: the baseline
  scores 1.000 on both and the advanced arm loses on one. Those ten cases turn
  out to be diff-legible, so they never exercise the mechanism.
  On `holdout4`, built specifically so the changed line reads as correct, the
  baseline collapses to 0.400 and the advantage returns (0.517) — but the
  advanced arm's recall is **0.417 there against 1.000 on the frozen
  benchmark**. **Headline recall does not generalise to hard cases we did not
  write.** All of this is reported as found; nothing was rewritten or dropped.
  The authoring agents are still LLMs, so this reduces author bias rather than
  removing it; harvested pull requests would be the real fix.
- **Human review time is still a proxy in the headline table.** A blind
  stopwatch harness (`vcr triage`) is implemented and documented, but the
  reported figure remains findings-to-triage per case until a session is run.
- **Every ablation is now measured.** `no-falsification`, `candidates-only` and
  `no-second-look` are reported in the ladder. `no-followup` could not be
  measured at the shipped tool budget, because the branch it disables never
  fires there — 0 `Insufficient` verdicts in 70. It is measured instead at a
  **starved** budget of 1 tool call per candidate, where `Insufficient` becomes
  common and the loop is worth +0.167 recall; see the anti-take below. That is
  a measurement at a different operating point, not at the shipped one, and it
  is labelled as such wherever it appears.
- **Textual investigation only.** `search` is literal-substring. Dynamic
  dispatch, trait objects, re-exports, aliasing, macro-generated call paths and
  deep indirection are blind spots. Every trap here is resolvable by reading
  call sites; a trap turning on a trait object would likely defeat it.
- **A finding anchors at one location, and the interaction is derived rather
  than claimed.** A defect that is wrong only as an interaction between two
  files — `c03` (`store.rs` × `handler.rs`), `h06` (`digest.rs` × `model.rs`) —
  is reported at one anchor. Reports now also print **"Depends on code in:"**,
  built by Rust from the files the investigation actually read and cited, minus
  the anchor's own file. On the worked example above that resolves to
  `src/store.rs` — the file that makes the guard wrong and that the diff never
  touches.
  It is derived, never asserted: the model cannot name a file it did not open,
  which is the same rule that governs every other piece of evidence here. What
  is still missing is a first-class *claim* about an interaction, and that is
  not being added: a new field in the reviewer's output contract changes the
  review prompt, which would invalidate fifteen trials, for something the
  evaluator matches on the primary location either way.
- **One model for the system under test.** Every advanced-arm figure is
  `gemini-3.7-flash` on Vertex AI. The *baseline* was reproduced on Claude
  Sonnet 5 and agreed case-for-case — same F1 0.857, same 12 of 12 per-case
  outcomes, same `issue_type` on all six matches
  ([`docs/cross-model.md`](docs/cross-model.md)) — so the gap the advanced arm
  closes is not an artefact of one model being unusually timid. What remains
  unchecked is whether the *pipeline* behaves the same on another model, and
  that was not run on purpose: reproducing a multi-turn tool loop outside this
  orchestrator means re-implementing the thing under test.
- **One language, plus a six-case pilot.** The measured benchmark is Rust. A
  Python pilot exists ([`docs/pilot-python.md`](docs/pilot-python.md)) and
  shows the same pattern — baseline F1 0.667, advanced 0.857, zero false
  positives in either arm, both traps cleared on repository evidence — but six
  cases and one run per arm establish nothing on their own. It also produced a
  concrete counter-example: the `v6` candidate-generation rule that fixed
  `c12` in Rust did **not** transfer to its Python twin `p03`.

## Main failure mode

**The system trusts a claim it has the means to check, when the claim is
phrased as an assertion about the code rather than as code.**

This was the last bug to go and the most instructive. In one run the verifier
rejected a genuine reachable panic because the function's doc comment said
*"Callers check `contains` first, so the session is known to be present."* That
sentence is false — `on_heartbeat` does not check — and the repository could
have settled it in one search. The verifier read the comment as evidence.

The correction is narrow and does not generalise as far as one would like:
comments about things the repository can settle are claims, comments about the
outside world are evidence. The first version of that rule, which simply
distrusted comments, promptly rejected two real defects because the facts they
rested on — a `VARCHAR(64)` column, production batch sizes — were also written
in comments. Both versions are in the changelog with their numbers.

What generalises: **the boundary of what an agent can verify is the boundary of
what it should be allowed to treat as settled**, and that boundary has to be
drawn deliberately, because the model will not draw it.

**Dogfooding sharpened this into a second, sharper statement.** We pointed the
reviewer at the commit that introduced this project's own worst defect — the
deduplication rule that borrowed the evaluator's scoring tolerance to decide two
claims were the same claim. It missed it, and the reason is instructive: every
line of that code is correct. The comparison is sound, the constant is real, the
arithmetic is safe, the tests pass. It is wrong only in **what the constant
means**. So:

> The system finds defects that are wrong *in the code*. It does not find
> defects that are wrong *in the intent behind a name*.

That is a limit of the approach and not just of this implementation.
Falsification needs a claim that repository evidence can settle, and "this
constant is being used for a purpose its definition does not support" is a claim
about design intent — which the repository does not contain. What actually
caught that bug was `vcr replay-dedup`, running the rule against every recorded
run and asking what it had done. Full write-up:
[`docs/dogfood.md`](docs/dogfood.md).

### The tool budget is never the constraint, and we can prove it

The obvious response to a missed lookup is "give it a bigger budget". We
checked, across every candidate ever adjudicated on all three benchmarks:

| | |
|---|---|
| Candidates adjudicated | **86** |
| That used at least one tool | 86 |
| That **exhausted** the 8-call budget | **0** |
| Most tool calls any one candidate used | 6 |
| That stopped after 1 or 2 calls | 57 (66%) |

**Not one investigation in the entire project has ever hit its ceiling.** The
budget is not binding and never has been; raising it would change nothing. What
decides when an investigation stops is the model's own judgement that it has
seen enough, and that judgement is the actual limiting component.

The held-out `h04` false positive is the sharp version of this. The
investigation ran `list_files`, was shown `src/graph.rs` — the file containing
the constructor that makes its claim impossible — and stopped, having used four
of eight calls. It did not run out of budget. It decided it was done.

For honesty about how much that one case proves: "listed files and then opened
none of them" happened **7 times in 86 investigations**, and on the frozen
benchmark the other 6 all reached the *correct* answer (five rejections of trap
candidates, one confirmed real defect). So the pattern is not on its own a
predictor of failure, and we are not claiming it is. What the 86-sample count
does establish is which knob is dead: more tool calls is not the fix, and any
follow-up should work on the stopping decision instead.

## Hot take

**Falsification filters for truth, not for significance — and most of what a
code reviewer should suppress is true.**

Our two worst false positives were *"SizeReport does not derive Clone"* and
*"asset_path returns Option but can never return None"*. Both accurate. Neither
a bug. The falsification step confirmed them, correctly, because it had been
asked whether the evidence supports the claim, and it does. On the very same
cases it had already rejected the genuinely dangerous-looking claims with
excellent reasoning. The verification step was working perfectly and the output
was still noise.

The lesson we would carry into the next agent: a verification step inherits
whatever question you asked it, and "is this true?" is almost never the
question that matters. The useful question is "does this change what the reader
should do?" — and that requires the verifier to hold a model of the *reader*,
not just of the evidence. We reframed ours from "does the evidence support this
claim" to "does the evidence establish a real defect", and two false positives
disappeared without any change to the investigation that fed it.

**Second take: every intermediate version of this system is worse than doing
nothing clever, and we can put numbers on it.**

| Configuration | F1 |
|---|---:|
| Simple baseline | 0.857 |
| Advanced prompt alone | **0.742** |
| + repository investigation | **0.828** |
| + falsification (full) | **0.992** |

Both middle rows sit *below* the baseline. A reviewer told to propose broadly
is worse than one told to be careful. Give it repository tools and it is still
worse. Only the complete pipeline clears the bar, and then by a wide margin.

That is an uncomfortable shape for incremental development, because it means
every honest checkpoint on the way to this system would have looked like a
regression. We shipped a changelog twice claiming to know which change mattered
most — first candidate generation, then falsification — and the ablations
corrected us both times. The real answer is that they are one mechanism:
investigation buys recall (0.750 → 1.000), falsification buys back the
precision that costs (0.707 → 0.985), and neither survives alone.

The generalisable version: **if you can only ship half of a
propose-then-verify design, ship neither.**

**Third, an anti-take: we built three features on good reasoning, measured all
three as worthless — and then found out that "worthless" was the charitable
reading of one of them.**

The first version of this section reported three zeros:

| Feature | The reasoning | First measurement |
|---|---|---|
| Follow-up on "Insufficient" | The verdict names what is missing — go get it | Fired **0** times in 36 verifications |
| Candidate deduplication | Duplicate reports cost a second triage | Fired **0** times in 3 trials |
| Within-case memory | Stop re-reading files a sibling candidate opened | Used everywhere; −3% calls, −1% cost, inside noise |

All three were correct, tested and kept. None was claimed as an improvement.
That felt like the honest end of the story. It was not, because **"fired 0
times" is a measurement of the trigger, not of the feature**, and we had only
measured the trigger on runs where it never occurred.

### Deduplication was not inert. It was wrong.

Replaying the rule over the advanced trajectories of **all 26 recorded runs** —
`vcr replay-dedup`, which calls no model and reads artifacts already in the
repository — the trigger fires **7 times**, and *not one of those firings is a
duplicate*. Six are the same pair, in `c08-order-name-limit`:

```text
Validation  src/order.rs:26-28   order.name  checked against MAX_QUANTITY
Validation  src/order.rs:30-32   order.notes checked against MAX_NAME_LEN
```

Two fields, two distinct defects, **both in the ground truth**. They are joined
only because 28 + 3 ≥ 30. Each merge would have turned two true positives into
one true positive and one false negative.

The seventh is in the Python pilot and arose independently: two `ApiContract`
claims on `p02`, one about `cluster.unhealthy()` returning a count and one about
`cluster.nodes()` returning a non-sequence. Different methods, different claims,
same wrong merge. Two languages, two case authors, seven firings, zero
duplicates.

The cause was a single borrowed constant: the rule reused
`cfg.match_line_tolerance`, the evaluator's ±3 slack. That slack exists to
forgive an off-by-a-line in a location *estimate* while scoring. Deciding that
two claims are *the same claim* is a different question and must not borrow it.
Overlap is now strict.

Two things kept this hidden, and both are worth naming:

1. **A later, unrelated change saved us.** The `v6` candidate prompt stopped
   producing that pair, so the trigger went quiet before anyone looked at what
   it did when it fired.
2. **The unit test encoded the bug.** The test written to prove deduplication
   worked used the `c08` geometry *verbatim* — 26-28 against 30-32 — and
   asserted the merge was correct. A green test suite was evidence for the
   defect.

### The follow-up loop is not worthless — it is idle, and we can prove the difference

Across 70 findings the verifier returned `Supports` 44 times and `Contradicts`
26 times, and `Insufficient` **zero** times. Broadening it to the evidence
gate's other dead end — `Supports` downgraded for want of concrete evidence —
would not have helped either: that path also fired 0 times. Measuring first
prevented building a fourth inert feature.

**Then we asked a better question: is the loop useless, or merely never
needed?** Those are different claims and they have different consequences, and
you can separate them by changing the operating conditions instead of the
feature. The verifier says `Insufficient` when the evidence is too thin to
settle a claim — so starve the investigation. With the tool budget cut from 8
calls per candidate to **1**, `Insufficient` stops being hypothetical, and the
loop can be measured against its own ablation on the same twelve cases:

| Tool budget = 1 per candidate, 3 trials | `Insufficient` verdicts | Recall | F1 |
|---|---:|---:|---:|
| Follow-up **disabled** (`--ablation no-followup`) | **9** of 36 | 0.750 ± 0.217 | 0.844 ± 0.154 |
| Follow-up **enabled** (shipped) | 3 of 36 | **0.917 ± 0.072** | **0.956 ± 0.039** |

The loop fires 6 times, resolves two thirds of the `Insufficient` verdicts, and
buys **+0.167 recall and +0.111 F1** — with precision 1.000 in both arms, so it
is not trading false positives for it. It also cuts the variance by four times
(σ 0.154 → 0.039): a starved reviewer without the loop is not just worse, it is
erratic.

So the honest statement is not "this feature does nothing". It is: **the
shipped tool budget is generous enough that the loop never gets a chance to
help, and if you constrain the agent it becomes one of the more valuable parts
of the system.** That is a fact about the operating point, not about the code —
and it is exactly the distinction that "fired 0 times" hides.

Nothing was tuned to produce this. The verifier was never nudged toward
`Insufficient`; the only thing changed was how much the investigator was
allowed to look, which is a knob a real deployment might well turn for cost.

The state that *does* occur is a case finishing with **nothing to report**. The
change being genuinely fine and the reviewer having looked in the wrong place
produce identical output, and the pipeline stopped either way. So it now looks
once more, shown each rejected claim **together with the repository facts that
closed it** — the only place in the pipeline where falsification output feeds
back into generation rather than only filtering it.

It fires. On the frozen benchmark it fired on **exactly the four traps** — the
only four cases that report nothing — and **declined on all four**, returning
an empty list rather than manufacturing something. On the Python pilot it fired
on both traps, declined on one, and on the other proposed a claim that was
perfectly true and not a defect, which the verifier confirmed and the evaluator
scored as a false positive.

Six firings, five correct declines, one invented finding, no recall gained on
either benchmark, ~14% more cost per case. **So it ships off.** A single
12-case trial with it enabled scored F1 1.000, and that number is not being
claimed: it is inside the noise of 0.992 ± 0.021, from one run against fifteen,
and this project has already been flattered by a single run once. The code, the
seven tests and the `--ablation no-second-look` flag are kept; the default is 0
and a test guards it.

### The revised scoreboard

| Feature | First measurement | What measuring properly showed |
|---|---|---|
| Candidate deduplication | fired 0 times | fires 7 times across all recorded runs, **wrong every time**; fixed |
| Follow-up on "Insufficient" | fired 0 times | **not useless, idle**: starve the tool budget to 1 and it fires 6 times, resolving 6 of 9 `Insufficient` verdicts for +0.167 recall and +0.111 F1 |
| Within-case memory | −3% calls, inside noise | built the measured headroom; it made things **worse** — see below |

### The third feature: we built the headroom, and it backfired

Memory was the one component whose "no measurable benefit" verdict survived
scrutiny. Replaying tool calls showed where the headroom was: **15 of 156 tool
calls** were reads of a region an earlier candidate in the same case had
already fetched. Roughly a tenth of all lookups, spent re-fetching text the
pipeline already had.

So we built it. Memory can now carry the whole tool response rather than a
one-line summary, capped so recollection cannot crowd out evidence. Three
trials, everything else identical:

| | shipped (one-line) | carries content |
|---|---:|---:|
| F1 | 0.992 ± 0.021 *(15 trials)* | 1.000 ± 0.000 *(3 trials)* |
| Cost/case | **$0.0157** | **$0.0178** *(+13%)* |
| Model calls/case | 6.31 | 6.83 |
| Tool calls/case | **2.23** | **2.67** *(+20%)* |

**Tool calls went up by a fifth.** The change was built specifically to reduce
them, and it did the opposite. The plausible reading is that showing the
investigator more retrieved text invites it to keep pulling threads rather than
to conclude it has enough — the prompt got longer *and* the behaviour got
hungrier.

The F1 of 1.000 is not evidence of improvement, and is not claimed as any: the
shipped configuration already scores 1.000 in **13 of its 15 trials**, so three
perfect trials is exactly what you would expect from it too.

Off by default, kept, and reported as a change that cost 13% for nothing. The
headroom was real; the intervention aimed at it was wrong.

### What we would carry forward

**"Never fired" and "is broken" produce identical evidence.** This was already
the lesson from the follow-up loop, where a scripted mock proved the branch
worked and, in doing so, found a real bug: the trajectory recorded only the
*final* verdict, so a follow-up would have appeared with no visible cause. We
wrote that lesson down, and then failed to apply it to the feature sitting next
to it. Deduplication had six unit tests and none of them replayed it against a
real run.

**An unused branch is not neutral, it is unmeasured.** The cost of a dormant
feature is not the CPU it does not use; it is that its next firing is
unobserved, and you will not be watching. If you cannot make the trigger occur
on purpose, replay the rule over artifacts you already have — it is cheap, it
calls no model, and it is the only thing that would have caught this.

**Prefer the measurement that could embarrass you.** Every check that made this
section longer — the replay, the by-hand
[matching audit](docs/matching-audit.md), the
[held-out benchmark](docs/holdout.md) — was optional, and every one of them
found something the happy path had missed.

## Agent trajectories

[`docs/trajectories.md`](docs/trajectories.md) — a guided reading of
representative runs covering **all five roles** (the baseline reviewer, and the
advanced system's reviewer, falsifier, investigator, and fresh verifier),
showing each role's instructions, its tool calls, the verbatim tool responses,
the feedback that shaped the next step, retries, and the human checkpoint.

Every trajectory records each role's full prompt text and its prompt version
string, so any output can be traced to the exact instructions that produced it.

**The two most useful ones are from the held-out benchmark**, because they are
the same machinery succeeding and failing on cases nobody tuned it for:
[`h06`](docs/trajectories/h06-digest-threshold-inline-advanced.md) asked what
the unseen predicate it replaced was actually defined as, read that file, and
found the off-by-one;
[`h04`](docs/trajectories/h04-include-flatten-recursion-advanced.md) asked a
question about the mechanism instead of the precondition, ran `list_files`, saw
the file holding the answer, and stopped without opening it with half its tool
budget unspent. Identical prompts, identical budgets — the question it chose
decided the outcome.

Every run in [`results/trajectories/`](results-final/t1/trajectories/) records the full
prompts, every tool call and its verbatim response, the falsification question,
the fresh-context verdict, the orchestrator's decision and reason, token usage,
retries, and runtime.

## Human in the loop

The system never merges, rejects, modifies, or deploys anything. It reads a
repository and writes JSON. Every run ends with an explicit `HumanCheckpoint`
event recording what is being handed over and reaffirming that a human decides.
Findings are advisory output for a qualified reviewer.

## Cost and human time

### Cost

Measured, at $0.75/Mtok input and $3.75/Mtok output:

| | Cost/case | Whole 12-case sweep |
|---|---:|---:|
| Baseline | $0.0032 | $0.038 |
| Advanced | $0.0157 | $0.189 |
| Advanced, no falsification | $0.0112 | $0.134 |

The advanced arm costs **4.9× the baseline**, which buys **+0.250 recall and
+0.135 F1**. At roughly 1.6 cents per case that is a trade most teams would
take, but it is a real cost, and it scales with the size of the change under
review rather than with the number of defects in it.

These are the 15-trial means. Earlier drafts of this section carried the
3-trial figures ($0.0147, 4.6×, +0.167 recall) after the headline had moved on
— caught by an outside reviewer, and the reason for the consistency sweep
recorded in `DECISIONS.md`.

Token usage is recorded for every request. Dollar cost is reported **only**
when rates are supplied in `.env`:

```bash
VCR_PRICE_INPUT_USD_PER_MTOK=0.75
```

Set both rates or neither — half-configured pricing is a startup error, and
absent pricing is reported as "unavailable" rather than as zero. The project
does not guess at prices. Cost is recomputed from recorded token counts at
evaluation time, so rates can be supplied after a run with no re-run and no
further spend.

### Human time

The headline table reports **findings to triage per case**, a labelled proxy.
The direct measurement ships as a blind stopwatch harness:

```bash
cargo run --quiet --bin vcr -- triage --arms baseline,advanced --reviewer your-name
```

Findings from both arms are pooled, shuffled with a recorded seed, and shown
one at a time with no indication of which system produced them and no access to
ground truth. Only the claim and its location are shown — not the evidence, not
the verifier's verdict — which deliberately understates the advanced system's
benefit and is what keeps the arms indistinguishable. Every session file
records that limitation alongside its numbers.

The proxy is not a bad stand-in here, because the two arms differ mostly in
*how many* findings reach a human: 0.50/case for the baseline against
0.67/case for the advanced arm, and 0.94/case with falsification switched off.

## Dependencies

Rust 1.98, edition 2021. `anyhow`, `chrono`, `clap`, `dotenvy`, `reqwest`
(rustls), `serde`, `serde_json`, `thiserror`, `tokio`, `uuid`; `tempfile` for
tests. No AST or call-graph library — the masterplan's instruction not to build
one without evidence that simpler tools were insufficient was followed, and
they were sufficient.

## What existed before this project

The repository began with `.gitignore`, an Apache-2.0 `LICENSE`, and a
one-line `README.md`. Everything else — the reviewer, the benchmark, the
evaluator, and all documentation — was written for this hackathon. The full
decision record is in [`DECISIONS.md`](DECISIONS.md).

## License

Apache-2.0. See [`LICENSE`](LICENSE).
