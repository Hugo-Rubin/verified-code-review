//! Flattening of a config file's `include` directives.

pub mod graph;
pub mod resolve;

pub use graph::{GraphError, IncludeGraph, MAX_UNITS};
pub use resolve::flatten;
