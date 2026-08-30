//! Trajectory logging.
//!
//! One trajectory per (case, agent) execution, written as JSON. The hackathon
//! requires trajectories a reader can follow from the agent's instructions to
//! its final result, including tool responses, retries, and human checkpoints
//! — so events are recorded in order, with the full prompt text.
//!
//! Nothing here ever records credentials: prompts are constructed from case
//! material only, and any provider error text is scrubbed before storage.

use crate::config::RunConfig;
use crate::finding::{CandidateFinding, Evidence, Finding, VerificationResult};
use crate::llm::{LlmResponse, Stage, Usage};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Which system produced this trajectory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    /// Direct single-pass review. The fair baseline.
    Baseline,
    /// Repository-aware investigation plus fresh-context falsification.
    Advanced,
}

impl AgentKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentKind::Baseline => "baseline",
            AgentKind::Advanced => "advanced",
        }
    }
}

/// A single step, in execution order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum TrajectoryEvent {
    /// A prompt was sent and a response came back.
    LlmCall {
        stage: Stage,
        prompt_version: String,
        system: String,
        user: String,
        response_text: String,
        usage: Usage,
        latency_ms: u64,
        /// Attempts including the successful one; > 1 means a retry happened.
        attempts: u32,
    },
    /// A request failed permanently after retries.
    LlmFailure {
        stage: Stage,
        prompt_version: String,
        error: String,
        attempts: u32,
    },
    /// The reviewer proposed a candidate finding.
    CandidateProposed { candidate: CandidateFinding },
    /// A repository tool ran on behalf of the agent.
    ToolCall {
        tool_call_id: String,
        candidate_id: String,
        tool: String,
        arguments: serde_json::Value,
        /// Verbatim tool output, truncated to the configured bound.
        response: String,
        /// False when the tool refused (bad path, no matches, limit hit).
        ok: bool,
        duration_ms: u64,
    },
    /// The falsification question for a candidate was fixed.
    FalsificationQuestion {
        candidate_id: String,
        question: String,
    },
    /// The evidence package handed to the fresh verifier.
    EvidenceAssembled {
        candidate_id: String,
        evidence: Vec<Evidence>,
    },
    /// The fresh-context verifier's judgment.
    Verification {
        candidate_id: String,
        result: VerificationResult,
    },
    /// The orchestrator's final classification, and why.
    Decision {
        candidate_id: String,
        status: crate::finding::FindingStatus,
        reason: String,
    },
    /// A point where a human must act. The system never proceeds past a
    /// consequential action on its own.
    HumanCheckpoint { note: String },
    /// Free-text note from the orchestrator.
    Note { note: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub trajectory_id: String,
    pub case_id: String,
    pub agent: AgentKind,
    pub model: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// Full run configuration, so a reader can reproduce the conditions.
    pub config: RunConfig,
    pub events: Vec<TrajectoryEvent>,
    pub final_findings: Vec<Finding>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    pub runtime_ms: u64,
    pub llm_calls: u32,
    pub tool_calls: u32,
    /// Extra attempts beyond the first, summed across calls.
    pub retries: u32,
}

impl Trajectory {
    pub fn new(case_id: &str, agent: AgentKind, config: &RunConfig) -> Self {
        Self {
            trajectory_id: uuid::Uuid::new_v4().to_string(),
            case_id: case_id.to_string(),
            agent,
            model: config.llm.model.clone(),
            started_at: chrono::Utc::now().to_rfc3339(),
            finished_at: None,
            config: config.clone(),
            events: Vec::new(),
            final_findings: Vec::new(),
            usage: Usage::default(),
            cost_usd: None,
            runtime_ms: 0,
            llm_calls: 0,
            tool_calls: 0,
            retries: 0,
        }
    }

    pub fn push(&mut self, event: TrajectoryEvent) {
        match &event {
            TrajectoryEvent::LlmCall {
                usage, attempts, ..
            } => {
                self.llm_calls += 1;
                self.retries += attempts.saturating_sub(1);
                self.usage.add(usage);
            }
            TrajectoryEvent::LlmFailure { attempts, .. } => {
                self.llm_calls += 1;
                self.retries += attempts.saturating_sub(1);
            }
            TrajectoryEvent::ToolCall { .. } => self.tool_calls += 1,
            _ => {}
        }
        self.events.push(event);
    }

