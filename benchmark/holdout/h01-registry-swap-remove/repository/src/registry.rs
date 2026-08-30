//! Name-addressed registry of service endpoints.

use std::collections::HashMap;

/// One registered service endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub addr: String,
}

/// Entries in registration order, plus a name -> position index so that
/// `addr_of` does not have to scan the vector.
#[derive(Debug, Default)]
pub struct Registry {
    entries: Vec<Entry>,
    index: HashMap<String, usize>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Register `name` at `addr`. Re-registering a known name updates it.
    pub fn insert(&mut self, name: &str, addr: &str) {
        if let Some(&pos) = self.index.get(name) {
            self.entries[pos].addr = addr.to_string();
            return;
        }
        self.index.insert(name.to_string(), self.entries.len());
        self.entries.push(Entry {
            name: name.to_string(),
            addr: addr.to_string(),
        });
    }

    /// The address registered under `name`, if any.
    pub fn addr_of(&self, name: &str) -> Option<&str> {
        let pos = *self.index.get(name)?;
        self.entries.get(pos).map(|e| e.addr.as_str())
    }

    /// Deregister `name`. Returns true if it had been registered.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(pos) = self.index.remove(name) else {
            return false;
        };
        self.entries.swap_remove(pos);
        true
    }

    /// Every registered name, in storage order.
    pub fn names(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_and_resolves() {
        let mut r = Registry::new();
        r.insert("auth", "10.0.0.1");
        r.insert("billing", "10.0.0.2");
        assert_eq!(r.len(), 2);
        assert_eq!(r.addr_of("auth"), Some("10.0.0.1"));
        assert_eq!(r.addr_of("billing"), Some("10.0.0.2"));
    }

    #[test]
    fn re_registering_updates_the_address() {
        let mut r = Registry::new();
        r.insert("auth", "10.0.0.1");
        r.insert("auth", "10.0.0.9");
        assert_eq!(r.len(), 1);
        assert_eq!(r.addr_of("auth"), Some("10.0.0.9"));
    }

    #[test]
    fn deregistering_the_last_entry_shrinks_the_registry() {
        let mut r = Registry::new();
        r.insert("auth", "10.0.0.1");
        r.insert("billing", "10.0.0.2");
        assert!(r.remove("billing"));
        assert_eq!(r.len(), 1);
        assert_eq!(r.addr_of("billing"), None);
        assert_eq!(r.addr_of("auth"), Some("10.0.0.1"));
    }

    #[test]
    fn deregistering_an_unknown_name_reports_false() {
        let mut r = Registry::new();
        r.insert("auth", "10.0.0.1");
        assert!(!r.remove("nope"));
        assert_eq!(r.len(), 1);
    }
}
