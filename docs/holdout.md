# The held-out benchmark

The frozen 12-case benchmark has a bias that no amount of care removes, and the
README has said so from the start: **the same person wrote the cases and the
reviewer.** Every prompt rule in this system was written after reading
trajectories from those cases. Even with the discipline of never naming a case
in a prompt — enforced by a test that fails the build if a review prompt
mentions a benchmark noun — the case set itself could still have been shaped,
unconsciously, around what the system already handles.

That is not a limitation you can argue your way out of. It needs different
cases, written by someone who cannot see the system.

## How independence was arranged

The six cases in [`../benchmark/holdout/`](../benchmark/holdout/) were authored
by a **separate agent** with an explicit blocklist. It was denied:

- `src/prompts.rs` — every prompt the reviewer uses
- `src/agent/**` — the pipeline itself
- `README.md`, `DECISIONS.md`, `docs/**` — all analysis of what the system
  finds easy or hard, including the changelog naming `c12` as the hardest case
  and `p03` as its unsolved Python twin
- every `results*/` directory — all trajectories and scores

It was given the case **file format** (`src/bench.rs`, `src/finding.rs`, one
existing case directory for layout, and the diff-generation script) and nothing
about the reviewer's behaviour. It never ran `vcr run`.

It was told to write against realistic Rust failure modes and to keep the
category mix of the frozen benchmark. It reported afterwards that it had read
`src/main.rs` lines 170–240 — outside its permitted list, though not on the
blocklist — to learn what `vcr check` reports as a warning. That file contains
no prompt, no pipeline logic and no result, so it does not compromise the
separation; it is recorded here rather than left out because the value of this
exercise is entirely in the honesty of its boundary.

### What this does and does not buy

**It removes the author's knowledge of the system from case design.** The
person who tuned the prompts had no hand in choosing these defects, and the
agent that chose them could not see what the prompts were tuned to catch.

**It does not make the cases independent of the model family.** The authoring
agent and the reviewer are both Claude-family models, and the reviewer under
test is Gemini. A case set written by an LLM may still favour defect shapes
that LLMs find natural to describe. Cases harvested from real pull requests
would be the stronger instrument; this is the strongest one available inside a
hackathon's time budget, and it is a genuine improvement on "the author wrote
both", not a solution to "synthetic benchmark".

**It is six cases.** Everything in the "twelve cases is small" limitation
applies here twice as hard.

## The cases

| Case | Category | Defect mechanism |
|---|---|---|
| `h01-registry-swap-remove` | RealIssue | `Vec::swap_remove` moves the last entry into the freed slot; the `name → index` map is not updated for the moved entry, so the map desyncs from the vector |
| `h02-status-class-guard` | RealIssue | a guarded arm `c if c >= 400` placed above `500..=599` makes the server-error arm unreachable — and a guard suppresses rustc's unreachable-pattern lint, so it compiles clean |
| `h03-cache-retain-polarity` | RealIssue | a removal condition copied verbatim into `Vec::retain`, which *keeps* what its predicate accepts; eviction is inverted |
| `h04-include-flatten-recursion` | Trap | recursion with no visited-set guard. Safe: the only constructor rejects cycles and caps the graph at 64 units |
| `h05-lease-early-return` | Trap | `?` replaces explicit `drop(lease)` bail-outs. Safe: `Lease` has a `Drop` impl in a file the change does not touch |
| `h06-digest-threshold-inline` | Challenging | an inlined `severity > 7` replacing `is_page_worthy`, which is `>= PAGE_THRESHOLD` where `PAGE_THRESHOLD == 7`; off by one exactly at the boundary, and the predicate it replaced is in an untouched file |

Note that two of the six turn on Rust-specific compiler behaviour — `h02` on
match-guard lint suppression, `h05` on `Drop` running at an early return. That
is a shape the original author did not use, which is some evidence the
independence did what it was meant to.

## Verification, done separately from the author

The authoring agent reported that it had verified everything by execution. That
report was not taken on trust — "I verified this" from a model is exactly the
standard this project exists to reject. Every claim below was re-derived here,
against the committed files:

- **All six suites pass with their defects in place**, and build with zero
  warnings. A defect an existing test already catches is not a review problem.
- **No `ground_truth.json` exists inside any `repository/`**, so the sandbox
  cannot serve answers to the reviewer.
- **Every `diff.patch` matches its before/after trees** (`make_diffs.py --check`).
- **`vcr check --benchmark benchmark/holdout` reports no anchoring warnings** —
  every expected finding falls inside its case's changed lines.
- **Descriptions are neutral.** All six read as a commit message with the
  author's stated rationale; the category cannot be guessed from the prose.

### Defects re-derived by execution

```text
h01  insert auth/billing/cache, then remove("auth")
     names      = ["cache", "billing"]
     addr_of("cache") = None            <- the map now points at nothing

h02  classify(404) = ClientError  retryable = false
     classify(500) = ClientError  retryable = false   <- expected ServerError / true
     classify(503) = ClientError  retryable = false   <- expected ServerError / true
     plan_retry(503, 0, 5) = None                     <- expected Some(..)

h03  put("expired", deadline 10); put("fresh", deadline 30); evict_expired(20)
     len = 1
     get("expired") = Some("stale")     <- should have been evicted
     get("fresh")   = None              <- should have survived

h06  PAGE_THRESHOLD = 7
     alert 2, severity 7 -> is_page_worthy = true
     digest.paging = [1]                <- alert 2 dropped; the inlined `> 7` excludes it
```

### Traps re-derived by execution

The direction that matters more: a "trap" that is actually unsafe would poison
the benchmark by scoring a correct finding as a false positive.

```text
h04  from_pairs(a -> a)               -> Cycle("a")
     from_pairs(a -> b -> a)          -> Cycle("a")
     from_pairs(a -> b -> c -> a)     -> Cycle("a")
     from_pairs(undeclared child)     -> UnknownInclude("c")
     deepest legal chain: 64 units    -> flatten returns 64 entries
     from_pairs(65 units)             -> TooManyUnits
     => the unguarded recursion cannot run away: no cycle survives construction
        and depth is capped at 64.

h05  2000 failing runs (1000 empty input, 1000 unparseable field)
       active = 0
     1 successful run -> Ok(6), active = 0, completions = [(2000, 6)]
     1000 further successful runs -> active = 0, completions = 1001
     => `?` returning early never leaks a lease; Drop runs on every path.
```

## Results

Not yet run. No figure from this benchmark appears anywhere in this repository
until it has been, and when it is, it will be reported whether or not it
flatters the system — a held-out set whose result is only published when it
agrees with the headline is not a held-out set.
