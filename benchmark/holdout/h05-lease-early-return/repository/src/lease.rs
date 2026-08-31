//! Slot leasing.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Hands out `Lease` values and tracks how many are outstanding.
#[derive(Debug, Default)]
pub struct LeasePool {
    active: Arc<AtomicUsize>,
    issued: AtomicUsize,
    completed: Mutex<Vec<(usize, i64)>>,
}

impl LeasePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Take a slot. The slot is given back when the returned `Lease` is
    /// dropped.
    pub fn lease(&self) -> Lease {
        self.active.fetch_add(1, Ordering::SeqCst);
        let id = self.issued.fetch_add(1, Ordering::SeqCst);
        Lease {
            id,
            active: Arc::clone(&self.active),
        }
    }

    /// How many slots are currently taken.
    pub fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }

    /// Record a finished job and hand the slot back.
    pub fn record_completion(&self, lease: Lease, total: i64) {
        self.completed
            .lock()
            .expect("completion log poisoned")
            .push((lease.id, total));
    }

    pub fn completions(&self) -> Vec<(usize, i64)> {
        self.completed
            .lock()
            .expect("completion log poisoned")
            .clone()
    }
}

/// One taken slot.
#[derive(Debug)]
pub struct Lease {
    id: usize,
    active: Arc<AtomicUsize>,
}

impl Lease {
    pub fn id(&self) -> usize {
        self.id
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}
