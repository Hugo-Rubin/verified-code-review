# Trajectory — `c01-pool-counter-leak` · Baseline

| | |
|---|---|
| Agent | Baseline |
| Case | `c01-pool-counter-leak` |
| Model | `gemini-3.7-flash` |
| Provider | Vertex |
| Temperature | 0.0 |
| Trajectory id | `0ebde541-4f23-4244-a466-a5db08ec41d6` |
| Started | 2026-08-30T11:03:45.740139300+00:00 |
| Runtime | 10545 ms |
| Model calls | 1 |
| Tool calls | 0 |
| Retries | 0 |
| Tokens | 1619 in / 746 out |
| Cost | not configured |
| Match tolerance | ±3 lines |
| Tool-call budget | 8 per candidate |

---

## Steps

### 1. Model call — Review

Prompt version `baseline-review/v2` · 1619 in / 746 out · 10545 ms · attempt(s) 1

<details><summary>System instructions</summary>

```
You are an experienced Rust reviewer examining a proposed change before it merges.

Report defects that are actually present in this change. For each one, give the
file and the line range in the file's current state, classify it, and state the
problem in one sentence.

Be precise and be selective:
- Report an issue only when you are confident the code is genuinely wrong.
- A pattern that merely looks risky is not a defect.
- Do not report style preferences, naming, formatting, or missing comments.
- Do not invent a defect to have something to say.

Return JSON of exactly this shape and nothing else:

{
  "findings": [
    {
      "issue_type": "<one of the values below, spelled exactly>",
      "severity": "Low" | "Medium" | "High",
      "file": "<repository-relative path, forward slashes>",
      "start_line": <integer, 1-based, in the file's CURRENT state after the change>,
      "end_line": <integer, >= start_line>,
      "claim": "<one sentence stating precisely what is wrong>",
      "reasoning": "<why you believe it, 1-3 sentences>"
    }
  ]
}

Valid issue_type values:
  - Correctness
  - ErrorHandling
  - Validation
  - StateManagement
  - ResourceManagement
  - Concurrency
  - ApiContract
  - Testing
  - Performance

If the change contains no real defect, return {"findings": []}. An empty
result is a correct answer when the code is sound.
```

</details>

<details><summary>User message</summary>

````
## Change under review

The capacity check in `Pool::acquire` was rewritten so that the active counter is incremented first and the limit is then compared with `>` instead of `>=`. The author's stated intent was to remove the duplicated increment and make the comparison read more naturally against the number of connections that would be outstanding.

## Diff

```diff
--- a/src/pool.rs
+++ b/src/pool.rs
@@ -53,12 +53,12 @@
     /// Lease a connection, or fail if the pool is at capacity.
     pub fn acquire(&self) -> Result<Conn, PoolError> {
         let mut n = self.active.lock().expect("pool counter poisoned");
+        *n += 1;
 
-        if *n >= self.max {
+        if *n > self.max {
             return Err(PoolError::Exhausted);
         }
 
-        *n += 1;
         drop(n);
         Ok(Conn {
             active: Arc::clone(&self.active),

```

## Current contents of the changed files


### src/pool.rs

```rust
    1 | //! A bounded connection pool.
    2 | 
    3 | use std::sync::{Arc, Mutex};
    4 | 
    5 | #[derive(Debug, PartialEq, Eq)]
    6 | pub enum PoolError {
    7 |     /// All permits are in use.
    8 |     Exhausted,
    9 | }
   10 | 
   11 | /// A handle to a leased connection. Releasing it returns the permit.
   12 | pub struct Conn {
   13 |     active: Arc<Mutex<usize>>,
   14 |     released: bool,
   15 | }
   16 | 
   17 | impl Conn {
   18 |     pub fn release(mut self) {
   19 |         self.released = true;
   20 |         let mut n = self.active.lock().expect("pool counter poisoned");
   21 |         *n -= 1;
   22 |     }
   23 | }
   24 | 
   25 | impl Drop for Conn {
   26 |     fn drop(&mut self) {
   27 |         if !self.released {
   28 |             let mut n = self.active.lock().expect("pool counter poisoned");
   29 |             *n -= 1;
   30 |         }
   31 |     }
   32 | }
   33 | 
   34 | /// Hands out at most `max` concurrent connections.
   35 | pub struct Pool {
   36 |     active: Arc<Mutex<usize>>,
   37 |     max: usize,
   38 | }
   39 | 
   40 | impl Pool {
   41 |     pub fn new(max: usize) -> Self {
   42 |         Self {
   43 |             active: Arc::new(Mutex::new(0)),
   44 |             max,
   45 |         }
   46 |     }
   47 | 
   48 |     /// Number of connections currently leased out.
   49 |     pub fn active(&self) -> usize {
   50 |         *self.active.lock().expect("pool counter

[... 1332 more characters of user message; full text in the JSON ...]
````

</details>

**Response**

```json
{
  "findings": [
    {
      "issue_type": "StateManagement",
      "severity": "High",
      "file": "src/pool.rs",
      "start_line": 56,
      "end_line": 60,
      "claim": "Failed connection acquisitions leave the active counter incremented, permanently leaking permits and causing pool exhaustion.",
      "reasoning": "When `*n > self.max`, `acquire` returns an `Err` without decrementing `*n` back to its previous value. Since no `Conn` handle is returned to decrement the counter on drop or release, every failed acquire permanently inflates `self.active` and will eventually prevent valid acquisitions even after existing connections are released."
    }
  ]
}
```

### 2. Candidate proposed — `c01-pool-counter-leak-base-1`

**StateManagement** · severity High · `src/pool.rs:56-60`

> Failed connection acquisitions leave the active counter incremented, permanently leaking permits and causing pool exhaustion.

Reasoning: When `*n > self.max`, `acquire` returns an `Err` without decrementing `*n` back to its previous value. Since no `Conn` handle is returned to decrement the counter on drop or release, every failed acquire permanently inflates `self.active` and will eventually prevent valid acquisitions even after existing connections are released.

### 3. Human checkpoint

> 1 finding(s) reported for human review. The system takes no action on the code: it does not merge, reject, or modify anything.

---

## Final findings

### `c01-pool-counter-leak-base-1` — Verified (shown to the human)

**StateManagement** at `src/pool.rs:56-60`

> Failed connection acquisitions leave the active counter incremented, permanently leaking permits and causing pool exhaustion.

Status reason: baseline: reported as produced, no verification stage

Evidence items: 0

