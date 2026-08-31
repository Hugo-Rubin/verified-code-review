# Solution video — script and shot list

Target: **5:00 maximum**. The plan below runs to 4:55, leaving margin.

Narration is synthesised locally — see [`../tools/tts/README.md`](../tools/tts/README.md).
The spoken text is extracted to `tools/tts/narration.txt`; render it with
`python tools/tts/narrate.py --script tools/tts/narration.txt` and check the
total it prints against the 300-second limit before cutting.

Everything on screen exists in the repository. No slides are needed beyond the
two tables, both of which the CLI prints.

## Before recording

```bash
cargo run --quiet --bin vcr -- check
```

Confirms config and that all 12 cases load. Then have these open in tabs:

1. Terminal at the project root.
2. `benchmark/cases/c12-slot-guard-capacity/diff.patch` and
   `benchmark/cases/c12-slot-guard-capacity/repository/src/store.rs`.
3. `docs/trajectories/c12-slot-guard-capacity-advanced.md`.
4. `docs/trajectories/c11-asset-path-check-trap-advanced.md`.
5. `docs/improvement-changelog.md` (ablation table).
6. `docs/holdout.md` (results table).
7. `README.md` at the anti-take section.

Because a live advanced run takes ~40 s per case, **run the sweep beforehand**
and show `results-final/` during the walkthrough, or run a single case live and
cut. `vcr replay-dedup --root .` runs instantly and calls no model, so it can
be shown live in the closing section.

---

## 0:00 – 0:35 · Problem and user

**Say:**

> A developer reviewing a Rust pull request in a codebase they didn't entirely
> write. The question they need answered isn't "what looks suspicious" — it's
> "is this actually broken?"
>
> And most of what makes a change dangerous isn't in the change. Whether an
> unwrap can fire depends on callers in files the diff never touches. The diff
> shows you the suspicious line. The repository holds the verdict.
>
> A false positive costs a full investigation to dismiss. A false negative
> ships. A tool that trades one for the other hasn't helped.

**Show:** `benchmark/cases/c12-slot-guard-capacity/diff.patch` — the new
`fetch` function with its bounds check.

> This is a real case from our benchmark. New endpoint, explicit bounds check,
> doc comment promising it returns None instead of panicking, three passing
> tests. It's broken.

---

## 0:35 – 1:05 · The simple baseline

**Say:**

> The baseline is one direct review pass. Same model, same output schema, same
> view of the diff and every changed file, and an explicit instruction not to
> speculate. The only thing it doesn't get is repository tools.

**Show:** `docs/trajectories/c12-slot-guard-capacity-baseline.md` — scroll to
the response.

```json
{ "findings": [] }
```

> Nothing. And that's not the model reasoning badly — it's reasoning correctly
> about insufficient information. `Store::len()` returns capacity, not the
> number of records present. That fact lives in a file this change doesn't
> touch, and the baseline never sees it.
>
> Across twelve cases the baseline scores F1 0.857 with perfect precision. It
> misses both cases whose evidence sits outside the diff.

---

## 1:05 – 2:25 · One realistic execution, end to end

**Show:** `docs/trajectories/c12-slot-guard-capacity-advanced.md`, scrolling
through the five stages.

> Same case, the advanced reviewer. Seven model calls, three tool calls,
> forty-seven seconds.

**Stage 1 — candidate:**

> It proposes: `fetch` assumes `record_at(index)` is valid for every index
> below `len()`, which fails if the store allows sparse slots.

**Stage 2 — falsification question:**

> Before gathering any evidence, it's made to write down what would prove it
> *wrong*: "Does the Store implementation allow deletions, vacant slots, or any
> index below `store.len()` to be unoccupied?"
>
> That's a separate call on purpose. A question written after the verdict just
> rationalises it.

**Stage 3 — investigation:**

> `search` for `struct Store`, then two bounded reads of `src/store.rs` — the
> file the change never touched. Rust runs the tools and builds the evidence;
> the model can request a lookup and read the result, but it cannot author an
> evidence item.

**Stage 4 — fresh-context verification:**

> The verdict comes from a separate stateless request that gets the claim and
> the excerpts and nothing else. Not the reviewer's reasoning. Not the fact
> that an earlier stage believed it. There's no conversation object that could
> leak it.

Read the verdict on screen:

> "`Store::len()` returns the configured capacity, whereas `record_at` indexes
> directly into `self.records`. Any index where `filled() <= index < len()`
> passes the guard and panics."

> Cited to three exact line ranges. It reached the ground-truth mechanism on
> its own.

**Stage 5 — decision:**

> Rust assigns the status, not the model. Supports plus real
> investigation-derived evidence equals Verified. Supports with nothing
> retrieved would be downgraded to Uncertain — "the model said so" isn't
> verification.

---

## 2:25 – 3:00 · Falsification killing a finding

**Show:** `docs/trajectories/c11-asset-path-check-trap-advanced.md`.

