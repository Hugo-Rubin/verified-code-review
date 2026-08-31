//! A set of named counters shared between threads.

use std::collections::HashMap;
use std::sync::Mutex;

/// Named `u64` counters. Cheap to share behind an `Arc`.
#[derive(Default)]
pub struct Counters {
    inner: Mutex<HashMap<String, u64>>,
}

impl Counters {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current value of `key`, or zero if it has never been set.
    pub fn get(&self, key: &str) -> u64 {
        let inner = self.inner.lock().expect("counters poisoned");
        inner.get(key).copied().unwrap_or(0)
    }

    /// Set `key` to `f(current)`.
    pub fn update<F>(&self, key: &str, f: F)
    where
        F: FnOnce(u64) -> u64,
    {
        let current = self.get(key);
        let next = f(current);
        let mut inner = self.inner.lock().expect("counters poisoned");
        inner.insert(key.to_string(), next);
    }

    /// Add `delta` to `key`.
    pub fn add(&self, key: &str, delta: u64) {
        self.update(key, |current| current + delta);
    }

    /// Every counter, sorted by name.
    pub fn snapshot(&self) -> Vec<(String, u64)> {
        let inner = self.inner.lock().expect("counters poisoned");
        let mut out: Vec<(String, u64)> = inner.iter().map(|(k, v)| (k.clone(), *v)).collect();
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_counter_reads_as_zero() {
        let c = Counters::new();
        assert_eq!(c.get("misses"), 0);
    }

    #[test]
    fn add_accumulates() {
        let c = Counters::new();
        c.add("hits", 2);
        c.add("hits", 3);
        assert_eq!(c.get("hits"), 5);
    }

    #[test]
    fn update_stores_the_computed_value() {
        let c = Counters::new();
        c.add("hits", 4);
        c.update("hits", |current| current * 10);
        assert_eq!(c.get("hits"), 40);
    }

    #[test]
    fn snapshot_is_sorted_by_name() {
        let c = Counters::new();
        c.add("b", 1);
        c.add("a", 2);
        assert_eq!(
            c.snapshot(),
            vec![("a".to_string(), 2), ("b".to_string(), 1)]
        );
    }
}
