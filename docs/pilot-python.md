# Python pilot

**This is a pilot, not a result.** Three cases, one run, one model. Nothing
here appears in any headline figure, and it is not evidence that the reviewer
works on Python in general.

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

So two of the three cases are deliberate analogues of Rust cases whose
behaviour we already understand.

## The cases

Located in [`../benchmark/pilot-python/`](../benchmark/pilot-python/), in the
same layout as the Rust benchmark. Each is a small package whose test suite
passes **despite** the defect.

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

## Ground truth, verified by execution

As with the Rust benchmark, no ground-truth claim here rests on reading the
code. Each was executed:

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
```

Test suites: p01 3 passed, p02 4 passed, p03 5 passed — every one of them
green with the defects in place.

## What the pilot does not establish

- **Three cases prove nothing statistically.** One trap and two
  context-dependent cases, single run.
- **No Python-specific tooling exists.** No `pytest` execution, no AST or
  call-graph analysis. The reviewer uses the same literal search and bounded
  reads it uses for Rust.
- **The case set is biased toward what we already know works.** Two of three
  are ports of Rust cases the system handles. That is deliberate — it isolates
  language transfer from case difficulty — but it means the pilot cannot
  discover Python failure modes we did not think to write.
- **Idiomatic Python defects are absent.** Mutable default arguments, late
  binding in closures, `__eq__`/`__hash__` mismatches, generator exhaustion,
  and the many ways `asyncio` goes wrong are all untested.

A real Python capability claim needs a Python benchmark built the way the Rust
one was — a dozen cases, execution-verified, frozen, with traps designed
around Python's own failure modes. This pilot is one afternoon's evidence that
the architecture does not obviously fall over, and nothing more.

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
accordingly; nothing else in the pipeline changes.
