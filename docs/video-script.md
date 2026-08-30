# Solution video — script and shot list

Target: **5:00 maximum**. The plan below runs to 4:50, leaving margin.

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
5. `docs/improvement-changelog.md`.

Because a live advanced run takes ~40 s per case, **run the sweep beforehand**
and show `results/` during the walkthrough, or run a single case live and cut.

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
cargo run --quiet --bin vcr -- report --out results
```

> Twelve frozen cases: six real defects, four traps designed to produce
> plausible false positives, two where the evidence lives outside the diff.
> Same model, same temperature, same cases for both arms.
>
> F1: 0.857 to 0.941. Recall: 0.75 to 1.0 — it now finds every real defect,
> including both the baseline missed. Precision drops from 1.0 to 0.889: one
> false positive, a design nitpick.
>
> It costs about six times the tokens and thirty extra seconds a case. That's
> the honest trade.

**Show:** the by-category table.

> All of the gain is on the challenging cases. Both arms are perfect on defects
> visible in the diff and clean on all four traps.
>
> And scoring is fully deterministic — no LLM judges anything. Ground truth for
> every case was verified by *executing* it.

---

## 3:40 – 4:20 · Changelog and the change that mattered most

**Show:** `docs/improvement-changelog.md`.

> The biggest single win wasn't the clever part. Early on, the advanced
> reviewer produced *zero* candidates on two of three cases — because both arms
> shared an instruction saying "an empty result is a correct answer." Right for
> a reviewer whose output is its report. Fatal for a stage feeding an
> investigator. Splitting that one instruction took recall from 0.5 to 1.0.
>
> Verification machinery is worthless downstream of a generator that's been
> told to keep quiet.

**Show:** the n=12 rows.

> Also worth saying plainly: on its first full twelve-case run, the advanced
> system **lost** — F1 0.667 against the baseline's 0.857. The three-case
> result that looked like a clean win didn't generalise. That run is in the
> repository.

---

## 4:20 – 4:40 · The experiment we removed

**Show:** the `fresh-verify/v4` row.

> One we took out. The verifier had rejected a genuine panic because the
> function's own doc comment claimed callers check first — the comment was
> false. So we told it comments aren't evidence.
>
> It recovered both challenging cases and then rejected two real defects,
> because the facts they rested on — a VARCHAR(64) column, production batch
> sizes — were also written in comments. F1 dropped from 0.933 to 0.857.
>
> The fix was narrower: a comment about something the repository can check is a
> claim, go read the call sites. A comment about the outside world is the best
> evidence you have.

---

## 4:40 – 4:50 · Hot take and limitation

> Our hot take: **falsification filters for truth, not for significance — and
> most of what a code reviewer should suppress is true.**
>
> Our two worst false positives were "this struct doesn't derive Clone" and
> "this function returns Option but never returns None." Both accurate. Neither
> a bug. The verifier confirmed them correctly, because we'd asked "is this
> true?" — and that is almost never the question that matters. We changed it to
> "does this establish a real defect", and both disappeared.
>
> Limitation: twelve cases, one run per arm, one model. Search is
> literal-substring, so trait objects and macro-generated call paths are blind
> spots. Treat the direction as the result, not the third decimal place.

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
