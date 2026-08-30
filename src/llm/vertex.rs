//! Google Vertex AI (Gemini) client.

use super::{LlmError, LlmRequest, LlmResponse, Usage};
use crate::config::{LlmConfig, VertexAuth};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::time::{Duration, Instant};

/// Credential material resolved once at construction.
enum Credential {
    ApiKey(String),
    Bearer(String),
}

pub struct VertexClient {
    http: reqwest::Client,
    endpoint: String,
    credential: Credential,
    model: String,
    temperature: f32,
    max_output_tokens: u32,
    timeout: Duration,
    max_retries: u32,
}

impl VertexClient {
    pub fn new(cfg: &LlmConfig) -> Result<Self> {
        let credential = match cfg.auth {
            VertexAuth::ApiKey => Credential::ApiKey(
                require_env("VERTEX_API_KEY")
                    .context("VERTEX_AUTH=api_key requires VERTEX_API_KEY")?,
            ),
            VertexAuth::AccessToken => Credential::Bearer(
                require_env("VERTEX_ACCESS_TOKEN")
                    .context("VERTEX_AUTH=access_token requires VERTEX_ACCESS_TOKEN")?,
            ),
            VertexAuth::GcloudCli => Credential::Bearer(gcloud_access_token()?),
        };

        let endpoint = build_endpoint(cfg)?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()
            .context("building HTTP client")?;

        Ok(Self {
            http,
            endpoint,
            credential,
            model: cfg.model.clone(),
            temperature: cfg.temperature,
            max_output_tokens: cfg.max_output_tokens,
            timeout: Duration::from_secs(cfg.timeout_secs),
            max_retries: cfg.max_retries,
        })
    }

    pub async fn complete(&self, req: &LlmRequest) -> Result<LlmResponse, LlmError> {
        let started = Instant::now();
        let mut last_err = None;

        for attempt in 1..=(self.max_retries + 1) {
            match self.attempt(req).await {
                Ok((text, usage)) => {
                    return Ok(LlmResponse {
                        text,
                        usage,
                        latency_ms: started.elapsed().as_millis() as u64,
                        attempts: attempt,
                        model: self.model.clone(),
                    })
                }
                Err(e) => {
                    if !e.is_retryable() || attempt > self.max_retries {
                        return Err(e);
                    }
                    // Exponential backoff, capped. Keeps a 429 storm from
                    // burning the deadline.
                    let backoff = Duration::from_millis(500u64 << (attempt - 1).min(4));
                    tokio::time::sleep(backoff).await;
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| LlmError::Transport("retry loop exhausted".into())))
    }

    async fn attempt(&self, req: &LlmRequest) -> Result<(String, Usage), LlmError> {
        let mut generation_config = json!({
            "temperature": self.temperature,
            "maxOutputTokens": self.max_output_tokens,
        });
        if req.json_mode {
            generation_config["responseMimeType"] = json!("application/json");
        }

        let body = json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": req.user }]
            }],
            "systemInstruction": {
                "parts": [{ "text": req.system }]
            },
            "generationConfig": generation_config,
        });

        let mut rb = self.http.post(&self.endpoint).json(&body);
        rb = match &self.credential {
            Credential::ApiKey(k) => rb.header("x-goog-api-key", k),
            Credential::Bearer(t) => rb.bearer_auth(t),
        };

        let resp = match tokio::time::timeout(self.timeout, rb.send()).await {
            Err(_) => return Err(LlmError::Timeout(self.timeout.as_secs())),
            Ok(Err(e)) if e.is_timeout() => return Err(LlmError::Timeout(self.timeout.as_secs())),
            Ok(Err(e)) => return Err(LlmError::Transport(e.to_string())),
            Ok(Ok(r)) => r,
        };

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| LlmError::Transport(format!("reading body: {e}")))?;

        if !status.is_success() {
            return Err(LlmError::Status {
                status: status.as_u16(),
                body: truncate(&text, 800),
            });
        }

        parse_generate_content(&text)
    }
}

