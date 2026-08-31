# Python pilot

**This is a pilot, not a result.** Six cases, one run per arm, one model.
Nothing here appears in any headline figure, and it is not evidence that the
reviewer works on Python in general.

It exists to convert one claim from an assertion into a measurement. The
README states that the verification architecture is language-independent and
that only the prompts and the benchmark are Rust-specific. That was an
inspection of the module map. This is the smallest honest test of it.

## The question

Not "can it find Python bugs" — a competent LLM will find some. The question
is narrower and more useful:

> Does the **investigation behaviour** transfer? Specifically, does the
> reviewer still go and read a file the change did not touch, and does
> falsification still clear a plausible-looking finding on repository
> evidence, when the failure mode is an `IndexError` rather than a panic and
> "private" is a convention rather than a compiler rule?

So three of the six cases are deliberate analogues of Rust cases whose
behaviour we already understand, and three are defect classes the Rust
benchmark **structurally cannot contain**.

## The cases

Located in [`../benchmark/pilot-python/`](../benchmark/pilot-python/), in the
same layout as the Rust benchmark. Each is a small package whose test suite
passes **despite** the defect.

| Case | Category | Rust analogue |
|---|---|---|
| `p01-retry-swallows-failure` | Challenging | ported shape |
| `p02-primary-node-trap` | Trap | `c02-shard-index-trap` |
| `p03-len-is-capacity` | Challenging | `c12-slot-guard-capacity` |
| `p04-mutable-default-cache` | RealIssue | **none possible** |
| `p05-shared-config-trap` | Trap | `c02`/`c10` in shape only |
| `p06-generator-consumed-twice` | RealIssue | **none possible** |

### `p01-retry-swallows-failure` — Challenging

`Uploader.upload_chunk` stops raising `UploadError` when its retries are
exhausted and returns `None` instead, presented as an API simplification.

The defect is not visible in the changed file. `upload_all` appends the `None`
straight into its results, and `build_manifest` in an untouched module
dereferences `r.etag` on every receipt — so a permanently failing chunk
surfaces as `AttributeError: 'NoneType' object has no attribute 'etag'`, far
from the upload that actually failed. The bare `except Exception` also captures
the transport error into `last` and then discards it.

All three tests pass: `FlakyTransport` is never configured to fail more times
than `max_attempts` allows.

### `p02-primary-node-trap` — Trap (clean)

The direct analogue of `c02-shard-index-trap`. A new `status` module indexes
`nodes[0].name`, which reads as an obvious `IndexError` on an empty cluster.

It is unreachable: `Cluster.__init__` raises `ClusterError` on an empty
sequence, `_nodes` is private, `replace` swaps in place under a bounds check
and cannot shorten the list, and `nodes()` returns a copy so a caller mutating
the result cannot affect the cluster. All of that lives in a file the change
does not touch.

**An honest wrinkle with no Rust equivalent.** Python privacy is a convention,
so `cluster._nodes.clear()` really would work from outside the class. A
reviewer who reports the finding *specifically on that basis* has made a real
argument — and it still scores as a false positive under our ground truth,
because the case is asking whether the constructor invariant is recognised at
all. That scoring decision is recorded here rather than buried, and it is a
genuine limitation of porting a Rust trap to Python.

### `p03-len-is-capacity` — Challenging

The analogue of `c12-slot-guard-capacity`, and arguably nastier. `fetch`
guards with `index >= len(cache)`, which reads as obviously correct. But
`PagedCache.__len__` returns the configured *capacity*, not the number of
pages present, so on a partially filled cache any index between `filled` and
`capacity` passes the guard and raises `IndexError` inside `page_at`.

Overriding `__len__` to mean something other than "how many items are in here"
is a Python-specific footgun with no direct Rust equivalent. Every test builds
a cache with `n == capacity`, so `len()` and `filled` always coincide and the
suite never touches the failing configuration.

### `p04-mutable-default-cache` — RealIssue

`collect_enabled` took `into=None` and allocated a list when the caller did not
supply one. The sentinel branch is removed and the parameter now defaults to
`into=[]` directly — presented, accurately, as a simplification.

