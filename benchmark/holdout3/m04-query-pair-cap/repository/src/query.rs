//! Parsing of urlencoded key/value input, from the URL and from the body.

use crate::limits::{MAX_BODY_LEN, MAX_PAIRS, MAX_QUERY_LEN};

#[derive(Debug, PartialEq, Eq)]
pub enum QueryError {
    /// The input is longer than the limit for that input's source.
    TooLong { len: usize, limit: usize },
    /// The input carries more pairs than the router will hold.
    TooManyPairs,
    /// A segment such as `=v` names no key.
    EmptyKey,
}

/// Parse the query string of a URL.
///
/// Empty segments are skipped, so a trailing `&` is tolerated. A segment with
/// no `=` is a key with an empty value.
pub fn parse_query(q: &str) -> Result<Vec<(String, String)>, QueryError> {
    if q.len() > MAX_QUERY_LEN {
        return Err(QueryError::TooLong {
            len: q.len(),
            limit: MAX_QUERY_LEN,
        });
    }

    let mut out = Vec::new();
    for seg in q.split('&') {
        if seg.is_empty() {
            continue;
        }
        let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
        if k.is_empty() {
            return Err(QueryError::EmptyKey);
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

/// Parse an urlencoded request body, which is allowed to be far larger than a
/// query string.
pub fn parse_body(b: &str) -> Result<Vec<(String, String)>, QueryError> {
    if b.len() > MAX_BODY_LEN {
        return Err(QueryError::TooLong {
            len: b.len(),
            limit: MAX_BODY_LEN,
        });
    }

    let mut out = Vec::new();
    for seg in b.split('&') {
        if seg.is_empty() {
            continue;
        }
        let (k, v) = seg.split_once('=').unwrap_or((seg, ""));
        if k.is_empty() {
            return Err(QueryError::EmptyKey);
        }
        if out.len() == MAX_PAIRS {
            return Err(QueryError::TooManyPairs);
        }
        out.push((k.to_string(), v.to_string()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(k, x)| ((*k).to_string(), (*x).to_string()))
            .collect()
    }

    #[test]
    fn parses_an_ordinary_query() {
        assert_eq!(
            parse_query("a=1&b=2"),
            Ok(pairs(&[("a", "1"), ("b", "2")]))
        );
    }

    #[test]
    fn tolerates_empty_segments_and_missing_values() {
        assert_eq!(parse_query("&a&&b=&"), Ok(pairs(&[("a", ""), ("b", "")])));
    }

    #[test]
    fn refuses_a_segment_with_no_key() {
        assert_eq!(parse_query("a=1&=2"), Err(QueryError::EmptyKey));
    }

    #[test]
    fn refuses_an_overlong_query() {
        let q = "a=".to_string() + &"x".repeat(MAX_QUERY_LEN);
        assert!(matches!(parse_query(&q), Err(QueryError::TooLong { .. })));
    }

    #[test]
    fn parses_an_ordinary_body() {
        assert_eq!(parse_body("a=1&b=2"), Ok(pairs(&[("a", "1"), ("b", "2")])));
    }

    #[test]
    fn refuses_a_body_with_too_many_pairs() {
        let body = (0..MAX_PAIRS + 1)
            .map(|i| format!("k{i}=1"))
            .collect::<Vec<_>>()
            .join("&");
        assert_eq!(parse_body(&body), Err(QueryError::TooManyPairs));
    }

    #[test]
    fn refuses_an_overlong_body() {
        let body = "x".repeat(MAX_BODY_LEN + 1);
        assert!(matches!(parse_body(&body), Err(QueryError::TooLong { .. })));
    }
}
