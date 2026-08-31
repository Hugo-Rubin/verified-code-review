//! Name lookups against a built catalogue. This is the dispatch hot path.

use crate::catalog::{Catalog, Entry};

/// The entry registered under `name`.
pub fn find<'a>(catalog: &'a Catalog, name: &str) -> Option<&'a Entry> {
    let items = catalog.entries();
    let index = items.binary_search_by(|e| e.name.as_str().cmp(name)).ok()?;
    Some(&items[index])
}

/// The target `name` routes to.
pub fn target(catalog: &Catalog, name: &str) -> Option<u32> {
    find(catalog, name).map(|e| e.target)
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

    fn catalog() -> Catalog {
        Catalog::build([
            entry("delta", 4),
            entry("alpha", 1),
            entry("charlie", 3),
            entry("bravo", 2),
        ])
    }

    #[test]
    fn every_registered_name_is_found() {
        let c = catalog();
        for (name, want) in [("alpha", 1), ("bravo", 2), ("charlie", 3), ("delta", 4)] {
            assert_eq!(find(&c, name).map(|e| e.target), Some(want));
        }
    }

    #[test]
    fn an_unregistered_name_is_not_found() {
        let c = catalog();
        assert!(find(&c, "echo").is_none());
        assert!(find(&c, "aardvark").is_none());
    }

    #[test]
    fn target_reports_the_registered_target() {
        assert_eq!(target(&catalog(), "charlie"), Some(3));
        assert_eq!(target(&catalog(), "nobody"), None);
    }

    #[test]
    fn a_lookup_in_an_empty_catalog_finds_nothing() {
        assert!(find(&Catalog::build([]), "alpha").is_none());
    }
}
