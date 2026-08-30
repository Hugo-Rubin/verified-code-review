# Verified Code Reviewer

An LLM code reviewer for Rust changes that investigates its own candidate
findings against the repository, then tries to **disprove** them in a fresh
reasoning context before deciding whether a human should ever see them.

On a frozen 12-case benchmark it recovers the defects a direct reviewer misses
— the ones whose deciding evidence lives in files the change does not touch —
while staying clean on all four false-positive traps.

Mean of **3 trials per arm**, same model (`gemini-3.7-flash`), temperature 0.

| Metric | Simple baseline | Agent solution | Change |
|---|---:|---:|---:|
| **Primary outcome — finding F1** | 0.857 | **0.980** | **+0.123** |
| Precision | 1.000 | 0.963 | −0.037 |
| Recall | 0.750 | **1.000** | **+0.250** |
| Human time per task (proxy) ¹ | 0.50 findings/case | 0.69 findings/case | +0.19 |
| **Cost per task** | **$0.0032** | **$0.0159** | ×5.0 |
| Runtime per task | 11.1 s | 38.9 s | +27.8 s |
| Evidence accuracy ² | n/a — gathers none | **1.000** | — |

The advanced arm found **every real defect in every trial** (recall 1.000,
σ = 0.000) and beat the baseline in all three runs — its worst F1 (0.941) still
exceeds the baseline's (0.857, identical in all three).

