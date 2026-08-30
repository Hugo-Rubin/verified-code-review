//! A very small time-to-live cache with explicit, caller-driven eviction.

pub mod cache;

pub use cache::{CacheEntry, TtlCache};
