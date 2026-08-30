# Improvement Changelog

How the Verified Code Reviewer got from a direct-prompt baseline to its final
configuration, including the three changes that made it worse.

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

## Final comparison

Both arms, same model (`gemini-3.7-flash` via Vertex AI), temperature 0, same
session, frozen 12-case benchmark.

| Metric | Baseline | Advanced | Change |
|---|---:|---:|---:|
| Precision | 1.000 | 0.889 | −0.111 |
| Recall | 0.750 | **1.000** | **+0.250** |
| **F1** | 0.857 | **0.941** | **+0.084** |
| False positives/case | 0.00 | 0.08 | +0.08 |
| Findings to triage/case ¹ | 0.50 | 0.75 | +0.25 |
| Runtime/case | 6.2 s | 38.8 s | +32.6 s |
| Input tokens/case | 1,544 | 9,468 | ×6.1 |
| Output tokens/case | 532 | 2,598 | ×4.9 |
| Model calls/case | 1.00 | 7.08 | ×7.1 |
| Tool calls/case | 0.00 | 2.58 | — |
| Cost/case | see note ² | see note ² | — |

¹ A manual-triage proxy: how many findings a human must read and judge. **Not**
a direct measurement of human review time.

² Token counts above are measured. Cost is reported only when token rates are
configured in `.env`; the project does not invent prices. Once
`VCR_PRICE_INPUT_USD_PER_MTOK` and `VCR_PRICE_OUTPUT_USD_PER_MTOK` are set,
`vcr evaluate` recomputes cost from the token counts already recorded — no
re-run and no further spend required.

By case category:

| Category | n | Baseline TP/FP/FN | Advanced TP/FP/FN |
|---|---:|---|---|
| RealIssue | 6 | 6 / 0 / 0 | 6 / 1 / 0 |
| Trap | 4 | 0 / 0 / 0 | 0 / 0 / 0 |
| Challenging | 2 | 0 / 0 / 2 | **2 / 0 / 0** |

The whole gain is on the challenging cases — precisely where the hypothesis
said it should be. Both arms find every defect that is visible in the diff and
both stay clean on all four traps. The advanced arm additionally resolves the
two cases whose deciding evidence lives in a file the change does not touch,
and pays for it with one false positive: a design nitpick about the notes
length limit in c08.

---

## Which change contributed most

**Broadening candidate generation (A2), and it is not close.** Nothing
downstream can investigate a candidate that was never raised, and the original
advanced arm produced zero candidates on two of three seed cases. That single
change moved seed recall from 0.500 to 1.000.

But it is only safe because of what surrounds it. Broadening candidates on its
own floods the pipeline with plausible-looking claims. What makes the trade pay
is that five of them were investigated and **rejected on repository evidence**
in the final run — one on each of the four traps, plus an overbroad candidate
on c03 — so precision stayed at 0.889 rather than collapsing.

The honest framing: investigation supplies the recall, falsification pays for
it.

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
| `results/` | 12 | **final: v5, comments weighed by checkability** | **0.857** | **0.941** |

Nothing has been removed from this table. The three ❌ rows are changes that
made the system worse and were reverted or refined.

---

## Nondeterminism

LLM responses are not deterministic even at temperature 0. Benchmark inputs,
prompts, model configuration, tool limits, the evaluator, and the line
tolerance are all fixed, but repeated runs may produce small differences in
output and therefore in metrics.

Two observations from the runs above make this concrete. Between run 2 and run
3 the advanced arm's handling of c03 and c12 swapped: one run verified the c03
panic and missed c12, the next did the reverse. And run 2's advanced arm
rejected the correct c03 finding while an earlier n=3 run had verified it under
similar prompts.

The reported figures come from a single run of each arm, both executed in the
same session on the frozen benchmark. Multiple trials per configuration would
give error bars and were not run; that is a real limitation of this evaluation
and is stated as such rather than smoothed over.
