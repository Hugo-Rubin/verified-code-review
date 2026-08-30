//! Deterministic evaluation.
//!
//! No LLM is involved in scoring. A prediction matches an expected finding
//! when its `issue_type` is identical and its location overlaps the expected
//! range within a fixed line tolerance. That is the whole rule (masterplan §6).
//!
//! Matching is one-to-one. Two predictions that both land on the same real
//! defect produce one true positive and one false positive: telling a reviewer
//! the same thing twice still costs them a second triage.

use crate::bench::{CaseCategory, GroundTruth};
use crate::finding::{Finding, Location};
use serde::{Deserialize, Serialize};

/// Core counts. Everything else is derived from these.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub true_positives: u32,
    pub false_positives: u32,
    pub false_negatives: u32,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.true_positives += other.true_positives;
        self.false_positives += other.false_positives;
        self.false_negatives += other.false_negatives;
    }

    /// Fraction of reported findings that were real.
    ///
    /// Defined as 0.0 when nothing was reported. A system that reports nothing
    /// has demonstrated no precision; scoring it 1.0 would let an empty
    /// reviewer top the table.
    pub fn precision(&self) -> f64 {
        let denom = self.true_positives + self.false_positives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    /// Fraction of real defects that were reported.
    ///
    /// Defined as 0.0 when there was nothing to find, so that clean-only
    /// slices do not inflate the aggregate.
    pub fn recall(&self) -> f64 {
        let denom = self.true_positives + self.false_negatives;
        if denom == 0 {
            return 0.0;
        }
        self.true_positives as f64 / denom as f64
    }

    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            return 0.0;
        }
        2.0 * p * r / (p + r)
    }
}

/// One prediction paired with the expected finding it satisfied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchedPair {
    pub prediction_id: String,
    pub expected_id: String,
    pub location: String,
    pub issue_type: String,
}

/// A reported finding that matched nothing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmatchedPrediction {
    pub prediction_id: String,
    pub location: String,
    pub issue_type: String,
    pub claim: String,
}

/// An expected finding nothing reported.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissedFinding {
    pub expected_id: String,
    pub location: String,
    pub issue_type: String,
    pub description: String,
}

/// Result of scoring one case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseEvaluation {
    pub case_id: String,
    pub category: CaseCategory,
    pub counts: Counts,
    pub matched: Vec<MatchedPair>,
    pub unmatched_predictions: Vec<UnmatchedPrediction>,
    pub missed: Vec<MissedFinding>,
    /// Findings the system produced but did not report (Rejected/Uncertain).
    /// Excluded from scoring; kept because a cleared finding is evidence that
    /// falsification did work.
    pub withheld: u32,
}

