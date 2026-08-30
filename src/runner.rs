//! Run orchestration and result aggregation.

use crate::agent;
use crate::bench::{self, CaseCategory};
use crate::config::{Provider, RunConfig};
use crate::eval::{self, CaseEvaluation, Counts};
use crate::llm::LlmClient;
use crate::trajectory::{AgentKind, Trajectory};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Per-case operational figures, kept alongside the evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseRunStats {
    pub case_id: String,
    pub trajectory_id: String,
    pub runtime_ms: u64,
    pub llm_calls: u32,
    pub tool_calls: u32,
    pub retries: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Findings actually shown to a human.
    pub reported_findings: u32,
    /// Findings the system produced but withheld.
    pub withheld_findings: u32,
}

impl CaseRunStats {
    fn from_trajectory(t: &Trajectory) -> Self {
        Self {
            case_id: t.case_id.clone(),
            trajectory_id: t.trajectory_id.clone(),
            runtime_ms: t.runtime_ms,
            llm_calls: t.llm_calls,
            tool_calls: t.tool_calls,
            retries: t.retries,
            input_tokens: t.usage.input_tokens,
            output_tokens: t.usage.output_tokens,
            cost_usd: t.cost_usd,
            reported_findings: t
                .final_findings
                .iter()
                .filter(|f| f.status.is_reported())
                .count() as u32,
            withheld_findings: t
                .final_findings
                .iter()
                .filter(|f| !f.status.is_reported())
                .count() as u32,
        }
    }
}

/// Everything one arm of the experiment produced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub agent: AgentKind,
    pub model: String,
    pub provider: Provider,
    pub started_at: String,
    pub finished_at: String,
    pub benchmark_dir: String,
    pub case_count: usize,
    pub config: RunConfig,
    pub stats: Vec<CaseRunStats>,
}

impl RunSummary {
    /// True when these numbers came from the offline stub and therefore must
    /// not be presented as a measurement.
    pub fn is_mock(&self) -> bool {
        self.provider == Provider::Mock
    }
}

/// Run one agent over every case in a benchmark directory.
pub async fn run_benchmark(
    benchmark_dir: &Path,
    agent_kind: AgentKind,
    cfg: &RunConfig,
    out_dir: &Path,
) -> Result<RunSummary> {
    let case_dirs = bench::discover_cases(benchmark_dir)?;
    if case_dirs.is_empty() {
        bail!(
            "no cases found under {} (a case directory must contain case.json)",
            benchmark_dir.display()
        );
    }

    let client = LlmClient::from_config(&cfg.llm).context("constructing LLM client")?;
    let traj_dir = out_dir.join("trajectories").join(agent_kind.as_str());
    let started_at = chrono::Utc::now().to_rfc3339();
    let mut stats = Vec::new();

    for dir in &case_dirs {
        let case = bench::load_case(dir)?;
        eprintln!("[{}] {}", agent_kind.as_str(), case.id());

        let traj = match agent_kind {
            AgentKind::Baseline => agent::baseline::run(&case, &client, cfg).await?,
            AgentKind::Advanced => agent::advanced::run(&case, &client, cfg).await?,
        };

        traj.write(&traj_dir)?;
        stats.push(CaseRunStats::from_trajectory(&traj));
    }

    let summary = RunSummary {
        agent: agent_kind,
        model: cfg.llm.model.clone(),
        provider: cfg.llm.provider,
        started_at,
        finished_at: chrono::Utc::now().to_rfc3339(),
        benchmark_dir: benchmark_dir.display().to_string(),
        case_count: case_dirs.len(),
        config: cfg.clone(),
        stats,
    };

    write_json(
        &out_dir.join(format!("summary-{}.json", agent_kind.as_str())),
        &summary,
    )?;
    Ok(summary)
}

/// Aggregate figures for one arm. Every field is derived from measurements;
/// nothing here is estimated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Aggregate {
    pub agent: AgentKind,
    pub model: String,
    pub provider: Provider,
    pub case_count: usize,
    pub counts: Counts,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub false_positives_per_case: f64,
    /// Reported findings per case. This is the labelled proxy for human review
    /// time: it counts how many items a reviewer must triage. It is NOT a
    /// direct measurement of human review time.
    pub manual_triage_findings_per_case: f64,
    pub withheld_findings_per_case: f64,
    /// `None` when pricing was not configured for the run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mean_cost_usd_per_case: Option<f64>,
    pub mean_runtime_ms_per_case: f64,
    pub mean_llm_calls_per_case: f64,
    pub mean_tool_calls_per_case: f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    /// Set when the run used the offline stub, so a reader cannot mistake it
    /// for a measurement.
    ///
    /// Omitted from the JSON when false to keep real results uncluttered, so
    /// it needs a default on the way back in — absent means a real run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mock_run: bool,
}

