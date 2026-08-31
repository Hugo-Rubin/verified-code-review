# The benchmarks, and what each one is for

There are now six case sets. They are not interchangeable, and the differences
between them turned out to matter more than the totals.

| Set | Cases | Author | Trials | What it is for |
|---|---:|---|---:|---|
| [`benchmark/cases`](../benchmark/cases/) | 12 | this project | 15 | The frozen benchmark. Every headline figure. |
| [`benchmark/holdout`](../benchmark/holdout/) | 6 | agent, blocklisted | 6 | Author-bias check. See [`holdout.md`](holdout.md). |
| [`benchmark/holdout2`](../benchmark/holdout2/) | 5 | agent, blocklisted, theme: concurrency | 3 | Second author-bias check |
| [`benchmark/holdout3`](../benchmark/holdout3/) | 5 | agent, blocklisted, theme: error handling | 3 | Third author-bias check |
| [`benchmark/holdout4`](../benchmark/holdout4/) | 6 | agent, blocklisted, targeted at latent defects | 3 | Tests the claim's *condition* directly |
| [`benchmark/pilot-python`](../benchmark/pilot-python/) | 6 | this project | 1 | Language transfer. See [`pilot-python.md`](pilot-python.md). |

Every case in every set: the test suite **passes despite the defect**, ground
truth is **recorded as executed output** in its own `notes`, and
`scripts/verify_benchmark.py` passes on the directory.

---

## The result that matters most is a negative one

`holdout2` and `holdout3` were written by two further agents under the same
blocklist as `holdout` — no access to the prompts, the pipeline, the docs, or
any result — and were denied each other's work so they could not converge. Each
was given a theme and asked for the same mix as the frozen set, including one
Challenging case whose deciding evidence lives in a file the diff does not
touch.

Three trials per arm:

| | `holdout2` baseline | `holdout2` advanced | `holdout3` baseline | `holdout3` advanced |
|---|---:|---:|---:|---:|
| Precision | 1.000 ± 0.000 | 1.000 ± 0.000 | 1.000 ± 0.000 | 0.867 |
| Recall | 1.000 ± 0.000 | 1.000 ± 0.000 | 1.000 ± 0.000 | 1.000 ± 0.000 |
| **F1** | **1.000 ± 0.000** | **1.000 ± 0.000** | **1.000 ± 0.000** | **0.926 ± 0.064** |

**The baseline scores a perfect 1.000 on both, in every trial.** The advanced
arm ties on one and **loses** on the other, fooled by the `m04` trap in 2 of 3
trials into reporting that a query could exceed `MAX_PAIRS` — which execution
disproves: the densest legal query yields 64 pairs against a cap of 100.

So on ten cases written specifically to be independent of us, the advanced
pipeline bought **nothing**, cost three times as much, and introduced a false
positive in two trials of three.

### Why, and why it is the most useful thing here

Because these ten cases do not test what the system is for. Look at what the
baseline said about the two cases their authors designated Challenging:

> **`k05`** — *"Holding an immutable RefCell borrow across visitor calls and
> recursion causes a runtime panic if the visitor mutates the node."*
>
> **`m05`** — *"Using `unwrap_or_default()` sets missing or unparseable burst
> values to 0 instead of DEFAULT_BURST."*

Both correct. Both reached **from the diff alone**, without opening the
untouched file that supposedly held the deciding evidence. The changed line is
a *recognisable smell* — holding a `RefCell` borrow across a callback,
`unwrap_or_default()` on a value with a meaningful default — and a competent
reviewer flags the shape without needing to confirm the consequence.

That is the distinction the frozen benchmark's hard cases are built on and
these are not:

- **`c12`** — the changed line is `if index >= store.len() { return None }`.
  That reads as *correct*. Only `Store::len` returning capacity makes it wrong,
  and that is in a file the diff never touches.
- **`c03`** — an `unwrap` with a doc comment explaining why callers guarantee
  it is safe. The comment is false, and only the call sites show it.
- **`h06`** — an inlined `severity > 7` replacing a predicate defined as
  `>= PAGE_THRESHOLD`. The inlined line is unremarkable; the off-by-one exists
  only relative to the definition it replaced. The baseline missed this in
  **6 of 6** trials.

**A defect is only hard when the changed line looks fine.** "Evidence lives in
another file" is not sufficient — if the diff itself is suspicious, a reviewer
gets there by pattern recognition and the investigation is redundant.

Two independent agents were explicitly asked for a case of that shape and
neither produced one. That is worth knowing on its own: it is genuinely
difficult to construct, which is some evidence that the frozen benchmark's hard
cases are not trivial constructions.

### What this does to the headline claim

It sharpens it rather than contradicting it. The claim was never "this beats a
direct reviewer on all code review"; the ladder and the by-category table have
always shown the gain concentrated in the challenging cases. These ten cases
put a number on the other half:

> **When a defect is legible in the diff, the advanced pipeline adds cost and
> risk and nothing else.** Its entire advantage is on defects whose changed
> line reads as correct, and it should be run where that is expected —
> unfamiliar code, cross-module changes, contract edits — not on everything.

The frozen benchmark holds 2 such cases in 12; `holdout` holds 1 in 6, and the
advantage replicates there (baseline 0.750, advanced 0.944 over 6 trials).
`holdout2` and `holdout3` hold none, and it does not.

### What we did not do

