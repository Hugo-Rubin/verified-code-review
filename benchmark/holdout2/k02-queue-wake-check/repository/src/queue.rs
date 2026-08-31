//! A blocking queue shared by any number of producers and consumers.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

struct State<T> {
    items: VecDeque<T>,
    closed: bool,
    waiting: usize,
}

/// Hands items from producers to consumers, blocking consumers while empty.
pub struct Queue<T> {
    state: Mutex<State<T>>,
    ready: Condvar,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Queue<T> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                items: VecDeque::new(),
                closed: false,
                waiting: 0,
            }),
            ready: Condvar::new(),
        }
    }

    /// Append an item and wake consumers.
    pub fn push(&self, item: T) {
        let mut state = self.state.lock().expect("queue poisoned");
        state.items.push_back(item);
        drop(state);
        self.ready.notify_all();
    }

    /// Stop accepting items and release every blocked consumer.
    pub fn close(&self) {
        let mut state = self.state.lock().expect("queue poisoned");
        state.closed = true;
        drop(state);
        self.ready.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.state.lock().expect("queue poisoned").closed
    }

    /// How many consumers are currently blocked in `pop`. Exposed for metrics.
    pub fn waiting(&self) -> usize {
        self.state.lock().expect("queue poisoned").waiting
    }

    /// Take the next item, blocking until one is available.
    ///
    /// Returns `None` only once the queue has been closed and drained.
    pub fn pop(&self) -> Option<T> {
        let mut state = self.state.lock().expect("queue poisoned");
        if state.items.is_empty() && !state.closed {
            state.waiting += 1;
            state = self.ready.wait(state).expect("queue poisoned");
            state.waiting -= 1;
        }
        state.items.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn items_come_back_in_order() {
        let q = Queue::new();
        q.push(1);
        q.push(2);
        assert_eq!(q.pop(), Some(1));
        assert_eq!(q.pop(), Some(2));
    }

    #[test]
    fn a_consumer_blocks_until_an_item_arrives() {
        let q = Arc::new(Queue::new());
        let consumer = Arc::clone(&q);
        let handle = thread::spawn(move || consumer.pop());
        while q.waiting() == 0 {
            thread::yield_now();
        }
        q.push(9);
        assert_eq!(handle.join().unwrap(), Some(9));
    }

    #[test]
    fn closing_releases_a_blocked_consumer() {
        let q: Arc<Queue<u32>> = Arc::new(Queue::new());
        let consumer = Arc::clone(&q);
        let handle = thread::spawn(move || consumer.pop());
        while q.waiting() == 0 {
            thread::yield_now();
        }
        q.close();
        assert_eq!(handle.join().unwrap(), None);
        assert!(q.is_closed());
    }

    #[test]
    fn a_closed_queue_still_drains() {
        let q = Queue::new();
        q.push(4);
        q.close();
        assert_eq!(q.pop(), Some(4));
        assert_eq!(q.pop(), None);
    }
}
