//! A small job-processing crate.

pub(crate) mod queue;
pub mod worker;

pub use queue::Job;
pub use worker::{Report, Worker};