/// Full evaluation of one arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evaluation {
    pub aggregate: Aggregate,
    pub per_case: Vec<CaseEvaluation>,
    pub per_case_stats: Vec<CaseRunStats>,
    /// Breakdown by case category, so a precision gain on traps is visible
    /// separately from a recall change on real issues.
    pub by_category: Vec<CategoryBreakdown>,
    pub line_tolerance: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryBreakdown {
    pub category: CaseCategory,
    pub case_count: usize,
    pub counts: Counts,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
}

/// Score a completed run against the benchmark's ground truth.
///
/// `pricing_override` recomputes cost from the token counts already recorded
/// in the run. Token usage is measured during the run; turning it into dollars
/// is arithmetic over published rates, so there is no reason to spend the
/// model again because the operator learned the price afterwards. `None` falls
/// back to whatever cost the run itself recorded.
pub fn evaluate_run(
    benchmark_dir: &Path,
    summary: &RunSummary,
    out_dir: &Path,
    pricing_override: Option<crate::config::Pricing>,
) -> Result<Evaluation> {
    let traj_dir = out_dir.join("trajectories").join(summary.agent.as_str());
    let mut per_case = Vec::new();

    for dir in bench::discover_cases(benchmark_dir)? {
        let case = bench::load_case(&dir)?;
        let truth = bench::load_ground_truth(&dir)?;

        if truth.case_id != case.manifest.case_id {
            bail!(
                "ground_truth.json case_id {:?} does not match case.json case_id {:?}",
                truth.case_id,
                case.manifest.case_id
            );
        }

        let path = traj_dir.join(format!("{}-{}.json", case.id(), summary.agent.as_str()));
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading trajectory {}", path.display()))?;
        let traj: Trajectory = serde_json::from_str(&raw)
            .with_context(|| format!("parsing trajectory {}", path.display()))?;

        per_case.push(eval::evaluate_case(
            case.id(),
            case.manifest.category,
            &traj.final_findings,
            &truth,
            summary.config.match_line_tolerance,
        ));
    }

    let n = per_case.len().max(1) as f64;
    let mut counts = Counts::default();
    for c in &per_case {
        counts.add(&c.counts);
    }

    let sum = |f: fn(&CaseRunStats) -> f64| summary.stats.iter().map(f).sum::<f64>();

    // Per-case cost, recomputed from measured tokens when rates are supplied.
    let case_costs: Vec<Option<f64>> = summary
        .stats
        .iter()
        .map(|s| match pricing_override {
            Some(p) => Some(
                (s.input_tokens as f64 / 1_000_000.0) * p.input_usd_per_mtok
                    + (s.output_tokens as f64 / 1_000_000.0) * p.output_usd_per_mtok,
            ),
            None => s.cost_usd,
        })
        .collect();

    // Cost is aggregated only when every case carried a price; a partial sum
    // would understate it.
    let mean_cost = if !case_costs.is_empty() && case_costs.iter().all(|c| c.is_some()) {
        Some(case_costs.iter().filter_map(|c| *c).sum::<f64>() / n)
    } else {
        None
    };

    // Reflect the recomputed cost in the per-case rows too, so the detail and
    // the aggregate cannot disagree.
    let per_case_stats: Vec<CaseRunStats> = summary
        .stats
        .iter()
        .zip(&case_costs)
        .map(|(s, c)| CaseRunStats {
            cost_usd: *c,
            ..s.clone()
        })
        .collect();

    let aggregate = Aggregate {
        agent: summary.agent,
        model: summary.model.clone(),
        provider: summary.provider,
        case_count: per_case.len(),
        counts,
        precision: counts.precision(),
        recall: counts.recall(),
        f1: counts.f1(),
        false_positives_per_case: counts.false_positives as f64 / n,
        manual_triage_findings_per_case: sum(|s| s.reported_findings as f64) / n,
        withheld_findings_per_case: sum(|s| s.withheld_findings as f64) / n,
        mean_cost_usd_per_case: mean_cost,
        mean_runtime_ms_per_case: sum(|s| s.runtime_ms as f64) / n,
        mean_llm_calls_per_case: sum(|s| s.llm_calls as f64) / n,
        mean_tool_calls_per_case: sum(|s| s.tool_calls as f64) / n,
        total_input_tokens: summary.stats.iter().map(|s| s.input_tokens).sum(),
        total_output_tokens: summary.stats.iter().map(|s| s.output_tokens).sum(),
        mock_run: summary.is_mock(),
    };

    let by_category = [
        CaseCategory::RealIssue,
        CaseCategory::Trap,
        CaseCategory::Challenging,
    ]
    .into_iter()
    .filter_map(|cat| {
        let slice: Vec<&CaseEvaluation> = per_case.iter().filter(|c| c.category == cat).collect();
        if slice.is_empty() {
            return None;
        }
        let mut c = Counts::default();
        for s in &slice {
            c.add(&s.counts);
        }
        Some(CategoryBreakdown {
            category: cat,
            case_count: slice.len(),
            counts: c,
            precision: c.precision(),
            recall: c.recall(),
            f1: c.f1(),
        })
    })
    .collect();

    let evaluation = Evaluation {
        aggregate,
        per_case,
        per_case_stats,
        by_category,
        line_tolerance: summary.config.match_line_tolerance,
    };

    write_json(
        &out_dir.join(format!("evaluation-{}.json", summary.agent.as_str())),
        &evaluation,
    )?;
    Ok(evaluation)
}