The canonical Python footgun, and included because Rust **has no mutable
default arguments at all**: this is a defect class the Rust benchmark cannot
express. The default is evaluated once at definition time, so every call that
omits `into` appends to one shared list. The docstring still promises "a fresh
list each call", so the file contains its own contradiction.

Exactly one test reaches the default path, and reaches it once. A second call
anywhere in the suite would expose it — a fair description of how this defect
survives review and then bites in production.

### `p05-shared-config-trap` — Trap (clean)

A new `client` module stamps a user-agent onto settings derived from a
module-level `BASE` dict. Shared mutable module state is a real and common
Python defect, and the change appears to write into it.

It does not. `with_overrides` calls `base_settings()`, which returns
`dict(BASE)` — a new dict — and every value in `BASE` is an immutable scalar,
so the shallow copy is a complete one. That last point is what makes the case
require *reading* rather than pattern-matching: a nested value would make the
same reasoning fail. The deciding facts live in `src/defaults.py`, which the
change does not touch.

### `p06-generator-consumed-twice` — RealIssue

`parse_records` stops building a list and returns a generator expression,
presented as a laziness optimisation for large inputs.

`summarise`, unchanged, iterates its argument twice — `sum(1 for _ in records)`
and then `[r.key for r in records]`. The first pass exhausts the generator, so
the second yields nothing. Callers get a correct count with an empty key list:
a silently wrong result rather than an error. Single-consumption of an iterator
has no Rust analogue that fails this quietly.

All three tests call `list(parse_records(...))` first, materialising the
generator before anything touches it.

## Ground truth, verified by execution

As with the Rust benchmark, no ground-truth claim here rests on reading the
code. Each was executed, and each was re-executed as a check before this
document was written:

```text
p01  a transport that always raises -> upload_chunk returns None
     upload_all -> [None, None]
     build_manifest -> AttributeError: 'NoneType' object has no attribute 'etag'

p02  Cluster([]) -> ClusterError
     after replace(), node count = 1
     after clearing the list returned by nodes(), node count = 1  (it is a copy)

p03  capacity=100, filled=3
     fetch(index=1)  -> page 1
     fetch(index=50) -> IndexError: list index out of range

p04  summarise([Flag("a", True)]) -> ['a']
     summarise([Flag("b", True)]) -> ['a', 'b']        <- the previous call's result

p05  BASE before -> {'retries': 3, 'timeout_secs': 30, 'verify_tls': True}
     build_client({'retries': 9})
     BASE after  -> {'retries': 3, 'timeout_secs': 30, 'verify_tls': True}   (unchanged)
     client.settings -> {'retries': 9, 'timeout_secs': 30, 'verify_tls': True,
                         'user_agent': 'vcr/1.0'}

p06  summarise(parse_records(LINES))       -> {'count': 3, 'keys': []}
     summarise(list(parse_records(LINES))) -> {'count': 3, 'keys': ['a','b','c']}
```

Test suites: p01 3 passed, p02 4 passed, p03 5 passed, p04 3 passed, p05 4
passed, p06 3 passed — every one green with the defects in place.

## Result

One run per arm, six cases. `gemini-3.7-flash`, temperature 0, same
configuration as the Rust benchmark.

| Metric | Baseline | Advanced |
|---|---:|---:|
| Precision | 1.000 | 1.000 |
| Recall | 0.500 | **0.750** |
| **F1** | **0.667** | **0.857** |
| False positives/case | 0.00 | 0.00 |
| Findings to triage/case | 0.33 | 0.50 |
| Evidence accuracy | n/a (0 citations) | **1.000** (51/51) |
| Cost/case | $0.0041 | $0.0243 |
| Runtime/case | 7.5 s | 63.6 s |

| Case | Baseline | Advanced |
|---|---|---|
| `p01-retry-swallows-failure` | missed | **found** (1 further candidate rejected) |
| `p02-primary-node-trap` | clean | **clean** — candidate rejected on evidence |
| `p03-len-is-capacity` | missed | **missed** (2 candidates, both wrong, both rejected) |
| `p04-mutable-default-cache` | found | **found** |
| `p05-shared-config-trap` | clean | **clean** — both candidates rejected on evidence |
| `p06-generator-consumed-twice` | found | **found** |