/// Score one case.
///
/// Only `Verified` findings are treated as predictions. `Rejected` and
/// `Uncertain` findings are counted in `withheld` and do not affect the score
/// — that is precisely the behaviour the advanced system is meant to buy.
pub fn evaluate_case(
    case_id: &str,
    category: CaseCategory,
    findings: &[Finding],
    truth: &GroundTruth,
    line_tolerance: u32,
) -> CaseEvaluation {
    let withheld = findings.iter().filter(|f| !f.status.is_reported()).count() as u32;

    // Deterministic prediction order: location, then issue type, then id.
    let mut predictions: Vec<&Finding> =
        findings.iter().filter(|f| f.status.is_reported()).collect();
    predictions.sort_by(|a, b| {
        (
            &a.candidate.location.file,
            a.candidate.location.start_line,
            a.candidate.issue_type.as_str(),
            &a.candidate.id,
        )
            .cmp(&(
                &b.candidate.location.file,
                b.candidate.location.start_line,
                b.candidate.issue_type.as_str(),
                &b.candidate.id,
            ))
    });

    let mut expected: Vec<&crate::bench::ExpectedFinding> =
        truth.expected_findings.iter().collect();
    expected.sort_by(|a, b| a.id.cmp(&b.id));

    let mut prediction_used = vec![false; predictions.len()];
    let mut matched = Vec::new();
    let mut missed = Vec::new();

    for exp in &expected {
        let exp_loc = exp.location();

        // Among unused predictions that qualify, take the closest. Distance is
        // the gap between range midpoints, so a tightly-scoped prediction wins
        // over a sprawling one that happens to overlap.
        let best = predictions
            .iter()
            .enumerate()
            .filter(|(i, _)| !prediction_used[*i])
            .filter(|(_, p)| {
                exp.accepts_type(p.candidate.issue_type)
                    && p.candidate.location.overlaps(&exp_loc, line_tolerance)
            })
            .min_by_key(|(_, p)| midpoint_distance(&p.candidate.location, &exp_loc));

        match best {
            Some((i, p)) => {
                prediction_used[i] = true;
                matched.push(MatchedPair {
                    prediction_id: p.candidate.id.clone(),
                    expected_id: exp.id.clone(),
                    location: p.candidate.location.to_string(),
                    issue_type: exp.issue_type.to_string(),
                });
            }
            None => missed.push(MissedFinding {
                expected_id: exp.id.clone(),
                location: exp_loc.to_string(),
                issue_type: exp.issue_type.to_string(),
                description: exp.description.clone(),
            }),
        }
    }

    let unmatched_predictions: Vec<UnmatchedPrediction> = predictions
        .iter()
        .enumerate()
        .filter(|(i, _)| !prediction_used[*i])
        .map(|(_, p)| UnmatchedPrediction {
            prediction_id: p.candidate.id.clone(),
            location: p.candidate.location.to_string(),
            issue_type: p.candidate.issue_type.to_string(),
            claim: p.candidate.claim.clone(),
        })
        .collect();

    CaseEvaluation {
        case_id: case_id.to_string(),
        category,
        counts: Counts {
            true_positives: matched.len() as u32,
            false_positives: unmatched_predictions.len() as u32,
            false_negatives: missed.len() as u32,
        },
        matched,
        unmatched_predictions,
        missed,
        withheld,
    }
}

/// Result of checking recorded evidence against the repository it came from.
///
/// The system claims every evidence excerpt is verbatim repository content at
/// a cited location. That claim is itself checkable, so it is checked: a
/// reviewer trusting a citation should not have to take our word that the
/// citation is real.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceAudit {
    /// Evidence items recorded across all findings.
    pub total: u32,
    /// Items carrying a file, a line range, and a non-empty excerpt, and so
    /// checkable against the repository at all.
    pub checkable: u32,
    /// Checkable items whose excerpt matches the file at the cited lines.
    pub accurate: u32,
    /// Human-readable description of each mismatch.
    pub mismatches: Vec<String>,
}

impl EvidenceAudit {
    /// Fraction of checkable evidence that really is where it says it is.
    ///
    /// Defined as 1.0 when there was nothing checkable: a run that gathered no
    /// evidence has not misquoted anything. Read it alongside `checkable`.
    pub fn accuracy(&self) -> f64 {
        if self.checkable == 0 {
            return 1.0;
        }
        self.accurate as f64 / self.checkable as f64
    }

    pub fn merge(&mut self, other: &EvidenceAudit) {
        self.total += other.total;
        self.checkable += other.checkable;
        self.accurate += other.accurate;
        self.mismatches.extend(other.mismatches.iter().cloned());
    }
}

/// Strip the `"  123 | "` gutter that bounded reads prepend, leaving the
/// original source line.
fn strip_line_gutter(line: &str) -> &str {
    let trimmed = line.trim_start();
    let Some(bar) = trimmed.find('|') else {
        return line;
    };
    if trimmed[..bar].trim().chars().all(|c| c.is_ascii_digit()) && bar > 0 {
        // The gutter is `<spaces><digits> | `; one space follows the bar.
        trimmed[bar + 1..]
            .strip_prefix(' ')
            .unwrap_or(&trimmed[bar + 1..])
    } else {
        line
    }
}

