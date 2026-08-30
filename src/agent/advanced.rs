//! Advanced reviewer: candidate → falsification question → investigation →
//! fresh-context verification → decision.
//!
//! Three properties are deliberate and load-bearing:
//!
//! 1. **The falsification question is fixed before any evidence is gathered.**
//!    A question written after the verdict would only rationalise it.
//!
//! 2. **The verifier runs in a genuinely fresh context.** It is a separate
//!    stateless request that receives the claim and the collected excerpts and
//!    nothing else — not the reviewer's reasoning, not the fact that a prior
//!    stage believed the claim, not the investigation's running commentary.
//!    There is no conversation object that could leak the anchor.
//!
//! 3. **Rust assigns the final status, not the model.** The verifier returns a
//!    judgement; [`decide`] decides what that judgement is worth given the
//!    evidence actually on file. A model that says "Supports" without
//!    repository-grounded evidence gets `Uncertain`.

use super::{case_file_context, parse_review};
use crate::bench::Case;
use crate::config::{Ablation, RunConfig};
use crate::finding::{
    CandidateFinding, Evidence, Finding, FindingStatus, VerificationOutcome, VerificationResult,
};
use crate::llm::{extract_json, LlmClient, LlmRequest, Stage};
use crate::prompts;
use crate::repo::RepoRoot;
use crate::tools::{self, ToolCall};
use crate::trajectory::{AgentKind, Trajectory, TrajectoryEvent};
use anyhow::Result;
use std::time::Instant;

const FILE_CONTEXT_LINES: u32 = 400;

/// Tool-call budget for a follow-up investigation. Smaller than the first
/// pass: it is aimed at one specific gap, not at exploring.
const FOLLOW_UP_TOOL_BUDGET: u32 = 4;