We did not rewrite `k05` or `m05` into harder cases after seeing this. They
were authored independently and are reported as authored; editing a held-out
case because the result was inconvenient would destroy the only property that
makes it worth having. They stand as a measurement of the boundary of the
claim.

We also did not drop them for being "bad cases". They are perfectly good cases
of the kind a real reviewer meets most of the time, and the fact that a
one-call baseline handles them for a third of the cost is a genuine finding
about when to reach for this system.

---

## `holdout4`: the same question asked properly, and a harder answer

`holdout2` and `holdout3` failed to test the mechanism because their defects
were legible in the diff. So a fourth agent was commissioned with a spec that
never mentions this reviewer — only a property of the code:

> The changed hunk, read entirely on its own, must be consistent with **correct
> behaviour**, because some plausible implementation of the referenced-but-unseen
> code would make it fine.

Each case has to state that explicitly: *"If `<unseen thing>` were
`<alternative>`, this change would be correct. It is not, because `<actual>` at
`<file:line>`, which the diff does not touch."* Four Challenging cases, two
Traps.

**Selecting for this shape measures whether the claim is true, not how often
the condition arises.** `holdout2`/`holdout3` measure the complement. The three
sets are only honest read together.

### Result — 3 trials per arm

| | Baseline | Advanced |
|---|---:|---:|
| Precision | 1.000 ± 0.000 | 0.917 ± 0.144 |
| Recall | **0.250 ± 0.000** | **0.417 ± 0.289** |
| **F1** | **0.400 ± 0.000** | **0.517 ± 0.202** |

Three things follow, and the second is the one that matters most.

**1. The shape is genuinely hard, and the spec worked.** The baseline scored
1.000 on `holdout2` and `holdout3`. Here it scores **0.400**, finding one real
defect in four. Whatever else is true, these cases are not solvable by reading
the diff.

**2. The advantage returns — and the headline recall does not.** Advanced beats
baseline by +0.117 F1 and +0.167 recall, so the condition really is what
governs whether this system helps. But its recall here is **0.417**, against
**1.000 ± 0.000** on the frozen benchmark. On hard cases this project's author
did not write, the system finds fewer than half the defects.

> The frozen benchmark's perfect recall is a property of *those twelve cases*,
> not of the method. This is the single most important qualification in the
> repository, and it exists only because someone else wrote the cases.

**3. Both traps stayed clean**, in both arms, in all three trials — unlike
`holdout`, where two traps produced false positives.

### Per case

| Case | Baseline | Advanced |
|---|---|---|
| `n01-log-capacity-guard` | missed ×3 | **missed ×3** |
| `n02-batch-summary-counters` | **found ×3** | found ×1 — *regression* |
| `n03-editor-dirty-check` | missed ×3 | found ×1 (+1 FP) |
| `n04-limiter-snapshot-limit` | missed ×3 | **found ×3** |
| `n05-frame-header-slice` (trap) | clean ×3 | clean ×3 |
| `n06-catalog-binary-search` (trap) | clean ×3 | clean ×3 |

`n04` is the clean win the design predicts: a limit snapshotted at construction
from a `Settings` that has interior mutability and an untouched caller that
changes it. The baseline never got it; the advanced arm got it every time.

### The most instructive failure in this project

`n02` is a **regression**: the baseline found it in all three trials, the
advanced arm in one. The defect is that `Summary` reads
`counters.accepted()`, which is a destructive `swap(0)` — so the summary is
right and the metrics scrape that follows sees zero.

Here is what the advanced arm actually did. It proposed:

> *"`finish_batch` reads cumulative accepted counts from counters without
> resetting or tracking a per-batch baseline, causing subsequent batch
> summaries to report counts accumulated across prior batches."*

That is the real defect **stated backwards**. It then investigated — reading
`counters.rs` and even `telemetry.rs`, where the consequence lands — and the
verifier correctly refuted it:

> *"`Counters::accepted()` performs an atomic swap to zero
> (`self.accepted.swap(0, Ordering::SeqCst)`), resetting the counter
> destructive upon read and returning only the count since the previous read."*

**The verifier names the destructive read, in its own words, and uses it to
reject the finding.** The one fact that proves the defect is the same fact that
refutes the claim as posed, and the pipeline discards it and moves on.

That sharpens the hot take considerably:

> Falsification refutes the claim you gave it. If the claim points the wrong
> way, the evidence that kills it may be the very evidence that proves a real
> defect — and nothing in a refutation-shaped pipeline is looking for that.

The baseline has no falsification step to talk itself out of it, so it simply
reported what it saw. **A verification stage can cost you a finding a direct
pass would have kept**, and that is not a hypothetical: it is one case in four,
measured, on cases we did not write.

### What we did not do

We did not rewrite `n01` or `n02`, and we did not tune anything against them.
The set was authored independently, verified by execution here, run once at
three trials, and reported. Its author flagged `n04` as the weakest on the
"opaque from the diff" axis before any of it was run; that judgement is
recorded in the case notes and turned out to be the case the system handled
best.

---

## Reproducing any of them

```bash
cargo run --quiet --bin vcr -- run --agent advanced --benchmark benchmark/holdout2 --out results-holdout2/t1
```

```bash
python scripts/verify_benchmark.py benchmark/holdout2
```

`--language python` for the pilot. Full instructions in
[`reproduction.md`](reproduction.md).
