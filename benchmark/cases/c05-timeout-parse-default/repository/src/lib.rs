//! Configuration for a small HTTP client.

pub mod config;

pub use config::{is_unbounded, request_timeout, ConfigError, DEFAULT_TIMEOUT_SECS, NO_TIMEOUT};
