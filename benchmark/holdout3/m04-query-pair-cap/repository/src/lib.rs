//! A compact request parser for a constrained HTTP router.

pub mod limits;
pub mod query;

pub use query::{parse_body, parse_query, QueryError};
