//! Fixed-slot record storage.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub id: u64,
    pub value: String,
}

/// A store backed by a fixed number of slots.
///
/// Slots are allocated up front as a capacity, and filled over time. A store
/// with capacity 100 holding 3 records has 97 empty slots.
pub struct Store {
    records: Vec<Record>,
    capacity: usize,
}

impl Store {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            records: Vec::new(),
            capacity,
        }
    }

    /// The number of slots this store was configured with.
    ///
    /// Note this is the configured slot count, not the number of records
    /// present. Use [`Store::filled`] for that.
    pub fn len(&self) -> usize {
        self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// How many slots currently hold a record.
    pub fn filled(&self) -> usize {
        self.records.len()
    }

    /// Add a record. Returns false when every slot is taken.
    pub fn push(&mut self, record: Record) -> bool {
        if self.records.len() >= self.capacity {
            return false;
        }
        self.records.push(record);
        true
    }

    /// The record in slot `index`.
    pub fn record_at(&self, index: usize) -> &Record {
        &self.records[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u64) -> Record {
        Record {
            id,
            value: format!("v{id}"),
        }
    }

    #[test]
    fn len_reports_configured_capacity() {
        let s = Store::with_capacity(10);
        assert_eq!(s.len(), 10);
        assert_eq!(s.filled(), 0);
    }

    #[test]
    fn push_fills_slots_until_capacity() {
        let mut s = Store::with_capacity(2);
        assert!(s.push(record(1)));
        assert!(s.push(record(2)));
        assert!(!s.push(record(3)));
        assert_eq!(s.filled(), 2);
    }

    #[test]
    fn record_at_returns_the_slot() {
        let mut s = Store::with_capacity(4);
        s.push(record(7));
        assert_eq!(s.record_at(0).id, 7);
    }
}
