//! Replay the deduplication rule over recorded runs.
//!
//! Deduplication fired zero times in the final configuration, which is the
//! kind of measurement that ends an investigation too early. "Never fires" and
//! "fires wrongly whenever it fires" produce the same headline number on a run
//! where the trigger never occurs.
//!
//! This walks every trajectory in a results tree and asks two separate
//! questions of each pair of candidates the reviewer put forward:
//!
//! * would the **strict** rule merge them — do the line ranges intersect?
//! * would the **tolerant** rule merge them — do they come within the
//!   evaluator's `±tolerance` matching slack without intersecting?
//!
//! The second number is the one that matters. It is the set of merges the rule
//! would have performed on the strength of borrowed slack, and every one of
//! them has to be inspected by hand, because a merge that joins two distinct
//! defects converts a true positive into a false negative.
//!
//! Nothing here calls a model. It reads artifacts that already exist.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The slice of a trajectory this analysis needs.
#[derive(Debug, Deserialize)]
struct RecordedTrajectory {
    case_id: String,
    #[serde(default)]
    final_findings: Vec<RecordedFinding>,
}

#[derive(Debug, Deserialize)]
struct RecordedFinding {
    #[serde(default)]
    id: String,
    issue_type: String,
    location: RecordedLocation,
    #[serde(default)]
    claim: String,
}

#[derive(Debug, Deserialize)]
struct RecordedLocation {
    file: String,
    start_line: u32,
    end_line: u32,
}

/// One pair the rule would merge.
#[derive(Debug)]
pub struct Merge {
    pub run: String,
    pub case_id: String,
    pub issue_type: String,
    pub file: String,
    pub a: (u32, u32),
    pub b: (u32, u32),
    pub a_claim: String,
    pub b_claim: String,
    /// True when the ranges genuinely intersect; false when only the
    /// tolerance brought them together.
    pub strict: bool,
}

fn intersects(a: (u32, u32), b: (u32, u32), slack: u32) -> bool {
    a.0.saturating_sub(slack) <= b.1 && b.0 <= a.1.saturating_add(slack)
}

/// Find every advanced trajectory under `root`, at any depth.
fn advanced_trajectories(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("json")
                && p.parent()
                    .and_then(|d| d.file_name())
                    .and_then(|d| d.to_str())
                    == Some("advanced")
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Replay the rule at both strictnesses over every run under `root`.
pub fn replay(root: &Path, tolerance: u32) -> Result<Vec<Merge>> {
    let mut merges = Vec::new();

    for path in advanced_trajectories(root) {
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        // A results tree can hold artifacts from several tool versions. One
        // that does not parse is skipped rather than aborting the sweep.
        let Ok(traj) = serde_json::from_str::<RecordedTrajectory>(&raw) else {
            continue;
        };

        // The run is everything above `trajectories/`, so trials stay distinct.
        let run = path
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| path.display().to_string());

        let f = &traj.final_findings;
        for i in 0..f.len() {
            for j in (i + 1)..f.len() {
                let (a, b) = (&f[i], &f[j]);
                if a.issue_type != b.issue_type || a.location.file != b.location.file {
                    continue;
                }
                let ra = (a.location.start_line, a.location.end_line);
                let rb = (b.location.start_line, b.location.end_line);
                if !intersects(ra, rb, tolerance) {
                    continue;
                }
                merges.push(Merge {
                    run: run.clone(),
                    case_id: traj.case_id.clone(),
                    issue_type: a.issue_type.clone(),
                    file: a.location.file.clone(),
                    a: ra,
                    b: rb,
                    a_claim: a.claim.clone(),
                    b_claim: b.claim.clone(),
                    strict: intersects(ra, rb, 0),
                });
            }
        }
    }

    Ok(merges)
}

/// Print the replay as a report.
pub fn report(root: &Path, tolerance: u32) -> Result<()> {
    let merges = replay(root, tolerance)?;

    let strict = merges.iter().filter(|m| m.strict).count();
    let tolerant_only = merges.len() - strict;

    println!("Deduplication replay over {}", root.display());
    println!("  evaluator matching tolerance: ±{tolerance} lines\n");
    println!(
        "  pairs the STRICT rule merges (ranges intersect):        {strict}\n  \
         pairs merged ONLY because of the ±{tolerance} tolerance:            {tolerant_only}"
    );

    if tolerant_only > 0 {
        println!(
            "\n  Every pair below was merged on borrowed slack. The tolerance exists to\n  \
             forgive an off-by-a-line in a location estimate while scoring; it is not\n  \
             evidence that two claims are the same claim.\n"
        );
        let mut by_case: BTreeMap<(&str, &str), Vec<&Merge>> = BTreeMap::new();
        for m in merges.iter().filter(|m| !m.strict) {
            by_case
                .entry((m.case_id.as_str(), m.issue_type.as_str()))
                .or_default()
                .push(m);
        }
        for ((case_id, issue_type), group) in &by_case {
            let m = group[0];
            println!(
                "  {case_id}  [{issue_type}]  x{} run(s)\n    {}:{}-{}  {}\n    {}:{}-{}  {}",
                group.len(),
                m.file,
                m.a.0,
                m.a.1,
                truncate(&m.a_claim, 88),
                m.file,
                m.b.0,
                m.b.1,
                truncate(&m.b_claim, 88),
            );
            for r in group {
                println!("      seen in {}", r.run);
            }
            println!();
        }
    }

    if strict == 0 && tolerant_only == 0 {
        println!("\n  No pair in any recorded run matches the rule at either strictness.");
    }

    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n.saturating_sub(1)).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_that_share_a_line_intersect() {
        assert!(intersects((20, 25), (25, 30), 0));
    }

    #[test]
    fn ranges_one_line_apart_do_not_intersect() {
        assert!(!intersects((20, 24), (25, 30), 0));
    }

    #[test]
    fn the_tolerance_joins_ranges_that_do_not_touch() {
        // The c08 geometry: two distinct defects, two adjacent fields.
        assert!(!intersects((26, 28), (30, 32), 0));
        assert!(intersects((26, 28), (30, 32), 3));
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        let merges = replay(Path::new("no/such/place"), 3).unwrap();
        assert!(merges.is_empty());
    }
}

