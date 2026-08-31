//! Runs a batch of jobs across a fixed pool of worker threads.

use crate::metrics::{Metrics, Totals};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct Job {
    pub id: u64,
    pub fails: bool,
}

/// Run every job across `workers` threads and return the totals.
///
/// The only reader of the counters is the `snapshot` call at the end of this
/// function, and every worker handle has been joined by the time it runs, so
/// the returned totals account for all of them.
pub fn run(jobs: Vec<Job>, workers: usize) -> Totals {
    let metrics = Arc::new(Metrics::new());
    let queue = Arc::new(Mutex::new(jobs));
    let mut handles = Vec::with_capacity(workers);

    for _ in 0..workers {
        let metrics = Arc::clone(&metrics);
        let queue = Arc::clone(&queue);
        handles.push(thread::spawn(move || loop {
            let job = queue.lock().expect("queue poisoned").pop();
            match job {
                Some(job) if job.fails => metrics.record_err(),
                Some(_) => metrics.record_ok(),
                None => break,
            }
        }));
    }

    for handle in handles {
        handle.join().expect("worker panicked");
    }

    metrics.snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_account_for_every_job() {
        let jobs: Vec<Job> = (0..500)
            .map(|id| Job {
                id,
                fails: id % 5 == 0,
            })
            .collect();
        let totals = run(jobs, 8);
        assert_eq!(totals.processed, 400);
        assert_eq!(totals.failed, 100);
    }

    #[test]
    fn an_empty_batch_reports_zero() {
        let totals = run(Vec::new(), 4);
        assert_eq!(totals.processed, 0);
        assert_eq!(totals.failed, 0);
    }
}
