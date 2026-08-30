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
                p.candidate.issue_type == exp.issue_type
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