pub async fn run(case: &Case, client: &LlmClient, cfg: &RunConfig) -> Result<Trajectory> {
    let started = Instant::now();
    let mut traj = Trajectory::new(case.id(), AgentKind::Advanced, cfg);

    if cfg.ablation != Ablation::None {
        traj.push(TrajectoryEvent::Note {
            note: format!(
                "ABLATION: {} — this run deliberately disables part of the pipeline and is                  not the full system",
                cfg.ablation.as_str()
            ),
        });
    }

    let candidates = propose_candidates(case, client, cfg, &mut traj).await;
    let mut findings = Vec::new();

    for candidate in candidates {
        // Candidates-only: no investigation, no verification. Reported as
        // produced, which is what the advanced prompt alone is worth.
        if cfg.ablation == Ablation::CandidatesOnly {
            traj.push(TrajectoryEvent::Decision {
                candidate_id: candidate.id.clone(),
                status: FindingStatus::Verified,
                reason: "ablation candidates-only: reported without investigation or                          verification"
                    .to_string(),
            });
            findings.push(Finding {
                candidate,
                falsification_question: String::new(),
                evidence: Vec::new(),
                verification: None,
                status: FindingStatus::Verified,
                status_reason: "ablation candidates-only".to_string(),
            });
            continue;
        }

        let question = falsification_question(&candidate, client, cfg, &mut traj).await;
        traj.push(TrajectoryEvent::FalsificationQuestion {
            candidate_id: candidate.id.clone(),
            question: question.clone(),
        });

        let first_budget = cfg.max_tool_calls_per_finding;
        let mut evidence = investigate(
            case,
            &candidate,
            &question,
            client,
            cfg,
            &mut traj,
            None,
            "",
            first_budget,
        )
        .await;
        traj.push(TrajectoryEvent::EvidenceAssembled {
            candidate_id: candidate.id.clone(),
            evidence: evidence.clone(),
        });

        // No-falsification: investigation still runs, so the reviewer has the
        // same repository evidence, but nothing adjudicates it. Any candidate
        // the investigation backed is reported. This is the arm that shows
        // what falsification is actually worth.
        if cfg.ablation == Ablation::NoFalsification {
            let concrete = concrete_evidence_count(&evidence);
            let (status, reason) = if concrete > 0 {
                (
                    FindingStatus::Verified,
                    format!(
                        "ablation no-falsification: reported on {concrete} evidence item(s),                          unadjudicated"
                    ),
                )
            } else {
                (
                    FindingStatus::Uncertain,
                    "ablation no-falsification: investigation returned no evidence".to_string(),
                )
            };
            traj.push(TrajectoryEvent::Decision {
                candidate_id: candidate.id.clone(),
                status,
                reason: reason.clone(),
            });
            findings.push(Finding {
                candidate,
                falsification_question: question,
                evidence,
                verification: None,
                status,
                status_reason: reason,
            });
            continue;
        }

        let mut verification =
            verify_fresh(&candidate, &question, &evidence, client, cfg, &mut traj).await;

        // Recorded as soon as it exists. If a follow-up replaces it below, both
        // verdicts appear in the trajectory in order — a reader needs to see
        // the "Insufficient" that triggered the second look, not just the
        // answer that superseded it.
        if let Some(v) = &verification {
            traj.push(TrajectoryEvent::Verification {
                candidate_id: candidate.id.clone(),
                result: v.clone(),
            });
        }

        // Self-correction: an "Insufficient" verdict is not a dead end, it is
        // a statement of what is missing. Feed that back into one more
        // targeted investigation rather than discarding the candidate.
        //
        // Bounded to a single extra pass on purpose. The verdict that comes
        // back is what an independent reader concluded from the evidence; if a
        // second, directed look still cannot close the gap, a third is
        // unlikely to, and "Uncertain" is the honest answer.
        let needs_more = verification
            .as_ref()
            .map(|v| v.outcome == VerificationOutcome::Insufficient)
            .unwrap_or(false);

        if needs_more && cfg.max_followup_investigations > 0 && cfg.ablation != Ablation::NoFollowup
        {
            let gap = verification
                .as_ref()
                .map(|v| v.rationale.clone())
                .unwrap_or_default();

            traj.push(TrajectoryEvent::Note {
                note: format!(
                    "{}: verification was Insufficient; re-investigating against the stated gap",
                    candidate.id
                ),
            });

            let follow_up_budget = first_budget.min(FOLLOW_UP_TOOL_BUDGET);
            let extra = investigate(
                case,
                &candidate,
                &question,
                client,
                cfg,
                &mut traj,
                Some(&gap),
                "f",
                follow_up_budget,
            )
            .await;

            if extra.is_empty() {
                traj.push(TrajectoryEvent::Note {
                    note: format!(
                        "{}: follow-up investigation found nothing further; keeping the original verdict",
                        candidate.id
                    ),
                });
            } else {
                evidence.extend(extra);
                traj.push(TrajectoryEvent::EvidenceAssembled {
                    candidate_id: candidate.id.clone(),
                    evidence: evidence.clone(),
                });
                // Re-adjudicated from scratch on the fuller package, in a
                // fresh context again — the second verifier is not told that
                // an earlier one was unsure.
                verification =
                    verify_fresh(&candidate, &question, &evidence, client, cfg, &mut traj).await;

                if let Some(v) = &verification {
                    traj.push(TrajectoryEvent::Verification {
                        candidate_id: candidate.id.clone(),
                        result: v.clone(),
                    });
                }
            }
        }

        let (status, reason) = decide(&candidate, &evidence, verification.as_ref(), &case.repo);
        traj.push(TrajectoryEvent::Decision {
            candidate_id: candidate.id.clone(),
            status,
            reason: reason.clone(),
        });

        findings.push(Finding {
            candidate,
            falsification_question: question,
            evidence,
            verification,
            status,
            status_reason: reason,
        });
    }

    let reported = findings.iter().filter(|f| f.status.is_reported()).count();
    let cleared = findings
        .iter()
        .filter(|f| f.status == FindingStatus::Rejected)
        .count();
    let uncertain = findings
        .iter()
        .filter(|f| f.status == FindingStatus::Uncertain)
        .count();

    traj.push(TrajectoryEvent::HumanCheckpoint {
        note: format!(
            "{reported} verified finding(s) reported for human review; {cleared} investigated \
             and cleared; {uncertain} left uncertain. All findings, including cleared and \
             uncertain ones, remain in this trajectory. The system takes no action on the code: \
             it does not merge, reject, or modify anything.",
        ),
    });

    traj.finish(findings, started.elapsed().as_millis() as u64);
    Ok(traj)
}

// --------------------------------------------------------------------------
// Stage 1 — candidates
// --------------------------------------------------------------------------

