//! CLI entry point.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use verified_code_reviewer::{
    bench,
    config::RunConfig,
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

#[derive(Subcommand)]
enum Command {
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
    },
    /// Score a completed run against ground truth.
    Evaluate {
        #[arg(long, value_enum)]
        agent: AgentArg,
        #[arg(long, default_value = "benchmark/cases")]
        benchmark: PathBuf,
        #[arg(long, default_value = "results")]
        out: PathBuf,
    },
    /// Print the baseline vs advanced comparison table.
    Report {
        #[arg(long, default_value = "results")]
        out: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // A missing .env is fine: variables may be exported directly.
    let _ = dotenvy::dotenv();

    match Cli::parse().command {
        Command::Check { benchmark } => cmd_check(&benchmark),
        Command::Run {
            agent,
            benchmark,
            out,
            dry_run,
        } => cmd_run(agent.into(), &benchmark, &out, dry_run).await,
        Command::Evaluate {
            agent,
            benchmark,
            out,
        } => cmd_evaluate(agent.into(), &benchmark, &out),
        Command::Report { out } => cmd_report(&out),
    }
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
                    (Ok(c), Ok(gt)) => println!(
                        "  {name:<24} {:?}  {} expected finding(s)",
                        c.manifest.category,
                        gt.expected_findings.len()
                    ),
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
) -> Result<()> {
    let cfg = if dry_run {
        eprintln!("dry run: using the offline stub. Results are NOT measurements.");
        RunConfig::mock()
    } else {
        RunConfig::from_env().context("loading configuration (see .env.example)")?
    };

    let summary = runner::run_benchmark(benchmark, agent, &cfg, out).await?;

    println!(
        "\n{} case(s) run with {} ({:?}).",
        summary.case_count, summary.model, summary.provider
    );
    println!(
        "Trajectories: {}",
        out.join("trajectories").join(agent.as_str()).display()
    );
    println!(
        "Summary:      {}",
        out.join(format!("summary-{}.json", agent.as_str()))
            .display()
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
) -> Result<()> {
    let summary_path = out.join(format!("summary-{}.json", agent.as_str()));
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
