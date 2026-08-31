# Solution video — script and shot list

Target: **5:00 maximum.** This script is written to land near **4:45**.

Narration is synthesised locally (see [`../tools/tts/README.md`](../tools/tts/README.md)).
The spoken lines are the block quotes below; `scripts/extract_narration.py`
pulls them into `tools/tts/narration.txt`, so the narration cannot drift from
the script. Check the duration it prints **before** cutting.

```bash
python scripts/extract_narration.py
python tools/tts/narrate.py --script tools/tts/narration.txt --out tools/tts/audio
```

## Before recording

```bash
cargo run --quiet --bin vcr -- check
```

Have open: a terminal at the project root; `benchmark/cases/c12-slot-guard-capacity/`
(`diff.patch` and `repository/src/store.rs`); `docs/trajectories/c12-slot-guard-capacity-advanced.md`;
the README results and ablation tables; `docs/benchmarks.md`.

A live advanced run is ~40 s per case, so run the sweep beforehand and show
`results-final/`. `vcr replay-dedup` is instant and can be shown live.

---

## 0:00 – 0:40 · The problem

**Show:** `benchmark/cases/c12-slot-guard-capacity/diff.patch`.

> A developer reviewing a Rust pull request in code they didn't write. The
> question isn't "what looks suspicious" — it's "is this actually broken?"
>
> Here's a real case. A new endpoint, an explicit bounds check, a doc comment
> promising it returns None rather than panicking, three passing tests. It's
> broken.
>
> Nothing in that diff can tell you so. The guard compares against
> `store.len()`, and `len` returns capacity, not how many records are present —
> a fact that lives in a file the change never touches. The diff shows the
> suspicious line; the repository holds the verdict.

## 0:40 – 1:10 · The simple baseline

**Show:** `docs/trajectories/c12-slot-guard-capacity-baseline.md`.

> The baseline is one direct review pass. Same model, same schema, same view of
> the diff and every changed file. Only repository tools are withheld.
>
> It reports nothing — not reasoning badly, but reasoning correctly from
> insufficient information.
>
> Across twelve cases it scores F one of zero point eight five seven with
> perfect precision, identical in all fifteen trials, missing both cases whose
> evidence sits outside the diff.

## 1:10 – 2:20 · One execution, end to end

**Show:** `docs/trajectories/c12-slot-guard-capacity-advanced.md`, scrolling.

> Same case, the advanced reviewer. Four roles, each a separate stateless
> request.
>
> It proposes a candidate: `fetch` assumes every index below `len` is valid.
>
> Then, before gathering any evidence, it must write down what would prove it
> wrong: "Does the store guarantee every index below `len` is occupied?" A
> separate call on purpose — a question written after the verdict just
> rationalises it.
>
> It reads `store.rs`, the file the change never touched. Rust runs the tools
> and builds the evidence; the model can request a lookup, but cannot author an
> evidence item.
>
> Then the part that matters. Claim and evidence go to a **fresh context** — a
> stateless request that never sees the reviewer's reasoning, or that anything
> believed the claim. It answers from the text: `len` returns capacity, so an
> index below it can panic.
>
> And Rust assigns the final status, not the model. "Supports" with no
> repository evidence behind it becomes "uncertain" — because "the model said
> so" is the standard this project exists to reject.

## 2:20 – 3:00 · The comparison

**Show:** `cargo run --quiet --bin vcr -- report --out results-final`, then the ablation table.

> Twelve frozen cases, fifteen trials per arm. F one goes from zero point eight
> five seven to zero point nine nine two; recall from zero point seven five to a
> flat one point zero — every real defect, every trial — at one and a half cents
> a file.
>
> Now switch the stages off one at a time, and it gets uncomfortable. The
> advanced prompt alone scores zero point seven four two, worse than doing
> nothing clever. Add repository investigation: zero point eight two eight,
> still below the baseline. Only with falsification does it reach zero point
> nine nine two.
>
> Both halves are worse than the simple prompt you started with. Investigation
> buys recall; falsification makes that recall affordable. They're one
> mechanism, and if you can only ship half of it, ship neither.

## 3:00 – 3:40 · The changelog, and one experiment we removed

**Show:** `docs/improvement-changelog.md`.

> Every iteration is logged, including five that made things worse.
>
> Here's one we removed. The verifier had rejected a genuine panic because the
> function's own doc comment claimed callers check first — and that comment was
> false. So we told it comments aren't evidence. It then rejected two real
> defects, because the facts they rested on were also written in comments. F one
> dropped from zero point nine three three to zero point eight five seven, and
> we reverted it.
>
> And one we got wrong the other way. We'd reported three features as
> contributing nothing. Then we replayed one over every run this project has
> recorded: it fires seven times, and is **wrong every time**, merging two
> genuinely different defects because it borrowed a tolerance belonging to the
> scoring code. The test suite had been evidence *for* the bug — it asserted
> the bad merge was correct.

## 3:40 – 4:30 · Where it stops, and the hot take

**Show:** `docs/benchmarks.md`, then the false-positive examples.

> One more result, because it's the most useful one we have.
>
> Three separate agents wrote sixteen more cases, with no access to our
> prompts, our pipeline, or any result. On the first set the advantage
> replicates. On the other two it vanishes — the baseline scores a perfect one
> point zero, and our system loses on one of them.
>
> Those cases are legible in the diff: the changed line is a recognisable
> smell, so a reviewer flags it on sight. A defect isn't hard because the
> evidence sits in another file. It's hard when **the changed line looks
> correct**. That's where this earns its cost, and nowhere else. We didn't
> rewrite those cases afterwards.
>
> And the hot take. Falsification filters for truth, not significance — and
> most of what a reviewer should suppress is true. Our worst false positive was
> "this function returns Option but never returns None". Accurate. Not a bug.
> The verifier confirmed it correctly, because we had asked "is this true?" —
> almost never the question that matters.
>
> A verification step inherits whatever question you ask it. Ask the wrong one
> and it answers perfectly, and still hands a human noise.

---

## Recording notes

- Show real terminal output, not slides, wherever possible.
- Do not read tables aloud; let them sit on screen while the narration
  summarises the direction.
- Every number spoken here is in `results-final/` or `results-holdout*/`.
