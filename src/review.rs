//! Reviewing a real change, outside the benchmark.
//!
//! Everything else in this crate exists to *measure* the reviewer. This is the
//! reviewer used as a tool: point it at a working tree and a diff and it runs
//! the same pipeline, with the same prompts, the same sandbox and the same
//! evidence gate that produced every number in the README.
//!
//! Nothing here is a special path. A benchmark case is a directory containing
//! `case.json`, `diff.patch` and `repository/`; a real review is the same
//! [`Case`] value assembled in memory from a repository path and a diff file.
//! If this produced better results than the benchmark harness, the benchmark
//! would be measuring the wrong thing.
//!
//! There is deliberately **no ground truth** here and no score. The output is a
//! report for a human, which is the only thing this system is allowed to
//! produce.

use crate::bench::{Case, CaseCategory, CaseManifest, Language};
use crate::config::RunConfig;
use crate::finding::{Finding, FindingStatus};
use crate::llm::LlmClient;
use crate::repo::RepoRoot;
use crate::trajectory::{AgentKind, Trajectory};
use crate::{agent, runner};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

/// What to review.
pub struct ReviewRequest {
    pub repo: PathBuf,
    pub diff_path: Option<PathBuf>,
    pub title: String,
    pub description: String,
    pub language: Language,
    pub agent: AgentKind,
    pub out: Option<PathBuf>,
}

/// Read the diff from a file, or from stdin when the path is `-` or absent.
fn read_diff(path: Option<&Path>) -> Result<String> {
    let diff = match path {
        Some(p) if p != Path::new("-") => std::fs::read_to_string(p)
            .with_context(|| format!("reading diff from {}", p.display()))?,
        _ => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading diff from stdin")?;
            buf
        }
    };
    if diff.trim().is_empty() {
        bail!("the diff is empty; nothing to review");
    }
    Ok(diff)
}

/// Assemble the same `Case` type the benchmark uses, in memory.
///
/// `category` is `RealIssue` and is never read by either agent — it exists for
/// reporting breakdowns over a benchmark and has no meaning for a real change.
/// The value is arbitrary precisely because nothing may branch on it.
fn build_case(req: &ReviewRequest, diff: String) -> Result<Case> {
    let repo = RepoRoot::new(&req.repo)
        .with_context(|| format!("opening repository at {}", req.repo.display()))?;

    Ok(Case {
        manifest: CaseManifest {
            case_id: "review".to_string(),
            title: req.title.clone(),
            description: req.description.clone(),
            category: CaseCategory::RealIssue,
            language: req.language,
        },
        diff,
        repo,
        dir: req.repo.clone(),
    })
}

/// Run a review and return the trajectory.
pub async fn review(req: &ReviewRequest, cfg: &RunConfig) -> Result<Trajectory> {
    let diff = read_diff(req.diff_path.as_deref())?;
    let case = build_case(req, diff)?;

    let client = LlmClient::from_config(&cfg.llm).context("constructing LLM client")?;

    let traj = match req.agent {
        AgentKind::Baseline => agent::baseline::run(&case, &client, cfg).await?,
        AgentKind::Advanced => agent::advanced::run(&case, &client, cfg).await?,
    };

    if let Some(out) = &req.out {
        let path = traj.write(out)?;
        eprintln!("trajectory: {}", path.display());
    }

    Ok(traj)
}

