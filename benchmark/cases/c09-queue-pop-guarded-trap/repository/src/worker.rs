//! Drains the pending queue.

use crate::queue::{Job, Queue};

#[derive(Debug, PartialEq, Eq)]
pub struct Report {
    pub processed: usize,
    pub skipped: usize,
}

#[derive(Default)]
pub struct Worker {
    queue: Queue,
    max_payload: usize,
}

impl Worker {
    pub fn new(max_payload: usize) -> Self {
        Self {
            queue: Queue::new(),
            max_payload,
        }
    }

    pub fn submit(&mut self, id: u64, payload: &str) {
        self.queue.push(Job {
            id,
            payload: payload.to_string(),
        });
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Process everything currently queued.
    pub fn drain(&mut self) -> Report {
        let mut processed = 0;
        let mut skipped = 0;

        while !self.queue.is_empty() {
            let job = self.queue.pop_front();
            if job.payload.len() > self.max_payload {
                skipped += 1;
            } else {
                processed += 1;
            }
        }

        Report { processed, skipped }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_everything_queued() {
        let mut w = Worker::new(10);
        w.submit(1, "short");
        w.submit(2, "also-short");
        assert_eq!(
            w.drain(),
            Report {
                processed: 2,
                skipped: 0
            }
        );
        assert_eq!(w.pending(), 0);
    }

    #[test]
    fn skips_oversized_payloads() {
        let mut w = Worker::new(4);
        w.submit(1, "way-too-long");
        assert_eq!(
            w.drain(),
            Report {
                processed: 0,
                skipped: 1
            }
        );
    }

    #[test]
    fn draining_an_empty_worker_is_a_no_op() {
        let mut w = Worker::new(10);
        assert_eq!(
            w.drain(),
            Report {
                processed: 0,
                skipped: 0
            }
        );
    }
}
