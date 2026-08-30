//! Mapping numeric upstream response codes onto coarse outcome classes.

pub mod classify;
pub mod retry;

pub use classify::{classify, Class};
pub use retry::plan_retry;
