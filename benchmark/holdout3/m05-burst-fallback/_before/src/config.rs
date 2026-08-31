//! Reading a tenant's throttle configuration.

use crate::limiter::Limiter;
use std::collections::HashMap;

/// Rate used when a tenant file does not name one.
pub const DEFAULT_RATE: u32 = 1;

/// Burst used when a tenant file does not name one.
pub const DEFAULT_BURST: u32 = 5;

#[derive(Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A key was present but its value did not parse.
    Invalid(&'static str),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Config {
    pub rate_per_sec: u32,
    pub burst: u32,
}

impl Config {
    pub fn into_limiter(self) -> Limiter {
        Limiter::new(self.rate_per_sec, self.burst)
    }
}

/// Parse a tenant file of `key = value` lines. Blank lines and `#` comments
/// are skipped, and unknown keys are ignored.
pub fn parse_config(text: &str) -> Result<Config, ConfigError> {
    let mut raw: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            raw.insert(k.trim(), v.trim());
        }
    }

    let rate_per_sec = match raw.get("rate") {
        Some(v) => v.parse::<u32>().map_err(|_| ConfigError::Invalid("rate"))?,
        None => DEFAULT_RATE,
    };

    let burst = match raw.get("burst") {
        Some(v) => v.parse::<u32>().map_err(|_| ConfigError::Invalid("burst"))?,
        None => DEFAULT_BURST,
    };

    Ok(Config {
        rate_per_sec,
        burst,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rate_and_burst() {
        assert_eq!(
            parse_config("rate = 10\nburst = 20"),
            Ok(Config {
                rate_per_sec: 10,
                burst: 20
            })
        );
    }

    #[test]
    fn skips_comments_and_blank_lines() {
        let text = "# tenant acme\n\nrate = 3\n\n# burst below\nburst = 9\n";
        assert_eq!(
            parse_config(text),
            Ok(Config {
                rate_per_sec: 3,
                burst: 9
            })
        );
    }

    #[test]
    fn refuses_a_rate_that_does_not_parse() {
        assert_eq!(
            parse_config("rate = fast\nburst = 9"),
            Err(ConfigError::Invalid("rate"))
        );
    }

    #[test]
    fn a_parsed_config_builds_a_working_limiter() {
        let mut l = parse_config("rate = 1\nburst = 1").unwrap().into_limiter();
        assert!(l.allow());
        assert!(!l.allow());
    }
}
