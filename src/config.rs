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
    ///
    /// Never serialized. `RunConfig` is embedded verbatim in every trajectory
    /// and summary, and those are part of the submission; a cloud project id
    /// identifies the operator and is nobody else's business. It is also not
    /// needed to reproduce a run — whoever reproduces it supplies their own in
    /// `.env`. `serde(default)` keeps older artifacts loadable.
    #[serde(skip_serializing, default)]
    pub project_id: Option<String>,
    pub location: String,
    pub auth: VertexAuth,
    pub temperature: f32,
    pub max_output_tokens: u32,
    pub timeout_secs: u64,
    /// Retries on transport errors, 5xx, 429, and unparseable responses.
    pub max_retries: u32,
    /// Minimum gap between requests, in milliseconds.
    ///
    /// The advanced reviewer makes several calls per case where the baseline
    /// makes one, so it reaches a per-minute quota roughly six times sooner.
    /// Without pacing, a comparison run under quota pressure measures the
    /// quota as much as the systems, and penalises only the arm under test.
    ///
    /// Defaulted on deserialize so trajectories written before this field
    /// existed stay loadable. Recorded runs are evidence; a new setting must
    /// never make an old artifact unreadable.
    #[serde(default)]
    pub min_request_interval_ms: u64,
    /// `None` means "operator did not configure pricing"; cost is then
    /// reported as unavailable rather than as zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing: Option<Pricing>,
}

/// Which part of the advanced pipeline to switch off, so its contribution can
/// be measured rather than argued about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ablation {
    /// The complete pipeline.
    None,
    /// Skip the falsification question and the fresh-context verifier
    /// entirely. Any candidate backed by investigation evidence is reported.
    /// Isolates what falsification contributes on top of investigation.
    NoFalsification,
    /// Keep falsification, but never send an "Insufficient" verdict back for
    /// a second targeted look. Isolates the self-correction feedback loop.
    NoFollowup,
    /// Skip investigation and verification. Candidates are reported as
    /// produced, which makes the advanced arm a second baseline with a
    /// different prompt. Isolates the prompt from the machinery.
    CandidatesOnly,
    /// Keep everything, but never take a second look at a case that finished
    /// with nothing to report. Isolates the one feedback path that sends
    /// falsification output back into candidate generation.
    NoSecondLook,
}

impl Ablation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ablation::None => "none",
            Ablation::NoFalsification => "no-falsification",
            Ablation::NoFollowup => "no-followup",
            Ablation::CandidatesOnly => "candidates-only",
            Ablation::NoSecondLook => "no-second-look",
        }
    }

    /// Suffix for output filenames, empty for a full run so the default
    /// artifacts keep their existing names.
    pub fn suffix(&self) -> String {
        match self {
            Ablation::None => String::new(),
            other => format!("-{}", other.as_str()),
        }
    }
}

/// Settings that control how a review run behaves. Recorded in the trajectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    pub llm: LlmConfig,
    /// Line-range slack used when matching a prediction to ground truth.
    pub match_line_tolerance: u32,
    /// Upper bound on investigation tool calls per candidate finding.
    pub max_tool_calls_per_finding: u32,
    /// How many times an "Insufficient" verdict may send the investigation
    /// back for more evidence, steered by the gap the verifier named.
    ///
    /// 0 disables the feedback loop, which is what the `no-followup` ablation
    /// measures.
    ///
    /// Defaulted on deserialize for the same reason as
    /// [`LlmConfig::min_request_interval_ms`]: older trajectories predate the
    /// field, and they must still load.
    #[serde(default = "default_followups")]
    pub max_followup_investigations: u32,
    /// Upper bound on lines returned by a single bounded file read.
    pub max_read_lines: u32,
    /// Upper bound on matches returned by a single search.
    pub max_search_results: u32,
    /// How many times a case that reported nothing may be looked at again.
    ///
    /// Defaulted for the same reason as `max_followup_investigations`: a
    /// trajectory recorded before this setting existed has no such field and
    /// must still load. A recorded run is evidence, and a new knob must never
    /// make old evidence unreadable.
    #[serde(default = "default_second_looks")]
    pub max_second_looks: u32,
    /// Whether within-case memory carries the *content* of regions already
    /// read, rather than a one-line summary of each lookup.
    ///
    /// Off by default, for the same reason the second look is: the shipped
    /// figures were measured without it, and turning it on changes what the
    /// investigator sees. Measured separately; see the changelog.
    #[serde(default)]
    pub memory_carries_content: bool,
    /// Which stage, if any, is switched off for this run.
    #[serde(default = "ablation_none")]
    pub ablation: Ablation,
}

fn ablation_none() -> Ablation {
    Ablation::None
}

fn default_followups() -> u32 {
    1
}

