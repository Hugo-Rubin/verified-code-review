# Improvement Changelog

How the Verified Code Reviewer got from a direct-prompt baseline to its final
configuration, including the four changes that made it worse and the one that
did nothing at all.

Every row was measured with the same deterministic evaluator. Rows marked
**n=3** ran against the three seed cases before the benchmark was expanded;
rows marked **n=12** ran against the frozen 12-case benchmark. The two are not
comparable to each other, and the split is exactly the point — the seed result
did not survive the larger benchmark.

Raw artifacts for every row are in [`results-archive/`](../results-archive/),
one directory per run, each containing full trajectories and evaluations.

---

## The story in one line

Investigation buys recall on defects whose evidence lives outside the diff.
Falsification is what makes it safe to go looking. Getting there required
learning three separate lessons about what "verification" actually has to
check: that the triggering state is **reachable**, that the finding is
**material**, and that the code's own comments are **claims about some things
and evidence about others**.

---

## Seed phase (n = 3)

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **E0 — Baseline** | One direct review pass. Same model, same JSON contract, same view of the diff and changed files as the advanced arm. The only thing withheld is repository tools. | P 1.000 · R 0.500 · **F1 0.667**. Found the real bug, correctly ignored the trap, missed the context-dependent panic. | Kept as the baseline. |
| **Prompt leakage fix** | While reading E0's output I found the baseline prompt named `unwrap()` on a non-`None` value and an in-bounds index as examples of correct code — near-verbatim descriptions of two benchmark cases. That is coaching the baseline past the exact situations under test. | Removed; scores **identical** (F1 0.667). The conservatism belongs to the model, not the prompt. | Kept the neutral prompt. A regression test now fails the build if a review prompt mentions a benchmark noun. |
| **A1 — Advanced, candidates as-is** | Full pipeline: candidate → falsification question → investigation → fresh-context verification. | **F1 0.667**, unchanged. Tool calls 0.33/case. Two of three cases produced *zero* candidates, so there was nothing to investigate. | Diagnosed: the bottleneck was candidate generation, not verification. |
| **A2 — Broaden candidate generation** | Both arms shared a contract ending "an empty result is a correct answer". Right for a reviewer whose output *is* its report; wrong for a stage feeding an investigator. Split it: the advanced stage is told under-proposing is the expensive mistake. | R 0.500 → 1.000, **F1 1.000**. | Kept. |
| **A3 — Seed the claimed region as evidence** ❌ | In A2 the trap was withheld for a bad reason: the verifier said the evidence was insufficient because it had never been shown the file the claim was about. Fixing that looked obviously right. | **Regression: F1 1.000 → 0.667**, precision 1.000 → 0.500. Shown the code, the verifier confirmed the claim: *"If `router` were to contain no shards ... indexing at index 0 triggers a panic."* | Kept the evidence, fixed the standard — see A4. |
| **A4 — Reachability is part of the claim** | A3 exposed that a claim phrased as a conditional is true of the mechanism whether or not the condition can hold, so it cannot be falsified. The verifier now must find the triggering state *reachable*, and evidence that it is prevented **contradicts** the claim. | **F1 1.000**, precision restored. The trap cleared as `Rejected`, citing the constructor, the private field, and count-preserving mutations. | Kept. |

**Lesson from the seed phase:** "X will panic if Y" cannot be disproved.
Falsification only does work when the claim asserts something that can be shown
not to happen.

---

## Benchmark expansion and the reality check (n = 12)

