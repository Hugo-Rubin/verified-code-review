//! A worker that leases a slot from a fixed pool for the duration of a job.

pub mod job;
pub mod lease;

pub use job::{JobError, Worker};
pub use lease::{Lease, LeasePool};
