# Matching audit

Location-plus-category matching is a **proxy** for "found the defect". A claim
that lands on the right lines under an accepted category scores a true positive
whether or not it describes the actual bug, and no deterministic matcher can
tell the difference. Using a model to judge would reintroduce exactly the
standard this project exists to reject.

So the matches were read by hand, and the raw text is reproduced here so the
reading can be checked and disagreed with. It is reproducible without a model:

```bash
cargo run --quiet --bin vcr -- audit-matches --benchmark benchmark/cases --root results-final
```

That command pairs each scored true positive with the ground truth it was
credited for and prints both. It computes no verdict of its own — the reading
below is a person's, and yours may differ.

Scope. The headline run now holds **120 matches** (8 defects × 15 trials).
Read in full by hand: **all 40 matches from trials 1–5**, reproduced below, plus
**all 15 claims for `c12`** — the one case whose wording is not crisp. The
remaining matches are paraphrases of the same eight defects and were not each
read individually; `vcr audit-matches --benchmark benchmark/cases --root
results-final` prints all 120 for anyone who wants to.

**Outcome: 7 of 8 exact, 1 hedged, 0 spurious.**

**The hedge is stable at n=15.** Every one of the fifteen `c12` claims states
the cause conditionally — *"if slots can be uninitialized or sparse"*, *"if
`Store` supports sparse or tombstoned slots"*, *"if `store.len()` reflects
populated count rather than maximum index"* — and not one resolves into the flat
assertion that `Store::len` returns capacity. Ten trials more than the original
audit did not turn up a single crisp phrasing. That makes it a property of how
this system reports boundary-condition defects rather than an artefact of a
small sample, and it lines up with `h06` on the held-out benchmark, which hedges
the same way on the only other off-by-one defect in either case set.

| Defect | Verdict |
|---|---|
| `c01-f1` | EXACT |
| `c03-f1` | EXACT |
| `c04-f1` | EXACT |
| `c05-f1` | EXACT |
| `c06-f1` | EXACT |
| `c07-f1` | EXACT |
| `c08-f1` | EXACT |
| `c12-f1` | **HEDGED** |

---

## `c01-f1` — EXACT

**Ground truth** — `ResourceManagement` at `src/pool.rs:56-60`

> The early return on the exhausted path leaves the active counter incremented. `acquire` raises `*n` before testing it, but the `Err(PoolError::Exhausted)` branch returns without undoing the increment and no `Conn` is constructed, so nothing will ever decrement it. Each rejected acquisition therefore permanently inflates the counter, and after `max` rejections the pool reports itself exhausted forever even when every connection has been released.

**Reading.** Names the increment, the error return, and that nothing decrements it.

**Claims as reported:**

- *(t1)* When `Pool::acquire` fails due to capacity exhaustion, it returns `Err` without rolling back the increment to the active counter, permanently leaking pool permits.
- *(t2)* When `Pool::acquire` fails because the pool is exhausted, the active counter remains incremented, permanently leaking permits and rendering the pool unusable.
- *(t3)* Failed calls to `acquire` leave the `active` counter incremented without returning a `Conn` to decrement it, causing the counter to drift upward and permanently exhausting the pool.
- *(t4)* Failed calls to `Pool::acquire` increment the `active` counter without decrementing it on error, permanently leaking permits.
- *(t5)* Failed calls to `acquire` increment `active` without decrementing it upon returning an error, causing counter leakage.

---

## `c03-f1` — EXACT

**Ground truth** — `ErrorHandling` at `src/store.rs:53-55`

> The stated precondition does not hold for all callers, so the `unwrap` is reachable and will panic. `Server::on_heartbeat` in src/handler.rs calls `touch` with no `contains` check, unlike `Server::on_request` which does check. Because `Server::sweep` calls `expire_before` and can remove a session while its client is still connected and sending heartbeats, an id that was valid moments earlier can reach `touch` after removal. The previous no-op behaviour absorbed exactly this race; the change converts it into a panic that takes down the server.

