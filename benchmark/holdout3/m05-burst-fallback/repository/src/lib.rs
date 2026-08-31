//! Per-tenant request throttling.

pub mod config;
pub mod limiter;

pub use config::{parse_config, Config, ConfigError, DEFAULT_BURST, DEFAULT_RATE};
pub use limiter::Limiter;