    /// Record a successful LLM call together with the prompt that produced it.
    pub fn record_call(
        &mut self,
        stage: Stage,
        prompt_version: &str,
        system: &str,
        user: &str,
        resp: &LlmResponse,
    ) {
        self.push(TrajectoryEvent::LlmCall {
            stage,
            prompt_version: prompt_version.to_string(),
            system: system.to_string(),
            user: user.to_string(),
            response_text: resp.text.clone(),
            usage: resp.usage,
            latency_ms: resp.latency_ms,
            attempts: resp.attempts,
        });
    }

    pub fn record_failure(
        &mut self,
        stage: Stage,
        prompt_version: &str,
        error: &str,
        attempts: u32,
    ) {
        self.push(TrajectoryEvent::LlmFailure {
            stage,
            prompt_version: prompt_version.to_string(),
            error: scrub(error),
            attempts,
        });
    }

    pub fn finish(&mut self, findings: Vec<Finding>, runtime_ms: u64) {
        self.final_findings = findings;
        self.runtime_ms = runtime_ms;
        self.finished_at = Some(chrono::Utc::now().to_rfc3339());
        self.cost_usd = self.usage.cost_usd(self.config.llm.pricing);
    }

    /// The experimental arm this trajectory belongs to: the agent, plus the
    /// ablation when one is active.
    ///
    /// Filenames are built from this rather than from the agent alone, because
    /// an ablation run and a full run share an agent and would otherwise
    /// collide — and the evaluator, which looks runs up by arm, would not find
    /// them.
    pub fn arm(&self) -> String {
        format!("{}{}", self.agent.as_str(), self.config.ablation.suffix())
    }

    /// Write the trajectory as pretty JSON under `dir`.
    pub fn write(&self, dir: impl AsRef<Path>) -> Result<std::path::PathBuf> {
        let dir = dir.as_ref();
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let path = dir.join(format!("{}-{}.json", self.case_id, self.arm()));
        let json = serde_json::to_string_pretty(self).context("serializing trajectory")?;
        std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(path)
    }
}

