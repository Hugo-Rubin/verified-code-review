# Cross-model check

"One model, one provider" was a stated limitation of this evaluation. The
obvious worry: the baseline misses both context-dependent defects, and the
whole argument rests on that — so is it a property of the *setup*, or just of
Gemini being conservative?

This answers that for the baseline arm, and the answer is unusually clean.

## Result

| | Gemini 3.7 Flash | Claude Sonnet 5 |
|---|---:|---:|
| Precision | 1.000 | 1.000 |
| Recall | 0.750 | 0.750 |
| **F1** | **0.857** | **0.857** |
| False positives/case | 0.00 | 0.00 |

Not just the same aggregate — the same **cases**:

| Case | Category | Gemini TP/FP/FN | Sonnet TP/FP/FN | Agree |
|---|---|---|---|---|
| c01-pool-counter-leak | RealIssue | 1/0/0 | 1/0/0 | yes |
| c02-shard-index-trap | Trap | 0/0/0 | 0/0/0 | yes |
| c03-session-touch-context | Challenging | 0/0/1 | 0/0/1 | yes |
| c04-counter-lost-update | RealIssue | 1/0/0 | 1/0/0 | yes |
| c05-timeout-parse-default | RealIssue | 1/0/0 | 1/0/0 | yes |
| c06-dedup-quadratic | RealIssue | 1/0/0 | 1/0/0 | yes |
| c07-report-unflushed-writer | RealIssue | 1/0/0 | 1/0/0 | yes |
| c08-order-name-limit | RealIssue | 1/0/0 | 1/0/0 | yes |
| c09-queue-pop-guarded-trap | Trap | 0/0/0 | 0/0/0 | yes |
| c10-chunk-total-overflow-trap | Trap | 0/0/0 | 0/0/0 | yes |
| c11-asset-path-check-trap | Trap | 0/0/0 | 0/0/0 | yes |
| c12-slot-guard-capacity | Challenging | 0/0/1 | 0/0/1 | yes |

**12 of 12 per-case agreement.** Both models found the same six defects, stayed
clean on the same four traps, missed the same two challenging cases, and chose
the *same* `issue_type` on all six matches.

## What this establishes, and what it does not

**Establishes:** the baseline's blind spot is structural, not a quirk of one
model. Two different model families, given byte-identical inputs, fail on
exactly the cases whose deciding evidence lives in a file the change does not
touch — and neither invents anything on the traps. A diff-only reviewer cannot
resolve what is not in the diff, and being a better model does not help.

That is the premise the whole project rests on, and it is no longer resting on
one vendor.

**Does not establish:** anything about the *advanced* arm across models. Only
the baseline was run on Sonnet. The advanced arm is a multi-turn loop with tool
calls, and reproducing it faithfully outside the Rust orchestrator would mean
re-implementing the thing under test — at which point a difference in results
could come from the re-implementation rather than the model. That would be a
worse experiment than none.

So the honest scope is: **the problem generalises across models; the solution
has only been measured on one.**

## Method

The comparison is only meaningful if the second model gets *exactly* the same
task. Three things make that true:

**1. Byte-identical prompts.** The baseline is a single stateless call, and
every trajectory records the full system and user text verbatim. Those were
exported rather than regenerated:

```bash
python scripts/export_baseline_prompts.py --run results --out /tmp/prompts
```

Twelve `<case>.system.txt` / `<case>.user.txt` pairs, prompt version
`baseline-review/v2`, with the system prompt confirmed identical across all
cases.

**2. The same constraints.** The second model was instructed to treat each case
as independent, to use only its two prompt files, and specifically **not** to
read the benchmark directory, any `repository/`, any `ground_truth.json`, or
any existing results. It was also told plainly that several changes are
genuinely correct and that an empty answer is the right answer for those — so
it was not primed to expect a bug in every case.

**3. The same scoring.** Its answers are converted into ordinary run artifacts
and scored by the same deterministic evaluator:

```bash
python scripts/import_external_baseline.py \
    --responses /tmp/sonnet-responses --out results-sonnet --model claude-sonnet-5
```

```bash
cargo run --bin vcr -- evaluate --agent baseline --out results-sonnet
```

The import script's parser deliberately mirrors `agent::mod::parse_review`:
unknown `issue_type` values, line 0, and empty claims are dropped with a
recorded warning rather than coerced, so a model that ignores the schema is
penalised identically in both arms.

## Cost: an estimate, and why it is not in any results table

Sonnet's token usage was **not measured**. The run went through an agent
harness rather than the Rust Vertex client, so nothing recorded a
`usageMetadata` block, and `vcr evaluate` reports its cost as unavailable
rather than as `$0.00000`.

It can still be estimated, because both halves of the arithmetic are anchored
to something real:

| | Value | Basis |
|---|---:|---|
| Input text | 57,799 chars | identical bytes both models received |
| Input tokens | 18,530 | **measured**, by Gemini's tokenizer, on that exact text |
| Observed ratio | 3.12 chars/token | derived from the two rows above |
| Sonnet output text | 3,796 chars | the response files, exactly |
| Sonnet output tokens | ~1,217 | **estimated** at the ratio above |

At published Sonnet 5 rates ($2.00/Mtok in, $10.00/Mtok out):

| | 12 cases | Per case |
|---|---:|---:|
| Sonnet 5, standard | ~$0.049 | **~$0.0041** (estimate) |
| Sonnet 5, batch ($1/$5) | ~$0.025 | ~$0.0021 (estimate) |
| Gemini 3.7 Flash | $0.038 | **$0.00315** (measured) |

**Sonnet costs roughly 1.3× the Gemini baseline despite charging 2.7× the
price per token**, because it wrote far less: about 1,200 output tokens against
Gemini's measured 6,388 for identical work and an identical schema. Nearly all
of that difference is prose in the `reasoning` field. Both arms found the same
six defects, so the extra tokens bought nothing measurable here.

Three reasons this is an estimate and not a measurement, all of which could
move it:

1. **Tokenizers differ.** Applying Gemini's observed chars/token ratio to
   Sonnet is a proxy. Expect error on the order of ±15%, and in an unknown
   direction.
2. **Input tokens are Gemini's count**, reused because the text is
   byte-identical. Sonnet's own count for those bytes would differ.
3. **The harness is not the API.** Driving Sonnet through an agent consumed far
   more than a direct call would — it read files, carried its own instructions,
   and made tool calls. The figures above estimate an *equivalent direct API
   call*, which is the fair comparison, not what this particular run actually
   burned.

The estimate appears here and nowhere else. It is not in the README table, not
in `evaluation-*.json`, and not in any figure described as measured.

## Caveats worth stating plainly

- **Token counts are not recorded** for the external run. See the cost section
  above: the reported cost is unavailable, and the estimate is kept separate
  from every measured figure.
- **One run**, not three. The Gemini baseline was perfectly stable across three
  trials, so a single Sonnet run is a reasonable sample of a low-variance
  quantity — but it is still one run.
- **The second model was driven through an agent harness**, not the Rust Vertex
  client. Same prompts and same scoring, different transport. Temperature and
  sampling settings were not controlled the way they are for the Gemini arm.
- **Two models is not "models in general."** Both are current frontier models
  from 2026; an older or much smaller model might behave differently in either
  direction.

Raw artifacts: [`results-sonnet/`](../results-sonnet/), same layout as any
other run, including per-case trajectories.
