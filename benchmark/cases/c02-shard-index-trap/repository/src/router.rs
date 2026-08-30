//! Shard routing.

use crate::shard::Shard;

#[derive(Debug, PartialEq, Eq)]
pub enum RouterError {
    /// A router with no shards cannot route anything.
    NoShards,
}

/// Routes keys to shards.
///
/// # Invariant
///
/// `shards` is never empty. [`Router::new`] is the only constructor and it
/// rejects an empty vector, and no method removes a shard once the router is
/// built. Callers may therefore index `shards` at 0 without checking.
pub struct Router {
    shards: Vec<Shard>,
}

impl Router {
    /// Build a router over `shards`.
    ///
    /// Returns [`RouterError::NoShards`] when `shards` is empty, which is what
    /// establishes the non-empty invariant for the rest of the type.
    pub fn new(shards: Vec<Shard>) -> Result<Self, RouterError> {
        if shards.is_empty() {
            return Err(RouterError::NoShards);
        }
        Ok(Self { shards })
    }

    /// The router's shards. Never empty.
    pub fn shards(&self) -> &[Shard] {
        &self.shards
    }

    /// Pick the shard that owns `key`.
    pub fn shard_for(&self, key: &str) -> &Shard {
        let h = key.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
        &self.shards[(h % self.shards.len() as u64) as usize]
    }

    /// Replace a shard in place. The shard count does not change.
    pub fn replace(&mut self, index: usize, shard: Shard) -> bool {
        match self.shards.get_mut(index) {
            Some(slot) => {
                *slot = shard;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_empty_shard_set() {
        assert!(matches!(Router::new(vec![]), Err(RouterError::NoShards)));
    }

    #[test]
    fn routes_keys_to_some_shard() {
        let r = Router::new(vec![Shard::new("a"), Shard::new("b")]).unwrap();
        assert!(["a", "b"].contains(&r.shard_for("hello").id()));
    }

    #[test]
    fn replacing_preserves_the_shard_count() {
        let mut r = Router::new(vec![Shard::new("a")]).unwrap();
        assert!(r.replace(0, Shard::new("c")));
        assert_eq!(r.shards().len(), 1);
        assert!(!r.replace(9, Shard::new("d")));
    }
}