/// Build the `generateContent` URL.
///
/// Two shapes exist:
///
/// * **Express mode** — an API key with no project. The URL carries no project
///   or location segment; the key identifies everything.
/// * **Full Vertex** — a project and location path. Required for bearer-token
///   auth, and available to API keys that name a project.
///
/// For the full form, the `global` location uses the unprefixed host while
/// regional locations use a `{location}-` prefix.
fn build_endpoint(cfg: &LlmConfig) -> Result<String> {
    let location = if cfg.location.trim().is_empty() {
        "global"
    } else {
        cfg.location.trim()
    };

    // An API key authenticates against the express endpoint, which carries no
    // project or location segment — the key identifies both. A project may
    // still be configured for reference; it simply does not belong in this URL.
    if cfg.auth == VertexAuth::ApiKey {
        return Ok(format!(
            "https://aiplatform.googleapis.com/v1/publishers/google/models/{}:generateContent",
            cfg.model
        ));
    }

    let project = match &cfg.project_id {
        Some(p) => p.clone(),
        None => bail!(
            "VERTEX_PROJECT_ID is required to build the Vertex endpoint; \
             set it in .env"
        ),
    };

    let host = if location == "global" {
        "aiplatform.googleapis.com".to_string()
    } else {
        format!("{location}-aiplatform.googleapis.com")
    };

    Ok(format!(
        "https://{host}/v1/projects/{project}/locations/{location}/publishers/google/models/{}:generateContent",
        cfg.model
    ))
}

/// Extract text and usage from a Vertex `generateContent` response.
fn parse_generate_content(raw: &str) -> Result<(String, Usage), LlmError> {
    let v: serde_json::Value = serde_json::from_str(raw)
        .map_err(|e| LlmError::Malformed(format!("response was not JSON: {e}")))?;

    let usage = v
        .get("usageMetadata")
        .map(|u| {
            let input = u
                .get("promptTokenCount")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            // Thinking models bill reasoning tokens as output; count them so
            // cost is not under-reported.
            let output = u
                .get("candidatesTokenCount")
                .and_then(|x| x.as_u64())
                .unwrap_or(0)
                + u.get("thoughtsTokenCount")
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0);
            let total = u
                .get("totalTokenCount")
                .and_then(|x| x.as_u64())
                .unwrap_or(input + output);
            Usage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: total,
            }
        })
        .unwrap_or_default();

    let candidate = v
        .get("candidates")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first());

    let Some(candidate) = candidate else {
        // A blocked prompt returns no candidates but does return a reason.
        let reason = v
            .get("promptFeedback")
            .and_then(|f| f.get("blockReason"))
            .and_then(|r| r.as_str())
            .unwrap_or("no candidates in response");
        return Err(LlmError::Malformed(format!(
            "no candidate returned ({reason})"
        )));
    };

    // A response truncated by the token cap yields text that will not parse as
    // JSON downstream; surfacing it here gives a clearer trajectory entry.
    if let Some("MAX_TOKENS") = candidate.get("finishReason").and_then(|r| r.as_str()) {
        return Err(LlmError::Malformed(
            "response truncated at maxOutputTokens; raise VCR_MAX_OUTPUT_TOKENS".into(),
        ));
    }

    let text: String = candidate
        .get("content")
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter(|p| {
                    // Skip the model's internal thought parts; they are not the
                    // answer and would corrupt JSON parsing.
                    !p.get("thought").and_then(|t| t.as_bool()).unwrap_or(false)
                })
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if text.trim().is_empty() {
        return Err(LlmError::Malformed("candidate contained no text".into()));
    }

    Ok((text, usage))
}

fn require_env(key: &str) -> Result<String> {
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => Ok(v.trim().to_string()),
        _ => bail!("{key} is not set"),
    }
}

