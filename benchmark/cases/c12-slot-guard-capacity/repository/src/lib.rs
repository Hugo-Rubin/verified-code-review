//! Slot-based record storage with read endpoints.

pub mod api;
pub mod store;

pub use api::{fetch, fetch_many};
pub use store::{Record, Store};
