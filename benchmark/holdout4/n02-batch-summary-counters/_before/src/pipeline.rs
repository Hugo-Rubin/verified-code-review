//! The ingest pipeline itself.

use crate::counters::Counters;

/// What one batch did.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub total: usize,
    pub elapsed_ms: u64,
}

pub struct Pipeline {
    counters: Counters,
    total: usize,
    elapsed_ms: u64,
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Pipeline {
    pub fn new() -> Self {
        Self {
            counters: Counters::new(),
            total: 0,
            elapsed_ms: 0,
        }
    }

    /// The counters this pipeline writes into.
    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    /// Take one record. Blank records are rejected.
    pub fn ingest(&mut self, record: &str) -> bool {
        self.total += 1;
        self.elapsed_ms += 2;
        if record.trim().is_empty() {
            self.counters.record_rejected();
            false
        } else {
            self.counters.record_accepted();
            true
        }
    }

    /// Close the current batch and describe it.
    pub fn finish_batch(&mut self) -> Summary {
        let summary = Summary {
            total: self.total,
            elapsed_ms: self.elapsed_ms,
        };
        self.total = 0;
        self.elapsed_ms = 0;
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_batch_summary_counts_the_batch() {
        let mut p = Pipeline::new();
        assert!(p.ingest("a"));
        assert!(p.ingest("b"));
        assert!(!p.ingest("  "));
        let s = p.finish_batch();
        assert_eq!(s.total, 3);
        assert_eq!(s.elapsed_ms, 6);
    }

    #[test]
    fn a_new_batch_starts_from_zero() {
        let mut p = Pipeline::new();
        p.ingest("a");
        p.finish_batch();
        let s = p.finish_batch();
        assert_eq!(s.total, 0);
        assert_eq!(s.elapsed_ms, 0);
    }

    #[test]
    fn blank_records_are_rejected() {
        let mut p = Pipeline::new();
        assert!(!p.ingest(""));
        assert!(!p.ingest("\t "));
        assert!(p.ingest("x"));
    }
}
