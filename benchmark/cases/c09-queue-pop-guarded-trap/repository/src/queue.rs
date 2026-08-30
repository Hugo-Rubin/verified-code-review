//! The pending work queue.
//!
//! Crate-internal: `Queue` is not exported from the crate root, so every call
//! site lives in this crate.

use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: u64,
    pub payload: String,
}

#[derive(Default)]
pub(crate) struct Queue {
    items: VecDeque<Job>,
}

impl Queue {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn push(&mut self, job: Job) {
        self.items.push_back(job);
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Take the next job.
    ///
    /// Callers check `is_empty` first, so the queue is non-empty here.
    pub(crate) fn pop_front(&mut self) -> Job {
        self.items
            .pop_front()
            .expect("pop_front called on an empty queue")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: u64) -> Job {
        Job {
            id,
            payload: format!("j{id}"),
        }
    }

    #[test]
    fn pops_in_fifo_order() {
        let mut q = Queue::new();
        q.push(job(1));
        q.push(job(2));
        assert_eq!(q.pop_front().id, 1);
        assert_eq!(q.pop_front().id, 2);
        assert!(q.is_empty());
    }

    #[test]
    fn tracks_length() {
        let mut q = Queue::new();
        assert_eq!(q.len(), 0);
        q.push(job(1));
        assert_eq!(q.len(), 1);
    }
}