/// Off by default, and that is a measurement rather than a hedge.
///
/// The second look fires on exactly the cases that report nothing, which on
/// both benchmarks means the traps. Across 6 firings it declined 5 times --
/// the correct answer -- and on the sixth proposed a true-but-immaterial claim
/// that the verifier confirmed and the evaluator scored as a false positive.
/// It bought no recall on either benchmark and cost roughly 14% more per case.
///
/// The five-trial headline was measured without it, and a single 12-case trial
/// with it scored F1 1.000 -- which is inside the noise of 0.988 +- 0.026 and
/// is exactly the kind of single run this project has already been flattered
/// by once. So it ships off, with the code, the tests and the ablation flag
/// kept, and the numbers reported.
fn default_second_looks() -> u32 {
    0
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
                max_retries: env_or("VCR_MAX_RETRIES", 5_u32)?,
                min_request_interval_ms: env_or("VCR_MIN_REQUEST_INTERVAL_MS", 1_500_u64)?,
                pricing,
            },
            match_line_tolerance: env_or("VCR_MATCH_LINE_TOLERANCE", 3_u32)?,
            max_tool_calls_per_finding: env_or("VCR_MAX_TOOL_CALLS_PER_FINDING", 8_u32)?,
            max_followup_investigations: env_or("VCR_MAX_FOLLOWUP_INVESTIGATIONS", 1_u32)?,
            max_second_looks: env_or("VCR_MAX_SECOND_LOOKS", 0_u32)?,
            memory_carries_content: env_opt("VCR_MEMORY_CARRIES_CONTENT")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            max_read_lines: env_or("VCR_MAX_READ_LINES", 200_u32)?,
            max_search_results: env_or("VCR_MAX_SEARCH_RESULTS", 40_u32)?,
            ablation: Ablation::None,
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
                min_request_interval_ms: 0,
                pricing: None,
            },
            match_line_tolerance: 3,
            max_tool_calls_per_finding: 8,
            max_followup_investigations: 1,
            // Enabled in the offline config so the branch is exercised by
            // tests. The shipped default is 0; see `default_second_looks`.
            max_second_looks: 1,
            memory_carries_content: false,
            max_read_lines: 200,
            max_search_results: 40,
            ablation: Ablation::None,
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
    fn a_trajectory_written_before_newer_settings_existed_still_loads() {
        // Recorded runs are evidence. Adding a config field must never make an
        // older artifact unreadable — this exact break stopped `vcr triage`
        // from opening runs recorded a few commits earlier.
        let older = r#"{
            "llm": {
                "provider": "Vertex",
                "model": "gemini-3.7-flash",
                "location": "global",
                "auth": "ApiKey",
                "temperature": 0.0,
                "max_output_tokens": 8192,
                "timeout_secs": 180,
                "max_retries": 3
            },
            "match_line_tolerance": 3,
            "max_tool_calls_per_finding": 8,
            "max_read_lines": 200,
            "max_search_results": 40
        }"#;

        let cfg: RunConfig = serde_json::from_str(older).expect("older config must still load");
        assert_eq!(cfg.llm.model, "gemini-3.7-flash");
        assert_eq!(cfg.llm.min_request_interval_ms, 0);
        assert_eq!(cfg.max_followup_investigations, 1);
        assert_eq!(cfg.ablation, Ablation::None);
        assert!(cfg.llm.project_id.is_none());
    }

    #[test]
    fn project_id_never_reaches_a_serialized_artifact() {
        // Trajectories and summaries embed RunConfig verbatim and ship in the
        // submission. The operator's cloud project id must not travel with
        // them.
        let mut cfg = RunConfig::mock();
        cfg.llm.project_id = Some("some-private-project-4815".to_string());
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("some-private-project-4815"));
        assert!(!json.contains("project_id"));
    }

    #[test]
    fn a_config_without_project_id_still_deserializes() {
        let cfg = RunConfig::mock();
        let json = serde_json::to_string(&cfg).unwrap();
        let back: RunConfig = serde_json::from_str(&json).unwrap();
        assert!(back.llm.project_id.is_none());
        assert_eq!(back.llm.model, cfg.llm.model);
    }

    #[test]
    fn mock_config_never_claims_a_price() {
        // Guards against a future default sneaking in: a fabricated rate would
        // silently produce fabricated cost numbers in the results table.
        let c = RunConfig::mock();
        assert!(c.llm.pricing.is_none());
        assert_eq!(c.llm.provider, Provider::Mock);
    }
    #[test]
    fn the_second_look_ships_off() {
        // Guards a decision, not an implementation detail. The headline
        // figures were measured with this at 0, and a run that silently
        // enabled it would no longer describe the configuration reported in
        // the README.
        assert_eq!(default_second_looks(), 0);
    }

    #[test]
    fn a_trajectory_recorded_before_the_second_look_existed_still_loads() {
        // Same rule as `max_followup_investigations`: a recorded run is
        // evidence, and a new setting must never make old evidence
        // unreadable.
        let older = r#"{
            "llm": {"provider":"Vertex","model":"m","location":"global","auth":"ApiKey",
                    "temperature":0.0,"max_output_tokens":8192,"timeout_secs":180,"max_retries":5},
            "match_line_tolerance": 3,
            "max_tool_calls_per_finding": 8,
            "max_read_lines": 200,
            "max_search_results": 40
        }"#;
        let cfg: RunConfig =
            serde_json::from_str(older).expect("an older trajectory must still load");
        assert_eq!(cfg.max_second_looks, 0);
        assert_eq!(cfg.max_followup_investigations, 1);
    }
}
