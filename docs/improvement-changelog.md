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

All arms, `gemini-3.7-flash` via Vertex AI, temperature 0, frozen 12-case
benchmark, **3 trials each**. Mean ± sample standard deviation. Artifacts in
[`results-final/`](../results-final/).

| Metric | Baseline | Prompt alone | + investigation | **Advanced (full)** |
|---|---:|---:|---:|---:|
| Precision | 1.000 ± 0.000 | 0.607 ± 0.052 | 0.707 ± 0.035 | **0.963 ± 0.064** |
| Recall | 0.750 ± 0.000 | 0.958 ± 0.072 | 1.000 ± 0.000 | **1.000 ± 0.000** |
| **F1** | 0.857 ± 0.000 | 0.742 ± 0.052 | 0.828 ± 0.024 | **0.980 ± 0.034** |
| False positives/case | 0.00 | 0.42 | 0.28 | 0.03 |
| Findings to triage/case ¹ | 0.50 | 1.06 | 0.94 | 0.69 |
| Evidence accuracy ² | n/a | 1.000 | 1.000 | 1.000 ± 0.000 |
| Cost/case | $0.0032 | $0.0038 | $0.0112 | $0.0159 |
| Runtime/case | 11.1 s | 8.9 s | 34.0 s | 38.9 s |

¹ A manual-triage proxy, not a stopwatch. `vcr triage` implements the direct
blind measurement; no session has been run.

² Fraction of cited excerpts that really appear at the lines they cite, checked
deterministically. Zero mismatches in any run, any arm, either language.

By category, full system, per trial:

| Category | n | Baseline TP/FP/FN | Advanced TP/FP/FN |
|---|---:|---|---|
| RealIssue | 6 | 6 / 0 / 0 | 6 / 0 / 0 |
| Trap | 4 | 0 / 0 / 0 | 0 / 0–1 / 0 |
| Challenging | 2 | 0 / 0 / 2 | **2 / 0 / 0** |

**Recall is 1.000 with σ = 0.000** — every real defect, every trial, including
both cases the baseline never finds. All remaining variance is one case,
`c03`, which produced one extra false positive in one trial of three.

---

## Which change contributed most

**We answered this wrong twice before the ablations settled it.**

The first answer was "broadening candidate generation". The second, after the
first ablation, was "falsification". Both were too simple. The full ladder:

| Configuration | F1 | vs baseline |
|---|---:|---|
| Simple baseline | 0.857 | — |
| Advanced prompt alone | 0.742 | **−0.115** |
| + repository investigation | 0.828 | **−0.029** |
| + falsification (full) | **0.980** | **+0.123** |

**Every intermediate configuration is worse than the baseline.** The prompt
alone is worse. The prompt plus investigation is still worse. Only the complete
pipeline beats it, and it beats it by a wide margin.

Falsification is the largest single step (+0.152 F1) and is the component the
result depends on. But it is worth nothing on its own — it exists to reject bad
candidates, and without broadened generation and investigation there are no
candidates and no evidence to reject them with. The three are one mechanism.

The measurable division of labour:

- **Investigation buys recall**: 0.750 → 1.000, by reaching evidence outside
  the diff.
- **Falsification buys back the precision that costs**: 0.707 → 0.963.

Neither is optional, and neither is sufficient. That is a less quotable answer
than "X mattered most", and it is what the numbers say.

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

