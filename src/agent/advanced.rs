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

    let proposed = propose_candidates(case, client, cfg, &mut traj).await;

    let (candidates, merged) = deduplicate_candidates(proposed);
    for (dropped, kept) in &merged {
        traj.push(TrajectoryEvent::Note {
            note: format!(
                "candidate {dropped} describes the same defect as {kept} (same category,                  overlapping lines); merged so it is investigated and reported once"
            ),
        });
    }

    // Facts carried between candidates within this case. Never verdicts.
    let mut memory = CaseMemory::default();
    let mut findings = Vec::new();

    for candidate in candidates {
        let f = adjudicate(case, candidate, client, cfg, &mut traj, &mut memory).await;
        findings.push(f);
    }

    // Second look: the case finished with nothing to report.
    //
    // Two very different situations produce that state — the change really is
    // fine, or the reviewer looked in the wrong place — and the pipeline
    // cannot tell them apart from the outside. The second is the expensive
    // one, so it looks once more before going quiet.
    //
    // This is the only path where falsification output feeds back into
    // generation instead of only filtering it: the second pass is shown each
    // rejected claim together with the repository facts that closed it, and
    // told to look elsewhere. Anything it proposes re-enters the full
    // pipeline. Nothing is reported because a second look suggested it.
    let nothing_to_report = !findings.iter().any(|f| f.status.is_reported());
    if nothing_to_report
        && cfg.ablation == Ablation::None
        && cfg.max_second_looks > 0
        && !findings.is_empty()
    {
        traj.push(TrajectoryEvent::Note {
            note: format!(
                "second look: {} candidate(s) were investigated and none survived                  adjudication, so the change is being read again against the questions                  already settled",
                findings.len()
            ),
        });

        let again = propose_again(case, &findings, client, cfg, &mut traj).await;
        if again.is_empty() {
            traj.push(TrajectoryEvent::Note {
                note: "second look: nothing further proposed; the silence stands".to_string(),
            });
        } else {
            let (again, merged) = deduplicate_candidates(again);
            for (dropped, kept) in &merged {
                traj.push(TrajectoryEvent::Note {
                    note: format!("second look: candidate {dropped} merged into {kept}"),
                });
            }
            for candidate in again {
                let f = adjudicate(case, candidate, client, cfg, &mut traj, &mut memory).await;
                findings.push(f);
            }
        }
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

/// Take one candidate from proposal to final status.
///
/// Falsification question, investigation, fresh-context verification, then the
/// evidence gate. Pulled out of the main loop so a second look can run
/// candidates through exactly the same path — a finding that reached a report
/// by a shorter route would not mean the same thing.
async fn adjudicate(
    case: &Case,
    candidate: CandidateFinding,
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
    memory: &mut CaseMemory,
) -> Finding {
    // Candidates-only: no investigation, no verification. Reported as
    // produced, which is what the advanced prompt alone is worth.
    if cfg.ablation == Ablation::CandidatesOnly {
        traj.push(TrajectoryEvent::Decision {
            candidate_id: candidate.id.clone(),
            status: FindingStatus::Verified,
            reason: "ablation candidates-only: reported without investigation or                          verification"
                .to_string(),
        });
        return Finding {
            candidate,
            falsification_question: String::new(),
            evidence: Vec::new(),
            verification: None,
            status: FindingStatus::Verified,
            status_reason: "ablation candidates-only".to_string(),
        };
    }

    let question = falsification_question(&candidate, client, cfg, traj).await;
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
        traj,
        None,
        "",
        first_budget,
        memory,
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
        return Finding {
            candidate,
            falsification_question: question,
            evidence,
            verification: None,
            status,
            status_reason: reason,
        };
    }

    let mut verification = verify_fresh(&candidate, &question, &evidence, client, cfg, traj).await;

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

    if needs_more && cfg.max_followup_investigations > 0 && cfg.ablation != Ablation::NoFollowup {
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
            traj,
            Some(&gap),
            "f",
            follow_up_budget,
            memory,
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
            verification = verify_fresh(&candidate, &question, &evidence, client, cfg, traj).await;

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

    Finding {
        candidate,
        falsification_question: question,
        evidence,
        verification,
        status,
        status_reason: reason,
    }
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

/// Render the questions a case has already settled, for the second look.
///
/// Each closed claim is paired with the reason it closed. The reason is the
/// verifier's rationale where there is one — an argument from repository text,
/// not an opinion — and the gate's reason otherwise. Without the reasons this
/// would just be a list of things not to say, and the model would have no way
/// to tell a closed question from an unexplored one.
fn settled_questions(findings: &[Finding]) -> String {
    findings
        .iter()
        .filter(|f| !f.status.is_reported())
        .map(|f| {
            let reason = f
                .verification
                .as_ref()
                .map(|v| v.rationale.trim())
                .filter(|r| !r.is_empty())
                .unwrap_or(f.status_reason.trim());
            format!(
                "- **{}** at `{}`
  Claim: {}
  Ruled out because: {}",
                f.candidate.issue_type.as_str(),
                f.candidate.location,
                f.candidate.claim.trim(),
                reason
            )
        })
        .collect::<Vec<_>>()
        .join(
            "

",
        )
}

/// Propose again, after every first-pass candidate was ruled out.
///
/// Anything that merely restates a closed claim is dropped here rather than
/// investigated: the prompt asks for a different question, and a second pass
/// that re-argues a settled one would spend the budget proving the same point
/// twice. The check is the same strict overlap deduplication uses.
async fn propose_again(
    case: &Case,
    settled: &[Finding],
    client: &LlmClient,
    cfg: &RunConfig,
    traj: &mut Trajectory,
) -> Vec<CandidateFinding> {
    let system = prompts::advanced_system(case.manifest.language.as_str());
    let user = prompts::second_look_user(
        &case.manifest.description,
        &case.diff,
        &case_file_context(case, FILE_CONTEXT_LINES),
        &settled_questions(settled),
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
                prompts::SECOND_LOOK_V,
                &e.to_string(),
                cfg.llm.max_retries + 1,
            );
            return Vec::new();
        }
    };

    traj.record_call(Stage::Review, prompts::SECOND_LOOK_V, &system, &user, &resp);

    let Ok(value) = extract_json(&resp.text) else {
        traj.push(TrajectoryEvent::Note {
            note: "second look: unparseable response".to_string(),
        });
        return Vec::new();
    };

    let parsed = parse_review(&value, &format!("{}-adv2", case.id()));
    for w in &parsed.warnings {
        traj.push(TrajectoryEvent::Note { note: w.clone() });
    }

    let mut fresh = Vec::new();
    for c in parsed.candidates {
        let restates = settled.iter().any(|f| {
            f.candidate.issue_type == c.issue_type && f.candidate.location.overlaps(&c.location, 0)
        });
        if restates {
            traj.push(TrajectoryEvent::Note {
                note: format!(
                    "second look: {} restates a question already settled at {}; dropped",
                    c.id, c.location
                ),
            });
            continue;
        }
        traj.push(TrajectoryEvent::CandidateProposed {
            candidate: c.clone(),
        });
        fresh.push(c);
    }
    fresh
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

/// Collapse candidates that describe the same defect.
///
/// The reviewer is told to propose broadly, and it sometimes proposes the same
/// problem twice — once at the guard and once at the call it guards, or the
/// same check under two adjacent line ranges. Both then get investigated
/// separately, both get reported, and the evaluator scores one true positive
/// and one false positive, because telling a reviewer the same thing twice
/// still costs a second triage.
///
/// The rule is deliberately conservative: same file, same `issue_type`, and
/// line ranges that **genuinely overlap**. Two distinct defects that happen to
/// sit near each other are left alone, and so are same-category defects under
/// different categories. Under-merging costs a false positive; over-merging
/// hides a real defect, which is worse.
///
/// The survivor is the most specific claim — the narrowest line range — since
/// that is the one a human can act on with least searching.
///
/// # Why the overlap is strict
///
/// This function originally reused `cfg.match_line_tolerance`, the evaluator's
/// ±3 slack. That was a category error, and it took a replay over every
/// archived run to see it. The tolerance exists to forgive an off-by-a-line in
/// a location *estimate* while scoring. Deciding that two claims are the same
/// claim is a different question, and it must not borrow that slack.
///
/// Replayed across all 19 archived runs, the tolerant rule fires 5 times and
/// **not one of those firings is a duplicate**. Every one is this pair, in
/// `c08-order-name-limit`:
///
/// ```text
/// Validation  src/order.rs:26-28   order.name  checked against MAX_QUANTITY
/// Validation  src/order.rs:30-32   order.notes checked against MAX_NAME_LEN
/// ```
///
/// Two different fields, two different defects, both in the ground truth. They
/// are joined only because 28 + 3 >= 30. Merging them would have converted two
/// true positives into one true positive and one false negative.
///
/// So the feature was not merely inert. Every firing it would ever have had
/// was wrong, and it escaped notice because a later change to candidate
/// generation stopped producing that pair. See `vcr replay-dedup`.
fn deduplicate_candidates(
    candidates: Vec<CandidateFinding>,
) -> (Vec<CandidateFinding>, Vec<(String, String)>) {
    let mut kept: Vec<CandidateFinding> = Vec::new();
    let mut merged: Vec<(String, String)> = Vec::new();

    for candidate in candidates {
        let duplicate_of = kept.iter_mut().find(|k| {
            k.issue_type == candidate.issue_type
                // Tolerance 0: ranges must actually intersect.
                && k.location.overlaps(&candidate.location, 0)
        });

        match duplicate_of {
            None => kept.push(candidate),
            Some(existing) => {
                let existing_span = existing.location.end_line - existing.location.start_line;
                let candidate_span = candidate.location.end_line - candidate.location.start_line;

                if candidate_span < existing_span {
                    // The newcomer is more specific; it survives instead.
                    merged.push((existing.id.clone(), candidate.id.clone()));
                    *existing = candidate;
                } else {
                    merged.push((candidate.id.clone(), existing.id.clone()));
                }
            }
        }
    }

    (kept, merged)
}

/// Repository facts gathered earlier in the same review.
///
/// Carries **what was looked at and what was found** — never a verdict, never
/// a conclusion about whether some earlier claim held. Passing judgements
/// forward would quietly reintroduce the anchor that the fresh-context
/// verifier exists to remove; passing facts forward just stops the second
/// candidate re-reading the file the first one already opened.
#[derive(Default)]
struct CaseMemory {
    entries: Vec<String>,
}

impl CaseMemory {
    fn record(&mut self, tool: &str, arguments: &serde_json::Value, response: &str, ok: bool) {
        if !ok {
            return;
        }
        // One line per lookup: enough to recognise a repeat, not enough to
        // replace actually reading the file.
        let summary = response.lines().next().unwrap_or("").trim().to_string();
        let entry = format!("{tool} {arguments} -> {summary}");
        if !self.entries.contains(&entry) {
            self.entries.push(entry);
        }
    }

    fn render(&self) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        Some(self.entries.join(
            "
",
        ))
    }
}

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
    memory: &mut CaseMemory,
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
            memory.render().as_deref(),
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

        memory.record(&call.tool, &call.arguments, &result.text, result.ok);

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

    // --- candidate deduplication ---

    fn cand(id: &str, ty: IssueType, file: &str, start: u32, end: u32) -> CandidateFinding {
        CandidateFinding {
            id: id.into(),
            issue_type: ty,
            severity: Severity::Medium,
            location: Location::new(file, start, end),
            claim: format!("claim {id}"),
            reasoning: String::new(),
        }
    }

    #[test]
    fn overlapping_same_category_candidates_are_merged() {
        // A real duplicate: the same claim stated over two ranges that
        // genuinely intersect.
        let (kept, merged) = deduplicate_candidates(vec![
            cand("a", IssueType::Validation, "src/order.rs", 26, 30),
            cand("b", IssueType::Validation, "src/order.rs", 28, 32),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn adjacent_but_non_overlapping_candidates_are_never_merged() {
        // This is the literal geometry from `c08-order-name-limit`, and it is
        // the only shape the tolerant version of this rule ever fired on --
        // 5 times across 19 archived runs, wrongly every time.
        //
        //   src/order.rs:26-28   order.name  checked against MAX_QUANTITY
        //   src/order.rs:30-32   order.notes checked against MAX_NAME_LEN
        //
        // Two fields, two defects, both in the ground truth. The earlier
        // version of this test asserted these SHOULD merge, which is how the
        // bug survived: the test encoded the defect it was meant to catch.
        let (kept, merged) = deduplicate_candidates(vec![
            cand("name", IssueType::Validation, "src/order.rs", 26, 28),
            cand("notes", IssueType::Validation, "src/order.rs", 30, 32),
        ]);
        assert_eq!(
            kept.len(),
            2,
            "merging these turns two true positives into one true positive              and one false negative"
        );
        assert!(merged.is_empty());
    }

    #[test]
    fn candidates_touching_at_a_single_line_are_merged() {
        // 20-25 and 25-30 share line 25, so they are talking about the same
        // code. This is the boundary the strict rule draws.
        let (kept, _) = deduplicate_candidates(vec![
            cand("a", IssueType::Correctness, "a.rs", 20, 25),
            cand("b", IssueType::Correctness, "a.rs", 25, 30),
        ]);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn candidates_one_line_apart_are_kept_apart() {
        // 20-24 and 25-30 share nothing. One line of slack would have merged
        // them; that slack belongs to the evaluator, not to this function.
        let (kept, _) = deduplicate_candidates(vec![
            cand("a", IssueType::Correctness, "a.rs", 20, 24),
            cand("b", IssueType::Correctness, "a.rs", 25, 30),
        ]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn the_most_specific_claim_survives_a_merge() {
        let (kept, _) = deduplicate_candidates(vec![
            cand("wide", IssueType::Correctness, "a.rs", 10, 40),
            cand("tight", IssueType::Correctness, "a.rs", 20, 22),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "tight", "the narrower range is more actionable");
    }

    #[test]
    fn different_categories_are_never_merged() {
        // Two genuinely distinct defects can sit on the same lines. Merging
        // them would hide one, which is worse than a duplicate report.
        let (kept, merged) = deduplicate_candidates(vec![
            cand("a", IssueType::Validation, "a.rs", 10, 12),
            cand("b", IssueType::Concurrency, "a.rs", 10, 12),
        ]);
        assert_eq!(kept.len(), 2);
        assert!(merged.is_empty());
    }

    #[test]
    fn distant_same_category_candidates_are_kept_apart() {
        let (kept, _) = deduplicate_candidates(vec![
            cand("a", IssueType::Testing, "a.rs", 10, 12),
            cand("b", IssueType::Testing, "a.rs", 200, 202),
        ]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn candidates_in_different_files_are_kept_apart() {
        let (kept, _) = deduplicate_candidates(vec![
            cand("a", IssueType::Testing, "a.rs", 10, 12),
            cand("b", IssueType::Testing, "b.rs", 10, 12),
        ]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn deduplication_preserves_a_single_candidate() {
        let (kept, merged) =
            deduplicate_candidates(vec![cand("a", IssueType::Testing, "a.rs", 1, 1)]);
        assert_eq!(kept.len(), 1);
        assert!(merged.is_empty());
    }

    // --- within-case memory ---

    #[test]
    fn memory_records_successful_lookups_only() {
        let mut m = CaseMemory::default();
        m.record(
            "search",
            &serde_json::json!({"pattern": "x"}),
            "2 matches
foo",
            true,
        );
        m.record(
            "read",
            &serde_json::json!({"file": "missing.rs"}),
            "could not read",
            false,
        );
        let rendered = m.render().unwrap();
        assert!(rendered.contains("search"));
        assert!(
            !rendered.contains("could not read"),
            "refusals are not facts"
        );
    }

    #[test]
    fn memory_does_not_repeat_an_identical_lookup() {
        let mut m = CaseMemory::default();
        for _ in 0..3 {
            m.record(
                "search",
                &serde_json::json!({"pattern": "x"}),
                "2 matches",
                true,
            );
        }
        assert_eq!(m.render().unwrap().lines().count(), 1);
    }

    #[test]
    fn memory_is_empty_until_something_is_looked_up() {
        assert!(CaseMemory::default().render().is_none());
    }

    #[test]
    fn memory_carries_lookups_not_verdicts() {
        // The whole point: facts may cross between candidates, conclusions may
        // not, or the fresh verifier stops being fresh.
        let mut m = CaseMemory::default();
        m.record(
            "read",
            &serde_json::json!({"file": "src/router.rs"}),
            "src/router.rs lines 1-40 of 80:",
            true,
        );
        let rendered = m.render().unwrap();
        for verdict in [
            "Supports",
            "Contradicts",
            "Insufficient",
            "Verified",
            "Rejected",
        ] {
            assert!(
                !rendered.contains(verdict),
                "memory leaked a verdict: {verdict}"
            );
        }
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

    // --- second look: a case that finished with nothing to report ---
    //
    // The follow-up loop taught this project that "never fired" and "is
    // broken" produce identical evidence, so this branch is driven with a
    // scripted model before any claim is made about what it does in a real
    // run.

    /// A second-look reply proposing something at a different place.
    fn second_candidate() -> String {
        r#"{"findings":[{"issue_type":"ResourceManagement","severity":"High",
             "file":"src/lib.rs","start_line":2,"end_line":2,
             "claim":"caller leaks the handle","reasoning":"r2"}]}"#
            .to_string()
    }

    /// A second-look reply that restates the first candidate verbatim.
    fn restated_candidate() -> String {
        r#"{"findings":[{"issue_type":"Correctness","severity":"High",
             "file":"src/lib.rs","start_line":1,"end_line":1,
             "claim":"a() can be reached with bad state, restated","reasoning":"r"}]}"#
            .to_string()
    }

    fn second_look_client(reviews: Vec<String>, verify: Vec<&str>, inv_turns: usize) -> LlmClient {
        let mut script = std::collections::HashMap::new();
        script.insert(Stage::Review, reviews);
        script.insert(
            Stage::Falsify,
            vec![
                r#"{"falsification_question":"do callers guard it?"}"#.to_string(),
                r#"{"falsification_question":"is the handle released?"}"#.to_string(),
            ],
        );
        let mut inv = Vec::new();
        for _ in 0..inv_turns {
            inv.extend(read_then_done());
        }
        script.insert(Stage::Investigate, inv);
        script.insert(
            Stage::Verify,
            verify.into_iter().map(|s| s.to_string()).collect(),
        );
        LlmClient::Mock(crate::llm::MockClient::scripted(script))
    }

    const CONTRADICTS: &str = r#"{"outcome":"Contradicts","rationale":"the caller guards it","decisive_evidence":["src/lib.rs:2"]}"#;
    const SUPPORTS: &str = r#"{"outcome":"Supports","rationale":"nothing releases it","decisive_evidence":["src/lib.rs:2"]}"#;

    #[tokio::test]
    async fn a_case_that_reports_nothing_is_looked_at_again() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let cfg = RunConfig::mock();
        assert_eq!(cfg.max_second_looks, 1);

        let client = second_look_client(
            vec![one_candidate(), second_candidate()],
            vec![CONTRADICTS, SUPPORTS],
            2,
        );

        let t = run(&case, &client, &cfg).await.unwrap();

        assert!(
            t.events.iter().any(|e| matches!(
                e,
                TrajectoryEvent::Note { note } if note.starts_with("second look:")
            )),
            "the second look must be visible in the trajectory"
        );
        assert_eq!(t.final_findings.len(), 2, "both candidates are adjudicated");
        assert_eq!(t.final_findings[0].status, FindingStatus::Rejected);
        assert_eq!(
            t.final_findings[1].status,
            FindingStatus::Verified,
            "a second-look candidate still has to pass verification and the evidence gate"
        );
    }

    #[tokio::test]
    async fn a_second_look_that_restates_a_settled_question_is_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let cfg = RunConfig::mock();

        let client = second_look_client(
            vec![one_candidate(), restated_candidate()],
            vec![CONTRADICTS],
            1,
        );

        let t = run(&case, &client, &cfg).await.unwrap();

        assert!(
            t.events.iter().any(|e| matches!(
                e,
                TrajectoryEvent::Note { note } if note.contains("restates a question already settled")
            )),
            "re-argued claims are dropped before they cost an investigation"
        );
        assert_eq!(t.final_findings.len(), 1);
    }

    #[tokio::test]
    async fn no_second_look_when_something_was_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let cfg = RunConfig::mock();

        // Only one Review reply is scripted: a second call would exhaust it.
        let client = second_look_client(vec![one_candidate()], vec![SUPPORTS], 1);

        let t = run(&case, &client, &cfg).await.unwrap();

        assert_eq!(t.final_findings.len(), 1);
        assert_eq!(t.final_findings[0].status, FindingStatus::Verified);
        assert!(
            !t.events.iter().any(|e| matches!(
                e,
                TrajectoryEvent::Note { note } if note.starts_with("second look:")
            )),
            "the trigger is silence, not failure"
        );
    }

    #[tokio::test]
    async fn the_second_look_is_disabled_by_its_ablation() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let mut cfg = RunConfig::mock();
        cfg.ablation = Ablation::NoSecondLook;

        let client = second_look_client(vec![one_candidate()], vec![CONTRADICTS], 1);

        let t = run(&case, &client, &cfg).await.unwrap();

        assert_eq!(t.final_findings.len(), 1);
        assert!(!t.events.iter().any(|e| matches!(
            e,
            TrajectoryEvent::Note { note } if note.starts_with("second look:")
        )));
    }

    #[tokio::test]
    async fn the_second_look_is_disabled_when_its_budget_is_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let case = loop_case(tmp.path());
        let mut cfg = RunConfig::mock();
        cfg.max_second_looks = 0;

        let client = second_look_client(vec![one_candidate()], vec![CONTRADICTS], 1);

        let t = run(&case, &client, &cfg).await.unwrap();
        assert_eq!(t.final_findings.len(), 1);
    }

    #[test]
    fn settled_questions_carry_the_reason_a_claim_was_ruled_out() {
        let f = Finding {
            candidate: candidate("src/lib.rs"),
            falsification_question: "q".into(),
            evidence: vec![],
            verification: Some(VerificationResult {
                outcome: VerificationOutcome::Contradicts,
                rationale: "the constructor rejects an empty list".into(),
                decisive_evidence: vec![],
            }),
            status: FindingStatus::Rejected,
            status_reason: "gate said so".into(),
        };
        let rendered = settled_questions(&[f]);
        assert!(
            rendered.contains("the constructor rejects an empty list"),
            "the second look needs the repository fact, not the gate's summary"
        );
        assert!(rendered.contains("src/lib.rs"));
    }

    #[test]
    fn a_reported_finding_is_not_offered_back_as_settled() {
        let mut f = Finding {
            candidate: candidate("src/lib.rs"),
            falsification_question: "q".into(),
            evidence: vec![],
            verification: None,
            status: FindingStatus::Verified,
            status_reason: "ok".into(),
        };
        assert!(settled_questions(&[f.clone()]).is_empty());
        f.status = FindingStatus::Rejected;
        assert!(!settled_questions(&[f]).is_empty());
    }
}
