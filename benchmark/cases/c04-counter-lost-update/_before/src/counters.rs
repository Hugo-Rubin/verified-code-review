//! Per-key request counters shared across worker threads.

use std::collections::HashMap;
use std::sync::Mutex;

/// Thread-safe counters keyed by route name.
pub struct Counters {
    counts: Mutex<HashMap<String, u64>>,
}

impl Default for Counters {
    fn default() -> Self {
        Self::new()
    }
}

impl Counters {
    pub fn new() -> Self {
        Self {
            counts: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> u64 {
        let guard = self.counts.lock().expect("counters poisoned");
        guard.get(key).copied().unwrap_or(0)
    }

    /// Record one request against `key`.
    pub fn record(&self, key: &str) {
        let mut guard = self.counts.lock().expect("counters poisoned");
        *guard.entry(key.to_string()).or_insert(0) += 1;
    }

    /// Reset a key and return what it held.
    pub fn take(&self, key: &str) -> u64 {
        let mut guard = self.counts.lock().expect("counters poisoned");
        guard.remove(key).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_sequentially() {
        let c = Counters::new();
        c.record("a");
        c.record("a");
        c.record("b");
        assert_eq!(c.get("a"), 2);
        assert_eq!(c.get("b"), 1);
    }

    #[test]
    fn take_resets_the_key() {
        let c = Counters::new();
        c.record("a");
        assert_eq!(c.take("a"), 1);
        assert_eq!(c.get("a"), 0);
    }
}
