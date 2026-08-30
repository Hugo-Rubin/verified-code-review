//! LLM abstraction.
//!
//! Dispatch is a plain enum rather than a trait object: there are exactly two
//! backends, and this keeps the async signatures simple with no extra
//! dependency.

pub mod mock;
pub mod vertex;

use crate::config::{LlmConfig, Provider};
use anyhow::Result;
use serde::{Deserialize, Serialize};

pub use mock::MockClient;
pub use vertex::VertexClient;

/// Token accounting for a single request.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.total_tokens += other.total_tokens;
    }

    /// Cost in USD, or `None` when the operator has not configured pricing.
    pub fn cost_usd(&self, pricing: Option<crate::config::Pricing>) -> Option<f64> {
        let p = pricing?;
        Some(
            (self.input_tokens as f64 / 1_000_000.0) * p.input_usd_per_mtok
                + (self.output_tokens as f64 / 1_000_000.0) * p.output_usd_per_mtok,
        )
    }
}

/// Which pipeline stage issued a request. Local metadata: it is recorded in
/// the trajectory and drives the mock client, and is never sent to a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    /// Produce candidate findings from the diff.
    Review,
    /// Choose the next investigation tool call.
    Investigate,
    /// Formulate the falsification question.
    Falsify,
    /// Fresh-context adjudication of claim against evidence.
    Verify,
}

/// A single completion request.
///
/// Deliberately stateless: every call carries its whole context. That is what
/// makes the fresh-context verifier genuinely fresh — there is no conversation
/// object that could leak the reviewer's prior reasoning into it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub stage: Stage,
    pub system: String,
    pub user: String,
    /// Ask the provider for `application/json` output.
    pub json_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub usage: Usage,
    pub latency_ms: u64,
    /// How many attempts were needed, including the successful one.
    pub attempts: u32,
    pub model: String,
}

/// Errors worth distinguishing in the trajectory.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("request timed out after {0}s")]
    Timeout(u64),
    #[error("provider returned status {status}: {body}")]
    Status { status: u16, body: String },
    #[error("response could not be parsed: {0}")]
    Malformed(String),
    #[error("transport error: {0}")]
    Transport(String),
}

impl LlmError {
    /// Whether retrying the same request could plausibly succeed.
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::Timeout(_) => true,
            LlmError::Transport(_) => true,
            LlmError::Malformed(_) => true,
            LlmError::Status { status, .. } => *status == 429 || *status >= 500,
        }
    }
}

pub enum LlmClient {
    Vertex(VertexClient),
    Mock(MockClient),
}

impl LlmClient {
    pub fn from_config(cfg: &LlmConfig) -> Result<Self> {
        match cfg.provider {
            Provider::Vertex => Ok(LlmClient::Vertex(VertexClient::new(cfg)?)),
            Provider::Mock => Ok(LlmClient::Mock(MockClient::new())),
        }
    }

    pub async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        match self {
            LlmClient::Vertex(c) => c.complete(req).await,
            LlmClient::Mock(c) => c.complete(req).await,
        }
    }
}

/// Pull a JSON value out of a model response.
///
/// Models routinely wrap JSON in prose or in a ```json fence even when asked
/// not to. Rather than failing the whole review on formatting, strip the
/// common wrappers; if nothing parses, that is a genuine `Malformed` error and
/// the caller retries.
pub fn extract_json(text: &str) -> Result<serde_json::Value, LlmError> {
    let trimmed = text.trim();

    if let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(v);
    }

    // ```json ... ``` or ``` ... ```
    if let Some(inner) = strip_code_fence(trimmed) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(inner.trim()) {
            return Ok(v);
        }
    }

    // Fall back to the outermost balanced {...} or [...] span.
    if let Some(span) = outermost_json_span(trimmed) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(span) {
            return Ok(v);
        }
    }

    Err(LlmError::Malformed(format!(
        "no JSON value found in response of {} chars; starts with: {:?}",
        text.len(),
        trimmed.chars().take(200).collect::<String>()
    )))
}