¹ Manual-triage proxy — findings a human must read and judge. **Not** a direct
measurement of human review time. A blind stopwatch harness for the real
measurement ships as `vcr triage`; see [Cost and human time](#cost-and-human-time).
² Fraction of cited excerpts that really appear at the lines they cite, checked
deterministically against the repository, 48–60 citations per run.

### The whole ladder, measured

Every stage switched off in turn, 3 trials each, same benchmark:

| Configuration | F1 | Precision | Recall | Cost/case |
|---|---:|---:|---:|---:|
| Simple baseline | 0.857 | 1.000 | 0.750 | $0.0032 |
| Advanced prompt alone (no investigation, no falsification) | 0.742 | 0.607 | 0.958 | $0.0038 |
| **+ investigation**, no falsification | 0.828 | 0.707 | 1.000 | $0.0112 |
| **+ falsification** — the full system | **0.980** | 0.963 | 1.000 | $0.0159 |

Read the middle two rows carefully, because they are the result.

**Neither half beats the baseline on its own.** The advanced prompt alone scores
0.742 — *worse than doing nothing clever*. Adding repository investigation
lifts it to 0.828, which is still **below** the baseline's 0.857. Only when
falsification is added does the system reach 0.980.

Investigation supplies the recall (0.750 → 1.000). Falsification is what makes
that recall affordable, taking precision from 0.707 to 0.963. Remove either and
you have something worse than the simple prompt you started with.

Full numbers: [`results-trials/`](results-trials/) and [`results/`](results-final/).
Full history, including four changes that made things worse and one feature
that did nothing at all:
[`docs/improvement-changelog.md`](docs/improvement-changelog.md).

---

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
| **Reviewer** | `advanced-review/v5` | diff + changed files | Propose candidates, erring toward proposing |
| **Falsifier** | `advanced-falsify/v2` | the claim | Write the one question whose answer would show the claim is **wrong** |
| **Investigator** | `advanced-investigate/v1` | claim, question, results so far | Choose the next `search` / `read` / `list_files` call, or stop |
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

## Results

All arms, `gemini-3.7-flash` via Vertex AI, temperature 0, frozen benchmark,
**3 trials each**. Mean ± sample standard deviation.

| Metric | Baseline | Prompt alone | + investigation | **Advanced (full)** |
|---|---:|---:|---:|---:|
| Precision | 1.000 ± 0.000 | 0.607 ± 0.052 | 0.707 ± 0.035 | **0.963 ± 0.064** |
| Recall | 0.750 ± 0.000 | 0.958 ± 0.072 | 1.000 ± 0.000 | **1.000 ± 0.000** |
| **F1** | 0.857 ± 0.000 | 0.742 ± 0.052 | 0.828 ± 0.024 | **0.980 ± 0.034** |
| False positives/case | 0.00 | 0.42 | 0.28 | 0.03 |
| Findings to triage/case | 0.50 | 1.06 | 0.94 | 0.69 |
| Evidence accuracy | n/a | 1.000 | 1.000 | 1.000 ± 0.000 |
| Cost/case | $0.0032 | $0.0038 | $0.0112 | $0.0159 |
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
scored identically on all twelve cases in all three trials, σ = 0.000 on every
metric. The advanced arm's recall is also σ = 0.000 — it found every defect
every time. All of its remaining variance is one case, `c03`, which produced
one extra false positive in one trial out of three.

**Precision costs more than recall here.** Getting recall to 1.000 was a
candidate-generation problem and was solved by an instruction. Getting
precision back to 0.963 afterwards took the entire falsification apparatus —
four roles, ~6.5 model calls and ~2.5 tool calls per case, and 5× the cost of
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
  the direction as the result, not the third decimal place.
- **Three trials, not thirty.** Enough to show the baseline is perfectly stable
  and that the advanced arm's spread comes from a single case (`c12`), but far
  too few for a confidence interval. The arms were not run interleaved, so a
  drift in provider behaviour between arms would be invisible.
- **Synthetic benchmark.** The cases are realistic in shape and every
  ground-truth claim was verified by execution, but they are small crates
  written for this project, not harvested from real pull requests. They were
  also written by the same person who built the reviewer, which is a bias no
  amount of care removes.
- **Human review time is still a proxy in the headline table.** A blind
  stopwatch harness (`vcr triage`) is implemented and documented, but the
  reported figure remains findings-to-triage per case until a session is run.
- **The falsification ablation is the only one measured.** `no-followup` and
  `candidates-only` are implemented but were not run across trials.
- **Textual investigation only.** `search` is literal-substring. Dynamic
  dispatch, trait objects, re-exports, aliasing, macro-generated call paths and
  deep indirection are blind spots. Every trap here is resolvable by reading
  call sites; a trap turning on a trait object would likely defeat it.
- **Findings are single-location.** A defect spanning several files that is
  wrong only in combination has no representation in the schema.
- **One model, one provider.** Everything here is `gemini-3.7-flash` on Vertex
  AI. Nothing has been checked for generalisation across models.
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
| + falsification (full) | **0.980** |

Both middle rows sit *below* the baseline. A reviewer told to propose broadly
is worse than one told to be careful. Give it repository tools and it is still
worse. Only the complete pipeline clears the bar, and then by a wide margin.

That is an uncomfortable shape for incremental development, because it means
every honest checkpoint on the way to this system would have looked like a
regression. We shipped a changelog twice claiming to know which change mattered
most — first candidate generation, then falsification — and the ablations
corrected us both times. The real answer is that they are one mechanism:
investigation buys recall (0.750 → 1.000), falsification buys back the
precision that costs (0.707 → 0.963), and neither survives alone.

The generalisable version: **if you can only ship half of a
propose-then-verify design, ship neither.**

**Third, an anti-take: we built three features on good reasoning and measured
all three as worthless.**

| Feature | The reasoning | The measurement |
|---|---|---|
| Follow-up on "Insufficient" | The verdict names what is missing — go get it | Fired **0** times in 36 verifications |
| Candidate deduplication | Duplicate reports cost a second triage | Fired **0** times in 3 trials |
| Within-case memory | Stop re-reading files a sibling candidate opened | Used everywhere; −3% calls, −1% cost, inside noise |

Each was prompted by a real observation in the trajectories. Each seemed
obviously worth building. All three are correct, tested, and kept — they cost
nothing idle — and none is claimed as an improvement.

Saying that honestly took more work than building them. "Never fired" and "is
broken" produce identical evidence, so the follow-up loop had to be driven with
a scripted mock to prove it works when its trigger occurs. That test found a
bug: the trajectory recorded only the *final* verdict, so a follow-up would
have appeared with no visible cause. It had survived precisely because the
branch had never run.

Two lessons we would carry forward. The difference between "a self-correcting
agent" and "an agent with an unused self-correction branch" is whether someone
measured. And the difference between "inert" and "broken" is whether someone
tested — an untested path in an agent is where defects hide, and "it never
triggers" is exactly when you are least likely to look.

## Agent trajectories

[`docs/trajectories.md`](docs/trajectories.md) — a guided reading of
representative runs covering **all five roles** (the baseline reviewer, and the
advanced system's reviewer, falsifier, investigator, and fresh verifier),
showing each role's instructions, its tool calls, the verbatim tool responses,
the feedback that shaped the next step, retries, and the human checkpoint.

Every trajectory records each role's full prompt text and its prompt version
string, so any output can be traced to the exact instructions that produced it.

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
| Advanced | $0.0147 | $0.176 |
| Advanced, no falsification | $0.0108 | $0.130 |

The advanced arm costs **4.6× the baseline**, which buys +0.167 recall and
+0.060 F1. At roughly 1.5 cents a file that is a trade most teams would take,
but it is a real cost and it scales with the number of files in a pull request,
not with the number of defects.

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
