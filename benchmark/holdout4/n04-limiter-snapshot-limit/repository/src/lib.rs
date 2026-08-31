//! Admission control for a single-threaded job runner.
//!
//! The runner polls a control channel and a work queue from the same loop, so
//! operator updates and job starts interleave with each other while the process
//! is up.

pub mod limiter;
pub mod reload;
pub mod settings;
