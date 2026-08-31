//! Interval counters for the ingest pipeline.

use std::sync::atomic::{AtomicU64, Ordering};

/// Counters describing what the pipeline has seen.
///
/// Reading a counter is *destructive*: the tally is swapped back to zero as it
/// is read, so that two consecutive reads describe two disjoint intervals
/// rather than two overlapping running totals. That property is what makes the
/// numbers safe to add up downstream, and it means each tally has exactly one
/// legitimate reader.
#[derive(Debug, Default)]
pub struct Counters {
    accepted: AtomicU64,
    rejected: AtomicU64,
}

impl Counters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_accepted(&self) {
        self.accepted.fetch_add(1, Ordering::SeqCst);
    }

    pub fn record_rejected(&self) {
        self.rejected.fetch_add(1, Ordering::SeqCst);
    }

    /// Records accepted since the previous read.
    pub fn accepted(&self) -> u64 {
        self.accepted.swap(0, Ordering::SeqCst)
    }

    /// Records rejected since the previous read.
    pub fn rejected(&self) -> u64 {
        self.rejected.swap(0, Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_reports_the_interval_and_restarts_it() {
        let c = Counters::new();
        c.record_accepted();
        c.record_accepted();
        assert_eq!(c.accepted(), 2);
        assert_eq!(c.accepted(), 0);
        c.record_accepted();
        assert_eq!(c.accepted(), 1);
    }

    #[test]
    fn the_two_tallies_are_independent() {
        let c = Counters::new();
        c.record_accepted();
        c.record_rejected();
        assert_eq!(c.accepted(), 1);
        assert_eq!(c.rejected(), 1);
    }
}
