# Reproduction guide

Written for someone starting from a clean machine with nothing installed.

By the end you will have run the baseline, run the agent solution, scored both
against the frozen benchmark with a deterministic evaluator, and printed the
comparison table from the README.

**Approximate cost:** one full sweep of both arms is ~132k input and ~38k
output tokens. At $0.75/Mtok in and $3.75/Mtok out — the rates these results
were measured at — that is **$0.21**: $0.038 for the baseline and $0.176 for
the advanced arm. The optional 3-trial sweep is roughly $1.10.

**Approximate runtime:** ~11 minutes wall clock for both arms (3.5 min +
7.5 min), plus a first-time Rust build of ~1 minute. The 3-trial sweep takes
~46 minutes.

---

## 1. Prerequisites

| Tool | Version used | Notes |
|---|---|---|
| Rust | 1.98.0 (`cargo` 1.98.0) | Any 1.98+ toolchain. Edition 2021. |
| Git | 2.38+ | |
| Python | 3.10+ | Only for the two maintenance scripts in `scripts/`. Not needed to run the reviewer. |

Install Rust from <https://rustup.rs>:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

On Windows, download and run `rustup-init.exe` from the same page. **Open a new
terminal afterwards** so `cargo` is on `PATH`.

Verify:

```bash
cargo --version
```

## 2. Get the code

```bash
git clone https://github.com/Hugo-Rubin/verified-code-review.git
```

```bash
cd verified-code-review
```

## 3. Build

```bash
cargo build --release
```

The binary is `vcr`. Examples below use `cargo run --quiet --bin vcr --`, which
works from the project root without adjusting `PATH`.

## 4. Run the test suite

Nothing here needs an API key or network access — 196 tests covering the
metrics, the ground-truth matcher, benchmark loading, path-traversal
prevention, malformed-response handling, and the verification state machine.

```bash
cargo test
```

Expected: 193 passed in the library suite and 3 in the binary suite, 0 failed.

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: no output beyond `Finished`.

## 5. Configure model access

You need a Google Vertex AI credential for Gemini. Copy the template:

```bash
cp .env.example .env
```

Then edit `.env`. **`.env` is gitignored and must never be committed.**

### Minimum configuration

```bash
VCR_PROVIDER=vertex
VERTEX_MODEL=gemini-3.7-flash
VERTEX_AUTH=api_key
VERTEX_API_KEY=<your key>
```

An API key uses the project-less express endpoint. `VERTEX_PROJECT_ID` is
optional in that mode and is never written into any output artifact.

### Alternative: bearer-token auth

```bash
VERTEX_AUTH=access_token
VERTEX_PROJECT_ID=<your gcp project id>
VERTEX_LOCATION=global
VERTEX_ACCESS_TOKEN=<token>
```

Get a token with `gcloud auth print-access-token`. These expire after about an
hour. Setting `VERTEX_AUTH=gcloud` instead makes the client fetch one itself at
startup, which requires the `gcloud` CLI on `PATH`.

### Settings that affect results

These are the values used for the reported run. Changing any of them
invalidates comparison with the published numbers.

```bash
VCR_TEMPERATURE=0.0
VCR_MAX_OUTPUT_TOKENS=8192
VCR_MATCH_LINE_TOLERANCE=3
VCR_MAX_TOOL_CALLS_PER_FINDING=8
VCR_MAX_READ_LINES=200
VCR_MAX_SEARCH_RESULTS=40
```

### Rate limiting

```bash
VCR_MAX_RETRIES=5
VCR_MIN_REQUEST_INTERVAL_MS=1500
```

The advanced reviewer issues about seven model calls per case where the
baseline issues one, so it meets a per-minute quota roughly seven times sooner.
The default 1500 ms pacing was enough on a standard quota. **If you see
`LlmFailure` events in the trajectories, raise it** — an unpaced run degrades
only the advanced arm and produces an unfair comparison. This is not
hypothetical; it happened to us and is written up in the changelog.

### Cost reporting (optional)

```bash
VCR_PRICE_INPUT_USD_PER_MTOK=<your rate>
VCR_PRICE_OUTPUT_USD_PER_MTOK=<your rate>
```

Set **both or neither** — half-configured pricing is rejected at startup, and
absent pricing is reported as "unavailable" rather than as zero. Cost is
recomputed from recorded token counts at evaluation time, so you can add rates
later and re-run only `vcr evaluate`, with no further model spend.

## 6. Verify the setup

```bash
cargo run --quiet --bin vcr -- check
```

Expected:

