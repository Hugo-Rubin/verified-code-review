//! CLI entry point.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use verified_code_reviewer::{
    bench,
    bench::Language,
    config::{Ablation, RunConfig},
    replay,
    review::{self, ReviewRequest},
    runner::{self, Aggregate},
    trajectory::AgentKind,
};

#[derive(Parser)]
#[command(
    name = "vcr",
    about = "Verified Code Reviewer — repository-aware review with fresh-context falsification",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum AgentArg {
    /// Direct single-pass review. The baseline.
    Baseline,
    /// Investigation plus fresh-context falsification.
    Advanced,
}

impl From<AgentArg> for AgentKind {
    fn from(a: AgentArg) -> Self {
        match a {
            AgentArg::Baseline => AgentKind::Baseline,
            AgentArg::Advanced => AgentKind::Advanced,
        }
    }
}

/// Which stage of the advanced pipeline to switch off.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum AblationArg {
    /// The complete pipeline.
    None,
    /// Drop the falsification question and the fresh-context verifier.
    NoFalsification,
    /// Keep falsification but never re-investigate after "Insufficient".
    NoFollowup,
    /// Report candidates as produced, with no investigation or verification.
    CandidatesOnly,
    /// Never look again at a case that finished with nothing to report.
    NoSecondLook,
}

impl From<AblationArg> for Ablation {
    fn from(a: AblationArg) -> Self {
        match a {
            AblationArg::None => Ablation::None,
            AblationArg::NoFalsification => Ablation::NoFalsification,
            AblationArg::NoFollowup => Ablation::NoFollowup,
            AblationArg::CandidatesOnly => Ablation::CandidatesOnly,
            AblationArg::NoSecondLook => Ablation::NoSecondLook,
        }
    }
}

/// Language a real change is written in.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum LanguageArg {
    Rust,
    Python,
}