**Reading.** Names the unguarded caller and the panic at `unwrap`.

**Claims as reported:**

- *(t1)* Existing callers that invoke `touch` with an unknown or expired session ID will panic at runtime on `.unwrap()`.
- *(t2)* Callers passing an unverified or expired session ID to `SessionStore::touch` will trigger a panic on `unwrap`, aborting the calling thread.
- *(t3)* Callers across the codebase invoke `touch` with an unverified or expired `SessionId` without a preceding `contains` check, causing an unhandled panic at runtime.
- *(t4)* Callers invoking `touch` on a missing or expired session id will panic via `unwrap()` rather than handling missing sessions safely.
- *(t5)* Calling `SessionStore::touch` with an unknown `SessionId` panics via `unwrap()`, terminating the process whenever a session is missing or has already expired.

---

## `c04-f1` — EXACT

**Ground truth** — `Concurrency` at `src/counters.rs:31-42`

> `record` acquires the mutex twice, so the read-modify-write is no longer atomic. The guard from the first `lock()` is a temporary that is dropped at the end of the statement that reads the current value, and a second `lock()` is taken to write it back. Two threads recording the same key can both observe the same `current` and both write `current + 1`, losing one increment. The previous entry-API version held a single guard across the whole update.

**Reading.** Names the two lock acquisitions and the lost update between them.

**Claims as reported:**

- *(t1)* Concurrent calls to `record` on the same key interleave between releasing the read lock and acquiring the write lock, resulting in lost counter increments.
- *(t2)* Concurrent calls to Counters::record for the same key can interleave between reading the existing count and writing the incremented value, causing lost updates and undercounting.
- *(t3)* Concurrent callers to `Counters::record` for the same key will interleave their read and write locks, causing counter increments to be overwritten and lost.
- *(t4)* Concurrent calls to Counters::record for the same key race on the separate read and write lock acquisitions, causing lost increments and inaccurate counts.
- *(t5)* Releasing and re-acquiring the mutex between reading and writing the counter value allows concurrent `record` calls on the same key to interleave and overwrite increments.

---

## `c05-f1` — EXACT

**Ground truth** — `ErrorHandling` at `src/config.rs:19-24`

> The parse failure falls back to 0, and 0 is not a neutral default in this crate - `NO_TIMEOUT` is documented ten lines above as meaning 'wait forever'. A typo such as `30s` or `thirty` therefore does not fall back to `DEFAULT_TIMEOUT_SECS`; it silently disables the request deadline entirely. The previous signature surfaced the malformed value as `ConfigError::NotANumber`.

**Reading.** Names the fallback to 0 and that 0 means no timeout rather than the default.

**Claims as reported:**

- *(t1)* Passing an invalid or unparseable timeout string causes request_timeout to return 0 (NO_TIMEOUT), silently disabling timeouts entirely rather than falling back to DEFAULT_TIMEOUT_SECS.
- *(t2)* Malformed timeout configuration strings fall back to 0 (unbounded timeout) rather than the default timeout or producing an error, unintentionally disabling request deadlines.
- *(t3)* Malformed timeout strings fall back to 0 seconds (unbounded timeout) instead of the default timeout, causing client requests to wait forever on configuration errors.
- *(t4)* An unparseable timeout string falls back to 0 seconds, causing invalid configurations to silently disable timeouts entirely instead of using DEFAULT_TIMEOUT_SECS.
- *(t5)* Malformed timeout configuration strings fall back to 0 seconds instead of DEFAULT_TIMEOUT_SECS, silently making requests wait indefinitely.

---

## `c06-f1` — EXACT

**Ground truth** — `Performance` at `src/dedup.rs:10-14`

> `Vec::contains` is a linear scan, so testing membership against the output vector inside the loop makes `unique_ids` quadratic in the number of distinct ids. The removed `HashSet` gave amortised constant-time membership, making the function linear. The module documents batches of 100_000 to 500_000 ids on the hot path, where the difference is between a linear pass and on the order of 10^10 comparisons.