```
config      OK
  provider  Vertex
  model     gemini-3.7-flash
  location  global
  auth      ApiKey
  pricing   $0.7500/Mtok in, $3.7500/Mtok out
  tolerance ±3 lines

benchmark   12 case(s) in benchmark/cases
  c01-pool-counter-leak    RealIssue  1 expected finding(s)
  ...
  c12-slot-guard-capacity  Challenging  1 expected finding(s)
```

`check` makes no model calls. If the benchmark section reports `BROKEN` for any
case, stop — do not run a sweep against a broken benchmark.

### Exercise the pipeline without spending anything

```bash
cargo run --quiet --bin vcr -- run --agent advanced --dry-run --out /tmp/vcr-dry
```

Uses a deterministic offline stub. It produces no findings and its output is
marked as a dry run so it can never be mistaken for a measurement. Useful for
confirming the plumbing works before spending tokens.

## 7. Required data

None external. The benchmark ships in `benchmark/cases/`, twelve
self-contained Rust crates. Each case directory holds:

```
case.json          agent-visible metadata (neutral description, category)
diff.patch         the change under review
repository/        the crate the agent may read — its sandbox root
_before/           pre-change state, used to regenerate diff.patch
ground_truth.json  expected findings — NOT reachable by the agent
```

Confirm the benchmark has not drifted:

```bash
python scripts/make_diffs.py --check
```

Expected: `all diffs match their before/after trees`.

## 8. Run the baseline

```bash
cargo run --quiet --bin vcr -- run --agent baseline --out results
```

~3.5 minutes, 12 model calls.

## 9. Run the agent solution

```bash
cargo run --quiet --bin vcr -- run --agent advanced --out results
```

~7.5 minutes, ~85 model calls. Progress prints per case.

## 10. Evaluate

Scoring is fully deterministic and makes no model calls.

```bash
cargo run --quiet --bin vcr -- evaluate --agent baseline --out results
```

```bash
cargo run --quiet --bin vcr -- evaluate --agent advanced --out results
```

## 11. Print the comparison

```bash
cargo run --quiet --bin vcr -- report --out results
```

## 12. Optional: the rest of the evaluation

### Repeated trials and variance

One run of an arm is a sample, not a measurement — LLM output is
nondeterministic even at temperature 0. This runs each arm several times and
reports the spread:

```bash
pwsh scripts/run_trials.ps1 -Trials 3 -Root results-trials
```

~55 minutes and roughly $1.10 at the rates in `.env`. It writes
`results-trials/t1/`, `t2/`, `t3/`, each with the same layout as a normal run,
then prints the summary. To re-print it later without re-running:

```bash
cargo run --quiet --bin vcr -- variance --root results-trials
```

The output names the specific cases whose true-positive count differed between
trials. That is more informative than a standard deviation: it is the
difference between "F1 moved a little" and "these two cases trade places".

### Ablations — what each stage contributes

```bash
cargo run --quiet --bin vcr -- run --agent advanced --ablation no-falsification --out results
```

| Ablation | What it switches off |
|---|---|
| `no-falsification` | The falsification question and the fresh-context verifier. Investigation still runs; any candidate with evidence is reported. |
| `no-followup` | The feedback loop only. An "Insufficient" verdict never triggers a second targeted look. |
| `candidates-only` | Investigation and verification both. Isolates the prompt from the machinery. |

Each writes `summary-advanced-<ablation>.json` and its own trajectory
directory, so an ablation can never overwrite a full run. Evaluate one with the
matching flag:

```bash
cargo run --quiet --bin vcr -- evaluate --agent advanced --ablation no-falsification --out results
```

### Measuring real human review time

The `Findings to triage/case` figure in the comparison table is a proxy. To
replace it with a stopwatch measurement:

```bash
cargo run --quiet --bin vcr -- triage --arms baseline,advanced --reviewer your-name --out results
```

Findings from both arms are pooled, shuffled, and shown one at a time with no
indication of which system produced them and no access to ground truth. You
decide `r` (real defect), `n` (not a bug), or `u` (unsure); open the repository
and read the code, because that reading time is the thing being measured.

Only the claim and its location are shown — not the gathered evidence, not the
verifier's verdict. That deliberately understates the advanced system's
benefit, and it is what keeps the arms indistinguishable. The limitation is
written into `results/triage-session.json` alongside the numbers.

Budget 20–40 minutes depending on how carefully you read.

### The Python pilot

A separate three-case benchmark demonstrating that the pipeline is not
Rust-specific. It is **not** part of any headline figure:

```bash
cargo run --quiet --bin vcr -- run --agent advanced --benchmark benchmark/pilot-python --out results-pilot
```

See [`pilot-python.md`](pilot-python.md).

## Expected output

A single run of each arm produces a table like this. Ours (the run stored in
`results/`) gave:

```
| Metric                       |   Baseline |   Advanced |     Change |
|------------------------------|------------|------------|------------|
| Precision                    |      1.000 |      0.889 |     -0.111 |
| Recall                       |      0.750 |      1.000 |     +0.250 |
| F1                           |      0.857 |      0.941 |     +0.084 |
| Evidence accuracy            |      0.000 |      1.000 |     +1.000 |
| Runtime/case (ms)            |       6208 |      38841 |     +32633 |
```

**Do not read that F1 as the result.** It is one sample, and it happened to be
the best of the three we ran. The reported figures are means over 3 trials:

| Arm | F1 mean | range | σ | cost/case |
|---|---:|---|---:|---:|
| Baseline | 0.857 | 0.857–0.857 | 0.000 | $0.0032 |
| Advanced | **0.917** | 0.875–0.941 | 0.036 | $0.0147 |
| Advanced, no falsification | 0.725 | 0.700–0.737 | 0.021 | $0.0108 |

Per category, full system, per trial:

| Category | n | Baseline TP/FP/FN | Advanced TP/FP/FN |
|---|---:|---|---|
| RealIssue | 6 | 6 / 0 / 0 | 6 / 0–1 / 0 |
| Trap | 4 | 0 / 0 / 0 | 0 / 0 / 0 |
| Challenging | 2 | 0 / 0 / 2 | 1–2 / 0 / 0–1 |

Token usage per 12-case sweep:

| | Input | Output |
|---|---:|---:|
| Baseline | ~18,500 | ~6,400 |
| Advanced | ~114,000 | ~31,000 |

### You will probably not match these exactly

LLM output is nondeterministic even at temperature 0. What should reproduce is
the *ordering*: the advanced arm ahead on recall and F1, both arms clean on all
four traps, the baseline missing both challenging cases.

Useful calibration from our three trials:

- The **baseline was perfectly stable** — identical on all twelve cases in all
  three runs. If yours varies, something differs in your configuration.
- The **advanced arm varied on exactly one case**, `c12-slot-guard-capacity`,
  found in 1 trial of 3. Expect that one to move.
- Treat a difference of one finding as noise. One finding is worth roughly
  0.03–0.06 F1 on a benchmark this size.
- Treat the advanced arm scoring *below* the baseline as a signal to check the
  trajectories for `LlmFailure` events before concluding anything. That is what
  rate limiting looks like, and it cost us an entire comparison once.

## What gets written

```
results/
  summary-baseline.json          per-case run stats, full configuration
  summary-advanced.json
  evaluation-baseline.json       scores, matches, misses, category breakdown
  evaluation-advanced.json
  trajectories/
    baseline/<case-id>-baseline.json
    advanced/<case-id>-advanced.json
```

Each trajectory carries the complete prompts sent, every tool call and its
verbatim response, the falsification question, the fresh-context verdict, the
orchestrator's decision and its reason, token usage, retries, runtime, and the
closing human checkpoint. See [`trajectories.md`](trajectories.md).

## Reproducing the intermediate experiments

Every configuration in the changelog is archived under `results-archive/`,
including the three that made results worse. To re-run one, check out the
commit that produced it and follow steps 8–11; the prompt version strings in
each trajectory (`fresh-verify/v3` and so on) identify which configuration
produced any given artifact.

## Troubleshooting

**`VERTEX_MODEL is required`** — `.env` is missing or not in the working
directory. Run from the project root.

**`VERTEX_AUTH=api_key requires VERTEX_API_KEY`** — the key is empty. A common
cause is a stray `#` at the start of the value, which `dotenvy` reads as a
comment and yields an empty string.

**HTTP 429 in trajectories** — quota. Raise
`VCR_MIN_REQUEST_INTERVAL_MS`. Do not compare arms across a rate-limited run.

**`no run found — expected results/summary-<agent>.json`** — `evaluate` was
called before `run`.

**`N case(s) failed to load`** — run `python scripts/make_diffs.py` to
regenerate any missing `diff.patch`.

## A note on scope

The reviewer never merges, rejects, modifies, or deploys anything. It reads
repositories under `benchmark/cases/*/repository/` and writes JSON under
`results/`. All filesystem access goes through a sandbox that rejects absolute
paths, parent traversal, and symlinks escaping the case root.
