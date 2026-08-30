//! Configuration, loaded from the environment (and therefore from `.env`).
//!
//! Everything that affects a run is captured here so it can be serialized into
//! the trajectory. Reproducibility depends on the run recording its own
//! settings rather than on the reader guessing them.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::env;

/// Which LLM backend to talk to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Provider {
    /// Google Vertex AI (Gemini).
    Vertex,
    /// Deterministic offline stub. Used by tests and by `--dry-run`; never
    /// used to produce reported results.
    Mock,
}

/// How to authenticate against Vertex AI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VertexAuth {
    /// `x-goog-api-key` header. Vertex AI express-mode style key.
    ApiKey,
    /// `Authorization: Bearer <token>` using a token supplied in the
    /// environment (e.g. from `gcloud auth print-access-token`).
    AccessToken,
    /// Shell out to `gcloud auth print-access-token` at client construction.
    /// Convenient locally; requires the gcloud CLI on PATH.
    GcloudCli,
}

/// Token pricing, in USD per million tokens.
///
/// Deliberately not defaulted to any number. If the operator does not supply
/// rates, cost is reported as "not configured" rather than as a guess. The
/// masterplan forbids inventing values for presentation (§10).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pricing {
    pub input_usd_per_mtok: f64,
    pub output_usd_per_mtok: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: Provider,
    pub model: String,
    /// Vertex project id. Required when `provider == Vertex` and auth is not
    /// an API key against the express endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub location: String,
    pub auth: VertexAuth,
    pub temperature: f32,
    pub max_output_tokens: u32,
    pub timeout_secs: u64,
    /// Retries on transport errors, 5xx, 429, and unparseable responses.
    pub max_retries: u32,
    /// `None` means "operator did not configure pricing"; cost is then
    /// reported as unavailable rather than as zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<Pricing>,
}

/// Settings that control how a review run behaves. Recorded in the trajectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub llm: LlmConfig,
    /// Line-range slack used when matching a prediction to ground truth.
    pub match_line_tolerance: u32,
    /// Upper bound on investigation tool calls per candidate finding.
    pub max_tool_calls_per_finding: u32,
    /// Upper bound on lines returned by a single bounded file read.
    pub max_read_lines: u32,
    /// Upper bound on matches returned by a single search.
    pub max_search_results: u32,
}

fn env_opt(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn env_or<T: std::str::FromStr>(key: &str, default: T) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    match env_opt(key) {
        None => Ok(default),
        Some(v) => v
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("{key}: cannot parse {v:?}: {e}")),
    }
}

impl RunConfig {
    /// Load configuration from the process environment.
    ///
    /// `dotenvy` is expected to have run first; missing `.env` is not an error
    /// because the operator may export variables directly.
    pub fn from_env() -> Result<Self> {
        let provider = match env_opt("VCR_PROVIDER").as_deref() {
            None | Some("vertex") => Provider::Vertex,
            Some("mock") => Provider::Mock,
            Some(other) => bail!("VCR_PROVIDER: expected `vertex` or `mock`, got {other:?}"),
        };

        let auth = match env_opt("VERTEX_AUTH").as_deref() {
            None | Some("api_key") => VertexAuth::ApiKey,
            Some("access_token") => VertexAuth::AccessToken,
            Some("gcloud") => VertexAuth::GcloudCli,
            Some(other) => {
                bail!("VERTEX_AUTH: expected `api_key`, `access_token`, or `gcloud`, got {other:?}")
            }
        };

        let model =
            env_opt("VERTEX_MODEL").context("VERTEX_MODEL is required (see .env.example)")?;
        let location = env_opt("VERTEX_LOCATION").unwrap_or_else(|| "global".to_string());
        let project_id = env_opt("VERTEX_PROJECT_ID");

        if provider == Provider::Vertex && auth != VertexAuth::ApiKey && project_id.is_none() {
            bail!("VERTEX_PROJECT_ID is required when VERTEX_AUTH is `access_token` or `gcloud`");
        }

        // Pricing is all-or-nothing: half-configured pricing would silently
        // under-report cost.
        let pricing = match (
            env_opt("VCR_PRICE_INPUT_USD_PER_MTOK"),
            env_opt("VCR_PRICE_OUTPUT_USD_PER_MTOK"),
        ) {
            (Some(i), Some(o)) => Some(Pricing {
                input_usd_per_mtok: i
                    .parse()
                    .context("VCR_PRICE_INPUT_USD_PER_MTOK must be a number")?,
                output_usd_per_mtok: o
                    .parse()
                    .context("VCR_PRICE_OUTPUT_USD_PER_MTOK must be a number")?,
            }),
            (None, None) => None,
            _ => bail!(
                "pricing must be fully configured or fully absent: set both \
                 VCR_PRICE_INPUT_USD_PER_MTOK and VCR_PRICE_OUTPUT_USD_PER_MTOK, or neither"
            ),
        };

        Ok(Self {
            llm: LlmConfig {
                provider,
                model,
                project_id,
                location,
                auth,
                temperature: env_or("VCR_TEMPERATURE", 0.0_f32)?,
                max_output_tokens: env_or("VCR_MAX_OUTPUT_TOKENS", 8192_u32)?,
                timeout_secs: env_or("VCR_TIMEOUT_SECS", 180_u64)?,
                max_retries: env_or("VCR_MAX_RETRIES", 3_u32)?,
                pricing,
            },
            match_line_tolerance: env_or("VCR_MATCH_LINE_TOLERANCE", 3_u32)?,
            max_tool_calls_per_finding: env_or("VCR_MAX_TOOL_CALLS_PER_FINDING", 8_u32)?,
            max_read_lines: env_or("VCR_MAX_READ_LINES", 200_u32)?,
            max_search_results: env_or("VCR_MAX_SEARCH_RESULTS", 40_u32)?,
        })
    }

    /// Offline configuration used by tests and `--dry-run`.
    pub fn mock() -> Self {
        Self {
            llm: LlmConfig {
                provider: Provider::Mock,
                model: "mock".to_string(),
                project_id: None,
                location: "none".to_string(),
                auth: VertexAuth::ApiKey,
                temperature: 0.0,
                max_output_tokens: 8192,
                timeout_secs: 30,
                max_retries: 0,
                pricing: None,
            },
            match_line_tolerance: 3,
            max_tool_calls_per_finding: 8,
            max_read_lines: 200,
            max_search_results: 40,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_is_none_when_unset() {
        assert!(RunConfig::mock().llm.pricing.is_none());
    }

    #[test]
    fn mock_config_never_claims_a_price() {
        // Guards against a future default sneaking in: a fabricated rate would
        // silently produce fabricated cost numbers in the results table.
        let c = RunConfig::mock();
        assert!(c.llm.pricing.is_none());
        assert_eq!(c.llm.provider, Provider::Mock);
    }
}