> The other half. This change deletes a path-traversal guard — the shape of a
> real vulnerability. The reviewer proposes it, exactly as it should.
>
> Then it investigates and disproves itself.

Read the verdict:

> "`asset_path` is crate-internal and all call sites pass fixed string literals
> from `AssetKind::file_name()`. No caller passes arbitrary string inputs,
> preventing directory traversal."

> Contradicts. Rejected. The developer never sees it — but the argument stays
> on the record. Five findings were investigated and cleared this way across
> the benchmark, one on each trap.

---

## 3:00 – 3:40 · The measured comparison

**Show:** live terminal.

```bash
cargo run --quiet --bin vcr -- report --out results-final
```

> Twelve frozen cases: six real defects, four traps designed to produce
> plausible false positives, two where the evidence lives outside the diff.
> Same model, same temperature, same cases for both arms.
>
> Five trials of each arm, because one run is a sample, not a measurement.
>
> F1: 0.857 to 0.988. Recall 0.75 to a flat 1.000 — the advanced arm found
> every real defect in every trial. One false positive in the entire five-trial
> run.
>
> The baseline was perfectly stable: identical on all twelve cases in all five
> runs, standard deviation zero on every metric. So the gap is not noise.
>
> Cost: a third of a cent per file, to one and a half cents. Just under five
> times more. That is the honest trade.

**Show:** the by-category table.

> All of the gain is on the challenging cases — the ones whose deciding
> evidence sits in a file the diff never touched.
>
> Scoring is fully deterministic. No LLM judges anything. Ground truth for
> every case was verified by *executing* it. And every citation the system
> produces is checked against the repository: two hundred and eighty-five cited
> excerpts across five trials, all of them correct.

---

## 3:40 – 4:05 · The ablation, and the claim it overturned

**Show:** the ablation table.

> We thought we knew which change mattered most. We wrote it down. We were
> wrong, and the ablation is what caught us.
>
> Switch falsification off, and leave everything else in place. F1 drops to
> 0.828 — **below the plain baseline of 0.857.** Take investigation away too,
> and it drops to 0.742. Worse again.
>
> Both intermediate versions of this system are worse than doing nothing
> clever. Investigation buys the recall; falsification is what makes that
> recall affordable. They are not two improvements to be ranked. They are one
> mechanism, and if you can only ship half of it, ship neither.

---

## 4:05 – 4:30 · The held-out benchmark

**Show:** `docs/holdout.md`, results table.

> Here is the problem with everything I have just shown you: I wrote the
> reviewer and I wrote the benchmark.
>
> So we had a separate agent write six more cases, with no access to the
> prompts, the pipeline, the documentation, or any result. It could not see
> what the system finds easy.
>
> The direction replicates. Baseline 0.750, advanced 0.889, separating on
> exactly the case whose evidence lives outside the diff.
>
> And it broke something we were claiming. On the frozen benchmark we report
> zero false positives on four traps across five trials. On the first trap this
> system had never seen, **both arms produced a false positive** — and the
> trajectory shows our own documented failure mode: the investigation listed
> the file holding the answer, saw it, and stopped without opening it, with
> half its tool budget unspent.
>
> We are not fixing that. Patching a prompt against a case we just watched fail
> is exactly the overfitting the ablations exist to catch, and it would destroy
> the only thing that makes a held-out set worth having.

---

## 4:30 – 4:55 · The anti-take, and limitations

**Show:** the anti-take table in the README.

> We built three features on good reasoning and measured all three as
> worthless. Then we checked properly, and "worthless" turned out to be the
> charitable reading.
>
> Deduplication had fired zero times, so we called it inert. But zero firings
> measures the *trigger*, not the feature. We replayed it over every run this
> project has ever recorded: it fires six times, and **every single firing is
> wrong** — merging two genuinely different defects, on two different fields,
> because it had borrowed the evaluator's three-line matching tolerance.
>
> It survived for two reasons. An unrelated prompt change had quietened the
> trigger. And the unit test written to prove it worked used that exact
> geometry and asserted the merge was correct. The test encoded the bug.
>
> The lesson we would carry: an unused branch is not neutral, it is unmeasured
> — and its next firing is the one nobody is watching.
>
> Limitations: twelve cases, five trials, one model. Search is
> literal-substring, so trait objects and macro-generated call paths are blind
> spots. Human review time is still a labelled proxy. Treat the direction as
> the result, not the third decimal place.

---

## Recording notes

- Increase terminal font size; the tables are the point.
- `vcr report` output is 7 lines — show it whole rather than scrolling.
- The rendered trajectories collapse prompts behind `<details>`; expand one
  briefly so viewers see the instructions are real, then collapse it.
- If showing a live run, use a single case:
  `cargo run --quiet --bin vcr -- run --agent advanced --benchmark benchmark/cases --out /tmp/demo`
  and cut the wait.
- Do not show `.env` on screen at any point.
