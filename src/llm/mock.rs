//! Deterministic offline client.
//!
//! Exists so the orchestration, evaluator, and trajectory code can be tested
//! and demonstrated without network access or spend. It is never a source of
//! reported results: every run records its provider, and `report` refuses to
//! present mock runs as measurements.

use super::{LlmError, LlmRequest, LlmResponse, Stage, Usage};
use std::collections::HashMap;
use std::sync::Mutex;

/// Responses queued per stage, consumed in order.
///
/// Exists so multi-turn behaviour can be tested without a provider. A stage
/// whose queue is exhausted falls back to its fixed default, so a script only
/// needs to specify the turns it cares about.
type Script = HashMap<Stage, Vec<String>>;

pub struct MockClient {
    /// `None` for the plain stub; `Some` when driving a scripted scenario.
    script: Option<Mutex<Script>>,
}

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClient {
    pub fn new() -> Self {
        Self { script: None }
    }

    /// A mock that returns queued responses per stage before falling back to
    /// the fixed defaults.
    pub fn scripted(script: Script) -> Self {
        Self {
            script: Some(Mutex::new(script)),
        }
    }

    /// Pop the next scripted response for `stage`, if one remains.
    fn next_scripted(&self, stage: Stage) -> Option<String> {
        let script = self.script.as_ref()?;
        let mut guard = script.lock().expect("mock script poisoned");
        let queue = guard.get_mut(&stage)?;
        if queue.is_empty() {
            return None;
        }
        Some(queue.remove(0))
    }

    pub async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        if let Some(scripted) = self.next_scripted(req.stage) {
            return Ok(LlmResponse {
                text: scripted,
                usage: Usage::default(),
                latency_ms: 0,
                attempts: 1,
                model: "mock-scripted".to_string(),
            });
        }

        let text = match req.stage {
            Stage::Review => r#"{"findings":[]}"#.to_string(),
            Stage::Investigate => r#"{"done":true,"tool":null,"rationale":"mock"}"#.to_string(),
            Stage::Falsify => {
                r#"{"falsification_question":"mock: what evidence would disprove this?"}"#
                    .to_string()
            }
            Stage::Verify => {
                r#"{"outcome":"Insufficient","rationale":"mock verifier","decisive_evidence":[]}"#
                    .to_string()
            }
        };

        // Token counts are deliberately zero: a mock must not contribute
        // plausible-looking numbers to a cost table.
        Ok(LlmResponse {
            text,
            usage: Usage::default(),
            latency_ms: 0,
            attempts: 1,
            model: "mock".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(stage: Stage) -> LlmRequest {
        LlmRequest {
            stage,
            system: String::new(),
            user: String::new(),
            json_mode: true,
        }
    }

    #[tokio::test]
    async fn every_stage_returns_parseable_json() {
        let c = MockClient::new();
        for stage in [
            Stage::Review,
            Stage::Investigate,
            Stage::Falsify,
            Stage::Verify,
        ] {
            let r = c.complete(&req(stage)).await.unwrap();
            super::super::extract_json(&r.text).expect("mock output must parse");
        }
    }

    #[tokio::test]
    async fn a_script_is_consumed_in_order_then_falls_back() {
        let mut script = std::collections::HashMap::new();
        script.insert(
            Stage::Verify,
            vec![r#"{"outcome":"Contradicts"}"#.to_string()],
        );
        let c = MockClient::scripted(script);

        let first = c.complete(&req(Stage::Verify)).await.unwrap();
        assert!(first.text.contains("Contradicts"));

        // Queue exhausted, so the fixed default returns.
        let second = c.complete(&req(Stage::Verify)).await.unwrap();
        assert!(second.text.contains("Insufficient"));
    }

    #[tokio::test]
    async fn an_unscripted_stage_uses_its_default() {
        let mut script = std::collections::HashMap::new();
        script.insert(Stage::Verify, vec![r#"{"outcome":"Supports"}"#.to_string()]);
        let c = MockClient::scripted(script);
        let r = c.complete(&req(Stage::Review)).await.unwrap();
        assert!(r.text.contains("findings"));
    }

    #[tokio::test]
    async fn mock_reports_zero_tokens() {
        let r = MockClient::new()
            .complete(&req(Stage::Review))
            .await
            .unwrap();
        assert_eq!(r.usage, Usage::default());
    }
}