/// Fetch an access token via the gcloud CLI.
fn gcloud_access_token() -> Result<String> {
    let out = std::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()
        .context("running `gcloud auth print-access-token` (is the gcloud CLI on PATH?)")?;

    if !out.status.success() {
        bail!(
            "`gcloud auth print-access-token` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }

    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if token.is_empty() {
        bail!("`gcloud auth print-access-token` returned an empty token");
    }
    Ok(token)
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let mut end = n;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... [{} more chars]", &s[..end], s.len() - end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Provider, VertexAuth};

    fn cfg(location: &str) -> LlmConfig {
        LlmConfig {
            provider: Provider::Vertex,
            model: "gemini-test".into(),
            project_id: Some("my-project".into()),
            location: location.into(),
            auth: VertexAuth::ApiKey,
            temperature: 0.0,
            max_output_tokens: 1024,
            timeout_secs: 30,
            max_retries: 1,
            pricing: None,
        }
    }

    fn bearer(location: &str) -> LlmConfig {
        let mut c = cfg(location);
        c.auth = VertexAuth::AccessToken;
        c
    }

    #[test]
    fn global_location_uses_unprefixed_host() {
        let url = build_endpoint(&bearer("global")).unwrap();
        assert!(url.starts_with(
            "https://aiplatform.googleapis.com/v1/projects/my-project/locations/global/"
        ));
        assert!(url.ends_with("gemini-test:generateContent"));
    }

    #[test]
    fn regional_location_is_prefixed() {
        let url = build_endpoint(&bearer("us-central1")).unwrap();
        assert!(url.starts_with("https://us-central1-aiplatform.googleapis.com/"));
        assert!(url.contains("/locations/us-central1/"));
    }

    #[test]
    fn api_key_without_a_project_uses_the_express_endpoint() {
        let mut c = cfg("global");
        c.project_id = None;
        let url = build_endpoint(&c).unwrap();
        assert_eq!(
            url,
            "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-test:generateContent"
        );
        assert!(!url.contains("/projects/"));
        assert!(!url.contains("/locations/"));
    }

    #[test]
    fn bearer_auth_without_a_project_is_an_error_not_a_bad_url() {
        let mut c = bearer("global");
        c.project_id = None;
        assert!(build_endpoint(&c).is_err());
    }

    #[test]
    fn api_key_ignores_a_configured_project() {
        // Express keys authenticate against the project-less endpoint. A
        // stray VERTEX_PROJECT_ID must not push the URL onto the full path,
        // where the key would be rejected.
        let url = build_endpoint(&cfg("us-central1")).unwrap();
        assert!(!url.contains("/projects/"));
        assert!(!url.contains("/locations/"));
    }

    #[test]
    fn parses_text_and_usage() {
        let raw = r#"{
          "candidates": [{"content": {"parts": [{"text": "hello"}]}, "finishReason": "STOP"}],
          "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5, "totalTokenCount": 15}
        }"#;
        let (t, u) = parse_generate_content(raw).unwrap();
        assert_eq!(t, "hello");
        assert_eq!(u.input_tokens, 10);
        assert_eq!(u.output_tokens, 5);
        assert_eq!(u.total_tokens, 15);
    }

    #[test]
    fn counts_thinking_tokens_as_output() {
        let raw = r#"{
          "candidates": [{"content": {"parts": [{"text": "ok"}]}}],
          "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 5,
                            "thoughtsTokenCount": 100, "totalTokenCount": 115}
        }"#;
        let (_, u) = parse_generate_content(raw).unwrap();
        assert_eq!(u.output_tokens, 105, "reasoning tokens must not be dropped");
    }

    #[test]
    fn skips_thought_parts_when_joining_text() {
        let raw = r#"{
          "candidates": [{"content": {"parts": [
            {"text": "internal musing", "thought": true},
            {"text": "{\"answer\": 1}"}
          ]}}]
        }"#;
        let (t, _) = parse_generate_content(raw).unwrap();
        assert_eq!(t, "{\"answer\": 1}");
    }

    #[test]
    fn concatenates_multiple_text_parts() {
        let raw = r#"{"candidates":[{"content":{"parts":[{"text":"{\"a\":"},{"text":"1}"}]}}]}"#;
        let (t, _) = parse_generate_content(raw).unwrap();
        assert_eq!(t, "{\"a\":1}");
    }

    #[test]
    fn blocked_prompt_is_malformed_with_reason() {
        let raw = r#"{"promptFeedback": {"blockReason": "SAFETY"}}"#;
        let e = parse_generate_content(raw).unwrap_err();
        assert!(matches!(e, LlmError::Malformed(ref m) if m.contains("SAFETY")));
    }

    #[test]
    fn truncated_generation_is_reported_clearly() {
        let raw = r#"{"candidates":[{"content":{"parts":[{"text":"{\"partial\":"}]},
                      "finishReason":"MAX_TOKENS"}]}"#;
        let e = parse_generate_content(raw).unwrap_err();
        assert!(matches!(e, LlmError::Malformed(ref m) if m.contains("truncated")));
    }

    #[test]
    fn empty_candidate_text_is_malformed() {
        let raw = r#"{"candidates":[{"content":{"parts":[{"text":"   "}]}}]}"#;
        assert!(parse_generate_content(raw).is_err());
    }

    #[test]
    fn non_json_body_is_malformed_not_a_panic() {
        assert!(parse_generate_content("<html>502 Bad Gateway</html>").is_err());
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        let s = "aa\u{e9}\u{e9}\u{e9}bbbb";
        let t = truncate(s, 4);
        assert!(t.starts_with("aa"));
        assert!(t.contains("more chars"));
    }
}