fn strip_code_fence(s: &str) -> Option<&str> {
    let rest = s.strip_prefix("```")?;
    // Skip an optional language tag on the opening fence line.
    let rest = &rest[rest.find('\n')? + 1..];
    let end = rest.rfind("```")?;
    Some(&rest[..end])
}

/// Find the outermost balanced JSON object or array, ignoring braces that
/// appear inside string literals.
fn outermost_json_span(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let (open, close) = {
        let first_obj = s.find('{');
        let first_arr = s.find('[');
        match (first_obj, first_arr) {
            (Some(o), Some(a)) if a < o => (b'[', b']'),
            (Some(_), _) => (b'{', b'}'),
            (None, Some(_)) => (b'[', b']'),
            (None, None) => return None,
        }
    };

    let start = bytes.iter().position(|&b| b == open)?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (i, &b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            x if x == open => depth += 1,
            x if x == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_json() {
        let v = extract_json(r#"{"a":1}"#).unwrap();
        assert_eq!(v["a"], 1);
    }

    #[test]
    fn parses_fenced_json() {
        let v = extract_json("```json\n{\"a\": 2}\n```").unwrap();
        assert_eq!(v["a"], 2);
    }

    #[test]
    fn parses_unlabelled_fence() {
        let v = extract_json("```\n[1,2,3]\n```").unwrap();
        assert_eq!(v[2], 3);
    }

    #[test]
    fn parses_json_surrounded_by_prose() {
        let v = extract_json("Here is my answer:\n{\"ok\": true}\nHope that helps!").unwrap();
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn ignores_braces_inside_strings() {
        let v = extract_json(r#"prefix {"msg": "a } b", "n": 1} suffix"#).unwrap();
        assert_eq!(v["msg"], "a } b");
        assert_eq!(v["n"], 1);
    }

    #[test]
    fn ignores_escaped_quotes_inside_strings() {
        let v = extract_json(r#"{"msg": "she said \"hi }\"", "n": 2}"#).unwrap();
        assert_eq!(v["n"], 2);
    }

    #[test]
    fn malformed_response_is_an_error_not_a_panic() {
        let e = extract_json("I'm sorry, I cannot help with that.").unwrap_err();
        assert!(matches!(e, LlmError::Malformed(_)));
        assert!(e.is_retryable());
    }

    #[test]
    fn empty_response_is_malformed() {
        assert!(extract_json("").is_err());
    }

    #[test]
    fn truncated_json_is_malformed_not_partially_accepted() {
        assert!(extract_json(r#"{"a": 1, "b": [1,2"#).is_err());
    }

    #[test]
    fn client_errors_are_not_retried() {
        let e = LlmError::Status {
            status: 400,
            body: "bad request".into(),
        };
        assert!(!e.is_retryable());
        let e = LlmError::Status {
            status: 403,
            body: "forbidden".into(),
        };
        assert!(!e.is_retryable());
    }

    #[test]
    fn rate_limit_and_server_errors_are_retried() {
        for status in [429u16, 500, 503] {
            let e = LlmError::Status {
                status,
                body: String::new(),
            };
            assert!(e.is_retryable(), "status {status} should be retryable");
        }
    }

    #[test]
    fn cost_is_none_without_pricing() {
        let u = Usage {
            input_tokens: 1000,
            output_tokens: 1000,
            total_tokens: 2000,
        };
        assert!(u.cost_usd(None).is_none());
    }

    #[test]
    fn cost_uses_configured_rates() {
        let u = Usage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            total_tokens: 1_500_000,
        };
        let p = crate::config::Pricing {
            input_usd_per_mtok: 0.30,
            output_usd_per_mtok: 2.50,
        };
        let c = u.cost_usd(Some(p)).unwrap();
        assert!((c - (0.30 + 1.25)).abs() < 1e-9, "got {c}");
    }
}