/// Render a review for a person to read.
///
/// Reported findings come first, because they are what a human is being asked
/// to spend attention on. What was investigated and cleared comes after, and it
/// is not hidden: a reviewer who disagrees with a rejection needs to see the
/// claim and the evidence that closed it, or the system is asking to be trusted
/// rather than checked.
pub fn render(traj: &Trajectory) -> String {
    let mut out = String::new();
    let reported: Vec<&Finding> = traj
        .final_findings
        .iter()
        .filter(|f| f.status.is_reported())
        .collect();
    let cleared: Vec<&Finding> = traj
        .final_findings
        .iter()
        .filter(|f| f.status == FindingStatus::Rejected)
        .collect();
    let uncertain: Vec<&Finding> = traj
        .final_findings
        .iter()
        .filter(|f| f.status == FindingStatus::Uncertain)
        .collect();

    out.push_str(&format!(
        "\n{} finding(s) for review · {} investigated and cleared · {} uncertain\n",
        reported.len(),
        cleared.len(),
        uncertain.len()
    ));
    out.push_str(&format!(
        "{} model call(s), {} tool call(s), {} ms\n",
        traj.llm_calls, traj.tool_calls, traj.runtime_ms
    ));
    if let Some(cost) = traj.cost_usd {
        out.push_str(&format!("cost: ${cost:.5}\n"));
    }

    if reported.is_empty() {
        // Two very different silences, and reporting the wrong one is a false
        // statement about what the system did. Caught by running this tool on
        // this project's own diff: it proposed nothing, and the report claimed
        // every candidate had been investigated and ruled out.
        let proposed = traj
            .events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    crate::trajectory::TrajectoryEvent::CandidateProposed { .. }
                )
            })
            .count();
        if proposed == 0 {
            out.push_str(
                "\nNothing reported, and nothing proposed: the reviewer read the change and \
                 raised\nno candidate to investigate. That is not the same as having checked \
                 something\nand cleared it, and it is weaker evidence that the change is \
                 sound.\n",
            );
        } else {
            out.push_str(
                "\nNothing reported. That is a result, not a failure to run — every candidate\n\
                 below was investigated against the repository and ruled out.\n",
            );
        }
    }

    for f in &reported {
        out.push_str(&format!(
            "\n──────────────────────────────────────────────────────────────\n\
             {} · {:?} · {}\n  {}\n",
            f.candidate.location,
            f.candidate.severity,
            f.candidate.issue_type.as_str(),
            f.candidate.claim
        ));
        out.push_str(&format!(
            "\n  Checked by asking: {}\n",
            f.falsification_question
        ));
        if let Some(v) = &f.verification {
            out.push_str(&format!("  Independent verdict: {:?}\n", v.outcome));
            if !v.rationale.trim().is_empty() {
                out.push_str(&format!("    {}\n", v.rationale.trim()));
            }
        }
        let cited: Vec<String> = f
            .evidence
            .iter()
            .filter_map(|e| {
                let file = e.file.as_ref()?;
                Some(match (e.start_line, e.end_line) {
                    (Some(a), Some(b)) if a != b => format!("{file}:{a}-{b}"),
                    (Some(a), _) => format!("{file}:{a}"),
                    _ => file.clone(),
                })
            })
            .collect();
        if !cited.is_empty() {
            out.push_str(&format!("  Evidence read: {}\n", cited.join(", ")));
        }
        // The anchor says where to start; this says what makes it wrong. Both
        // matter for a defect that is an interaction between two files.
        let related = f.related_files();
        if !related.is_empty() {
            out.push_str(&format!("  Depends on code in: {}\n", related.join(", ")));
        }
    }

    if !cleared.is_empty() {
        out.push_str("\n── Investigated and cleared ──────────────────────────────────\n");
        out.push_str("(shown so a rejection can be disagreed with, not to pad the report)\n");
        for f in &cleared {
            out.push_str(&format!(
                "\n  {} · {}\n    claim: {}\n    cleared because: {}\n",
                f.candidate.location,
                f.candidate.issue_type.as_str(),
                f.candidate.claim,
                f.verification
                    .as_ref()
                    .map(|v| v.rationale.trim())
                    .filter(|r| !r.is_empty())
                    .unwrap_or(f.status_reason.trim())
            ));
        }
    }

    if !uncertain.is_empty() {
        out.push_str("\n── Withheld as uncertain ─────────────────────────────────────\n");
        for f in &uncertain {
            out.push_str(&format!(
                "\n  {} · {}\n    claim: {}\n    withheld because: {}\n",
                f.candidate.location,
                f.candidate.issue_type.as_str(),
                f.candidate.claim,
                f.status_reason.trim()
            ));
        }
    }

    out.push_str(
        "\n──────────────────────────────────────────────────────────────\n\
         This system does not merge, reject, approve or modify anything. A\n\
         human decides. Findings are evidence-backed claims, not verdicts.\n",
    );
    out
}

