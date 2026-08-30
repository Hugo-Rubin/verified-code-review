//! Advanced reviewer: candidate → investigation → falsification → fresh-context
//! verification.
//!
//! PHASE 1 STATUS: skeleton. Candidate generation is live; the investigation
//! tools and the fresh-context verifier land in Phase 4, after the baseline
//! GO/NO-GO checkpoint has shown the failure mode is real. Building them before
//! that would be building a solution to an unmeasured problem.
//!
//! Until then every candidate is classified `Uncertain` with an explicit
//! reason. It must never be `Verified`: a finding that has not been through
//! falsification has not earned that status, and letting the skeleton report
//! findings would silently turn the advanced arm into a second baseline.

use super::{case_file_context, parse_review};
use crate::bench::Case;
use crate::config::RunConfig;
use crate::finding::{Finding, FindingStatus};
use crate::llm::{extract_json, LlmClient, LlmRequest, Stage};
use crate::prompts;
use crate::trajectory::{AgentKind, Trajectory, TrajectoryEvent};
use anyhow::Result;
use std::time::Instant;

const FILE_CONTEXT_LINES: u32 = 400;

/// Reason recorded on every finding while the loop is still a skeleton.
const NOT_YET_IMPLEMENTED: &str =
    "advanced pipeline incomplete (Phase 1 skeleton): no investigation or \
     fresh-context falsification has run, so this candidate cannot be verified";

pub async fn run(case: &Case, client: &LlmClient, cfg: &RunConfig) -> Result<Trajectory> {
    let started = Instant::now();
    let mut traj = Trajectory::new(case.id(), AgentKind::Advanced, cfg);

    traj.push(TrajectoryEvent::Note {
        note: "Phase 1 skeleton: candidate generation only.".to_string(),
    });

    let candidates = propose_candidates(case, client, cfg, &mut traj).await;

    let findings: Vec<Finding> = candidates
        .into_iter()
        .map(|c| {
            traj.push(TrajectoryEvent::Decision {
                candidate_id: c.id.clone(),
                status: FindingStatus::Uncertain,
                reason: NOT_YET_IMPLEMENTED.to_string(),
            });
            Finding {
                candidate: c,
                falsification_question: String::new(),
                evidence: Vec::new(),
                verification: None,
                status: FindingStatus::Uncertain,
                status_reason: NOT_YET_IMPLEMENTED.to_string(),
            }
        })
        .collect();

    traj.push(TrajectoryEvent::HumanCheckpoint {
        note: format!(
            "{} candidate(s) held as Uncertain pending the verification loop. The system \
             takes no action on the code: it does not merge, reject, or modify anything.",
            findings.len()
        ),
    });

    traj.finish(findings, started.elapsed().as_millis() as u64);
    Ok(traj)
}

/// Stage 1: propose candidate findings from the diff.
async fn propose_candidates(
    case: &Case,
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
) -> Vec<crate::finding::CandidateFinding> {
    let system = prompts::advanced_system();
    let user = prompts::review_user(
        &case.manifest.description,
        &case.diff,
        &case_file_context(case, FILE_CONTEXT_LINES),
    );

    let req = LlmRequest {
        stage: Stage::Review,
        system: system.clone(),
        user: user.clone(),
        json_mode: true,
    };

    let resp = match client.complete(&req).await {
        Ok(r) => r,
        Err(e) => {
            traj.record_failure(
                Stage::Review,
                prompts::ADVANCED_REVIEW_V,
                &e.to_string(),
                cfg.llm.max_retries + 1,
            );
            return Vec::new();
        }
    };

    traj.record_call(
        Stage::Review,
        prompts::ADVANCED_REVIEW_V,
        &system,
        &user,
        &resp,
    );

    let value = match extract_json(&resp.text) {
        Ok(v) => v,
        Err(e) => {
            traj.push(TrajectoryEvent::Note {
                note: format!("unparseable review response: {e}"),
            });
            return Vec::new();
        }
    };

    let parsed = parse_review(&value, &format!("{}-adv", case.id()));
    for w in &parsed.warnings {
        traj.push(TrajectoryEvent::Note { note: w.clone() });
    }
    for c in &parsed.candidates {
        traj.push(TrajectoryEvent::CandidateProposed {
            candidate: c.clone(),
        });
    }
    parsed.candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::load_case;
    use std::path::Path;

    fn seed_case(dir: &Path) -> Case {
        let d = dir.join("t01");
        std::fs::create_dir_all(d.join("repository/src")).unwrap();
        std::fs::write(
            d.join("case.json"),
            r#"{"case_id":"t01","title":"t","description":"d","category":"RealIssue"}"#,
        )
        .unwrap();
        std::fs::write(d.join("diff.patch"), "--- a/src/lib.rs\n+++ b/src/lib.rs\n").unwrap();
        std::fs::write(d.join("ground_truth.json"), r#"{"case_id":"t01"}"#).unwrap();
        std::fs::write(d.join("repository/src/lib.rs"), "fn a() {}\n").unwrap();
        load_case(&d).unwrap()
    }

    #[tokio::test]
    async fn skeleton_runs_and_reports_nothing_as_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();
        assert_eq!(t.agent, AgentKind::Advanced);
        assert!(
            t.final_findings
                .iter()
                .all(|f| f.status != FindingStatus::Verified),
            "the skeleton must never mark a finding Verified"
        );
    }

    #[tokio::test]
    async fn skeleton_records_a_human_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();
        assert!(t
            .events
            .iter()
            .any(|e| matches!(e, TrajectoryEvent::HumanCheckpoint { .. })));
    }
}