## Sprint 3 — the last capability gap, and three features that earned nothing

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **v6 — name your assumptions about code you cannot see** ✅ | `c12` was the only unstable case, and the cause was not verification: the reviewer proposed **zero candidates** on it in 2 of 3 trials, so there was nothing to investigate. Its Python twin `p03` failed the same way, and so did the `c10` trap. The fix is a general rule with no benchmark nouns in it — where the change calls something whose definition is not visible, state what it must do for the code to be right, and raise that as a candidate. | **Recall 0.750 → 1.000, σ = 0.000.** F1 0.917 → 0.980. It also made the traps *harder*: `c10` had not been challenged at all in 2 of 3 earlier trials, so "0 false positives on traps" now means more than it did. | Kept. |
| **Full ablation ladder** | Every stage switched off in turn, 3 trials each, all at the final configuration. | prompt alone **0.742** · + investigation **0.828** · + falsification **0.980** · baseline 0.857. | Kept as the central evidence. Both intermediate rows sit *below* the baseline. |
| **Candidate deduplication** ❌ | Across trials the reviewer occasionally reported one defect twice under the same category at adjacent ranges, costing a false positive for a second triage of the same thing. Conservative merge: same file, same `issue_type`, overlapping within the evaluator's tolerance, narrowest claim survives. | **Fired zero times** across 3 trials. The duplicates it was built for did not recur in these runs. | Kept, reported as inert. Six unit tests prove it merges what it should and refuses to merge different categories, distant ranges, or different files. |
| **Within-case memory** ❌ | Each candidate investigated independently, so a second candidate re-read files the first had already opened. Memory carries **lookups, never verdicts** — passing conclusions forward would reintroduce the anchor the fresh verifier exists to remove. | **Used on all 12 cases**, 91 investigate turns received prior lookups. Effect: model calls 6.72 → 6.53/case (−3%), cost $0.01602 → $0.01587 (−1%). | Kept, reported as no measurable benefit. Both figures are inside run-to-run noise. |

**Do not attribute the precision improvement to either feature.** Precision
moved 0.926 → 0.963 between the v6 sweep and the final sweep, and it is
tempting to credit deduplication. Deduplication **never ran**. With σ = 0.064
on precision across three trials, a change of 0.037 is comfortably inside
noise, and the honest reading is that the two sweeps are indistinguishable on
precision.

### Three features, three nothings

Counting the follow-up loop, this project has now added three components on
sound reasoning and measured all three as contributing nothing:

| Feature | Reasoning | Measured |
|---|---|---|
| Follow-up on "Insufficient" | A verdict of *insufficient* names what is missing; go get it | Fired 0 times in 36 verifications |
| Candidate deduplication | Duplicate reports cost a second triage | Fired 0 times in 3 trials |
| Within-case memory | Stop re-reading files a sibling candidate opened | Used everywhere; −3% calls, −1% cost |

All three are correct, tested, and kept — they cost nothing when idle and would
plausibly matter on a different benchmark. None is claimed as an improvement.

