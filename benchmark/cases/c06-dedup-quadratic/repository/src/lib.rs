//! Ingest-side batch processing.

pub mod dedup;

pub use dedup::{duplicate_count, unique_ids};
