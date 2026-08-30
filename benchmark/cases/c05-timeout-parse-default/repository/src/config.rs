//! Runtime configuration.

use std::time::Duration;

/// Timeout applied when the operator sets nothing.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A timeout of zero disables the deadline entirely: the client will wait
/// forever. This is intentional, and is how operators opt out of timeouts on
/// long-running administrative endpoints.
pub const NO_TIMEOUT: Duration = Duration::from_secs(0);

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    NotANumber(String),
}

/// Resolve the request timeout from its raw configured value.
pub fn request_timeout(raw: Option<&str>) -> Duration {
    match raw {
        None => Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        Some(s) => Duration::from_secs(s.trim().parse().unwrap_or(0)),
    }
}

/// True when `timeout` means "wait indefinitely".
pub fn is_unbounded(timeout: Duration) -> bool {
    timeout == NO_TIMEOUT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uses_the_default_when_unset() {
        assert_eq!(request_timeout(None), Duration::from_secs(30));
    }

    #[test]
    fn parses_a_configured_value() {
        assert_eq!(request_timeout(Some("5")), Duration::from_secs(5));
        assert_eq!(request_timeout(Some(" 90 ")), Duration::from_secs(90));
    }

    #[test]
    fn zero_means_unbounded() {
        assert!(is_unbounded(request_timeout(Some("0"))));
    }
}