**Reading.** Names the linear `contains`, the quadratic complexity, and the batch scale.

**Claims as reported:**

- *(t1)* Testing membership via `out.contains(&id)` degrades deduplication from O(N) to O(N^2) time complexity, causing massive latency and pipeline stalling on batches of 100,000 to 500,000 IDs.
- *(t2)* Replacing `HashSet` with `out.contains(&id)` makes `unique_ids` execute in O(N^2) time instead of O(N), causing severe latency stalls on batches of 100,000 to 500,000 IDs.
- *(t3)* Replacing the HashSet lookup with linear search via `out.contains` makes `unique_ids` run in O(N^2) time, causing severe latency spikes and ingest processing stalls on batches of 100,000 to 500,000 ids.
- *(t4)* Testing membership via `out.contains(&id)` degrades deduplication from O(N) to O(N^2) complexity, causing severe processing latency on batches of 100,000 to 500,000 IDs.
- *(t5)* Replacing the hash set lookup with `out.contains(&id)` makes deduplication O(N^2), causing unacceptable latency on ingest batches of 100,000 to 500,000 IDs.

---

## `c07-f1` — EXACT

**Ground truth** — `ErrorHandling` at `src/report.rs:15-24`

> `BufWriter` does flush on drop, but its `Drop` impl has no way to report failure and discards the error. Removing the explicit `flush()?` means an I/O failure while writing out the final buffered chunk - a full disk, a quota, a broken pipe - is silently swallowed, and `write_report` returns `Ok(())` after producing a truncated file. The `?` operators inside the loop do not cover this: they only see errors for data that has already left the buffer.

**Reading.** Names the dropped flush error and the `Ok(())` returned despite data loss.

**Claims as reported:**

- *(t1)* Removing explicit flush causes I/O errors that occur while flushing remaining buffered data on drop to be silently ignored, returning Ok(()) to callers despite data loss.
- *(t2)* write_report silently ignores I/O errors when flushing remaining buffered data on drop, leading callers to receive Ok(()) even when the write fails or the file is truncated.
- *(t3)* Removing explicit flushing causes I/O errors occurring during the final flush to be silently swallowed on Drop, leading callers to receive Ok(()) despite data loss.
- *(t4)* Removing explicit flush causes I/O errors that occur while flushing remaining buffered data on drop to be silently ignored, returning Ok to the caller despite incomplete or failed writes.
- *(t5)* Removing explicit flush causes any I/O errors that occur while flushing remaining buffered data on drop to be silently ignored, returning Ok(()) despite incomplete or failed file writes.

---

## `c08-f1` — EXACT

**Ground truth** — `Validation` at `src/order.rs:26-28`

> The name-length check now compares against `MAX_QUANTITY` (1000) instead of `MAX_NAME_LEN` (64). Names between 65 and 1000 characters pass validation and reach the database, where the doc comment on `MAX_NAME_LEN` states the `orders.name` column is `VARCHAR(64)` and will reject them at insert time. The intended limit is unenforced and the failure moves from a clean `ValidationError::Name` to a database error at write time.

**Reading.** Names the wrong constant, both values, and the failure at insertion.

**Claims as reported:**

- *(t1)* Validation checks order.name against MAX_QUANTITY instead of MAX_NAME_LEN, allowing names up to 1000 bytes to pass validation and fail at database insertion time.
- *(t2)* validate checks order.name against MAX_QUANTITY instead of MAX_NAME_LEN, causing names longer than 64 characters to pass validation and fail at database insertion.
- *(t3)* The name validation checks `order.name.len()` against `MAX_QUANTITY` instead of `MAX_NAME_LEN`, allowing names up to 1000 characters to pass validation and fail when inserted into the database.
- *(t4)* validate checks order.name.len() against MAX_QUANTITY (1000) instead of MAX_NAME_LEN (64), causing names longer than 64 characters to pass validation and fail at database insertion.
- *(t5)* validate checks order.name against MAX_QUANTITY instead of MAX_NAME_LEN, allowing names over 64 bytes to pass validation and fail at database insertion.