/// Write the rendered review next to the trajectory, if an output directory
/// was given.
pub fn write_report(traj: &Trajectory, out: &Path) -> Result<PathBuf> {
    let path = out.join("review.md");
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    std::fs::write(&path, render(traj)).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Re-exported so `main.rs` does not need to know how a summary is written.
pub use runner::write_json;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{CandidateFinding, IssueType, Location, Severity};

    fn finding(status: FindingStatus) -> Finding {
        Finding {
            candidate: CandidateFinding {
                id: "f1".into(),
                issue_type: IssueType::Correctness,
                severity: Severity::High,
                location: Location::new("src/a.rs", 10, 12),
                claim: "the counter is never decremented".into(),
                reasoning: "internal reasoning that must not be shown".into(),
            },
            falsification_question: "does any caller decrement it?".into(),
            evidence: vec![],
            verification: None,
            status,
            status_reason: "because".into(),
        }
    }

    fn traj_with(findings: Vec<Finding>) -> Trajectory {
        let cfg = RunConfig::mock();
        let mut t = Trajectory::new("review", AgentKind::Advanced, &cfg);
        t.finish(findings, 100);
        t
    }

    #[test]
    fn a_reported_finding_is_rendered_with_its_falsification_question() {
        let r = render(&traj_with(vec![finding(FindingStatus::Verified)]));
        assert!(r.contains("the counter is never decremented"));
        assert!(r.contains("does any caller decrement it?"));
    }

    #[test]
    fn the_reviewer_reasoning_is_never_shown_to_the_reader() {
        // The report carries claims and repository evidence. The model's
        // internal narration is exactly what a reader should not be asked to
        // weigh, and showing it invites trusting the prose over the evidence.
        let r = render(&traj_with(vec![finding(FindingStatus::Verified)]));
        assert!(!r.contains("internal reasoning that must not be shown"));
    }

    #[test]
    fn cleared_findings_are_shown_rather_than_dropped() {
        let r = render(&traj_with(vec![finding(FindingStatus::Rejected)]));
        assert!(r.contains("Investigated and cleared"));
        assert!(r.contains("the counter is never decremented"));
    }

    #[test]
    fn a_review_with_no_candidates_says_nothing_was_proposed() {
        // Distinct from "investigated and cleared". Claiming the stronger one
        // when no candidate was ever raised is a false statement about what
        // the system did, and the first run of this command on a real diff
        // made exactly that claim.
        let r = render(&traj_with(vec![]));
        assert!(r.contains("nothing proposed"));
        assert!(!r.contains("investigated against the repository and ruled out"));
    }

    #[test]
    fn a_review_that_cleared_everything_says_so_instead() {
        let cfg = RunConfig::mock();
        let mut t = Trajectory::new("review", AgentKind::Advanced, &cfg);
        let f = finding(FindingStatus::Rejected);
        t.push(crate::trajectory::TrajectoryEvent::CandidateProposed {
            candidate: f.candidate.clone(),
        });
        t.finish(vec![f], 100);
        let r = render(&t);
        assert!(r.contains("investigated against the repository and ruled out"));
        assert!(!r.contains("nothing proposed"));
    }

    #[test]
    fn every_report_states_that_no_action_is_taken() {
        for status in [
            FindingStatus::Verified,
            FindingStatus::Rejected,
            FindingStatus::Uncertain,
        ] {
            let r = render(&traj_with(vec![finding(status)]));
            assert!(
                r.contains("A\nhuman decides"),
                "the human-in-the-loop statement must appear on every report"
            );
        }
    }

    #[test]
    fn an_empty_diff_is_refused() {
        let err = read_diff(Some(Path::new("no/such/diff.patch"))).unwrap_err();
        assert!(err.to_string().contains("reading diff"));
    }

    #[test]
    fn a_review_case_carries_no_ground_truth_path() {
        // The same guarantee the benchmark relies on: `Case` has no field
        // pointing at ground truth, so an ad-hoc review cannot smuggle one in.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("lib.rs"), "fn a() {}\n").unwrap();
        let req = ReviewRequest {
            repo: tmp.path().to_path_buf(),
            diff_path: None,
            title: "t".into(),
            description: "d".into(),
            language: Language::Rust,
            agent: AgentKind::Advanced,
            out: None,
        };
        let case = build_case(&req, "--- a\n+++ b\n".into()).unwrap();
        assert_eq!(case.id(), "review");
        assert!(case.repo.resolve("ground_truth.json").is_err());
    }

    #[test]
    fn related_files_names_the_other_file_a_defect_depends_on() {
        let mut f = finding(FindingStatus::Verified);
        f.evidence = vec![
            crate::finding::Evidence {
                kind: crate::finding::EvidenceKind::FileRegion,
                file: Some("src/store.rs".into()),
                start_line: Some(10),
                end_line: Some(20),
                symbol: None,
                excerpt: "fn len(&self) -> usize { self.capacity }".into(),
                tool_call_id: "t1".into(),
            },
            // Same file as the anchor: already where the reader is looking.
            crate::finding::Evidence {
                kind: crate::finding::EvidenceKind::FileRegion,
                file: Some("src/a.rs".into()),
                start_line: Some(1),
                end_line: Some(5),
                symbol: None,
                excerpt: "fn a() {}".into(),
                tool_call_id: "t2".into(),
            },
        ];
        assert_eq!(f.related_files(), vec!["src/store.rs".to_string()]);
        assert!(render(&traj_with(vec![f])).contains("Depends on code in: src/store.rs"));
    }

    #[test]
    fn related_files_ignores_the_seeded_diff_hunk_and_directory_listings() {
        // Same rule as the evidence gate: the claimed region is the reviewer's
        // starting material and a file listing is not a fact about behaviour.
        let mut f = finding(FindingStatus::Verified);
        f.evidence = vec![
            crate::finding::Evidence {
                kind: crate::finding::EvidenceKind::DiffHunk,
                file: Some("src/seeded.rs".into()),
                start_line: Some(1),
                end_line: Some(9),
                symbol: None,
                excerpt: "context".into(),
                tool_call_id: "t1".into(),
            },
            crate::finding::Evidence {
                kind: crate::finding::EvidenceKind::FileList,
                file: Some("src/listed.rs".into()),
                start_line: None,
                end_line: None,
                symbol: None,
                excerpt: "src/listed.rs".into(),
                tool_call_id: "t2".into(),
            },
        ];
        assert!(f.related_files().is_empty());
    }

    #[test]
    fn a_finding_with_no_investigation_names_no_related_files() {
        assert!(finding(FindingStatus::Verified).related_files().is_empty());
    }
}
