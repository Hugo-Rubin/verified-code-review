//! Blind stopwatch harness for measuring human review time.
//!
//! The headline "human time per task" figure elsewhere in this project is a
//! labelled proxy — findings a reviewer must triage per case. This module
//! replaces it with a direct measurement.
//!
//! # Why it is blind
//!
//! Findings from every arm are pooled, shuffled with a recorded seed, and
//! presented one at a time with no indication of which system produced them
//! and no access to ground truth. A reviewer who can see that a finding came
//! from "the advanced system" will spend different effort on it, and that
//! difference would land squarely in the number being measured.
//!
//! # What is presented
//!
//! The claim and its location, and nothing else. Not the evidence, not the
//! verifier's verdict.
//!
//! That understates the advanced system's benefit, deliberately. A reviewer
//! handed a cited argument plausibly decides faster than one handed a bare
//! assertion, so including evidence would measure the whole product — but it
//! would also make the arms instantly distinguishable and destroy the
//! blinding. Measuring the conservative quantity honestly is worth more than
//! measuring the flattering one badly, and the limitation is recorded in the
//! output file.
//!
//! Time is attributed back per arm afterwards, and combined with each arm's
//! findings-per-case to give seconds per case.

use crate::bench;
use crate::finding::Finding;
use crate::runner::Evaluation;
use crate::trajectory::Trajectory;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

/// One finding queued for blind triage.
#[derive(Debug, Clone)]
pub struct TriageItem {
    pub arm: String,
    pub case_id: String,
    pub case_title: String,
    pub case_description: String,
    pub finding_id: String,
    pub issue_type: String,
    pub location: String,
    pub claim: String,
    pub repository_path: String,
}

/// What the reviewer decided about one finding, and how long it took.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageDecision {
    pub arm: String,
    pub case_id: String,
    pub finding_id: String,
    pub location: String,
    /// `real`, `not-a-bug`, or `unsure`.
    pub verdict: String,
    pub seconds: f64,
}

/// Per-arm totals derived from the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmTriageSummary {
    pub arm: String,
    pub findings_triaged: u32,
    pub total_seconds: f64,
    pub mean_seconds_per_finding: f64,
    /// Total triage seconds divided by the number of benchmark cases. This is
    /// the directly measured "human time per case".
    pub seconds_per_case: f64,
    pub case_count: usize,
    pub verdict_counts: BTreeMap<String, u32>,
}

/// The whole session, written to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriageSession {
    pub started_at: String,
    pub finished_at: String,
    pub reviewer: String,
    /// Recorded so the presentation order can be reproduced exactly.
    pub shuffle_seed: u64,
    pub blind: bool,
    pub evidence_shown: bool,
    pub arms: Vec<ArmTriageSummary>,
    pub decisions: Vec<TriageDecision>,
    pub notes: Vec<String>,
}