The benchmark was expanded to 6 real defects, 4 traps, and 2 challenging cases,
then frozen. Ground truth for every case was verified by executing it, not by
inspection.

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **First full sweep** ❌ | Ran the A4 configuration on all 12 cases. | **The advanced arm lost.** Baseline F1 0.857 · advanced **F1 0.667** (P 0.714, R 0.625). The n=3 result did not generalise. | Stopped and read every trajectory. Three unrelated causes, below. |
| **Fix 1 — Rate limiting** | 5 hard failures and 21 retries, *all* in the advanced arm, which makes ~6 model calls per case against the baseline's 1 and so meets a per-minute quota ~6× sooner. One correctly investigated finding was lost because its Verify call 429'd four times. | The comparison was partly measuring the quota, and the entire penalty fell on the arm under test. | `RateLimited` split from generic statuses, `Retry-After` honoured, 4 s exponential backoff capped at 60 s, retries 3 → 5, and a configurable minimum request interval (default 1500 ms). Plumbing, not tuning: it affects both arms identically. |
| **Fix 2 — A trailing comma** | A response contained a correct finding followed by an object ending `"reasoning": "...",}`. Invalid JSON, correctly rejected by `serde_json`, and the whole response — including the good finding — was discarded. | One real defect lost to punctuation. | Last-resort repair in `extract_json` that drops commas before `}` or `]` outside string literals, tried only after every strict parse fails. It cannot rescue genuinely broken JSON. |
| **Fix 3 — Materiality** | On *both* trap false positives the falsification step worked perfectly, rejecting the dangerous-looking claim with excellent reasoning. Both cases still scored an FP from a **second** candidate: *"SizeReport does not derive Clone"* and *"asset_path returns Option but can never return None"*. Both **true**. Neither a defect. | A verifier asked whether a claim is *accurate* confirms these, because they are accurate. | Verifier reframed from "does the evidence support this claim" to "does the evidence establish a real **defect**"; an accurate description that identifies nothing wrong is now `Contradicts`. Candidates must name a consequence. |
| **Run 2 — after fixes 1–3** | Both arms re-run under identical conditions. | Baseline F1 0.857 → advanced **F1 0.933** (P 1.000, R 0.875). Zero hard failures. | Kept, but one failure remained: c03. |
| **v4 — "The code's own claims are not evidence"** ❌ | Run 2 still missed c03, and for the most on-thesis reason available: the verifier rejected a genuine reachable panic because the function's doc comment asserted *"Callers check `contains` first"*. That assertion is false for `on_heartbeat`. The verifier trusted the code's description of itself. | **Regression: F1 0.933 → 0.857.** It recovered *both* challenging cases, then over-rejected two real defects: c06 because the 100k–500k batch scale "appears in a module doc comment rather than in concrete call sites", and c08 because the `VARCHAR(64)` column "is not confirmed by a schema file". | **Removed.** Told to distrust comments, it distrusted facts the repository has no way to state anywhere else. |
| **v5 — Weigh comments by checkability** ✅ | Keep the distinction the repository can actually make. A comment asserting something the repo *can* settle — which callers exist, what they pass, whether a guard runs — is a claim: go read the call sites. A comment stating a fact from *outside* the repo — a column type, production input sizes, a downstream contract — is the best evidence available and is reasoned from, not dismissed. | **F1 0.941**, recall **1.000**. Final configuration. | Kept. |

---

## Sprint 2 — measuring instead of asserting (n = 12, 3 trials)

Everything above rests on single runs. This stage replaced assertions with
measurements and, in two places, showed the assertions were wrong.

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **Cost accounting** | Token rates supplied, so cost per case became reportable. Computed from already-recorded token counts at evaluation time, so rates can be added after a run with no further spend. | Baseline **$0.0032/case**, advanced **$0.0147/case** (×4.6). Whole 12-case sweep: $0.038 vs $0.176. | Kept. |
| **Evidence accuracy** | The system claims every excerpt is verbatim repository content at a cited location. That claim is checkable, so `eval::audit_evidence` re-reads each cited file and compares line by line. Deterministic, no model. | **1.000** in every run, across 48–60 cited excerpts per advanced run, and 17/17 on Python. Zero mismatches ever observed. | Kept. The number is only meaningful reported alongside `checkable`, so both are printed. |
| **Repeated trials** | One run is a sample. Three trials per arm, with `vcr variance` reporting spread *and naming the cases that move*. | Baseline **identical on all 12 cases in all 3 trials** (σ = 0.000 on every metric). Advanced F1 0.917 ± 0.036, range 0.875–0.941, with **all** of that spread coming from one case (`c12`, found in 1 trial of 3). | Kept. The headline figures are now means over 3 trials, not single runs. |
| **Falsification ablation** | `--ablation no-falsification` keeps investigation and removes the falsification question and fresh verifier. Any candidate with evidence is reported. | **F1 0.725 ± 0.021, precision 0.619 — below the baseline's 0.857.** All 4 traps became false positives in all 3 trials. | Kept as the decisive measurement. It overturned our "which change mattered most" answer. |
| **Feedback loop: re-investigate on "Insufficient"** ❌ | An `Insufficient` verdict is a statement of what is missing, so feed it back into one more targeted investigation instead of giving up. Good idea, correctly implemented, bounded to one extra pass. | **Fired zero times.** Across 36 verifications in 3 trials the verifier returned only `Supports` (24) and `Contradicts` (12) — never `Insufficient`. | **Kept in the code, reported as inert.** It costs nothing when it does not fire and would matter on a benchmark with thinner evidence, but it contributed nothing here and is not claimed as an improvement. |
| **Language support + Python pilot** | `case.json` gains an optional `language`, threaded into the reviewer prompts and source fences. The verifier is language-neutral, and a test asserts it names no language. Three Python cases, ground truth verified by execution, in a **separate** benchmark so the frozen Rust one is untouched. | Baseline **F1 0.000** — found nothing on any of the three. Advanced **F1 0.500**, cleared the trap by rejecting both candidates on repository evidence, evidence accuracy 1.000. | Kept as a pilot. Three cases and one run; not a headline figure. See [`pilot-python.md`](pilot-python.md). |
| **Blind stopwatch harness** | `vcr triage` replaces the findings-to-triage proxy with a real measurement: findings pooled across arms, shuffled with a recorded seed, shown without saying which system produced them. | Implemented and tested; no session run yet, so the headline table still reports the proxy. | Kept. The proxy remains labelled as a proxy. |

