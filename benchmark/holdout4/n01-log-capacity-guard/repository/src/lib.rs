//! A small in-memory diagnostic log backed by a bounded ring buffer.
//!
//! Operators configure how many entries they want retained; the buffer decides
//! how many it is willing to allocate.

pub mod buffer;
pub mod logger;

/// Operator-supplied logger settings.
#[derive(Debug, Clone)]
pub struct Config {
    /// How many entries the operator asked the log to retain.
    pub max_entries: usize,
}

impl Config {
    pub fn new(max_entries: usize) -> Self {
        Self { max_entries }
    }
}