/// Check every evidence item in `findings` against the repository.
pub fn audit_evidence(repo: &crate::repo::RepoRoot, findings: &[Finding]) -> EvidenceAudit {
    let mut audit = EvidenceAudit::default();

    for finding in findings {
        for ev in &finding.evidence {
            audit.total += 1;

            let (Some(file), Some(start)) = (ev.file.as_ref(), ev.start_line) else {
                continue;
            };
            if ev.excerpt.trim().is_empty() {
                continue;
            }

            let Ok(content) = repo.read_to_string(file) else {
                audit.checkable += 1;
                audit.mismatches.push(format!(
                    "{}: cites {file} which cannot be read",
                    finding.candidate.id
                ));
                continue;
            };

            audit.checkable += 1;
            let file_lines: Vec<&str> = content.lines().collect();

            // Every excerpt line must appear at its stated position. Search
            // evidence is a single line; a bounded read is a contiguous block
            // starting at `start_line`.
            let excerpt_lines: Vec<&str> = ev.excerpt.lines().collect();
            let mut ok = true;
            let mut detail = String::new();

            for (offset, raw) in excerpt_lines.iter().enumerate() {
                let expected_line_no = start as usize + offset;
                let quoted = strip_line_gutter(raw).trim_end();

                match file_lines.get(expected_line_no.saturating_sub(1)) {
                    Some(actual) if actual.trim_end() == quoted => {}
                    Some(actual) => {
                        ok = false;
                        detail = format!(
                            "line {expected_line_no} reads {:?} but evidence quotes {:?}",
                            actual.trim(),
                            quoted.trim()
                        );
                        break;
                    }
                    None => {
                        ok = false;
                        detail = format!("line {expected_line_no} is past the end of {file}");
                        break;
                    }
                }
            }

            if ok {
                audit.accurate += 1;
            } else {
                audit.mismatches.push(format!(
                    "{} @ {file}:{start}: {detail}",
                    finding.candidate.id
                ));
            }
        }
    }

    audit
}

