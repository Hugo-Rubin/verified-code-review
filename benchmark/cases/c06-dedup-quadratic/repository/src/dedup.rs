//! Deduplication for ingest batches.
//!
//! Batches come off the ingest topic and routinely carry 100_000 to 500_000
//! ids, so everything in this module runs on the hot path.

/// The unique ids in `ids`, in first-seen order.
pub fn unique_ids(ids: &[u64]) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();

    for &id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }

    out
}

/// How many ids in `ids` are duplicates of an earlier entry.
pub fn duplicate_count(ids: &[u64]) -> usize {
    ids.len() - unique_ids(ids).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_duplicates_preserving_order() {
        assert_eq!(unique_ids(&[3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[test]
    fn handles_an_empty_batch() {
        assert!(unique_ids(&[]).is_empty());
    }

    #[test]
    fn counts_duplicates() {
        assert_eq!(duplicate_count(&[1, 1, 2, 2, 2]), 3);
    }
}
