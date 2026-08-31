//! The storage layer: a fixed ring of entries.

/// The largest ring this build will allocate.
///
/// Configuration is free to ask for more than this. The request is clamped so
/// that a mistyped configuration value cannot pin an unbounded amount of
/// memory in a process whose job is to serve traffic.
pub const HARD_CAP: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub level: u8,
    pub message: String,
}

/// A ring holding at most [`HARD_CAP`] entries.
///
/// The ring never grows and never refuses a push: once it is full, pushing
/// overwrites the oldest entry it is holding.
#[derive(Debug)]
pub struct RingBuffer {
    slots: Vec<Entry>,
    oldest: usize,
    cap: usize,
}

impl RingBuffer {
    /// Allocate a ring for `requested` entries, clamped to [`HARD_CAP`].
    pub fn with_capacity(requested: usize) -> Self {
        let cap = requested.clamp(1, HARD_CAP);
        Self {
            slots: Vec::with_capacity(cap),
            oldest: 0,
            cap,
        }
    }

    /// How many entries this ring can hold at once.
    pub fn capacity(&self) -> usize {
        self.cap
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.slots.len() == self.cap
    }

    /// Append an entry, replacing the oldest one when the ring is full.
    pub fn push(&mut self, entry: Entry) {
        if self.slots.len() < self.cap {
            self.slots.push(entry);
        } else {
            self.slots[self.oldest] = entry;
            self.oldest = (self.oldest + 1) % self.cap;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.slots.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(msg: &str) -> Entry {
        Entry {
            level: 1,
            message: msg.to_string(),
        }
    }

    #[test]
    fn a_request_above_the_hard_cap_is_clamped() {
        assert_eq!(RingBuffer::with_capacity(1_000).capacity(), HARD_CAP);
    }

    #[test]
    fn a_request_below_the_hard_cap_is_honoured() {
        assert_eq!(RingBuffer::with_capacity(5).capacity(), 5);
    }

    #[test]
    fn pushing_into_a_full_ring_replaces_the_oldest_entry() {
        let mut r = RingBuffer::with_capacity(2);
        r.push(entry("a"));
        r.push(entry("b"));
        assert!(r.is_full());
        r.push(entry("c"));
        assert_eq!(r.len(), 2);
        let msgs: Vec<&str> = r.iter().map(|e| e.message.as_str()).collect();
        assert!(!msgs.contains(&"a"));
        assert!(msgs.contains(&"c"));
    }

    #[test]
    fn a_fresh_ring_is_empty() {
        assert!(RingBuffer::with_capacity(4).is_empty());
    }
}