pub fn load_summary(path: &Path) -> Result<RunSummary> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value).context("serializing")?;
    std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_evaluation_survives_a_json_round_trip() {
        // `report` reads back what `evaluate` wrote. Fields skipped on the way
        // out must have a default on the way in, or the comparison table
        // cannot be produced from real results at all.
        let agg = Aggregate {
            agent: AgentKind::Baseline,
            model: "m".into(),
            provider: Provider::Vertex,
            case_count: 12,
            counts: Counts {
                true_positives: 6,
                false_positives: 0,
                false_negatives: 2,
            },
            precision: 1.0,
            recall: 0.75,
            f1: 0.857,
            false_positives_per_case: 0.0,
            manual_triage_findings_per_case: 0.5,
            withheld_findings_per_case: 0.0,
            mean_cost_usd_per_case: None,
            mean_runtime_ms_per_case: 17389.0,
            mean_llm_calls_per_case: 1.0,
            mean_tool_calls_per_case: 0.0,
            total_input_tokens: 1,
            total_output_tokens: 2,
            mock_run: false,
        };
        let e = Evaluation {
            aggregate: agg,
            per_case: vec![],
            per_case_stats: vec![],
            by_category: vec![],
            line_tolerance: 3,
        };
        let json = serde_json::to_string(&e).unwrap();
        assert!(
            !json.contains("mock_run"),
            "a real run should not carry the flag"
        );
        let back: Evaluation = serde_json::from_str(&json).unwrap();
        assert!(!back.aggregate.mock_run);
        assert_eq!(back.aggregate.case_count, 12);
    }

    #[test]
    fn mock_runs_are_flagged() {
        let cfg = RunConfig::mock();
        let s = RunSummary {
            agent: AgentKind::Baseline,
            model: "mock".into(),
            provider: cfg.llm.provider,
            started_at: "t".into(),
            finished_at: "t".into(),
            benchmark_dir: "b".into(),
            case_count: 0,
            config: cfg,
            stats: vec![],
        };
        assert!(s.is_mock());
    }

    #[test]
    fn cost_aggregation_requires_every_case_to_have_a_price() {
        let priced = |c: Option<f64>| CaseRunStats {
            case_id: "c".into(),
            trajectory_id: "t".into(),
            runtime_ms: 0,
            llm_calls: 1,
            tool_calls: 0,
            retries: 0,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: c,
            reported_findings: 0,
            withheld_findings: 0,
        };
        let all = vec![priced(Some(1.0)), priced(Some(3.0))];
        let partial = vec![priced(Some(1.0)), priced(None)];

        let mean = |v: &Vec<CaseRunStats>| -> Option<f64> {
            if !v.is_empty() && v.iter().all(|s| s.cost_usd.is_some()) {
                Some(v.iter().filter_map(|s| s.cost_usd).sum::<f64>() / v.len() as f64)
            } else {
                None
            }
        };
        assert_eq!(mean(&all), Some(2.0));
        assert_eq!(mean(&partial), None, "partial pricing must not be averaged");
    }
}