**The Rust pattern transfers.** The baseline missed both cases whose deciding
evidence sits outside the changed file, and found both whose defect is legible
in the diff — the same split it shows on the Rust benchmark, for the same
reason. The advanced arm recovered one of the two out-of-file defects and
cleared both traps.

**Falsification worked on Python, six times.** Across the run the verifier
rejected six candidates, every one on repository evidence from a file the
change does not touch: the `Cluster` constructor invariant on `p02`, the
`dict(BASE)` copy on `p05`, and on `p03` two plausible-but-wrong claims about
`page_at` returning `None` and about `cache` possibly lacking `__len__`. The
same reasoning that clears `c02` in Rust cleared its Python twin, against a
constructor invariant expressed as a raised exception rather than an `Err`.

**Zero false positives in either arm**, including on both traps. That is a
better precision result than the Rust benchmark produces, on a third as many
cases — read it as "the traps did not fool it", not as "precision is 1.000".

**Evidence citation works on Python.** All 51 cited excerpts were verified
against the repository at their stated line numbers, using the same
line-by-line audit as the Rust runs.

### `p03` was missed, and the trajectory says why

This is the most useful thing in the pilot, so it gets stated plainly. The
advanced arm did not miss `p03` because verification over-rejected. It missed
it because **candidate generation never proposed the defect at all**. The two
candidates it did propose were:

- `fetch_many` drops in-range slots because it filters on `p is not None`
- `fetch` will raise `TypeError` if some `cache` does not implement `__len__`

Both were investigated and correctly rejected. Neither is the bug. The real
claim — that `PagedCache.__len__` returns capacity rather than occupancy, so
the guard admits indices that are out of range — was never raised, so
falsification never had the chance to confirm it.

