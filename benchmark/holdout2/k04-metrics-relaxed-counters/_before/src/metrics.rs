//! Aggregate counters for one batch run.

use std::sync::atomic::{AtomicU64, Ordering};

/// The two totals a run reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Totals {
    pub processed: u64,
    pub failed: u64,
}

/// Counters shared between the workers of a single run.
#[derive(Default)]
pub struct Metrics {
    processed: AtomicU64,
    failed: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_ok(&self) {
        self.processed.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_err(&self) {
        self.failed.fetch_add(1, Ordering::SeqCst);
    }

    /// A point-in-time reading of the two totals.
    ///
    /// Increments made by workers that are still running may or may not be
    /// included; callers that need the final figures read them once the
    /// workers have been joined.
    pub fn snapshot(&self) -> Totals {
        Totals {
            processed: self.processed.load(Ordering::SeqCst),
            failed: self.failed.load(Ordering::SeqCst),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_metrics_starts_at_zero() {
        let m = Metrics::new();
        assert_eq!(
            m.snapshot(),
            Totals {
                processed: 0,
                failed: 0
            }
        );
    }

    #[test]
    fn each_record_bumps_its_own_counter() {
        let m = Metrics::new();
        m.record_ok();
        m.record_ok();
        m.record_err();
        assert_eq!(
            m.snapshot(),
            Totals {
                processed: 2,
                failed: 1
            }
        );
    }
}
