//! Error types, and the classification the retry loop runs on.

/// Why a fetch failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The connection or the peer failed before a complete body arrived.
    /// Another attempt may well succeed.
    Transport(String),
    /// A complete body arrived but could not be understood. The same request
    /// will produce the same body, so another attempt cannot help.
    Malformed(String),
    /// The credentials were rejected.
    Unauthorized,
}

impl FetchError {
    /// Only transport failures are worth another attempt.
    pub fn is_retryable(&self) -> bool {
        matches!(self, FetchError::Transport(_))
    }
}

/// Raised by the record parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transport_is_retryable() {
        assert!(FetchError::Transport("reset".into()).is_retryable());
        assert!(!FetchError::Malformed("bad".into()).is_retryable());
        assert!(!FetchError::Unauthorized.is_retryable());
    }
}
