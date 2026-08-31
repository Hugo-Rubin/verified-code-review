//! The catalogue and the only way to build one.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub target: u32,
}

/// A finished catalogue.
///
/// `items` is private and [`Catalog::build`] is the only constructor in the
/// crate; there is no method that adds, removes or reorders an entry after
/// construction. `build` funnels its input through a `BTreeMap` keyed by
/// `name`, so `items` is in ascending `name` order with one entry per name,
/// and stays that way for the life of the value.
#[derive(Debug)]
pub struct Catalog {
    items: Vec<Entry>,
}

impl Catalog {
    /// Build a catalogue from entries in any order.
    ///
    /// Later entries with a name already seen replace the earlier one, which
    /// is how an operator overrides a shipped default.
    pub fn build(entries: impl IntoIterator<Item = Entry>) -> Self {
        let mut by_name: BTreeMap<String, Entry> = BTreeMap::new();
        for entry in entries {
            by_name.insert(entry.name.clone(), entry);
        }
        Self {
            items: by_name.into_values().collect(),
        }
    }

    /// The catalogue's entries, in the order it holds them.
    pub fn entries(&self) -> &[Entry] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, target: u32) -> Entry {
        Entry {
            name: name.to_string(),
            target,
        }
    }

    #[test]
    fn building_orders_entries_by_name() {
        let c = Catalog::build([entry("zeta", 1), entry("alpha", 2), entry("mid", 3)]);
        let names: Vec<&str> = c.entries().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "mid", "zeta"]);
    }

    #[test]
    fn a_repeated_name_keeps_only_the_last_entry() {
        let c = Catalog::build([entry("a", 1), entry("b", 2), entry("a", 9)]);
        assert_eq!(c.len(), 2);
        assert_eq!(c.entries()[0], entry("a", 9));
    }

    #[test]
    fn an_empty_input_builds_an_empty_catalog() {
        let c = Catalog::build([]);
        assert!(c.is_empty());
    }
}
