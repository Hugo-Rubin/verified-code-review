# Dogfooding: pointing the reviewer at this repository

Every number in this project comes from benchmarks we built. That is a fair way
to measure a change in behaviour and a poor way to know whether the thing is
*useful*, so `vcr review` exists: point it at a working tree and a diff and it
runs the same pipeline, prompts, sandbox and evidence gate that produced the
headline figures.

```bash
cargo run --quiet --bin vcr -- review --repo . --diff my-change.patch --out results-review
```

No ground truth, no score. The output is a report for a person.

Two runs are recorded here. Neither was chosen because it flattered the
system — the second one is a failure, and it is the more useful of the two.

---

## Run 1 — reviewing its own newest code

The commit that added `vcr review` itself, reviewed by the thing it adds.

**Result: nothing proposed.** One model call, no tool calls, $0.0295.

That is a weak outcome, and the run is included because it caught a real bug —
not in the code under review, but **in the report**. The renderer said:

> Nothing reported. That is a result, not a failure to run — every candidate
> below was investigated against the repository and ruled out.

No candidate had been investigated. None had been *proposed*. The report was
making a stronger claim about the system's diligence than the run supported,
which is precisely the failure mode this whole project is built to avoid, sitting
in our own output. The renderer now distinguishes the two silences:

> Nothing reported, and nothing proposed: the reviewer read the change and
> raised no candidate to investigate. That is not the same as having checked
> something and cleared it, and it is weaker evidence that the change is sound.

Two tests pin the distinction. It was found by running the tool on ourselves,
which is the argument for doing it.

---

## Run 2 — reviewing the commit that introduced our own worst bug

The stronger test. Commit `70315f1` added candidate deduplication, and with it
the defect described in the [changelog](improvement-changelog.md#sprint-4b--measuring-the-three-features-that-earned-nothing):
the merge rule reused `cfg.match_line_tolerance`, the evaluator's ±3 scoring
slack, to decide that two claims were **the same claim**. Replayed later over
every recorded run, that rule fires six times and is wrong every time, merging
two genuinely different defects in `c08`.

So: reconstruct the repository at that commit, hand the reviewer the diff for
the file the function lives in, and describe the change the way its author
did — including the sentence *"Overlap is tested with the evaluator's existing
line tolerance"*, which is the defect stated out loud.

```bash
git archive 70315f1 | tar -x -C /tmp/vcr-at-70315f1
git show 70315f1 -- src/agent/advanced.rs > /tmp/dedup-introduced.patch
cargo run --quiet --bin vcr -- review --repo /tmp/vcr-at-70315f1 --diff /tmp/dedup-introduced.patch ...
```

**Result: it did not find the bug.** 10 model calls, 6 tool calls, $0.0659.

It proposed one candidate — that `end_line - start_line` could underflow on a
malformed candidate — investigated it across files the diff does not touch, and
**correctly cleared it**:

> Candidate line ranges are sanitized both during review parsing and upon
> `Location` construction. In `parse_review`, `end_line` is explicitly
> constrained via `.max(raw.start_line)`, and `Location::new` enforces
> `start_line <= end_line` by swapping them if necessary, preventing any
> subtraction underflow in `deduplicate_candidates`.

That is the machinery working exactly as designed: a plausible claim, a
falsification question, evidence gathered from two files outside the diff, and a
rejection grounded in what the repository actually says. It is also, on this
run, the entire output. The real defect went unmentioned.

### Why it missed it, and why that is worth saying

The deduplication bug is not wrong *in the code*. Every line is correct: the
comparison is sound, the tolerance is a real configured value, the arithmetic is
safe, the tests pass. It is wrong in **what the constant means** —
`match_line_tolerance` exists to forgive an off-by-a-line in a location estimate
while *scoring*, and merging two claims is a different question that must not
borrow that slack.

Nothing in `advanced.rs` reveals that. You have to know why the evaluator has
slack in the first place, and then notice that a second component quietly
inherited a justification that does not transfer. No amount of reading call
sites gets you there, and our reviewer reads call sites.

This sharpens the failure mode already stated in the README:

> **The system finds defects that are wrong in the code. It does not find
> defects that are wrong in the intent behind a name.**

It is also a fair description of the limits of the whole approach, not just this
implementation. Falsification needs a claim that repository evidence can settle.
"This constant is being used for a purpose its definition does not support" is a
claim about *design intent*, and the repository does not contain intent — it
contains text. The thing that found this bug was not a reviewer; it was
`vcr replay-dedup`, running the rule against six months of recorded behaviour
and asking what it actually did.

### What we did not do

We did not tune anything so this case would pass. A prompt written to catch
"a constant borrowed from another subsystem" after watching this exact miss
would be overfitting to one observation, and it would very likely fire on every
shared constant in every codebase. The miss is recorded and left standing, in
the same way the `h04` held-out failure is.

---

## What dogfooding changed

| | |
|---|---|
| Bugs found in the code under review | 0 |
| Bugs found **in our own output** | 1 (the renderer's false claim) |
| Correct rejections on real code, on evidence from untouched files | 1 |
| Prompts tuned in response | 0 |

The honest summary is that `vcr review` demonstrably runs the full pipeline on
arbitrary repositories and produces evidence-backed output, and that on two real
changes from this project's own history it surfaced nothing a human needed. Two
runs is not a measurement of usefulness, and this page is not claiming one.