One bug worth recording because it nearly cost a result: ablation trajectories
were named from the *agent* rather than the *arm*, so a `no-falsification` run
wrote `<case>-advanced.json` while the evaluator looked for
`<case>-advanced-no-falsification.json` and could not score the run at all. The
runs had succeeded; only the filenames were wrong, so renaming recovered them
without re-spending. `Trajectory::arm()` now builds the name, with tests.

## Final comparison

All arms, same model (`gemini-3.7-flash` via Vertex AI), temperature 0, frozen
12-case benchmark, **3 trials each**. Mean ± sample standard deviation.

| Metric | Baseline | Advanced | Change | Advanced, no falsification |
|---|---:|---:|---:|---:|
| Precision | 1.000 ± 0.000 | 0.921 ± 0.069 | −0.079 | 0.619 ± 0.031 |
| Recall | 0.750 ± 0.000 | **0.917 ± 0.072** | **+0.167** | 0.875 ± 0.000 |
| **F1** | 0.857 ± 0.000 | **0.917 ± 0.036** | **+0.060** | 0.725 ± 0.021 |
| False positives/case | 0.00 | 0.06 | +0.06 | 0.36 |
| Findings to triage/case ¹ | 0.50 | 0.67 | +0.17 | 0.94 |
| Evidence accuracy ² | n/a | 1.000 ± 0.000 | — | 1.000 ± 0.000 |
| **Cost/case** | **$0.0032** | **$0.0147** | ×4.6 | $0.0108 |
| Runtime/case | 6.5 s | 38.8 s | +32.3 s | 30.3 s |

¹ A manual-triage proxy: how many findings a human must read and judge. **Not**
a direct measurement of human review time. `vcr triage` implements the direct
blind measurement; no session has been run, so the proxy stands.

² Fraction of cited excerpts that really appear at the lines they cite, checked
deterministically against the repository. 48–60 citations per advanced run.
Zero mismatches were observed in any run, in any arm, in either language.

By case category, full system, per trial (out of 3):

| Category | n | Baseline TP/FP/FN | Advanced TP/FP/FN |
|---|---:|---|---|
| RealIssue | 6 | 6 / 0 / 0 | 6 / 0–1 / 0 |
| Trap | 4 | 0 / 0 / 0 | 0 / 0 / 0 |
| Challenging | 2 | 0 / 0 / 2 | **1–2 / 0 / 0–1** |

Three observations the numbers support and the single-run version did not.

**The advanced arm won every trial.** Its worst F1 (0.875) still beats the
baseline's (0.857), and the baseline scored *identically on all twelve cases in
all three trials* — σ = 0.000 on every metric. This is a small benchmark, but
the ordering was never in doubt across runs.

**Its variance is one case, not general noise.** `c12-slot-guard-capacity` was
found in 1 trial of 3; every other case scored the same every time. Reporting
"F1 0.917 ± 0.036" alone would obscure that the instability is a single
identifiable case, not diffuse jitter.