/// Distance between the midpoints of two line ranges, doubled to stay in
/// integers.
fn midpoint_distance(a: &Location, b: &Location) -> u32 {
    let ma = a.start_line + a.end_line;
    let mb = b.start_line + b.end_line;
    ma.abs_diff(mb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::ExpectedFinding;
    use crate::finding::{CandidateFinding, FindingStatus, IssueType, Severity};

    fn pred(id: &str, ty: IssueType, file: &str, s: u32, e: u32, status: FindingStatus) -> Finding {
        Finding {
            candidate: CandidateFinding {
                id: id.to_string(),
                issue_type: ty,
                severity: Severity::Medium,
                location: Location::new(file, s, e),
                claim: format!("claim {id}"),
                reasoning: String::new(),
            },
            falsification_question: String::new(),
            evidence: Vec::new(),
            verification: None,
            status,
            status_reason: String::new(),
        }
    }

    fn expect(id: &str, ty: IssueType, file: &str, s: u32, e: u32) -> ExpectedFinding {
        ExpectedFinding {
            id: id.to_string(),
            issue_type: ty,
            also_accept: Vec::new(),
            file: file.to_string(),
            start_line: s,
            end_line: e,
            description: format!("expected {id}"),
        }
    }

    fn truth(findings: Vec<ExpectedFinding>) -> GroundTruth {
        GroundTruth {
            case_id: "c".into(),
            expected_findings: findings,
            notes: String::new(),
        }
    }

    fn eval(preds: &[Finding], gt: &GroundTruth) -> CaseEvaluation {
        evaluate_case("c", CaseCategory::RealIssue, preds, gt, 3)
    }

    // --- counts and derived metrics ---

    #[test]
    fn precision_recall_f1_on_a_known_confusion() {
        let c = Counts {
            true_positives: 3,
            false_positives: 1,
            false_negatives: 1,
        };
        assert!((c.precision() - 0.75).abs() < 1e-12);
        assert!((c.recall() - 0.75).abs() < 1e-12);
        assert!((c.f1() - 0.75).abs() < 1e-12);
    }

    #[test]
    fn f1_is_harmonic_not_arithmetic() {
        // P = 1.0, R = 0.5. Arithmetic mean would be 0.75; F1 is 0.666...
        let c = Counts {
            true_positives: 1,
            false_positives: 0,
            false_negatives: 1,
        };
        assert!((c.f1() - 2.0 / 3.0).abs() < 1e-12, "got {}", c.f1());
    }

    #[test]
    fn empty_reviewer_scores_zero_not_one() {
        let c = Counts {
            true_positives: 0,
            false_positives: 0,
            false_negatives: 4,
        };
        assert_eq!(c.precision(), 0.0);
        assert_eq!(c.recall(), 0.0);
        assert_eq!(c.f1(), 0.0);
    }

    #[test]
    fn all_false_positives_scores_zero() {
        let c = Counts {
            true_positives: 0,
            false_positives: 7,
            false_negatives: 0,
        };
        assert_eq!(c.precision(), 0.0);
        assert_eq!(c.f1(), 0.0);
    }

    #[test]
    fn perfect_run_scores_one() {
        let c = Counts {
            true_positives: 5,
            false_positives: 0,
            false_negatives: 0,
        };
        assert_eq!(c.precision(), 1.0);
        assert_eq!(c.recall(), 1.0);
        assert_eq!(c.f1(), 1.0);
    }

    #[test]
    fn counts_accumulate() {
        let mut a = Counts {
            true_positives: 1,
            false_positives: 2,
            false_negatives: 3,
        };
        a.add(&Counts {
            true_positives: 10,
            false_positives: 20,
            false_negatives: 30,
        });
        assert_eq!(a.true_positives, 11);
        assert_eq!(a.false_positives, 22);
        assert_eq!(a.false_negatives, 33);
    }

    // --- matching ---

    #[test]
    fn exact_match_is_a_true_positive() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::ErrorHandling,
            "src/a.rs",
            10,
            12,
        )]);
        let preds = vec![pred(
            "p1",
            IssueType::ErrorHandling,
            "src/a.rs",
            10,
            12,
            FindingStatus::Verified,
        )];
        let e = eval(&preds, &gt);
        assert_eq!(e.counts.true_positives, 1);
        assert_eq!(e.counts.false_positives, 0);
        assert_eq!(e.counts.false_negatives, 0);
        assert_eq!(e.matched[0].expected_id, "g1");
    }

    #[test]
    fn right_location_wrong_issue_type_does_not_match() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::ErrorHandling,
            "src/a.rs",
            10,
            12,
        )]);
        let preds = vec![pred(
            "p1",
            IssueType::Performance,
            "src/a.rs",
            10,
            12,
            FindingStatus::Verified,
        )];
        let e = eval(&preds, &gt);
        assert_eq!(e.counts.true_positives, 0);
        assert_eq!(e.counts.false_positives, 1);
        assert_eq!(e.counts.false_negatives, 1);
    }

    #[test]
    fn an_also_accepted_issue_type_matches() {
        let mut e = expect("g1", IssueType::ResourceManagement, "src/a.rs", 10, 12);
        e.also_accept = vec![IssueType::StateManagement];
        let gt = truth(vec![e]);
        let preds = vec![pred(
            "p1",
            IssueType::StateManagement,
            "src/a.rs",
            10,
            12,
            FindingStatus::Verified,
        )];
        let r = eval(&preds, &gt);
        assert_eq!(r.counts.true_positives, 1);
        assert_eq!(r.counts.false_positives, 0);
    }

    #[test]
    fn also_accept_does_not_admit_an_unlisted_type() {
        let mut e = expect("g1", IssueType::ResourceManagement, "src/a.rs", 10, 12);
        e.also_accept = vec![IssueType::StateManagement];
        let gt = truth(vec![e]);
        let preds = vec![pred(
            "p1",
            IssueType::Performance,
            "src/a.rs",
            10,
            12,
            FindingStatus::Verified,
        )];
        assert_eq!(eval(&preds, &gt).counts.true_positives, 0);
    }

    #[test]
    fn also_accept_still_requires_location_overlap() {
        // The concession is on the category axis only. It must not let a
        // finding somewhere else in the file count as the same defect.
        let mut e = expect("g1", IssueType::ResourceManagement, "src/a.rs", 10, 12);
        e.also_accept = vec![IssueType::StateManagement];
        let gt = truth(vec![e]);
        let preds = vec![pred(
            "p1",
            IssueType::StateManagement,
            "src/a.rs",
            300,
            310,
            FindingStatus::Verified,
        )];
        assert_eq!(eval(&preds, &gt).counts.true_positives, 0);
        assert_eq!(eval(&preds, &gt).counts.false_positives, 1);
    }

    #[test]
    fn right_issue_type_wrong_file_does_not_match() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            12,
        )]);
        let preds = vec![pred(
            "p1",
            IssueType::Correctness,
            "src/b.rs",
            10,
            12,
            FindingStatus::Verified,
        )];
        assert_eq!(eval(&preds, &gt).counts.true_positives, 0);
    }

    #[test]
    fn match_is_allowed_within_tolerance_and_refused_beyond_it() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
        )]);
        let near = vec![pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            13,
            13,
            FindingStatus::Verified,
        )];
        let far = vec![pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            14,
            14,
            FindingStatus::Verified,
        )];
        assert_eq!(eval(&near, &gt).counts.true_positives, 1);
        assert_eq!(eval(&far, &gt).counts.true_positives, 0);
    }

    #[test]
    fn duplicate_predictions_yield_one_tp_and_one_fp() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
        )]);
        let preds = vec![
            pred(
                "p1",
                IssueType::Correctness,
                "src/a.rs",
                10,
                10,
                FindingStatus::Verified,
            ),
            pred(
                "p2",
                IssueType::Correctness,
                "src/a.rs",
                10,
                10,
                FindingStatus::Verified,
            ),
        ];
        let e = eval(&preds, &gt);
        assert_eq!(e.counts.true_positives, 1);
        assert_eq!(e.counts.false_positives, 1);
    }

    #[test]
    fn one_prediction_cannot_satisfy_two_expected_findings() {
        let gt = truth(vec![
            expect("g1", IssueType::Correctness, "src/a.rs", 10, 10),
            expect("g2", IssueType::Correctness, "src/a.rs", 11, 11),
        ]);
        let preds = vec![pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            11,
            FindingStatus::Verified,
        )];
        let e = eval(&preds, &gt);
        assert_eq!(e.counts.true_positives, 1);
        assert_eq!(e.counts.false_negatives, 1);
        assert_eq!(e.counts.false_positives, 0);
    }

    #[test]
    fn closest_prediction_wins_when_several_qualify() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            20,
            20,
        )]);
        let preds = vec![
            pred(
                "p_far",
                IssueType::Correctness,
                "src/a.rs",
                17,
                17,
                FindingStatus::Verified,
            ),
            pred(
                "p_near",
                IssueType::Correctness,
                "src/a.rs",
                20,
                20,
                FindingStatus::Verified,
            ),
        ];
        let e = eval(&preds, &gt);
        assert_eq!(e.matched[0].prediction_id, "p_near");
    }

    #[test]
    fn clean_case_turns_every_prediction_into_a_false_positive() {
        let gt = truth(vec![]);
        let preds = vec![
            pred(
                "p1",
                IssueType::Concurrency,
                "src/a.rs",
                5,
                5,
                FindingStatus::Verified,
            ),
            pred(
                "p2",
                IssueType::Validation,
                "src/b.rs",
                9,
                9,
                FindingStatus::Verified,
            ),
        ];
        let e = evaluate_case("c", CaseCategory::Trap, &preds, &gt, 3);
        assert_eq!(e.counts.false_positives, 2);
        assert_eq!(e.counts.true_positives, 0);
        assert_eq!(e.counts.false_negatives, 0);
    }

    #[test]
    fn clean_case_with_no_predictions_is_all_zeros() {
        let e = evaluate_case("c", CaseCategory::Trap, &[], &truth(vec![]), 3);
        assert_eq!(e.counts, Counts::default());
    }

    // --- withheld findings ---

    #[test]
    fn rejected_and_uncertain_findings_are_withheld_not_scored() {
        let gt = truth(vec![]);
        let preds = vec![
            pred(
                "p1",
                IssueType::Correctness,
                "src/a.rs",
                5,
                5,
                FindingStatus::Rejected,
            ),
            pred(
                "p2",
                IssueType::Correctness,
                "src/a.rs",
                6,
                6,
                FindingStatus::Uncertain,
            ),
        ];
        let e = evaluate_case("c", CaseCategory::Trap, &preds, &gt, 3);
        assert_eq!(
            e.counts.false_positives, 0,
            "withheld findings must not be scored"
        );
        assert_eq!(e.withheld, 2);
    }

    #[test]
    fn withholding_a_correct_finding_costs_a_false_negative() {
        // Guards against gaming: suppressing everything is not free.
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
        )]);
        let preds = vec![pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
            FindingStatus::Rejected,
        )];
        let e = eval(&preds, &gt);
        assert_eq!(e.counts.true_positives, 0);
        assert_eq!(e.counts.false_negatives, 1);
    }

    // --- evidence audit ---

    use crate::finding::{Evidence, EvidenceKind};

    fn audit_fixture() -> (tempfile::TempDir, crate::repo::RepoRoot) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/a.rs"),
            "fn one() {}\nfn two() {}\nfn three() {}\n",
        )
        .unwrap();
        let root = crate::repo::RepoRoot::new(dir.path()).unwrap();
        (dir, root)
    }

    fn with_evidence(evidence: Vec<Evidence>) -> Finding {
        let mut f = pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            1,
            1,
            FindingStatus::Verified,
        );
        f.evidence = evidence;
        f
    }

    fn ev(file: &str, start: u32, end: u32, excerpt: &str, kind: EvidenceKind) -> Evidence {
        Evidence {
            kind,
            file: Some(file.into()),
            start_line: Some(start),
            end_line: Some(end),
            symbol: None,
            excerpt: excerpt.into(),
            tool_call_id: "t1".into(),
        }
    }

    #[test]
    fn accurate_search_evidence_passes_the_audit() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/a.rs",
            2,
            2,
            "fn two() {}",
            EvidenceKind::Search,
        )]);
        let a = audit_evidence(&repo, &[f]);
        assert_eq!((a.total, a.checkable, a.accurate), (1, 1, 1));
        assert_eq!(a.accuracy(), 1.0);
    }

    #[test]
    fn evidence_quoting_the_wrong_line_is_caught() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/a.rs",
            1,
            1,
            "fn two() {}",
            EvidenceKind::Search,
        )]);
        let a = audit_evidence(&repo, &[f]);
        assert_eq!(a.accurate, 0);
        assert!(a.mismatches[0].contains("evidence quotes"));
    }

    #[test]
    fn a_read_block_with_line_gutters_is_checked_line_by_line() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/a.rs",
            1,
            3,
            "    1 | fn one() {}\n    2 | fn two() {}\n    3 | fn three() {}",
            EvidenceKind::FileRegion,
        )]);
        let a = audit_evidence(&repo, &[f]);
        assert_eq!(a.accurate, 1, "gutter-prefixed reads must be recognised");
    }

    #[test]
    fn a_read_block_starting_at_the_wrong_line_is_caught() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/a.rs",
            2,
            3,
            "    1 | fn one() {}\n    2 | fn two() {}",
            EvidenceKind::FileRegion,
        )]);
        assert_eq!(audit_evidence(&repo, &[f]).accurate, 0);
    }

    #[test]
    fn evidence_past_the_end_of_the_file_is_caught() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/a.rs",
            99,
            99,
            "fn one() {}",
            EvidenceKind::Search,
        )]);
        let a = audit_evidence(&repo, &[f]);
        assert_eq!(a.accurate, 0);
        assert!(a.mismatches[0].contains("past the end"));
    }

    #[test]
    fn evidence_citing_a_missing_file_is_counted_as_inaccurate() {
        let (_d, repo) = audit_fixture();
        let f = with_evidence(vec![ev(
            "src/gone.rs",
            1,
            1,
            "anything",
            EvidenceKind::Search,
        )]);
        let a = audit_evidence(&repo, &[f]);
        assert_eq!((a.checkable, a.accurate), (1, 0));
    }

    #[test]
    fn unlocatable_evidence_counts_as_total_but_not_checkable() {
        let (_d, repo) = audit_fixture();
        let mut e = ev("src/a.rs", 1, 1, "fn one() {}", EvidenceKind::FileList);
        e.file = None;
        let a = audit_evidence(&repo, &[with_evidence(vec![e])]);
        assert_eq!((a.total, a.checkable), (1, 0));
        assert_eq!(
            a.accuracy(),
            1.0,
            "nothing checkable means nothing misquoted"
        );
    }

    #[test]
    fn an_audit_with_no_evidence_is_vacuously_accurate() {
        let (_d, repo) = audit_fixture();
        assert_eq!(audit_evidence(&repo, &[]).accuracy(), 1.0);
    }

    #[test]
    fn audits_merge() {
        let mut a = EvidenceAudit {
            total: 2,
            checkable: 2,
            accurate: 1,
            mismatches: vec!["x".into()],
        };
        a.merge(&EvidenceAudit {
            total: 3,
            checkable: 3,
            accurate: 3,
            mismatches: vec![],
        });
        assert_eq!((a.total, a.checkable, a.accurate), (5, 5, 4));
        assert_eq!(a.mismatches.len(), 1);
    }

    // --- determinism ---

    #[test]
    fn result_is_independent_of_input_order() {
        let gt = truth(vec![
            expect("g1", IssueType::Correctness, "src/a.rs", 10, 10),
            expect("g2", IssueType::Testing, "src/b.rs", 40, 44),
        ]);
        let a = pred(
            "p1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
            FindingStatus::Verified,
        );
        let b = pred(
            "p2",
            IssueType::Testing,
            "src/b.rs",
            41,
            43,
            FindingStatus::Verified,
        );
        let c = pred(
            "p3",
            IssueType::Validation,
            "src/z.rs",
            1,
            1,
            FindingStatus::Verified,
        );

        let forward = eval(&[a.clone(), b.clone(), c.clone()], &gt);
        let reverse = eval(&[c, b, a], &gt);
        assert_eq!(forward.counts, reverse.counts);
        assert_eq!(
            forward
                .matched
                .iter()
                .map(|m| m.prediction_id.clone())
                .collect::<Vec<_>>(),
            reverse
                .matched
                .iter()
                .map(|m| m.prediction_id.clone())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn path_separator_style_does_not_affect_matching() {
        let gt = truth(vec![expect(
            "g1",
            IssueType::Correctness,
            "src/a.rs",
            10,
            10,
        )]);
        let preds = vec![pred(
            "p1",
            IssueType::Correctness,
            "src\\a.rs",
            10,
            10,
            FindingStatus::Verified,
        )];
        assert_eq!(eval(&preds, &gt).counts.true_positives, 1);
    }
}