That matters because `p03`'s Rust twin `c12` failed in exactly this way, and
the `v6` prompt change ("where the change calls something whose definition is
not visible, state what it must do for the code to be right, and raise that as
a candidate") was written to fix precisely this shape. It took `c12` from found
in 1 trial of 3 to found in 5 of 5. **It did not transfer to `p03`.** One case
is not a diagnosis, but it is a concrete counter-example to any claim that the
v6 rule is language-independent, and it is the first place a Python follow-up
should look.

### A ground-truth correction, recorded rather than absorbed

`p06`'s expected finding was originally anchored at `summarise` (lines 24-28),
the *consumer* of the generator. That is a defensible place to fix the defect.
It was also the only ground-truth finding in the project — 1 of 18 — anchored
outside the hunk its case changes; every other case anchors at the changed
code.

The advanced arm reported the defect at lines 15-19, at the change, with a
fully correct diagnosis naming `summarise`, the double iteration and the empty
result, and was scored a **false positive plus a false negative** for it.

The anchor was moved to the changed lines, for consistency with the convention
the other seventeen follow. Both figures are reported:

| Advanced, 6 cases | Precision | Recall | F1 |
|---|---:|---:|---:|
| p06 anchored as originally authored (24-28) | 0.667 | 0.500 | **0.571** |
| p06 anchored at the changed lines (15-19) | 1.000 | 0.750 | **0.857** |

The baseline is **0.667 either way**: it located the defect at 22-27, which
overlaps both anchors within the matching tolerance, so its score does not
depend on the choice. That is the reason to report both numbers — the
correction moves one arm and not the other, which is exactly the shape a
convenient benchmark edit would have, and the reader should be able to see it
rather than take our word for the motive.

The correction was prompted by a *result*, not by review, which is the
dangerous direction. Two guards were added rather than a promise:
`bench::findings_outside_the_diff` reports any expected finding outside its
case's changed ranges, and `vcr check` prints it as a warning. Both benchmarks
are clean under it; that is how we know 17 of 18 already followed the
convention, rather than believing it.

## A second run, with the second look enabled

The pilot was re-run once with the "second look" feedback path switched on —
the pass that re-reads a case which finished with nothing to report, given the
repository facts that closed each rejected claim. This is the pilot's other
job: the frozen benchmark has no case the system misses, so the pilot is the
only place a recall feature could show a gain.

| Metric | Advanced, shipped config | Advanced, second look on |
|---|---:|---:|
| Precision | 1.000 | 0.667 |
| Recall | 0.750 | 1.000 |
| **F1** | **0.857** | **0.800** |
| Cost/case | $0.0243 | $0.0260 |

It fired on both traps — the only two cases that reported nothing. On `p02` it
**declined**, returning an empty list rather than manufacturing a claim, which
is the behaviour the prompt asks for. On `p05` it proposed:

> Passing a `user_agent` key in overrides to `build_client` has no effect
> because it is unconditionally overwritten with `'vcr/1.0'`.

That is **true**. It is also not a defect — stamping a fixed user-agent is what
the function is for — and the verifier confirmed it because it was asked
whether the evidence establishes a defect and the statement is accurate at the
level it operates on. This is the README's hot take recurring exactly: *most of
what a code reviewer should suppress is true.*

**The recall gain is not the second look's doing.** `p03` scored a true
positive in this run, but the second look never ran on `p03` — something was
reported there, so the trigger did not fire. And the finding that scored is not
the defect. See below.

Combined with the frozen benchmark, where the second look fired on exactly the
four traps and declined on all four, the measurement is: **six firings, five
correct declines, one invented finding, no recall gained anywhere, ~14% more
cost.** It ships off by default.

## The matcher credited a claim that is not the defect

`p03`'s true positive in the second-look run was:

> Non-integer float indices bypass the bounds check and get passed directly to
> `cache.page_at`.

The real defect is that `PagedCache.__len__` returns the configured capacity
rather than the number of pages present. The reported claim is about floats.
It scored a true positive because it landed on `src/api.py:9-11` — the same
three lines — under `Validation`, which that finding's `also_accept` list
allows.

So **the second-look run's recall of 1.000 is overstated**: on the defect the
case is actually about, `p03` was missed a third time. The figures in the table
above are the evaluator's output, reported unaltered, with this correction
stated next to them rather than folded in.

This is the only spurious match found anywhere in the project. Every one of the
40 matches in the five-trial headline run was read by hand against ground truth
and none is spurious — see [`matching-audit.md`](matching-audit.md), which
exists because of this case.

## What the pilot does not establish

- **Six cases prove nothing statistically.** Two traps, two out-of-file
  defects, two in-diff defects, single run per arm. No variance figure.
- **No Python-specific tooling exists.** No `pytest` execution, no AST or
  call-graph analysis. The reviewer uses the same literal search and bounded
  reads it uses for Rust.
- **Half the case set is biased toward what we already know works.** Three of
  six are ports of Rust cases the system handles. That is deliberate — it
  isolates language transfer from case difficulty — but it means those three
  cannot discover Python failure modes we did not think to write. The other
  three (`p04`, `p05`, `p06`) were written against Python's own footguns
  precisely to reduce that bias, and the system handled all three.
- **Idiomatic Python defects are still largely absent.** Late binding in
  closures, `__eq__`/`__hash__` mismatches, `__slots__` interactions,
  decorator ordering, and the many ways `asyncio` goes wrong are untested.
- **Same author as the reviewer.** The Rust benchmark carries this bias and so
  does this one. The [held-out benchmark](holdout.md) addresses it for Rust
  only; there is no independently authored Python set.
- **`p03` has now been missed three times**, once scoring a true positive for
  the wrong claim. Any future Python work should start there.

A real Python capability claim needs a Python benchmark built the way the Rust
one was — a dozen cases, execution-verified, frozen, repeated trials, with
traps designed around Python's own failure modes. This pilot is evidence that
the architecture does not fall over, and that one of its Rust-derived prompt
rules does not automatically carry across. Nothing more.

## Running it

```bash
cargo run --quiet --bin vcr -- run --agent baseline --benchmark benchmark/pilot-python --out results-pilot
```

```bash
cargo run --quiet --bin vcr -- run --agent advanced --benchmark benchmark/pilot-python --out results-pilot
```

```bash
cargo run --quiet --bin vcr -- evaluate --agent advanced --benchmark benchmark/pilot-python --out results-pilot
```

The reviewer reads each case's `language` field and addresses itself
accordingly; nothing else in the pipeline changes. The verifier is never told
what language it is looking at, and a test asserts its prompt names none.