> **This conclusion did not survive Sprint 4b, and the correction is the more
> useful result.** "Fired 0 times" measures the *trigger*, not the feature.
> Replayed over every recorded run, deduplication fires 7 times and is **wrong
> every time**; the follow-up loop's trigger turned out to be unreachable
> rather than merely unlucky. Only within-case memory is genuinely inert. See
> [Sprint 4b](#sprint-4b--measuring-the-three-features-that-earned-nothing).

The pattern is the lesson. Every one of these was justified by a real
observation in the trajectories, and every one seemed obviously worth building.
The single component that *does* carry the result — falsification, worth +0.152
F1 on its own — is the one we nearly cut early on, when the seed benchmark
suggested candidate generation was the whole story.

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

> **Sprint 4b revisited this and reached a different answer**, without taking
> that tempting move. The verdict was not "make `Insufficient` more likely" but
> "find a trigger that already occurs": a case that finishes with **nothing to
> report**. That fires — on exactly the traps — and it still does not help, so
> it ships off. The loop is now measured rather than dormant.

---

## Sprint 4 — widening the evidence, and a benchmark defect we caught by result

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **Trials 4 and 5 on the headline arms** | Three trials is enough to see spread, not enough to trust it. Two more, same configuration, same frozen benchmark. | Advanced **F1 0.988 ± 0.026** over 5 trials (was 0.980 ± 0.034 over 3), recall **1.000 ± 0.000**, precision 0.978. Baseline **identical on all 12 cases in all 5 trials**, σ = 0.000 on every metric. | Kept. Headline figures are now means over 5 trials. |
| **Python pilot expanded 3 → 6** | The first three cases were all ports of Rust cases, so the pilot could only confirm transfer, never discover a Python-specific failure. Three new cases were written against defect classes Rust **cannot express**: a mutable default argument, a generator consumed twice, and a shared-module-state trap that is safe only because the accessor copies and the values are scalars. | Baseline **F1 0.667**, advanced **F1 0.857**, precision **1.000 in both arms**, zero false positives, both traps cleared on repository evidence, evidence accuracy 1.000 (51/51 citations). | Kept as a pilot. Six cases, one run per arm; still not a headline figure. |
| **`v6` did not transfer to Python** ❌ | The v6 rule ("name what unseen code must do for this to be right") took `c12` from found-in-1-of-3 to found-in-5-of-5. Its Python twin `p03` tests the same shape. | `p03` **missed again**, and the trajectory shows why: candidate generation never proposed the defect. The two candidates it did propose were investigated and correctly rejected. Verification was not the failure. | Reported, not patched. A prompt change made to fix one named case would be exactly the overfitting this project's ablations exist to catch. |
| **Ground-truth anchoring defect, found via a result** ❌ | `p06`'s expected finding was anchored at the *consumer* (lines 24-28) rather than at the changed lines. The advanced arm reported the defect at the change (15-19) with a fully correct diagnosis and was scored a false positive **plus** a false negative. | It was **1 of 18** findings in the project anchored outside its case's changed hunk; the other 17 already followed the convention. Corrected to 15-19. Advanced pilot F1 **0.571 → 0.857**; baseline **0.667 either way**, because its location at 22-27 overlaps both anchors. | Corrected, with both figures reported side by side in [`pilot-python.md`](pilot-python.md). The correction moves one arm and not the other, which is the shape a convenient edit would have — so the reader gets both numbers rather than our word for the motive. |
| **The convention is now checked, not remembered** | A benchmark defect found because a result looked wrong is the dangerous direction. A promise not to do it again is worth nothing. | `bench::findings_outside_the_diff` parses each case's own diff and reports any expected finding outside the changed ranges; `vcr check` prints it as a warning. Four unit tests cover single-line hunks, new files and deleted files. Both benchmarks are clean under it. | Kept. This is how we know 17 of 18 followed the convention rather than believing it. |

**On re-running the pilot.** `p01`–`p03` were re-executed as part of the
six-case sweep, so the three-case figures earlier in this document are a
different sample, not merely a re-scoring. The most visible change: the
duplicate false positive on `p01` in the three-case run did not recur — that
same second claim was investigated and **rejected** this time, which is why
pilot precision reads 1.000 rather than 0.500. One run either way; do not read
a trend into it.

---

## Sprint 4b — measuring the three features that "earned nothing"

Sprint 3 reported three components as contributing nothing and kept all three.
That was honest about what had been measured and wrong about what it meant,
because **"fired 0 times" measures the trigger, not the feature** — and the
trigger had only been observed on runs where it never occurred.

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **Deduplication replay** ❌❌ | "Fired 0 times in 3 trials" is not a statement about what the rule *does*. `vcr replay-dedup` replays it over the advanced trajectories of every recorded run — no model, artifacts only — and separates merges that rest on genuinely overlapping ranges from those that rest only on the evaluator's ±3 matching tolerance. | Across **all 26 recorded runs** the trigger fires **7 times, and 0 are duplicates.** Six are `c08-order-name-limit`: `Validation src/order.rs:26-28` (`order.name` vs `MAX_QUANTITY`) against `Validation src/order.rs:30-32` (`order.notes` vs `MAX_NAME_LEN`) — two distinct defects, **both in the ground truth**, joined only because 28 + 3 ≥ 30. Each merge would have converted two true positives into one true positive and one false negative. The seventh arose independently in the Python pilot: two `ApiContract` claims on `p02` about different methods. Two languages, two case authors, seven firings, zero duplicates. | **Fixed.** Overlap is now strict; the rule may not borrow the evaluator's slack. Two things had hidden it: the `v6` prompt change stopped producing that pair, and the unit test written to prove the feature worked used that exact geometry and asserted the merge was **correct**. The test encoded the defect. |
| **A reachable trigger for the follow-up loop** | The `Insufficient` verdict never occurs: 44 `Supports`, 26 `Contradicts`, **0 `Insufficient`** across 70 findings. Before building anything, the obvious alternative was measured too — `Supports` downgraded by the evidence gate for want of concrete evidence — and that path also fired **0** times. The state that *does* occur is a case finishing with nothing to report. | The second look is shown every rejected claim **together with the repository facts that closed it**, and asked what a reviewer focused on those questions would have walked past. It is the only path where falsification output feeds back into generation rather than only filtering it. Anything it proposes re-enters the full pipeline; anything restating a settled claim is dropped before it costs an investigation. | Kept, **off by default**. |
| **Second look, measured** ❌ | Run on both benchmarks. | Fired on **exactly the four traps** on the frozen benchmark — the only four cases that report nothing — and **declined on all four**. On the Python pilot it fired on both traps, declined on one, and on the other proposed a claim that was **true and not a defect**; the verifier confirmed it and the evaluator scored a false positive. Six firings, five correct declines, **no recall gained on either benchmark**, ~14% more cost per case. | **Default 0**, guarded by a test. A single 12-case trial with it enabled scored F1 1.000 — inside the noise of 0.988 ± 0.026, one run against five, and this document already records being flattered by a single run once. Not claimed. |
| **Within-case memory** ❌ | Re-examined for the same "trigger vs feature" error. | Unchanged: used on every case, −3% calls, −1% cost, inside noise. Replaying reads showed 15 of 156 tool calls were regions already covered by an earlier read in the same case, so there is real headroom — but nothing was built for it, because there was no time to measure a change as carefully as the other two were measured. | Kept, still reported as no measurable benefit. Headroom recorded as future work, not as an improvement. |

**The revised scoreboard.** Of the three "inert" features, one was actively
harmful and is fixed, one had an unreachable trigger and now has a reachable one
that measurably does not help, and one is genuinely inert.

---

## Sprint 4c — the held-out benchmark, and one honest revision

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **Six cases written without sight of the reviewer** | Every prompt rule here was written after reading trajectories from the frozen benchmark, and the same person wrote both. That bias cannot be argued away; it needs cases from an author who cannot see the system. A separate agent authored six, denied `src/prompts.rs`, `src/agent/**`, `README.md`, `DECISIONS.md`, `docs/**` and every `results*/` directory. It never ran the reviewer. | **Baseline F1 0.750, advanced F1 0.889.** The advantage replicates, by the same mechanism: the arms agree on every defect visible in the diff and separate on `h06`, whose deciding predicate lives in an untouched file. Evidence accuracy 1.000 (23/23) on files never seen before. | Kept. See [`holdout.md`](holdout.md). |
| **"Zero false positives on traps" did not survive** ❌ | The frozen benchmark's four traps are clean in all five trials. The held-out set has two. | **Both arms produced a false positive on `h04`.** The advanced trajectory fails in this project's *own documented main failure mode*: the falsification question targeted the mechanism (*"does `resolve` deduplicate before returning?"*) rather than the precondition, and the investigation ran `list_files`, saw `src/graph.rs` — which holds the only constructor, the one that rejects the graph shape the claim needs — and **stopped without opening it, with four of eight tool calls unspent**. | **Not patched.** A prompt change written against a case we just watched fail is the overfitting the ablation ladder exists to catch, and it would destroy the only property that makes a held-out set worth having. The README's trap claim is revised instead. |
| **By-hand audit of every true positive** | Location-plus-category matching is a proxy for "found the defect" and can credit a claim that lands on the right lines for the wrong reason. No deterministic matcher can tell; a model judge is forbidden here. | All 40 matches in the 5-trial run read against ground truth, raw text published in [`matching-audit.md`](matching-audit.md). **7 of 8 defects described exactly in all five trials; `c12` hedged** (right failure, cause stated conditionally). **The matcher did produce one spurious match — on the Python pilot**, where `p03` scored a true positive for a claim about float indices that landed on the real defect's three lines under an `also_accept` category. | Audit published. `c12` still counted as a true positive, with the hedge stated. The pilot's spurious match is reported where the pilot numbers are. |

---

## Sprint 4e — checking which knob is dead

Diagnosing the held-out `h04` false positive raised the obvious question: was
the investigation cut short? Measured across every candidate adjudicated on all
three benchmarks, from artifacts already recorded, with no model called.

| Stage | What was tried and why | Evidence | Decision |
|---|---|---|---|
| **Does the tool budget bind?** | The reflexive fix for a missed lookup is a bigger budget. Before touching it, count how often the existing one is reached. | **86 candidates adjudicated, 86 used at least one tool, 0 exhausted the 8-call budget.** The most any candidate used is 6; 57 of 86 (66%) stop after one or two calls. | **Budget left alone.** It has never been the binding constraint on any benchmark. Raising it would change nothing, and doing so would have looked like a fix while altering no behaviour. |
| **Is "listed files, opened none" a failure signal?** | The `h04` trajectory ran `list_files`, was shown the file holding the answer, and stopped. Tempting to treat that shape as a defect detector. | It occurs **7 times in 86 investigations**. On the frozen benchmark the other 6 all reached the correct answer — five trap candidates rejected, one real defect confirmed. | **Not claimed as a predictor.** One failing instance out of seven is a story, not a signal. Recorded with its denominator. |

**What this changes about the diagnosis.** The limiting component is not the
tool budget, the sandbox, or the search implementation — it is the model's own
judgement that it has seen enough. That is a narrower and more useful target
for a follow-up than "investigate harder", and it is the kind of claim that
only exists because the number was checked before the knob was turned.

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
| `results-trials/t1..t3/` | 12 | v5, 3 trials per arm | 0.857 | 0.917 mean |
| `results-trials/t1..t3/` | 12 | ablation: no falsification ❌ | 0.857 | 0.725 mean |
| `results-trials-v6/t1..t3/` | 12 | v6, assumptions about unseen code | 0.857 | 0.961 mean |
| `results-trials-v6/t1..t3/` | 12 | ablation: candidates only ❌ | 0.857 | 0.787 mean |
| `results-final/t1..t15/` | 12 | **final: v6 + dedup + memory, 15 trials** | **0.857** | **0.992 mean** |
| `results-final/t1..t3/` | 12 | ablation: no falsification ❌ | 0.857 | 0.828 mean |
| `results-final/t1..t3/` | 12 | ablation: candidates only ❌ | 0.857 | 0.742 mean |
| `results-pilot/` | 3 | Python pilot, first three cases (superseded by the run below) | 0.000 | 0.500 |
| `results-pilot/` | 6 | Python pilot, expanded and re-run (separate benchmark) | 0.667 | 0.857 |
| `results-sonnet/` | 12 | cross-model: baseline on Claude Sonnet 5 | 0.857 | — |
| `results-secondlook/` | 12 | second look enabled, single trial | — | 1.000 |
| `results-pilot-secondlook/` | 6 | Python pilot, second look enabled | — | 0.800 |
| `results-holdout/t1..t6/` | 6 | **held-out benchmark, authored without sight of the reviewer, 6 trials** | **0.750** | **0.944 mean** |

Nothing has been removed from this table. The ❌ rows are changes that made the
system worse; they were reverted, refined, or — in the ablation's case — run
deliberately to find out what a stage was worth.

Two things this table records that a summary would hide.

**A single run flattered us once already.** `results/` produced 0.941 and was
quoted in earlier drafts of this document. Three trials of that same
configuration average 0.917, range 0.875–0.941 — the single run was the best of
three. Every headline figure here is now a mean over **fifteen** trials.

**And five was not enough either.** At n=5 this document said the advanced
arm's spread came from one case, `c03`. At n=15 a second case, `c08`, produced
a false positive too. The same mistake, one order of magnitude further along:
a conclusion drawn from the sample that produced it.

**The Python pilot appears twice, and both rows stay.** The first three cases
were re-executed as part of the six-case sweep, so the second row is a new
sample rather than a re-scoring of the first. The `p06` ground-truth anchor was
also corrected between them; under the anchor as originally authored the
six-case advanced figure is 0.571 rather than 0.857, and both numbers are
reported in [`pilot-python.md`](pilot-python.md).

**The cross-model row has no advanced figure, deliberately.** Only the baseline
was run on Sonnet 5; reproducing the advanced arm's multi-turn tool loop
outside the Rust orchestrator would mean re-implementing the thing under test.
See [`cross-model.md`](cross-model.md).

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