async fn propose_candidates(
    case: &Case,
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
) -> Vec<CandidateFinding> {
    let system = prompts::advanced_system(case.manifest.language.as_str());
    let user = prompts::review_user(
        &case.manifest.description,
        &case.diff,
        &case_file_context(case, FILE_CONTEXT_LINES),
    );

    let resp = match client
        .complete(&LlmRequest {
            stage: Stage::Review,
            system: system.clone(),
            user: user.clone(),
            json_mode: true,
        })
        .await
    {
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

// --------------------------------------------------------------------------
// Stage 2 — falsification question
// --------------------------------------------------------------------------

/// Used when the model fails to produce a usable question. Investigation still
/// happens; it is simply steered by a generic disproof prompt rather than a
/// tailored one.
fn default_question(candidate: &CandidateFinding) -> String {
    format!(
        "What evidence in this repository would show that the following claim is false: {}",
        candidate.claim
    )
}

async fn falsification_question(
    candidate: &CandidateFinding,
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
) -> String {
    let system = prompts::falsify_system();
    let user = prompts::falsify_user(
        &candidate.claim,
        &candidate.location.to_string(),
        &candidate.reasoning,
    );

    let resp = match client
        .complete(&LlmRequest {
            stage: Stage::Falsify,
            system: system.clone(),
            user: user.clone(),
            json_mode: true,
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            traj.record_failure(
                Stage::Falsify,
                prompts::FALSIFY_V,
                &e.to_string(),
                cfg.llm.max_retries + 1,
            );
            return default_question(candidate);
        }
    };

    traj.record_call(Stage::Falsify, prompts::FALSIFY_V, &system, &user, &resp);

    extract_json(&resp.text)
        .ok()
        .and_then(|v| {
            v.get("falsification_question")
                .and_then(|q| q.as_str())
                .map(|s| s.trim().to_string())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_question(candidate))
}

// --------------------------------------------------------------------------
// Stage 3 — investigation
// --------------------------------------------------------------------------

/// Lines of surrounding context included with the claimed region.
const CLAIM_CONTEXT_LINES: u32 = 15;

/// Read the region the candidate points at, so the verifier can always see the
/// code being claimed about.
///
/// Without this the verifier can end up adjudicating a claim about code it was
/// never shown, and correctly answer "Insufficient" for a reason that has
/// nothing to do with the claim's merits. It is tagged `DiffHunk` rather than
/// `FileRegion` because it is the change under review rather than something
/// the investigation went and found — and [`concrete_evidence_count`] excludes
/// it for exactly that reason.
fn seed_claimed_region(repo: &RepoRoot, candidate: &CandidateFinding) -> Option<Evidence> {
    let loc = &candidate.location;
    let content = repo.read_to_string(&loc.file).ok()?;
    let total = content.lines().count() as u32;
    if total == 0 {
        return None;
    }

    let start = loc.start_line.saturating_sub(CLAIM_CONTEXT_LINES).max(1);
    let end = loc.end_line.saturating_add(CLAIM_CONTEXT_LINES).min(total);

    let excerpt: String = content
        .lines()
        .enumerate()
        .map(|(i, l)| (i as u32 + 1, l))
        .filter(|(n, _)| *n >= start && *n <= end)
        .map(|(n, l)| format!("{n:>5} | {l}"))
        .collect::<Vec<_>>()
        .join(
            "
",
        );

    Some(Evidence {
        kind: crate::finding::EvidenceKind::DiffHunk,
        file: Some(loc.file.clone()),
        start_line: Some(start),
        end_line: Some(end),
        symbol: None,
        excerpt,
        tool_call_id: format!("{}-claimed-region", candidate.id),
    })
}

/// Gather evidence for one candidate.
///
/// `gap` is what an independent check said it could not settle. It is `None`
/// on the first pass; on a follow-up it steers the investigation at the actual
/// gap instead of letting it pick a direction again from scratch. `pass`
/// labels this pass's tool-call ids in the trajectory, and `budget` bounds it.
#[allow(clippy::too_many_arguments)]
async fn investigate(
    case: &Case,
    candidate: &CandidateFinding,
    question: &str,
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
    gap: Option<&str>,
    pass: &str,
    budget: u32,
) -> Vec<Evidence> {
    let system = prompts::investigate_system(budget);
    let mut evidence = Vec::new();
    let mut history = String::new();

    // The claimed region is seeded once, on the first pass only.
    if gap.is_none() {
        if let Some(seed) = seed_claimed_region(&case.repo, candidate) {
            evidence.push(seed);
        }
    }

    for step in 0..budget {
        let user = prompts::investigate_user(
            &candidate.claim,
            &candidate.location.to_string(),
            question,
            &case.diff,
            if history.is_empty() {
                "(nothing yet)"
            } else {
                &history
            },
            gap,
        );

        let resp = match client
            .complete(&LlmRequest {
                stage: Stage::Investigate,
                system: system.clone(),
                user: user.clone(),
                json_mode: true,
            })
            .await
        {
            Ok(r) => r,
            Err(e) => {
                traj.record_failure(
                    Stage::Investigate,
                    prompts::INVESTIGATE_V,
                    &e.to_string(),
                    cfg.llm.max_retries + 1,
                );
                break;
            }
        };

        traj.record_call(
            Stage::Investigate,
            prompts::INVESTIGATE_V,
            &system,
            &user,
            &resp,
        );

        let Ok(value) = extract_json(&resp.text) else {
            traj.push(TrajectoryEvent::Note {
                note: format!(
                    "investigation step {} for {}: unparseable response, stopping",
                    step + 1,
                    candidate.id
                ),
            });
            break;
        };

        if value.get("done").and_then(|d| d.as_bool()).unwrap_or(false) {
            break;
        }

        let Some(tool) = value.get("tool").and_then(|t| t.as_str()) else {
            traj.push(TrajectoryEvent::Note {
                note: format!(
                    "investigation step {} for {}: no tool named and not done, stopping",
                    step + 1,
                    candidate.id
                ),
            });
            break;
        };

        let call = ToolCall {
            tool: tool.to_string(),
            arguments: value
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };

        let tool_call_id = format!("{}-{}t{}", candidate.id, pass, step + 1);
        let t0 = Instant::now();
        let result = tools::execute(&case.repo, &call, &tool_call_id, cfg);
        let duration_ms = t0.elapsed().as_millis() as u64;

        traj.push(TrajectoryEvent::ToolCall {
            tool_call_id: tool_call_id.clone(),
            candidate_id: candidate.id.clone(),
            tool: call.tool.clone(),
            arguments: call.arguments.clone(),
            response: result.text.clone(),
            ok: result.ok,
            duration_ms,
        });

        // The refusal text is fed back too, so the model can correct a bad
        // path or an empty search rather than repeating it.
        history.push_str(&format!(
            "\n--- step {} : {} {} ---\n{}\n",
            step + 1,
            call.tool,
            call.arguments,
            result.text
        ));

        evidence.extend(result.evidence);
    }

    evidence
}

// --------------------------------------------------------------------------
// Stage 4 — fresh-context verification
// --------------------------------------------------------------------------

/// Render evidence for the verifier.
///
/// Only repository-grounded items appear, each labelled with its origin so the
/// verifier can cite it. The reviewer's reasoning is deliberately absent.
fn render_evidence(evidence: &[Evidence]) -> String {
    if evidence.is_empty() {
        return "(no evidence was gathered)".to_string();
    }

    let mut out = String::new();
    for (i, e) in evidence.iter().enumerate() {
        let loc = match (&e.file, e.start_line, e.end_line) {
            (Some(f), Some(s), Some(en)) if s == en => format!("{f}:{s}"),
            (Some(f), Some(s), Some(en)) => format!("{f}:{s}-{en}"),
            (Some(f), _, _) => f.clone(),
            _ => "(repository)".to_string(),
        };
        out.push_str(&format!(
            "[E{}] {:?} from {}\n{}\n\n",
            i + 1,
            e.kind,
            loc,
            e.excerpt
        ));
    }
    out
}

async fn verify_fresh(
    candidate: &CandidateFinding,
    question: &str,
    evidence: &[Evidence],
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
) -> Option<VerificationResult> {
    let system = prompts::verify_system();
    let user = prompts::verify_user(
        &candidate.claim,
        &candidate.location.to_string(),
        question,
        &render_evidence(evidence),
    );

    let resp = match client
        .complete(&LlmRequest {
            stage: Stage::Verify,
            system: system.clone(),
            user: user.clone(),
            json_mode: true,
        })
        .await
    {
        Ok(r) => r,
        Err(e) => {
            traj.record_failure(
                Stage::Verify,
                prompts::VERIFY_V,
                &e.to_string(),
                cfg.llm.max_retries + 1,
            );
            return None;
        }
    };

    traj.record_call(Stage::Verify, prompts::VERIFY_V, &system, &user, &resp);

    let value = match extract_json(&resp.text) {
        Ok(v) => v,
        Err(e) => {
            traj.push(TrajectoryEvent::Note {
                note: format!("unparseable verification for {}: {e}", candidate.id),
            });
            return None;
        }
    };

    let outcome = match value.get("outcome").and_then(|o| o.as_str()).map(str::trim) {
        Some("Supports") => VerificationOutcome::Supports,
        Some("Contradicts") => VerificationOutcome::Contradicts,
        Some("Insufficient") => VerificationOutcome::Insufficient,
        other => {
            traj.push(TrajectoryEvent::Note {
                note: format!(
                    "verification for {} returned an unrecognised outcome {other:?}; \
                     treating as no verdict",
                    candidate.id
                ),
            });
            return None;
        }
    };

    Some(VerificationResult {
        outcome,
        rationale: value
            .get("rationale")
            .and_then(|r| r.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
        decisive_evidence: value
            .get("decisive_evidence")
            .and_then(|d| d.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

// --------------------------------------------------------------------------
// Stage 5 — decision (Rust, not the model)
// --------------------------------------------------------------------------

/// Evidence the investigation actually went and found.
///
/// A directory listing is not evidence about a claim, so `FileList` does not
/// count. Neither does an item with no file attached, nor an empty excerpt.
///
/// `DiffHunk` does not count either. That kind marks the claimed region, which
/// the orchestrator seeds automatically so the verifier can see the code under
/// discussion — it is the reviewer's starting material, not a discovery. If it
/// counted, every candidate would clear the evidence gate for free and the
/// gate would stop meaning anything. To reach `Verified`, a claim needs at
/// least one thing the investigation retrieved.
fn concrete_evidence_count(evidence: &[Evidence]) -> usize {
    evidence
        .iter()
        .filter(|e| {
            e.file.is_some()
                && !e.excerpt.trim().is_empty()
                && matches!(
                    e.kind,
                    crate::finding::EvidenceKind::Search | crate::finding::EvidenceKind::FileRegion
                )
        })
        .count()
}

/// Assign the final status.
///
/// This is the evidence-enforcement gate. The verifier's judgement is an
/// input, never the last word: `Supports` without concrete repository evidence
/// downgrades to `Uncertain`, because "the model said so" is exactly the
/// standard this project exists to reject.
pub fn decide(
    candidate: &CandidateFinding,
    evidence: &[Evidence],
    verification: Option<&VerificationResult>,
    repo: &RepoRoot,
) -> (FindingStatus, String) {
    // A finding whose file does not exist cannot be acted on by a human.
    if repo.resolve(&candidate.location.file).is_err()
        || !repo
            .resolve(&candidate.location.file)
            .map(|p| p.is_file())
            .unwrap_or(false)
    {
        return (
            FindingStatus::Rejected,
            format!(
                "location {} does not exist in the repository",
                candidate.location.file
            ),
        );
    }

    let Some(v) = verification else {
        return (
            FindingStatus::Uncertain,
            "no verification verdict was obtained".to_string(),
        );
    };

    let concrete = concrete_evidence_count(evidence);

    match v.outcome {
        VerificationOutcome::Contradicts => (
            FindingStatus::Rejected,
            format!(
                "fresh-context verification found the evidence contradicts the claim \
                 ({concrete} repository evidence item(s))"
            ),
        ),
        VerificationOutcome::Insufficient => (
            FindingStatus::Uncertain,
            format!(
                "fresh-context verification found the evidence insufficient to settle the \
                 claim ({concrete} repository evidence item(s))"
            ),
        ),
        VerificationOutcome::Supports if concrete == 0 => (
            FindingStatus::Uncertain,
            "verification reported support, but no concrete repository evidence was gathered; \
             an unsupported assertion is not a verification"
                .to_string(),
        ),
        VerificationOutcome::Supports => (
            FindingStatus::Verified,
            format!(
                "fresh-context verification found the evidence supports the claim, backed by \
                 {concrete} repository evidence item(s)"
            ),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::load_case;
    use crate::finding::{EvidenceKind, IssueType, Location, Severity};
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

    fn candidate(file: &str) -> CandidateFinding {
        CandidateFinding {
            id: "p1".into(),
            issue_type: IssueType::Correctness,
            severity: Severity::Medium,
            location: Location::new(file, 1, 1),
            claim: "something is wrong".into(),
            reasoning: "because".into(),
        }
    }

    fn concrete(kind: EvidenceKind) -> Evidence {
        Evidence {
            kind,
            file: Some("src/lib.rs".into()),
            start_line: Some(1),
            end_line: Some(1),
            symbol: None,
            excerpt: "fn a() {}".into(),
            tool_call_id: "t1".into(),
        }
    }

    fn verdict(outcome: VerificationOutcome) -> VerificationResult {
        VerificationResult {
            outcome,
            rationale: "r".into(),
            decisive_evidence: vec![],
        }
    }

    // --- the evidence-enforcement gate ---

    #[test]
    fn supports_with_evidence_is_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[concrete(EvidenceKind::FileRegion)],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Verified);
    }

    #[test]
    fn supports_without_evidence_is_downgraded_to_uncertain() {
        // The central rule: a model saying "verified" is not verification.
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, reason) = decide(
            &candidate("src/lib.rs"),
            &[],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
        assert!(reason.contains("not a verification"));
    }

    #[test]
    fn the_seeded_claimed_region_does_not_satisfy_the_evidence_gate() {
        // The claimed region is handed to the verifier so it can see the code
        // under discussion. It must not, by itself, promote a candidate to
        // Verified — otherwise every candidate clears the gate for free.
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[concrete(EvidenceKind::DiffHunk)],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
    }

    #[test]
    fn seeded_region_plus_an_investigation_finding_does_satisfy_the_gate() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[
                concrete(EvidenceKind::DiffHunk),
                concrete(EvidenceKind::Search),
            ],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Verified);
    }

    #[test]
    fn seeded_region_covers_the_claim_with_context() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path().join("t02");
        std::fs::create_dir_all(d.join("repository/src")).unwrap();
        std::fs::write(
            d.join("case.json"),
            r#"{"case_id":"t02","title":"t","description":"d","category":"RealIssue"}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("diff.patch"),
            "--- a
+++ b
",
        )
        .unwrap();
        std::fs::write(d.join("ground_truth.json"), r#"{"case_id":"t02"}"#).unwrap();
        let body: String = (1..=100)
            .map(|i| {
                format!(
                    "line {i}
"
                )
            })
            .collect();
        std::fs::write(d.join("repository/src/lib.rs"), body).unwrap();
        let case = crate::bench::load_case(&d).unwrap();

        let mut c = candidate("src/lib.rs");
        c.location = Location::new("src/lib.rs", 50, 50);
        let e = seed_claimed_region(&case.repo, &c).unwrap();

        assert_eq!(e.kind, EvidenceKind::DiffHunk);
        assert_eq!(e.start_line, Some(35));
        assert_eq!(e.end_line, Some(65));
        assert!(e.excerpt.contains("line 50"));
    }

    #[test]
    fn seeded_region_clamps_at_the_file_boundaries() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let mut c = candidate("src/lib.rs");
        c.location = Location::new("src/lib.rs", 1, 1);
        let e = seed_claimed_region(&case.repo, &c).unwrap();
        assert_eq!(e.start_line, Some(1));
        assert_eq!(e.end_line, Some(1));
    }

    #[test]
    fn seeded_region_is_none_for_a_path_outside_the_sandbox() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        assert!(seed_claimed_region(&case.repo, &candidate("../../secrets.txt")).is_none());
    }

    #[test]
    fn a_directory_listing_does_not_count_as_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let listing = Evidence {
            kind: EvidenceKind::FileList,
            file: Some("src/lib.rs".into()),
            start_line: None,
            end_line: None,
            symbol: None,
            excerpt: "src/lib.rs".into(),
            tool_call_id: "t1".into(),
        };
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[listing],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
    }

    #[test]
    fn empty_excerpt_does_not_count_as_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let mut e = concrete(EvidenceKind::Search);
        e.excerpt = "   ".into();
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[e],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
    }

    #[test]
    fn contradicts_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[concrete(EvidenceKind::Search)],
            Some(&verdict(VerificationOutcome::Contradicts)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Rejected);
    }

    #[test]
    fn insufficient_is_uncertain() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[concrete(EvidenceKind::Search)],
            Some(&verdict(VerificationOutcome::Insufficient)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
    }

    #[test]
    fn a_missing_verdict_is_uncertain_never_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("src/lib.rs"),
            &[concrete(EvidenceKind::FileRegion)],
            None,
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Uncertain);
    }

    #[test]
    fn a_finding_at_a_nonexistent_path_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, reason) = decide(
            &candidate("src/does_not_exist.rs"),
            &[concrete(EvidenceKind::FileRegion)],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Rejected);
        assert!(reason.contains("does not exist"));
    }

    #[test]
    fn a_finding_pointing_outside_the_sandbox_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let (s, _) = decide(
            &candidate("../../../etc/passwd"),
            &[concrete(EvidenceKind::FileRegion)],
            Some(&verdict(VerificationOutcome::Supports)),
            &case.repo,
        );
        assert_eq!(s, FindingStatus::Rejected);
    }

    // --- evidence rendering for the fresh verifier ---

    #[test]
    fn rendered_evidence_carries_locations() {
        let s = render_evidence(&[concrete(EvidenceKind::Search)]);
        assert!(s.contains("src/lib.rs:1"));
        assert!(s.contains("fn a() {}"));
    }

    #[test]
    fn rendered_evidence_says_so_when_empty() {
        assert!(render_evidence(&[]).contains("no evidence"));
    }

    #[test]
    fn the_verifier_is_never_shown_the_reviewer_reasoning() {
        // The anchor the fresh context exists to remove must not sneak back in
        // through the evidence block.
        let rendered = render_evidence(&[concrete(EvidenceKind::Search)]);
        let user = prompts::verify_user("claim", "src/lib.rs:1", "question", &rendered);
        assert!(
            !user.contains("because"),
            "reviewer reasoning leaked to the verifier"
        );
    }

    // --- end to end against the offline stub ---

    #[tokio::test]
    async fn runs_end_to_end_and_reports_nothing_without_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        let case = seed_case(tmp.path());
        let cfg = RunConfig::mock();
        let client = LlmClient::from_config(&cfg.llm).unwrap();

        let t = run(&case, &client, &cfg).await.unwrap();
        assert_eq!(t.agent, AgentKind::Advanced);
        assert!(t
            .events
            .iter()
            .any(|e| matches!(e, TrajectoryEvent::HumanCheckpoint { .. })));
        // The stub proposes nothing, so there is nothing to verify.
        assert!(t.final_findings.is_empty());
    }

    // --- the self-correction loop ---
    //
    // These matter more than they look. On the real benchmark the verifier
    // never returned `Insufficient`, so the follow-up branch never executed in
    // any measured run. "Never fired" and "cannot fire" are observationally
    // identical from the results alone, and reporting the loop as inert is
    // only honest if the loop demonstrably works when the condition it waits
    // for actually occurs. These tests force that condition.

    /// A case whose repository has something worth finding on a second look.
    fn loop_case(dir: &Path) -> Case {
        let d = dir.join("t03");
        std::fs::create_dir_all(d.join("repository/src")).unwrap();
        std::fs::write(
            d.join("case.json"),
            r#"{"case_id":"t03","title":"t","description":"d","category":"RealIssue"}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("diff.patch"),
            "--- a/src/lib.rs
+++ b/src/lib.rs
",
        )
        .unwrap();
        std::fs::write(d.join("ground_truth.json"), r#"{"case_id":"t03"}"#).unwrap();
        std::fs::write(
            d.join("repository/src/lib.rs"),
            "fn a() {}
fn caller() { a(); }
",
        )
        .unwrap();
        crate::bench::load_case(&d).unwrap()
    }

    fn one_candidate() -> String {
        r#"{"findings":[{"issue_type":"Correctness","severity":"High",
             "file":"src/lib.rs","start_line":1,"end_line":1,
             "claim":"a() can be reached with bad state","reasoning":"r"}]}"#
            .to_string()
    }

    fn read_then_done() -> Vec<String> {
        vec![
            r#"{"done":false,"tool":"read","arguments":{"file":"src/lib.rs","start_line":1,"end_line":2},"rationale":"look"}"#.to_string(),
            r#"{"done":true,"tool":null,"arguments":null,"rationale":"enough"}"#.to_string(),
        ]
    }

    fn scripted_client(verify: Vec<&str>, investigate_turns: usize) -> LlmClient {
        let mut script = std::collections::HashMap::new();
        script.insert(Stage::Review, vec![one_candidate()]);
        script.insert(
            Stage::Falsify,
            vec![r#"{"falsification_question":"do callers guard it?"}"#.to_string()],
        );
        let mut inv = Vec::new();
        for _ in 0..investigate_turns {
            inv.extend(read_then_done());
        }
        script.insert(Stage::Investigate, inv);
        script.insert(
            Stage::Verify,
            verify.into_iter().map(|s| s.to_string()).collect(),
        );
        LlmClient::Mock(crate::llm::MockClient::scripted(script))
    }

    #[tokio::test]
    async fn an_insufficient_verdict_triggers_a_second_investigation() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let cfg = RunConfig::mock();
        assert_eq!(cfg.max_followup_investigations, 1);

        let client = scripted_client(
            vec![
                r#"{"outcome":"Insufficient","rationale":"no callers shown","decisive_evidence":[]}"#,
                r#"{"outcome":"Supports","rationale":"caller found","decisive_evidence":["src/lib.rs:2"]}"#,
            ],
            2, // one investigation pass, then the follow-up pass
        );

        let t = run(&case, &client, &cfg).await.unwrap();

        let re_investigated = t.events.iter().any(
            |e| matches!(e, TrajectoryEvent::Note { note } if note.contains("re-investigating")),
        );
        assert!(
            re_investigated,
            "the follow-up must be recorded in the trajectory"
        );

        let verifications = t
            .events
            .iter()
            .filter(|e| matches!(e, TrajectoryEvent::Verification { .. }))
            .count();
        assert_eq!(verifications, 2, "the claim must be re-adjudicated");

        assert_eq!(t.final_findings.len(), 1);
        assert_eq!(
            t.final_findings[0].status,
            FindingStatus::Verified,
            "the second verdict decides the outcome"
        );
    }

    #[tokio::test]
    async fn the_follow_up_is_disabled_when_the_budget_is_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let mut cfg = RunConfig::mock();
        cfg.max_followup_investigations = 0;

        let client = scripted_client(
            vec![
                r#"{"outcome":"Insufficient","rationale":"no callers shown","decisive_evidence":[]}"#,
                r#"{"outcome":"Supports","rationale":"should never be reached","decisive_evidence":[]}"#,
            ],
            2,
        );

        let t = run(&case, &client, &cfg).await.unwrap();
        assert!(!t.events.iter().any(|e| {
            matches!(e, TrajectoryEvent::Note { note } if note.contains("re-investigating"))
        }));
        assert_eq!(t.final_findings[0].status, FindingStatus::Uncertain);
    }

    #[tokio::test]
    async fn the_no_followup_ablation_disables_the_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let mut cfg = RunConfig::mock();
        cfg.ablation = Ablation::NoFollowup;

        let client = scripted_client(
            vec![
                r#"{"outcome":"Insufficient","rationale":"no callers shown","decisive_evidence":[]}"#,
                r#"{"outcome":"Supports","rationale":"should never be reached","decisive_evidence":[]}"#,
            ],
            2,
        );

        let t = run(&case, &client, &cfg).await.unwrap();
        assert!(!t.events.iter().any(|e| {
            matches!(e, TrajectoryEvent::Note { note } if note.contains("re-investigating"))
        }));
        assert_eq!(t.final_findings[0].status, FindingStatus::Uncertain);
    }

    #[tokio::test]
    async fn a_decisive_verdict_never_triggers_a_second_look() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let cfg = RunConfig::mock();

        // This is what every real run looked like: Supports first time.
        let client = scripted_client(
            vec![r#"{"outcome":"Supports","rationale":"clear","decisive_evidence":["x"]}"#],
            1,
        );

        let t = run(&case, &client, &cfg).await.unwrap();
        assert_eq!(
            t.events
                .iter()
                .filter(|e| matches!(e, TrajectoryEvent::Verification { .. }))
                .count(),
            1
        );
        assert_eq!(t.final_findings[0].status, FindingStatus::Verified);
    }

    #[test]
    fn default_question_is_used_when_the_model_gives_none() {
        let q = default_question(&candidate("src/lib.rs"));
        assert!(q.contains("false"));
        assert!(q.contains("something is wrong"));
    }
}