**The gain is entirely on the challenging cases.** Both arms resolve all six
defects visible in the diff and stay clean on all four traps, in every trial.
The difference is the two cases whose deciding evidence lives in a file the
change never touches — the baseline misses both, in all three runs.

---

## Which change contributed most

**We answered this wrong the first time, and the ablation corrected us.**

The original answer was "broadening candidate generation (A2), and it is not
close" — reasoning that nothing downstream can investigate a candidate that was
never raised, and that the change took seed recall from 0.500 to 1.000. That
reasoning is sound and the conclusion was still wrong.

Switching falsification off while leaving broadened generation in place:

| | F1 | Precision | Recall | FP on the 4 traps |
|---|---:|---:|---:|---|
| Baseline | 0.857 | 1.000 | 0.750 | 0 |
| Advanced **without** falsification | **0.725** | 0.619 | 0.875 | **4 of 4, every trial** |
| Advanced | **0.917** | 0.921 | 0.917 | **0** |

Broadened candidate generation, on its own, produces a system **worse than the
plain baseline**. Every trap becomes a false positive in every trial. The
instruction we credited with the improvement is actively harmful without
something to kill what it proposes.

So the honest answer is that the question is malformed. **Broadening and
falsification are one mechanism, not two changes to be ranked.** Telling an
agent to propose freely is only safe if something can reject what it proposes;
building the rejector is only worth its cost if something proposes freely
enough to need it. Each half alone scores below the baseline; together they
score 0.060 above it.

This is the argument for running ablations rather than reasoning about your own
architecture from the inside. We had a plausible story, it was consistent with
every number we had, and it was wrong.

---

## The experiment we removed

**`fresh-verify/v4`, "the code's own claims about itself are not evidence."**

It was added for a good reason and it fixed the thing it targeted — both
challenging cases went from missed to found. It still had to go, because it
cost more than it bought: F1 0.933 → 0.857, by rejecting two genuine defects
whose supporting facts (a database column width, production batch sizes)
happened to be written in comments.

The rule was right and too broad. A repository can check whether callers honour
a documented precondition; it cannot check what a database schema says. v5
keeps the first half and drops the second, and is the only version that gets
both the challenging cases *and* the real defects.

Artifacts: [`results-archive/n12-run3-verify-v4-overrejected/`](../results-archive/n12-run3-verify-v4-overrejected/).

## The experiment that did nothing

**The "Insufficient" feedback loop.**

An `Insufficient` verdict is not a dead end — it is a statement of what is
missing. So the orchestrator gained a bounded self-correction step: take the
verifier's stated gap, run one more targeted investigation against it, and
re-adjudicate the fuller evidence package in a fresh context again.

It is a good idea and it is correctly implemented. It also fired **zero times**.

Across 36 verifications in three trials the verifier returned `Supports` 24
times and `Contradicts` 12 times. It never once said `Insufficient`, so the
branch was never reachable. The `no-followup` ablation would be bit-identical
to the full system on this benchmark, which is why it was not spent on.

### "Never fired" is not the same as "cannot fire"

Reporting a branch as inert is only honest if the branch demonstrably works
when its trigger occurs. From the results alone the two are indistinguishable:
a loop that never runs and a loop that is broken produce identical evidence.

So the loop is now driven directly. `MockClient::scripted` queues responses per
stage, and four tests exercise it against a real case:

| Test | Asserts |
|---|---|
| `an_insufficient_verdict_triggers_a_second_investigation` | An `Insufficient` first verdict produces the follow-up note, a second investigation pass, a **second** verification, and a final status decided by the later verdict |
| `a_decisive_verdict_never_triggers_a_second_look` | A `Supports` first verdict yields exactly one verification — the path every real run took |
| `the_follow_up_is_disabled_when_the_budget_is_zero` | `max_followup_investigations = 0` suppresses it |
| `the_no_followup_ablation_disables_the_loop` | The ablation flag suppresses it |

Writing them found a real bug. The trajectory recorded only the **final**
verdict, so when a follow-up replaced an `Insufficient` with a `Supports`, the
`Insufficient` that caused the second investigation vanished from the record. A
reader would have seen a second investigation with no visible reason for it.
Both verdicts are now recorded in order.

