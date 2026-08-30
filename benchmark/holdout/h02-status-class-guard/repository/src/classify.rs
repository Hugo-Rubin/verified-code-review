//! Outcome classification for upstream response codes.

/// Coarse outcome class for a numeric response code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Success,
    Redirect,
    ClientError,
    ServerError,
    Unknown,
}

impl Class {
    /// Whether a request that ended in this class should be retried.
    ///
    /// Only the upstream's own faults are worth retrying; a request we got
    /// wrong will be rejected identically on every attempt.
    pub fn retryable(&self) -> bool {
        matches!(self, Class::ServerError)
    }
}

/// Classify a numeric response code.
pub fn classify(code: u16) -> Class {
    match code {
        200..=299 => Class::Success,
        300..=399 => Class::Redirect,
        c if c >= 400 => Class::ClientError,
        500..=599 => Class::ServerError,
        _ => Class::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_hundreds_are_successes() {
        assert_eq!(classify(200), Class::Success);
        assert_eq!(classify(204), Class::Success);
        assert!(!classify(200).retryable());
    }

    #[test]
    fn three_hundreds_are_redirects() {
        assert_eq!(classify(301), Class::Redirect);
        assert_eq!(classify(308), Class::Redirect);
    }

    #[test]
    fn not_found_is_a_client_error_and_is_not_retried() {
        assert_eq!(classify(404), Class::ClientError);
        assert!(!classify(404).retryable());
    }

    #[test]
    fn codes_below_two_hundred_are_unknown() {
        assert_eq!(classify(100), Class::Unknown);
        assert_eq!(classify(0), Class::Unknown);
    }

    #[test]
    fn vendor_codes_past_the_standard_range_are_client_errors() {
        assert_eq!(classify(600), Class::ClientError);
    }
}