---

## `c12-f1` — HEDGED

**Ground truth** — `Correctness` at `src/api.rs:8-13`

> The bounds check uses `Store::len`, which returns the configured capacity rather than the number of records present. For a partially filled store - capacity 100 holding 3 records - any index in 3..100 passes the guard and reaches `store.record_at(index)`, which indexes the backing `Vec` directly and panics. The guard should compare against `Store::filled`.

**Reading.** Names the right failure -- an in-bounds index reaching a vacant slot and panicking -- but states the cause conditionally ("if slots can be vacant", "if `len()` reflects populated count rather than maximum index") instead of asserting that `Store::len` returns capacity. Counted as a true positive: a human handed this claim reads `Store::len` and finds the bug. Recorded as the one match in the set that is not crisp.

**Claims as reported:**

- *(t1)* Callers passing an index for an unoccupied or deleted slot in `Store` will cause `store.record_at(index)` to panic despite `index < store.len()`.
- *(t2)* Callers can pass a valid index within store.len() to fetch, and store.record_at will panic if slots can be uninitialized or sparse.
- *(t3)* Callers passing an index to an unoccupied slot in a sparse store will trigger a panic in `store.record_at(index)` despite passing the `store.len()` bounds check.
- *(t4)* Callers passing valid indices into a sparse or slot-reusing store can cause `record_at` to panic if `store.len()` reflects populated count rather than maximum index or if slots within bounds can be vacant.
- *(t5)* store.record_at(index) will panic on valid indices if Store contains vacant or deleted slots below store.len().

---

## The one spurious match we did find

It is not in the table above, because it is not on the frozen benchmark. On the
Python pilot, in the run with the second look enabled, `p03-len-is-capacity`
scored a true positive for:

> Non-integer float indices bypass the bounds check and get passed directly to
> cache.page_at.

The real defect is that `PagedCache.__len__` returns the configured capacity
rather than the number of pages present, so an index between `filled` and
`capacity` passes the guard. The reported claim is about float indices. It
matched because it landed on `src/api.py:9-11` — the same three lines — under
`Validation`, which that finding's `also_accept` list allows.

That is the matcher crediting the right location for the wrong reason, and it
is why this audit exists. It is reported in
[`pilot-python.md`](pilot-python.md) as well, and the pilot's recall figure for
that run should be read with it in mind.

## The held-out benchmark, audited the same way

`vcr audit-matches --benchmark benchmark/holdout --root results-holdout` covers
the twelve matches from the three held-out trials. `h01`, `h02` and `h03` are
described exactly, in every trial. **`h06` hedges**, in the same shape as
`c12`:

> Inlining `alert.severity > 7` diverges from the definition of
> `is_page_worthy`, causing alerts with boundary severity values **or
> additional page-worthiness conditions** to be incorrectly **included or
> excluded** from `Digest::paging`.

The divergence is named and the boundary is named; the direction is left open.

**This is the audit's most useful result, and it needed two benchmarks to
see.** `c12` and `h06` are the only boundary/off-by-one defects in either case
set, they were written by different authors, and the system hedged on **both**
while stating all eight other defects flatly. One hedge is a quirk of a case.
Two, on independently authored cases of the same defect class, look like a
property of the system: it will tell you *where* to look and that something is
off by one, but it will not commit to which side.

That is worth knowing for a reviewer using it — the claim is actionable but the
direction still has to be checked — and it is not something the frozen
benchmark alone could have shown.

## What would fix it properly

A ground-truth *mechanism key* — a short machine-checkable token per defect
(`len-returns-capacity`, `counter-not-decremented`) that a prediction must also
name — would let the matcher check the reason as well as the place, without a
model in the loop. That is a benchmark-format change, and changing the frozen
benchmark's format at this point would invalidate every recorded run, so it is
noted as future work rather than done.