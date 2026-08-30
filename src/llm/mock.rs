//! Deterministic offline client.
//!
//! Exists so the orchestration, evaluator, and trajectory code can be tested
//! and demonstrated without network access or spend. It is never a source of
//! reported results: every run records its provider, and `report` refuses to
//! present mock runs as measurements.

use super::{LlmError, LlmRequest, LlmResponse, Stage, Usage};

pub struct MockClient;

impl Default for MockClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockClient {
    pub fn new() -> Self {
        Self
    }

    pub async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
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
    async fn mock_reports_zero_tokens() {
        let r = MockClient::new()
            .complete(&req(Stage::Review))
            .await
            .unwrap();
        assert_eq!(r.usage, Usage::default());
    }
}