impl From<LanguageArg> for Language {
    fn from(l: LanguageArg) -> Self {
        match l {
            LanguageArg::Rust => Language::Rust,
            LanguageArg::Python => Language::Python,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Review a real change: a working tree plus a diff.
    ///
    /// Runs the same pipeline, prompts, sandbox and evidence gate that produced
    /// every number in the README. There is no ground truth and no score here
    /// — the output is a report for a human to act on, which is the only thing
    /// this system produces.
    Review {
        /// Repository to read. This is the sandbox boundary: nothing outside
        /// it can be opened.
        #[arg(long, default_value = ".")]
        repo: PathBuf,
        /// Unified diff to review. Use "-" or omit to read stdin.
        #[arg(long)]
        diff: Option<PathBuf>,
        /// One line describing the change, as its author would put it.
        #[arg(long, default_value = "A proposed change")]
        title: String,
        /// The author's stated rationale. Neutral wording matters: telling the
        /// reviewer a bug exists is a good way to be told one does.
        #[arg(long, default_value = "No description was supplied by the author.")]
        description: String,
        #[arg(long, value_enum, default_value = "rust")]
        language: LanguageArg,
        #[arg(long, value_enum, default_value = "advanced")]
        agent: AgentArg,
        /// Write the trajectory and a rendered review here.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check configuration and the benchmark without calling the model.
    Check {
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
    },
    /// Run an agent over every benchmark case.
    Run {
        #[arg(long, value_enum)]
        agent: AgentArg,
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
        #[arg(long, default_value = "results")]
        out: PathBuf,
        /// Use the offline stub instead of the configured provider. Produces
        /// no measurements; useful for exercising the pipeline.
        #[arg(long)]
        dry_run: bool,
        /// Switch off part of the advanced pipeline to measure its
        /// contribution. Writes to separate artifacts so a full run is never
        /// overwritten.
        #[arg(long, value_enum, default_value = "none")]
        ablation: AblationArg,
    },
    /// Score a completed run against ground truth.
    Evaluate {
        #[arg(long, value_enum)]
        agent: AgentArg,
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
        #[arg(long, default_value = "results")]
        out: PathBuf,
        /// Score an ablation run rather than the full pipeline.
        #[arg(long, value_enum, default_value = "none")]
        ablation: AblationArg,
    },
    /// Print the baseline vs advanced comparison table.
    Report {
        #[arg(long, default_value = "results")]
        out: PathBuf,
    },
    /// Blind stopwatch session measuring real human review time.
    ///
    /// Pools the reported findings from every named arm, shuffles them, and
    /// presents them one at a time without saying which system produced which.
    Triage {
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
        #[arg(long, default_value = "results")]
        out: PathBuf,
        /// Comma-separated arm names, matching `evaluation-<arm>.json`.
        #[arg(long, default_value = "baseline,advanced", value_delimiter = ',')]
        arms: Vec<String>,
        /// Shuffle seed, recorded so the order can be reproduced.
        #[arg(long, default_value_t = 20260830)]
        seed: u64,
        /// Recorded with the session for provenance.
        #[arg(long, default_value = "unnamed-reviewer")]
        reviewer: String,
    },
    /// Replay the deduplication rule over recorded runs.
    ///
    /// Reads artifacts only; calls no model. Reports how often the rule would
    /// merge two candidates, and separates the merges that rest on genuinely
    /// overlapping line ranges from those that rest only on the evaluator's
    /// matching tolerance.
    ReplayDedup {
        #[arg(long, default_value = ".")]
        root: PathBuf,
        /// The evaluator's matching tolerance, for the comparison arm.
        #[arg(long, default_value_t = 3)]
        tolerance: u32,
    },
    /// Pair every scored true positive with the ground truth it was credited
    /// for, so a person can check that the claim describes the actual defect.
    ///
    /// Reads artifacts only; calls no model and reaches no verdict. The
    /// evaluator matches on location and category, which is a proxy for
    /// "found the defect" -- this is how that proxy gets checked.
    AuditMatches {
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
        #[arg(long, default_value = "results-final")]
        root: PathBuf,
    },
    /// Summarise spread across repeated trials.
    ///
    /// Expects `<root>/<trial>/evaluation-<arm>.json`, i.e. one subdirectory
    /// per trial, each already evaluated.
    Variance {
        #[arg(long, default_value = "results-trials")]
        root: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // A missing .env is fine: variables may be exported directly.
    let _ = dotenvy::dotenv();

    match Cli::parse().command {
        Command::Check { benchmark } => cmd_check(&benchmark),
        Command::Review {
            repo,
            diff,
            title,
            description,
            language,
            agent,
            out,
        } => {
            cmd_review(ReviewRequest {
                repo,
                diff_path: diff,
                title,
                description,
                language: language.into(),
                agent: agent.into(),
                out,
            })
            .await
        }
        Command::Run {
            agent,
            benchmark,
            out,
            dry_run,
            ablation,
        } => cmd_run(agent.into(), &benchmark, &out, dry_run, ablation.into()).await,
        Command::Evaluate {
            agent,
            benchmark,
            out,
            ablation,
        } => cmd_evaluate(agent.into(), &benchmark, &out, ablation.into()),
        Command::Report { out } => cmd_report(&out),
        Command::Variance { root } => cmd_variance(&root),
        Command::ReplayDedup { root, tolerance } => replay::report(&root, tolerance),
        Command::AuditMatches { benchmark, root } => replay::report_matches(&benchmark, &root),
        Command::Triage {
            benchmark,
            out,
            arms,
            seed,
            reviewer,
        } => cmd_triage(&benchmark, &out, &arms, seed, &reviewer),
    }
}

async fn cmd_review(req: ReviewRequest) -> Result<()> {
    let cfg = RunConfig::from_env().context("configuration is not ready; run `vcr check`")?;

    eprintln!(
        "reviewing {} with the {} agent ({})",
        req.repo.display(),
        match req.agent {
            AgentKind::Baseline => "baseline",
            AgentKind::Advanced => "advanced",
        },
        cfg.llm.model
    );

    let traj = review::review(&req, &cfg).await?;
    print!("{}", review::render(&traj));

    if let Some(out) = &req.out {
        let path = review::write_report(&traj, out)?;
        eprintln!("review: {}", path.display());
    }
    Ok(())
}

fn cmd_check(benchmark: &std::path::Path) -> Result<()> {
    match RunConfig::from_env() {
        Ok(cfg) => {
            println!("config      OK");
            println!("  provider  {:?}", cfg.llm.provider);
            println!("  model     {}", cfg.llm.model);
            println!("  location  {}", cfg.llm.location);
            println!("  auth      {:?}", cfg.llm.auth);
            match cfg.llm.pricing {
                Some(p) => println!(
                    "  pricing   ${:.4}/Mtok in, ${:.4}/Mtok out",
                    p.input_usd_per_mtok, p.output_usd_per_mtok
                ),
                None => {
                    println!("  pricing   NOT SET — cost per case will be reported as unavailable")
                }
            }
            println!("  tolerance ±{} lines", cfg.match_line_tolerance);
        }
        Err(e) => println!("config      NOT READY — {e:#}"),
    }

    println!();
    match bench::discover_cases(benchmark) {
        Err(e) => println!("benchmark   NOT READY — {e:#}"),
        Ok(dirs) if dirs.is_empty() => {
            println!("benchmark   empty ({})", benchmark.display());
        }
        Ok(dirs) => {
            println!(
                "benchmark   {} case(s) in {}",
                dirs.len(),
                benchmark.display()
            );
            let mut problems = 0;
            for dir in &dirs {
                let name = dir.file_name().unwrap_or_default().to_string_lossy();
                match (bench::load_case(dir), bench::load_ground_truth(dir)) {
                    (Ok(c), Ok(gt)) => {
                        println!(
                            "  {name:<24} {:?}  {} expected finding(s)",
                            c.manifest.category,
                            gt.expected_findings.len()
                        );
                        // Every case should anchor its ground truth where the
                        // change is. A defect usually spans a change and
                        // something it interacts with, and either end is a
                        // defensible place to report it — but if the benchmark
                        // is inconsistent about which, a correct finding gets
                        // scored against a coin flip. One case drifted this
                        // way and cost a correct finding a false positive
                        // before it was caught.
                        for problem in bench::findings_outside_the_diff(&c, &gt) {
                            println!("      WARNING: {problem}");
                        }
                        // A description that gives away whether the change is
                        // sound stops the case measuring review skill.
                        let tells = bench::description_tells(&c.manifest);
                        if !tells.is_empty() {
                            println!(
                                "      WARNING: description reveals the category: {}",
                                tells.join(", ")
                            );
                        }
                    }
                    (Err(e), _) | (_, Err(e)) => {
                        problems += 1;
                        println!("  {name:<24} BROKEN — {e:#}");
                    }
                }
            }
            if problems > 0 {
                anyhow::bail!("{problems} case(s) failed to load");
            }
        }
    }
    Ok(())
}

async fn cmd_run(
    agent: AgentKind,
    benchmark: &std::path::Path,
    out: &std::path::Path,
    dry_run: bool,
    ablation: Ablation,
) -> Result<()> {
    let mut cfg = if dry_run {
        eprintln!("dry run: using the offline stub. Results are NOT measurements.");
        RunConfig::mock()
    } else {
        RunConfig::from_env().context("loading configuration (see .env.example)")?
    };
    cfg.ablation = ablation;

    if ablation != Ablation::None && agent != AgentKind::Advanced {
        anyhow::bail!(
            "--ablation applies to the advanced agent; the baseline has no stages to disable"
        );
    }
    if ablation != Ablation::None {
        eprintln!(
            "ABLATION: {} — part of the pipeline is disabled; this is not the full system",
            ablation.as_str()
        );
    }

    let arm = runner::arm_name(agent, ablation);
    let summary = runner::run_benchmark(benchmark, agent, &cfg, out).await?;

    println!(
        "\n{} case(s) run with {} ({:?}).",
        summary.case_count, summary.model, summary.provider
    );
    println!(
        "Trajectories: {}",
        out.join("trajectories").join(&arm).display()
    );
    println!(
        "Summary:      {}",
        out.join(format!("summary-{arm}.json")).display()
    );
    if summary.is_mock() {
        println!("\nNOTE: this was a dry run. Do not report these numbers.");
    }
    Ok(())
}

fn cmd_evaluate(
    agent: AgentKind,
    benchmark: &std::path::Path,
    out: &std::path::Path,
    ablation: Ablation,
) -> Result<()> {
    let arm = runner::arm_name(agent, ablation);
    let summary_path = out.join(format!("summary-{arm}.json"));
    let summary = runner::load_summary(&summary_path)
        .with_context(|| format!("no run found — expected {}", summary_path.display()))?;

    // Rates come from the current environment, not from the run, so cost can
    // be filled in after the fact without spending the model again.
    let pricing = RunConfig::from_env().ok().and_then(|c| c.llm.pricing);
    let e = runner::evaluate_run(benchmark, &summary, out, pricing)?;
    print_aggregate(&e.aggregate);

    println!("\nBy category");
    for b in &e.by_category {
        println!(
            "  {:<12} n={:<3} TP={:<3} FP={:<3} FN={:<3} F1={:.3}",
            format!("{:?}", b.category),
            b.case_count,
            b.counts.true_positives,
            b.counts.false_positives,
            b.counts.false_negatives,
            b.f1
        );
    }

    println!(
        "\nWritten: {}",
        out.join(format!("evaluation-{}.json", agent.as_str()))
            .display()
    );
    Ok(())
}

fn print_aggregate(a: &Aggregate) {
    println!("\n{} — {} ({:?})", a.agent.as_str(), a.model, a.provider);
    if a.mock_run {
        println!("*** DRY RUN — these are not measurements ***");
    }
    println!("  cases                 {}", a.case_count);
    println!(
        "  TP / FP / FN          {} / {} / {}",
        a.counts.true_positives, a.counts.false_positives, a.counts.false_negatives
    );
    println!("  precision             {:.3}", a.precision);
    println!("  recall                {:.3}", a.recall);
    println!("  F1                    {:.3}", a.f1);
    println!("  false positives/case  {:.2}", a.false_positives_per_case);
    println!(
        "  findings to triage    {:.2}/case  (proxy for human review time, not a measurement)",
        a.manual_triage_findings_per_case
    );
    println!(
        "  withheld/case         {:.2}",
        a.withheld_findings_per_case
    );
    match a.mean_cost_usd_per_case {
        Some(c) => println!("  cost/case             ${c:.5}"),
        None => println!("  cost/case             unavailable (pricing not configured)"),
    }
    if a.evidence_audit.checkable > 0 {
        println!(
            "  evidence accuracy     {:.3}  ({}/{} cited excerpts verified against the repo)",
            a.evidence_accuracy, a.evidence_audit.accurate, a.evidence_audit.checkable
        );
        for m in a.evidence_audit.mismatches.iter().take(5) {
            println!("      mismatch: {m}");
        }
    } else {
        println!("  evidence accuracy     n/a (no checkable evidence gathered)");
    }
    println!(
        "  runtime/case          {:.0} ms",
        a.mean_runtime_ms_per_case
    );
    println!("  LLM calls/case        {:.2}", a.mean_llm_calls_per_case);
    println!("  tool calls/case       {:.2}", a.mean_tool_calls_per_case);
}

fn cmd_report(out: &std::path::Path) -> Result<()> {
    let load = |agent: AgentKind| -> Result<runner::Evaluation> {
        let p = out.join(format!("evaluation-{}.json", agent.as_str()));
        let raw = std::fs::read_to_string(&p)
            .with_context(|| format!("no evaluation found — expected {}", p.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", p.display()))
    };

    let base = load(AgentKind::Baseline)?;
    let adv = load(AgentKind::Advanced)?;

    if base.aggregate.mock_run || adv.aggregate.mock_run {
        println!("*** One or both arms were dry runs. This table is NOT a measurement. ***\n");
    }
    if base.aggregate.model != adv.aggregate.model {
        println!(
            "*** WARNING: arms used different models ({} vs {}). The comparison is not fair. ***\n",
            base.aggregate.model, adv.aggregate.model
        );
    }
    if base.aggregate.case_count != adv.aggregate.case_count {
        println!(
            "*** WARNING: arms ran different case counts ({} vs {}). ***\n",
            base.aggregate.case_count, adv.aggregate.case_count
        );
    }

    let row = |name: &str, b: f64, a: f64, prec: usize| {
        println!(
            "| {name:<28} | {b:>10.prec$} | {a:>10.prec$} | {:>+10.prec$} |",
            a - b
        );
    };

    println!(
        "| {:<28} | {:>10} | {:>10} | {:>10} |",
        "Metric", "Baseline", "Advanced", "Change"
    );
    println!("|{:-<30}|{:->12}|{:->12}|{:->12}|", "", "", "", "");
    row(
        "Precision",
        base.aggregate.precision,
        adv.aggregate.precision,
        3,
    );
    row("Recall", base.aggregate.recall, adv.aggregate.recall, 3);
    row("F1", base.aggregate.f1, adv.aggregate.f1, 3);
    row(
        "False positives/case",
        base.aggregate.false_positives_per_case,
        adv.aggregate.false_positives_per_case,
        2,
    );
    row(
        "Findings to triage/case",
        base.aggregate.manual_triage_findings_per_case,
        adv.aggregate.manual_triage_findings_per_case,
        2,
    );
    row(
        "Evidence accuracy",
        base.aggregate.evidence_accuracy,
        adv.aggregate.evidence_accuracy,
        3,
    );
    row(
        "Runtime/case (ms)",
        base.aggregate.mean_runtime_ms_per_case,
        adv.aggregate.mean_runtime_ms_per_case,
        0,
    );

    match (
        base.aggregate.mean_cost_usd_per_case,
        adv.aggregate.mean_cost_usd_per_case,
    ) {
        (Some(b), Some(a)) => row("Cost/case (USD)", b, a, 5),
        _ => println!(
            "| {:<28} | {:>10} | {:>10} | {:>10} |",
            "Cost/case (USD)", "n/a", "n/a", "n/a"
        ),
    }

    println!(
        "\n\"Findings to triage/case\" is a manual-triage proxy rather than a direct \
         measurement of human review time."
    );
    Ok(())
}

fn cmd_triage(
    benchmark: &std::path::Path,
    out: &std::path::Path,
    arms: &[String],
    seed: u64,
    reviewer: &str,
) -> Result<()> {
    let session =
        verified_code_reviewer::triage::run_session(benchmark, out, arms, seed, reviewer)?;

    // A session where nothing was judged is not a measurement of zero, it is an
    // abandoned run. Writing it would leave an artifact that reads like a
    // result: every arm at 0 findings and 0.0 seconds.
    if session.decisions.is_empty() {
        println!("\nNo findings were triaged, so no session file was written.");
        return Ok(());
    }

    println!(
        "
{}",
        "=".repeat(72)
    );
    println!("MEASURED HUMAN REVIEW TIME");
    println!("{}", "=".repeat(72));
    println!(
        "  {:<28} {:>10} {:>14} {:>14}",
        "arm", "findings", "sec/finding", "sec/case"
    );
    for a in &session.arms {
        println!(
            "  {:<28} {:>10} {:>14.1} {:>14.1}",
            a.arm, a.findings_triaged, a.mean_seconds_per_finding, a.seconds_per_case
        );
    }
    println!(
        "
This is a direct measurement, not the findings-to-triage proxy."
    );
    for n in &session.notes {
        println!("  - {n}");
    }

    let path = out.join("triage-session.json");
    runner::write_json(&path, &session)?;
    println!(
        "
Written: {}",
        path.display()
    );
    Ok(())
}

fn cmd_variance(root: &std::path::Path) -> Result<()> {
    let arms = runner::variance_across_trials(root)?;

    for arm in &arms {
        println!(
            "
{} — {} · {} trial(s)",
            arm.arm, arm.model, arm.trials
        );
        if arm.trials < 2 {
            println!("  (a single trial has no spread to report)");
        }
        println!(
            "  {:<28} {:>9} {:>9} {:>9} {:>9}",
            "metric", "mean", "min", "max", "stdev"
        );
        for m in &arm.metrics {
            if m.mean.is_nan() {
                println!("  {:<28} {:>9}", m.metric, "n/a");
                continue;
            }
            let p = if m.metric.contains("runtime") { 0 } else { 4 };
            println!(
                "  {:<28} {:>9.p$} {:>9.p$} {:>9.p$} {:>9.p$}",
                m.metric,
                m.mean,
                m.min,
                m.max,
                m.stdev,
                p = p
            );
        }
        if arm.unstable_cases.is_empty() {
            println!("  every case scored identically in all trials");
        } else {
            println!("  cases that did not score identically across trials:");
            for c in &arm.unstable_cases {
                println!("    - {c}");
            }
        }
    }

    let path = root.join("variance.json");
    runner::write_json(&path, &arms)?;
    println!(
        "
Written: {}",
        path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use verified_code_reviewer::config::Provider;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn agent_arg_maps_to_agent_kind() {
        assert_eq!(AgentKind::from(AgentArg::Baseline), AgentKind::Baseline);
        assert_eq!(AgentKind::from(AgentArg::Advanced), AgentKind::Advanced);
    }

    #[test]
    fn dry_run_config_is_the_mock_provider() {
        assert_eq!(RunConfig::mock().llm.provider, Provider::Mock);
    }
}
