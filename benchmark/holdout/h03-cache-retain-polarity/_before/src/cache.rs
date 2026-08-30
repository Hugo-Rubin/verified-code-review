//! Entry storage and eviction.

/// One cached value together with the timestamp at which it stops being
/// usable. Times are opaque monotonic ticks supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub key: String,
    pub value: String,
    pub expires_at: u64,
}

#[derive(Debug, Default)]
pub struct TtlCache {
    entries: Vec<CacheEntry>,
}

impl TtlCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Store `value` under `key`, replacing any existing entry.
    pub fn put(&mut self, key: &str, value: &str, expires_at: u64) {
        self.entries.retain(|e| e.key != key);
        self.entries.push(CacheEntry {
            key: key.to_string(),
            value: value.to_string(),
            expires_at,
        });
    }

    /// The value stored under `key`, if the cache still holds it.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }

    /// Drop every entry whose deadline has already passed at `now`.
    pub fn evict_expired(&mut self, now: u64) {
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].expires_at <= now {
                self.entries.remove(i);
            } else {
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_back() {
        let mut c = TtlCache::new();
        c.put("a", "one", 100);
        assert_eq!(c.get("a"), Some("one"));
        assert_eq!(c.get("missing"), None);
    }

    #[test]
    fn putting_the_same_key_twice_keeps_one_entry() {
        let mut c = TtlCache::new();
        c.put("a", "one", 100);
        c.put("a", "two", 100);
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("a"), Some("two"));
    }

    #[test]
    fn eviction_halves_a_two_entry_cache() {
        let mut c = TtlCache::new();
        c.put("a", "one", 10);
        c.put("b", "two", 30);
        c.evict_expired(20);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn eviction_on_an_empty_cache_is_a_no_op() {
        let mut c = TtlCache::new();
        c.evict_expired(1_000);
        assert!(c.is_empty());
    }
}