That bug had never surfaced, because on the real benchmark the branch never
ran — which is exactly the class of defect that hides behind an untested path.

### The conclusion, now earned

The loop works and did not fire. We kept the code and report it as inert: it
costs nothing when idle and would plausibly matter where evidence is thinner or
tool budgets tighter, but it contributed exactly nothing here. The difference
between "a self-correcting agent" and "an agent with an unused self-correction
branch" is whether someone counted — and the difference between "inert" and
"broken" is whether someone tested.

The tempting move — nudging the verifier to say `Insufficient` more often so
the feature would have something to do — was not made. That is tuning the
measurement to justify the code.

---

## Every measured run

| Directory | n | Configuration | Baseline F1 | Advanced F1 |
|---|---:|---|---:|---:|
| `results-archive/baseline-prompt-v1/` | 3 | baseline prompt with case-specific tells | 0.667 | — |
| `results-archive/advanced-review-v2/` | 3 | A1, candidates as-is | 0.667 | 0.667 |
| `results-archive/advanced-v3-no-seeded-evidence/` | 3 | A2, broadened candidates | 0.667 | 1.000 |
| `results-archive/advanced-v4-seeded-no-reachability/` | 3 | A3, seeded region ❌ | 0.667 | 0.667 |
| `results-archive/seed3-n3-final/` | 3 | A4, reachability | 0.667 | 1.000 |
| `results-archive/n12-run1-advanced-regression/` | 12 | A4 on the frozen benchmark ❌ | 0.857 | 0.667 |
| `results-archive/n12-run2-verify-v3/` | 12 | + rate-limit, JSON and materiality fixes | 0.857 | 0.933 |
| `results-archive/n12-run3-verify-v4-overrejected/` | 12 | + comments-are-not-evidence ❌ | 0.857 | 0.857 |
| `results/` | 12 | v5, comments weighed by checkability (single run) | 0.857 | 0.941 |
| `results-trials/t1..t3/` | 12 | **final: v5, 3 trials per arm** | **0.857** | **0.917 mean** |
| `results-trials/t1..t3/` | 12 | ablation: no falsification ❌ | 0.857 | 0.725 mean |
| `results-pilot/` | 3 | Python pilot (separate benchmark) | 0.000 | 0.500 |

Nothing has been removed from this table. The ❌ rows are changes that made the
system worse; they were reverted, refined, or — in the ablation's case — run
deliberately to find out what a stage was worth.

Note the last two rows of the Rust benchmark. `results/` is the single run that
produced the 0.941 figure quoted in earlier drafts of this document;
`results-trials/` is three trials of the same configuration, whose mean is
0.917 and whose range is 0.875–0.941. **The single run was the best of three.**
The reported headline is the mean, not that run.

---

## Nondeterminism

LLM responses are not deterministic even at temperature 0. Benchmark inputs,
prompts, model configuration, tool limits, the evaluator, and the line
tolerance are all fixed, but repeated runs may produce small differences in
output and therefore in metrics.

This has now been measured rather than warned about. Three trials per arm on
the frozen benchmark:

| Arm | F1 mean | range | σ | cases that moved |
|---|---:|---|---:|---|
| Baseline | 0.857 | 0.857–0.857 | 0.000 | none — identical on all 12, all 3 trials |
| Advanced | 0.917 | 0.875–0.941 | 0.036 | `c12` only (found in 1 of 3) |
| Advanced, no falsification | 0.725 | 0.700–0.737 | 0.021 | none |

Two things follow.

**The baseline is deterministic in practice.** Twelve cases, three runs,
identical every time, σ = 0.000 on every metric. Whatever nondeterminism the
provider has at temperature 0, it did not change a single scored outcome for a
one-call-per-case reviewer.

**The advanced arm's spread is one case, not diffuse noise.** All of its
variance comes from `c12-slot-guard-capacity`. Reporting σ alone would suggest
general instability; naming the case is more useful and more honest. The
earlier c03/c12 swap observed between single runs was the same phenomenon seen
without enough samples to localise it.

Three trials is still few. It is enough to establish that the advanced arm beat
the baseline in every run — its worst F1, 0.875, exceeds the baseline's 0.857 —
and not enough for a confidence interval. The arms were also run sequentially
rather than interleaved, so a drift in provider behaviour between arms would
not be visible here.
