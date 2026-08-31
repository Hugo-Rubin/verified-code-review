//! Summarising a batch of alerts for the on-call engineer.

pub mod digest;
pub mod model;

pub use digest::{build, Digest};
pub use model::{is_page_worthy, Alert, PAGE_THRESHOLD};
