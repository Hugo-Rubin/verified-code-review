//! A batch job runner with aggregate counters.

pub mod metrics;
pub mod runner;

pub use metrics::{Metrics, Totals};
pub use runner::{run, Job};