/// Deterministic shuffle so a session's order can be replayed from its seed.
///
/// A xorshift is plenty for presentation order and avoids a dependency.
fn shuffle<T>(items: &mut [T], seed: u64) {
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for i in (1..items.len()).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// Collect every reported finding from the named arms.
pub fn collect_items(
    benchmark_dir: &Path,
    out_dir: &Path,
    arms: &[String],
) -> Result<(Vec<TriageItem>, usize)> {
    let case_dirs = bench::discover_cases(benchmark_dir)?;
    let mut items = Vec::new();

    for arm in arms {
        let eval_path = out_dir.join(format!("evaluation-{arm}.json"));
        if !eval_path.is_file() {
            bail!(
                "no evaluation for arm {arm:?} at {} — run and evaluate it first",
                eval_path.display()
            );
        }
        // Loaded to fail early if the arm was never evaluated.
        let _: Evaluation = serde_json::from_str(&std::fs::read_to_string(&eval_path)?)
            .with_context(|| format!("parsing {}", eval_path.display()))?;

        for dir in &case_dirs {
            let case = bench::load_case(dir)?;
            let traj_path = out_dir
                .join("trajectories")
                .join(arm)
                .join(format!("{}-{arm}.json", case.id()));
            if !traj_path.is_file() {
                continue;
            }
            let traj: Trajectory = serde_json::from_str(&std::fs::read_to_string(&traj_path)?)
                .with_context(|| format!("parsing {}", traj_path.display()))?;

            for f in traj
                .final_findings
                .iter()
                .filter(|f| f.status.is_reported())
            {
                items.push(item_from(arm, &case, f));
            }
        }
    }

    Ok((items, case_dirs.len()))
}

fn item_from(arm: &str, case: &bench::Case, f: &Finding) -> TriageItem {
    TriageItem {
        arm: arm.to_string(),
        case_id: case.manifest.case_id.clone(),
        case_title: case.manifest.title.clone(),
        case_description: case.manifest.description.clone(),
        finding_id: f.candidate.id.clone(),
        issue_type: f.candidate.issue_type.to_string(),
        location: f.candidate.location.to_string(),
        claim: f.candidate.claim.clone(),
        repository_path: case.repo.path().display().to_string(),
    }
}

/// Run an interactive blind triage session.
pub fn run_session(
    benchmark_dir: &Path,
    out_dir: &Path,
    arms: &[String],
    seed: u64,
    reviewer: &str,
) -> Result<TriageSession> {
    let (mut items, case_count) = collect_items(benchmark_dir, out_dir, arms)?;
    if items.is_empty() {
        bail!("no reported findings to triage across arms {arms:?}");
    }
    shuffle(&mut items, seed);

    let started_at = chrono::Utc::now().to_rfc3339();
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let mut decisions = Vec::new();

    println!("\n{}", "=".repeat(72));
    println!(
        "BLIND TRIAGE — {} finding(s) from {} arm(s)",
        items.len(),
        arms.len()
    );
    println!("{}", "=".repeat(72));
    println!(
        "\nYou will see one finding at a time. You are NOT told which system\n\
         produced it, and the benchmark's ground truth is not consulted.\n\n\
         For each one, decide whether it is a real defect worth acting on.\n\
         Open the repository and read the code — that reading time is the\n\
         thing being measured. Take exactly as long as you genuinely would.\n\n\
         Answer with:  r = real defect   n = not a bug   u = unsure   q = quit\n"
    );
    print!("Press Enter to start the clock... ");
    std::io::stdout().flush()?;
    let _ = lines.next();

    for (i, item) in items.iter().enumerate() {
        println!("\n{}", "-".repeat(72));
        println!("Finding {} of {}", i + 1, items.len());
        println!("{}", "-".repeat(72));
        println!("Change under review : {}", item.case_title);
        println!("{}", item.case_description);
        println!("\nRepository          : {}", item.repository_path);
        println!("Reported location   : {}", item.location);
        println!("Category            : {}", item.issue_type);
        println!("\nClaim:\n  {}", item.claim);
        println!();

        let started = Instant::now();
        let verdict = loop {
            print!("  real / not-a-bug / unsure  [r/n/u/q] > ");
            std::io::stdout().flush()?;
            let Some(line) = lines.next() else {
                break "unsure".to_string();
            };
            match line?.trim().to_lowercase().as_str() {
                "r" => break "real".to_string(),
                "n" => break "not-a-bug".to_string(),
                "u" => break "unsure".to_string(),
                "q" => {
                    println!("\nStopping early. {} finding(s) recorded.", decisions.len());
                    return finish(
                        started_at,
                        reviewer,
                        seed,
                        decisions,
                        arms,
                        case_count,
                        Some("session ended early by the reviewer"),
                    );
                }
                other => println!("  unrecognised {other:?} — use r, n, u, or q"),
            }
        };
        let seconds = started.elapsed().as_secs_f64();
        println!("  recorded: {verdict} in {seconds:.1}s");

        decisions.push(TriageDecision {
            arm: item.arm.clone(),
            case_id: item.case_id.clone(),
            finding_id: item.finding_id.clone(),
            location: item.location.clone(),
            verdict,
            seconds,
        });
    }

    finish(
        started_at, reviewer, seed, decisions, arms, case_count, None,
    )
}

fn finish(
    started_at: String,
    reviewer: &str,
    seed: u64,
    decisions: Vec<TriageDecision>,
    arms: &[String],
    case_count: usize,
    note: Option<&str>,
) -> Result<TriageSession> {
    let mut summaries = Vec::new();

    for arm in arms {
        let mine: Vec<&TriageDecision> = decisions.iter().filter(|d| &d.arm == arm).collect();
        // `sum()` over an empty slice can yield -0.0, which prints as "-0.0"
        // and reads like a measurement rather than an absence.
        let total: f64 = mine.iter().map(|d| d.seconds).sum::<f64>().max(0.0);
        let n = mine.len() as u32;

        let mut verdict_counts: BTreeMap<String, u32> = BTreeMap::new();
        for d in &mine {
            *verdict_counts.entry(d.verdict.clone()).or_insert(0) += 1;
        }

        summaries.push(ArmTriageSummary {
            arm: arm.clone(),
            findings_triaged: n,
            total_seconds: total,
            mean_seconds_per_finding: if n == 0 { 0.0 } else { total / n as f64 },
            seconds_per_case: if case_count == 0 {
                0.0
            } else {
                total / case_count as f64
            },
            case_count,
            verdict_counts,
        });
    }

    let mut notes = vec![
        "Findings from all arms were pooled, shuffled, and presented without \
         identifying which system produced them."
            .to_string(),
        "Only the claim and its location were shown. Evidence and verifier \
         verdicts were withheld to keep the arms indistinguishable, which \
         understates the advanced system's benefit."
            .to_string(),
        "seconds_per_case divides an arm's total triage time by the number of \
         benchmark cases, including cases where that arm reported nothing."
            .to_string(),
    ];
    if let Some(n) = note {
        notes.push(n.to_string());
    }

    Ok(TriageSession {
        started_at,
        finished_at: chrono::Utc::now().to_rfc3339(),
        reviewer: reviewer.to_string(),
        shuffle_seed: seed,
        blind: true,
        evidence_shown: false,
        arms: summaries,
        decisions,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_is_deterministic_for_a_seed() {
        let mut a: Vec<u32> = (0..50).collect();
        let mut b: Vec<u32> = (0..50).collect();
        shuffle(&mut a, 42);
        shuffle(&mut b, 42);
        assert_eq!(a, b, "the same seed must reproduce the same order");
    }

    #[test]
    fn different_seeds_give_different_orders() {
        let mut a: Vec<u32> = (0..50).collect();
        let mut b: Vec<u32> = (0..50).collect();
        shuffle(&mut a, 1);
        shuffle(&mut b, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn shuffle_preserves_every_item() {
        let mut a: Vec<u32> = (0..50).collect();
        shuffle(&mut a, 7);
        a.sort();
        assert_eq!(a, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn shuffle_handles_degenerate_lengths() {
        let mut empty: Vec<u32> = vec![];
        shuffle(&mut empty, 3);
        assert!(empty.is_empty());
        let mut one = vec![9];
        shuffle(&mut one, 3);
        assert_eq!(one, vec![9]);
    }

    fn decision(arm: &str, seconds: f64, verdict: &str) -> TriageDecision {
        TriageDecision {
            arm: arm.into(),
            case_id: "c".into(),
            finding_id: "f".into(),
            location: "a.rs:1".into(),
            verdict: verdict.into(),
            seconds,
        }
    }

    #[test]
    fn attributes_time_to_the_right_arm() {
        let decisions = vec![
            decision("baseline", 10.0, "real"),
            decision("advanced", 20.0, "real"),
            decision("advanced", 40.0, "not-a-bug"),
        ];
        let arms = vec!["baseline".to_string(), "advanced".to_string()];
        let s = finish("t".into(), "me", 1, decisions, &arms, 12, None).unwrap();

        let base = &s.arms[0];
        assert_eq!(base.findings_triaged, 1);
        assert!((base.total_seconds - 10.0).abs() < 1e-9);
        assert!((base.seconds_per_case - 10.0 / 12.0).abs() < 1e-9);

        let adv = &s.arms[1];
        assert_eq!(adv.findings_triaged, 2);
        assert!((adv.mean_seconds_per_finding - 30.0).abs() < 1e-9);
        assert_eq!(adv.verdict_counts["not-a-bug"], 1);
    }

    #[test]
    fn an_arm_that_reported_nothing_costs_no_time() {
        let arms = vec!["baseline".to_string(), "advanced".to_string()];
        let s = finish(
            "t".into(),
            "me",
            1,
            vec![decision("advanced", 5.0, "real")],
            &arms,
            12,
            None,
        )
        .unwrap();
        assert_eq!(s.arms[0].findings_triaged, 0);
        assert_eq!(s.arms[0].total_seconds, 0.0);
        assert_eq!(s.arms[0].seconds_per_case, 0.0);
    }

    #[test]
    fn an_arm_with_no_decisions_reports_positive_zero() {
        let s = finish("t".into(), "me", 1, vec![], &["a".to_string()], 12, None).unwrap();
        assert_eq!(s.arms[0].total_seconds, 0.0);
        assert!(
            s.arms[0].total_seconds.is_sign_positive(),
            "-0.0 prints as a measurement rather than an absence"
        );
    }

    #[test]
    fn the_session_records_its_own_limitations() {
        let s = finish("t".into(), "me", 1, vec![], &["a".to_string()], 1, None).unwrap();
        assert!(s.blind);
        assert!(!s.evidence_shown);
        assert!(s.notes.iter().any(|n| n.contains("understates")));
    }
}
