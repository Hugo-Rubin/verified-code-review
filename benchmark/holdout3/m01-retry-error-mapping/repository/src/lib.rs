//! A small fetch client with a retry policy.

pub mod client;
pub mod error;

pub use client::{fetch_once, fetch_with_retry, parse_record, Record, Transport, MAX_ATTEMPTS};
pub use error::{FetchError, ParseError};