/// Remove anything that looks like a credential from text bound for disk.
///
/// Prompts are built from case material and never contain secrets; this
/// guards the one path that could — provider error bodies echoing a header.
pub fn scrub(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for token in s.split_inclusive(char::is_whitespace) {
        let t = token.trim();
        let looks_secret = t.starts_with("AIza")
            || t.starts_with("ya29.")
            || t.starts_with("sk-")
            || (t.len() > 40
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        if looks_secret {
            out.push_str("[REDACTED]");
            if token.len() > t.len() {
                out.push(' ');
            }
        } else {
            out.push_str(token);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RunConfig;

    fn traj() -> Trajectory {
        Trajectory::new("c01", AgentKind::Baseline, &RunConfig::mock())
    }

    fn resp(input: u64, output: u64, attempts: u32) -> LlmResponse {
        LlmResponse {
            text: "{}".into(),
            usage: Usage {
                input_tokens: input,
                output_tokens: output,
                total_tokens: input + output,
            },
            latency_ms: 5,
            attempts,
            model: "mock".into(),
        }
    }

    #[test]
    fn accumulates_usage_across_calls() {
        let mut t = traj();
        t.record_call(Stage::Review, "v1", "s", "u", &resp(100, 50, 1));
        t.record_call(Stage::Verify, "v1", "s", "u", &resp(200, 25, 1));
        assert_eq!(t.usage.input_tokens, 300);
        assert_eq!(t.usage.output_tokens, 75);
        assert_eq!(t.llm_calls, 2);
    }

    #[test]
    fn counts_retries_as_attempts_beyond_the_first() {
        let mut t = traj();
        t.record_call(Stage::Review, "v1", "s", "u", &resp(1, 1, 3));
        t.record_call(Stage::Review, "v1", "s", "u", &resp(1, 1, 1));
        assert_eq!(t.retries, 2);
    }

    #[test]
    fn failures_are_recorded_and_counted() {
        let mut t = traj();
        t.record_failure(Stage::Review, "v1", "boom", 4);
        assert_eq!(t.llm_calls, 1);
        assert_eq!(t.retries, 3);
        assert!(matches!(t.events[0], TrajectoryEvent::LlmFailure { .. }));
    }

    #[test]
    fn cost_is_none_when_pricing_is_unset() {
        let mut t = traj();
        t.record_call(Stage::Review, "v1", "s", "u", &resp(1_000_000, 0, 1));
        t.finish(vec![], 10);
        assert!(t.cost_usd.is_none());
    }

    #[test]
    fn cost_is_computed_when_pricing_is_set() {
        let mut cfg = RunConfig::mock();
        cfg.llm.pricing = Some(crate::config::Pricing {
            input_usd_per_mtok: 1.0,
            output_usd_per_mtok: 4.0,
        });
        let mut t = Trajectory::new("c01", AgentKind::Advanced, &cfg);
        t.record_call(
            Stage::Review,
            "v1",
            "s",
            "u",
            &resp(1_000_000, 1_000_000, 1),
        );
        t.finish(vec![], 10);
        assert!((t.cost_usd.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn tool_calls_are_counted() {
        let mut t = traj();
        t.push(TrajectoryEvent::ToolCall {
            tool_call_id: "t1".into(),
            candidate_id: "p1".into(),
            tool: "search".into(),
            arguments: serde_json::json!({"pattern": "unwrap"}),
            response: "src/a.rs:12".into(),
            ok: true,
            duration_ms: 3,
        });
        assert_eq!(t.tool_calls, 1);
    }

    #[test]
    fn roundtrips_through_json() {
        let mut t = traj();
        t.record_call(Stage::Review, "v1", "sys", "usr", &resp(10, 10, 1));
        t.push(TrajectoryEvent::HumanCheckpoint {
            note: "human reviews findings".into(),
        });
        t.finish(vec![], 42);
        let json = serde_json::to_string(&t).unwrap();
        let back: Trajectory = serde_json::from_str(&json).unwrap();
        assert_eq!(back.runtime_ms, 42);
        assert_eq!(back.events.len(), 2);
        assert_eq!(back.case_id, "c01");
    }

    #[test]
    fn writes_a_predictable_filename() {
        let dir = tempfile::tempdir().unwrap();
        let mut t = traj();
        t.finish(vec![], 1);
        let path = t.write(dir.path()).unwrap();
        assert!(path.ends_with("c01-baseline.json"));
        assert!(path.is_file());
    }

    #[test]
    fn an_ablation_run_writes_under_its_own_arm_name() {
        // A real bug this caught: ablation trajectories were named after the
        // agent, so a no-falsification run wrote `<case>-advanced.json` while
        // the evaluator looked for `<case>-advanced-no-falsification.json` and
        // failed to score the run at all.
        let mut cfg = RunConfig::mock();
        cfg.ablation = crate::config::Ablation::NoFalsification;
        let mut t = Trajectory::new("c01", AgentKind::Advanced, &cfg);
        t.finish(vec![], 1);

        assert_eq!(t.arm(), "advanced-no-falsification");
        let dir = tempfile::tempdir().unwrap();
        let path = t.write(dir.path()).unwrap();
        assert!(
            path.ends_with("c01-advanced-no-falsification.json"),
            "got {}",
            path.display()
        );
    }

    #[test]
    fn a_full_run_keeps_the_plain_agent_name() {
        let t = Trajectory::new("c01", AgentKind::Advanced, &RunConfig::mock());
        assert_eq!(t.arm(), "advanced");
    }

    #[test]
    fn scrub_redacts_api_key_shapes() {
        let s = scrub("error: key AIzaSyC0ffee123456 was rejected");
        assert!(!s.contains("AIzaSyC0ffee123456"));
        assert!(s.contains("[REDACTED]"));
    }

    #[test]
    fn scrub_redacts_bearer_tokens() {
        assert!(!scrub("Authorization ya29.a0AfH6SMBxxxx").contains("ya29."));
    }

    #[test]
    fn scrub_keeps_ordinary_prose_intact() {
        let s = "the unwrap on line 42 of src/parser.rs can panic";
        assert_eq!(scrub(s), s);
    }
}
