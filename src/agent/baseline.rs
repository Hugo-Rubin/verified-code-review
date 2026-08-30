//! Baseline reviewer: one direct pass over the diff.
//!
//! This is the fair comparison point — a reasonable direct-review setup, not a
//! strawman. It uses the same model, the same temperature, the same output
//! schema, and the same view of the changed files as the advanced system. The
//! only thing it lacks is the ability to investigate the repository and to
//! falsify its own conclusions.
//!
//! Every finding it produces is reported as-is. That is the point: there is no
//! verification stage, so "the model said so" is the whole basis.

use super::{case_file_context, parse_review};
use crate::bench::Case;
use crate::config::RunConfig;
use crate::finding::Finding;
use crate::llm::{extract_json, LlmClient, LlmRequest, Stage};
use crate::prompts;
use crate::trajectory::{AgentKind, Trajectory, TrajectoryEvent};
use anyhow::Result;
use std::time::Instant;

/// Lines of each changed file shown to the reviewer.
const FILE_CONTEXT_LINES: u32 = 400;

pub async fn run(case: &Case, client: &LlmClient, cfg: &RunConfig) -> Result<Trajectory> {
    let started = Instant::now();
    let mut traj = Trajectory::new(case.id(), AgentKind::Baseline, cfg);

    let system = prompts::baseline_system(case.manifest.language.as_str());
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

    let findings: Vec<Finding> = match client.complete(&req).await {
        Err(e) => {
            // A failed review is a real outcome. It is recorded and the case
            // scores zero findings; it is never retried into a different
            // configuration or quietly dropped from the results.
            traj.record_failure(
                Stage::Review,
                prompts::BASELINE_REVIEW_V,
                &e.to_string(),
                cfg.llm.max_retries + 1,
            );
            Vec::new()
        }
        Ok(resp) => {
            traj.record_call(
                Stage::Review,
                prompts::BASELINE_REVIEW_V,
                &system,
                &user,
                &resp,
            );

            match extract_json(&resp.text) {
                Err(e) => {
                    traj.push(TrajectoryEvent::Note {
                        note: format!("unparseable review response: {e}"),
                    });
                    Vec::new()
                }
                Ok(value) => {
                    let parsed = parse_review(&value, &format!("{}-base", case.id()));
                    for w in &parsed.warnings {
                        traj.push(TrajectoryEvent::Note { note: w.clone() });
                    }
                    parsed
                        .candidates
                        .into_iter()
                        .map(|c| {
                            traj.push(TrajectoryEvent::CandidateProposed {
                                candidate: c.clone(),
                            });
                            Finding::from_candidate_unverified(c)
                        })
                        .collect()
                }
            }
        }
    };

    traj.push(TrajectoryEvent::HumanCheckpoint {
        note: format!(
            "{} finding(s) reported for human review. The system takes no action on the \
             code: it does not merge, reject, or modify anything.",
            findings.len()
        ),
    });

    traj.finish(findings, started.elapsed().as_millis() as u64);
    Ok(traj)
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
    async fn produces_a_complete_trajectory_against_the_mock() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();

        assert_eq!(t.case_id, "t01");
        assert_eq!(t.agent, AgentKind::Baseline);
        assert_eq!(t.llm_calls, 1);
        assert!(t.finished_at.is_some());
        // The mock returns no findings, so the run is empty but well-formed.
        assert!(t.final_findings.is_empty());
    }

    #[tokio::test]
    async fn always_ends_at_a_human_checkpoint() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();
        assert!(
            t.events
                .iter()
                .any(|e| matches!(e, TrajectoryEvent::HumanCheckpoint { .. })),
            "a run must record that a human is the final reviewer"
        );
    }

    #[tokio::test]
    async fn records_the_prompt_it_actually_sent() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();
        let call = t
            .events
            .iter()
            .find_map(|e| match e {
                TrajectoryEvent::LlmCall {
                    prompt_version,
                    user,
                    ..
                } => Some((prompt_version.clone(), user.clone())),
                _ => None,
            })
            .expect("a review call must be recorded");

        assert_eq!(call.0, prompts::BASELINE_REVIEW_V);
        assert!(
            call.1.contains("src/lib.rs"),
            "file context must be included"
        );
    }
}
