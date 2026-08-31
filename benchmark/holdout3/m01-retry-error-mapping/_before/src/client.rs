//! One attempt, and the loop that repeats it.

use crate::error::{FetchError, ParseError};

/// Attempts a single `fetch_with_retry` call will make at most.
pub const MAX_ATTEMPTS: usize = 4;

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub id: u32,
}

/// A source of response bodies.
pub trait Transport {
    /// One request attempt. `Err` means no complete body arrived.
    fn get(&mut self) -> Result<String, FetchError>;
}

/// Parse one record. The wire format is `id=<number>`.
pub fn parse_record(body: &str) -> Result<Record, ParseError> {
    let raw = body
        .strip_prefix("id=")
        .ok_or_else(|| ParseError(format!("no id= prefix in {body:?}")))?;
    let id = raw
        .trim()
        .parse::<u32>()
        .map_err(|_| ParseError(format!("id {raw:?} is not a number")))?;
    Ok(Record { id })
}

/// Fetch a body and parse it. One attempt, no retrying.
pub fn fetch_once(t: &mut dyn Transport) -> Result<Record, FetchError> {
    let body = t.get()?;
    parse_record(&body).map_err(|e| FetchError::Malformed(e.0))
}

/// Repeat `fetch_once` while the failure is retryable, up to `MAX_ATTEMPTS`
/// times. Returns the outcome and how many attempts were made.
pub fn fetch_with_retry(t: &mut dyn Transport) -> (Result<Record, FetchError>, usize) {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match fetch_once(t) {
            Ok(r) => return (Ok(r), attempts),
            Err(e) => {
                if !e.is_retryable() || attempts >= MAX_ATTEMPTS {
                    return (Err(e), attempts);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scripted(Vec<Result<String, FetchError>>);

    impl Transport for Scripted {
        fn get(&mut self) -> Result<String, FetchError> {
            if self.0.is_empty() {
                return Err(FetchError::Transport("script exhausted".into()));
            }
            self.0.remove(0)
        }
    }

    struct Fixed(Result<String, FetchError>);

    impl Transport for Fixed {
        fn get(&mut self) -> Result<String, FetchError> {
            self.0.clone()
        }
    }

    #[test]
    fn parses_a_record() {
        assert_eq!(parse_record("id=7"), Ok(Record { id: 7 }));
    }

    #[test]
    fn retries_a_transport_failure_then_succeeds() {
        let mut t = Scripted(vec![
            Err(FetchError::Transport("reset".into())),
            Err(FetchError::Transport("reset".into())),
            Ok("id=7".to_string()),
        ]);
        let (r, attempts) = fetch_with_retry(&mut t);
        assert_eq!(r, Ok(Record { id: 7 }));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn gives_up_after_max_attempts() {
        let mut t = Fixed(Err(FetchError::Transport("reset".into())));
        let (r, attempts) = fetch_with_retry(&mut t);
        assert!(r.is_err());
        assert_eq!(attempts, MAX_ATTEMPTS);
    }

    #[test]
    fn an_unauthorized_response_is_not_retried() {
        let mut t = Fixed(Err(FetchError::Unauthorized));
        let (r, attempts) = fetch_with_retry(&mut t);
        assert!(r.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn a_body_that_does_not_parse_is_reported_as_an_error() {
        let mut t = Fixed(Ok("nonsense".to_string()));
        let (r, _) = fetch_with_retry(&mut t);
        assert!(r.is_err());
    }
}