// --------------------------------------------------------------------------
// Match audit
// --------------------------------------------------------------------------

/// One matched pair, with both texts, ready to be read by a person.
#[derive(Debug)]
pub struct AuditedMatch {
    pub trial: String,
    pub case_id: String,
    pub expected_id: String,
    pub expected_issue_type: String,
    pub expected_location: String,
    pub truth: String,
    pub claim: String,
}

#[derive(Debug, Deserialize)]
struct RecordedEvaluation {
    per_case: Vec<RecordedCaseEval>,
}

#[derive(Debug, Deserialize)]
struct RecordedCaseEval {
    case_id: String,
    #[serde(default)]
    matched: Vec<RecordedMatch>,
}

#[derive(Debug, Deserialize)]
struct RecordedMatch {
    prediction_id: String,
    expected_id: String,
}

/// Pair every scored true positive with the ground truth it was credited for.
///
/// The evaluator matches on location and category. That is a **proxy** for
/// "found the defect": a claim landing on the right lines under an accepted
/// category scores a true positive whether or not it describes the actual bug,
/// and no deterministic matcher can tell the difference. Asking a model to
/// judge would reintroduce exactly the standard this project rejects.
///
/// So the answer is to put both texts in front of a person. This command does
/// only that — it reads artifacts, calls no model, and reaches no verdict of
/// its own.
pub fn audit_matches(benchmark: &Path, root: &Path) -> Result<Vec<AuditedMatch>> {
    // Ground truth, keyed by expected id.
    let mut truth = BTreeMap::new();
    for dir in crate::bench::discover_cases(benchmark)? {
        let Ok(gt) = crate::bench::load_ground_truth(&dir) else {
            continue;
        };
        for e in gt.expected_findings {
            truth.insert(
                e.id.clone(),
                (
                    e.issue_type.as_str().to_string(),
                    format!("{}:{}-{}", e.file, e.start_line, e.end_line),
                    e.description,
                ),
            );
        }
    }

    let mut out = Vec::new();
    for eval_path in evaluations(root) {
        let raw = std::fs::read_to_string(&eval_path)
            .with_context(|| format!("reading {}", eval_path.display()))?;
        let Ok(eval) = serde_json::from_str::<RecordedEvaluation>(&raw) else {
            continue;
        };

        // Claims live in the trajectories beside the evaluation.
        let dir = eval_path.parent().unwrap_or(Path::new("."));
        let mut claims = BTreeMap::new();
        for tp in advanced_trajectories(dir) {
            let Ok(raw) = std::fs::read_to_string(&tp) else {
                continue;
            };
            let Ok(t) = serde_json::from_str::<RecordedTrajectory>(&raw) else {
                continue;
            };
            for f in t.final_findings {
                claims.insert(f.id, f.claim);
            }
        }

        let trial = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| dir.display().to_string());

        for case in eval.per_case {
            for m in case.matched {
                let Some((issue_type, location, description)) = truth.get(&m.expected_id) else {
                    continue;
                };
                out.push(AuditedMatch {
                    trial: trial.clone(),
                    case_id: case.case_id.clone(),
                    expected_id: m.expected_id.clone(),
                    expected_issue_type: issue_type.clone(),
                    expected_location: location.clone(),
                    truth: description.clone(),
                    claim: claims
                        .get(&m.prediction_id)
                        .cloned()
                        .unwrap_or_else(|| "(claim text not found in trajectories)".to_string()),
                });
            }
        }
    }

    out.sort_by(|a, b| (&a.expected_id, &a.trial).cmp(&(&b.expected_id, &b.trial)));
    Ok(out)
}

/// Every `evaluation-advanced.json` under `root`, at any depth.
fn evaluations(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().and_then(|n| n.to_str()) == Some("evaluation-advanced.json") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Print the audit, grouping identical claims across trials.
pub fn report_matches(benchmark: &Path, root: &Path) -> Result<()> {
    let matches = audit_matches(benchmark, root)?;

    println!("Match audit: {} vs {}", benchmark.display(), root.display());
    println!(
        "  {} scored true positive(s). No verdict is computed here -- read the\n  \
         ground truth against the claim and decide whether the defect was found.\n",
        matches.len()
    );

    let mut current = String::new();
    let mut by_claim: BTreeMap<String, Vec<String>> = BTreeMap::new();

    let flush = |id: &str, by_claim: &mut BTreeMap<String, Vec<String>>| {
        if id.is_empty() {
            return;
        }
        for (claim, trials) in by_claim.iter() {
            println!("    CLAIM ({}): {claim}", trials.join(", "));
        }
        println!();
        by_claim.clear();
    };

    for m in &matches {
        if m.expected_id != current {
            flush(&current, &mut by_claim);
            current = m.expected_id.clone();
            println!(
                "  {} -- expected {} at {}",
                m.expected_id, m.expected_issue_type, m.expected_location
            );
            println!("    TRUTH: {}", m.truth);
        }
        by_claim
            .entry(m.claim.clone())
            .or_default()
            .push(m.trial.clone());
    }
    flush(&current, &mut by_claim);

    Ok(())
}
