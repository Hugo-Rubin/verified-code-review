//! Structured finding schema.
//!
//! The same `IssueType` enum is used by predictions and by ground truth so
//! that evaluation can be deterministic (masterplan §6). Free-form semantic
//! matching is deliberately avoided.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Controlled issue taxonomy. Shared by predictions and ground truth.
///
/// Any value outside this set is rejected at parse time rather than coerced,
/// so that a model inventing a category shows up as a malformed response
/// instead of silently becoming an unmatchable prediction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IssueType {
    Correctness,
    ErrorHandling,
    Validation,
    StateManagement,
    ResourceManagement,
    Concurrency,
    ApiContract,
    Testing,
    Performance,
}

impl IssueType {
    pub const ALL: [IssueType; 9] = [
        IssueType::Correctness,
        IssueType::ErrorHandling,
        IssueType::Validation,
        IssueType::StateManagement,
        IssueType::ResourceManagement,
        IssueType::Concurrency,
        IssueType::ApiContract,
        IssueType::Testing,
        IssueType::Performance,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            IssueType::Correctness => "Correctness",
            IssueType::ErrorHandling => "ErrorHandling",
            IssueType::Validation => "Validation",
            IssueType::StateManagement => "StateManagement",
            IssueType::ResourceManagement => "ResourceManagement",
            IssueType::Concurrency => "Concurrency",
            IssueType::ApiContract => "ApiContract",
            IssueType::Testing => "Testing",
            IssueType::Performance => "Performance",
        }
    }
}

impl fmt::Display for IssueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
}

/// A location in the reviewed repository.
///
/// `file` is always a repository-relative POSIX-style path. Line numbers are
/// 1-based and inclusive on both ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
}

impl Location {
    pub fn new(file: impl Into<String>, start_line: u32, end_line: u32) -> Self {
        let (start_line, end_line) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };
        Self {
            file: normalize_path(&file.into()),
            start_line,
            end_line,
        }
    }

    /// True when the two line ranges overlap once `tolerance` lines of slack
    /// are added to each end of `self`. Files must match exactly.
    pub fn overlaps(&self, other: &Location, tolerance: u32) -> bool {
        if self.file != other.file {
            return false;
        }
        let lo = self.start_line.saturating_sub(tolerance);
        let hi = self.end_line.saturating_add(tolerance);
        lo <= other.end_line && other.start_line <= hi
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start_line == self.end_line {
            write!(f, "{}:{}", self.file, self.start_line)
        } else {
            write!(f, "{}:{}-{}", self.file, self.start_line, self.end_line)
        }
    }
}

/// Normalize a path for comparison: backslashes to forward slashes, strip a
/// leading `./`. Ground truth and predictions are compared as strings, so both
/// sides go through this.
pub fn normalize_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p).to_string();
    p.trim_start_matches('/').to_string()
}

/// What the reviewer proposes before any investigation has happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateFinding {
    pub id: String,
    pub issue_type: IssueType,
    pub severity: Severity,
    pub location: Location,
    /// One-sentence assertion about what is wrong. This is the thing that gets
    /// falsified.
    pub claim: String,
    /// Why the reviewer believes the claim.
    pub reasoning: String,
}

/// The kind of repository artifact an evidence item points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceKind {
    /// Result of a repository search.
    Search,
    /// A bounded read of a file region.
    FileRegion,
    /// A hunk of the reviewed diff.
    DiffHunk,
    /// A listing of repository paths.
    FileList,
}

/// A concrete, repository-grounded piece of evidence.
///
/// Evidence is produced by Rust tools, never by the model. The model may
/// request a tool call and interpret the result, but it cannot author an
/// `Evidence` value. This is what makes "I verified this" insufficient
/// (masterplan §12).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub kind: EvidenceKind,
    /// Repository-relative path, when the evidence is anchored to one file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// The symbol or pattern that produced this evidence, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// Verbatim excerpt from the repository. Never model-authored prose.
    pub excerpt: String,
    /// Which tool call produced it, for trajectory cross-referencing.
    pub tool_call_id: String,
}

/// Outcome of the fresh-context falsification step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationOutcome {
    /// Evidence supports the claim.
    Supports,
    /// Evidence contradicts the claim.
    Contradicts,
    /// Evidence is inconclusive either way.
    Insufficient,
}

/// Final classification of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FindingStatus {
    /// Supported by concrete evidence. Reported to the human.
    Verified,
    /// Contradicted by concrete evidence. Reported in the "investigated and
    /// cleared" section, not as a finding.
    Rejected,
    /// Neither supported nor contradicted, or evidence was too thin to decide.
    Uncertain,
}

impl FindingStatus {
    /// Only `Verified` findings count as predictions in the primary metric.
    /// `Uncertain` is deliberately excluded: surfacing an unverified guess as a
    /// finding is the failure mode this project exists to reduce.
    pub fn is_reported(&self) -> bool {
        matches!(self, FindingStatus::Verified)
    }
}

/// The verifier's judgment, produced in a fresh context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub outcome: VerificationOutcome,
    /// The verifier's stated reason. Recorded for the trajectory; it does not
    /// substitute for evidence.
    pub rationale: String,
    /// Evidence ids/excerpts the verifier says were decisive.
    #[serde(default)]
    pub decisive_evidence: Vec<String>,
}

/// A candidate that has been through investigation and falsification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    #[serde(flatten)]
    pub candidate: CandidateFinding,
    /// "What evidence would prove this finding wrong?" — formulated before the
    /// verification step runs.
    pub falsification_question: String,
    pub evidence: Vec<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationResult>,
    pub status: FindingStatus,
    /// Why the orchestrator assigned this status. Written by Rust, not the
    /// model.
    pub status_reason: String,
}

impl Finding {
    /// Wrap a candidate that never went through investigation (baseline path).
    pub fn from_candidate_unverified(candidate: CandidateFinding) -> Self {
        Self {
            candidate,
            falsification_question: String::new(),
            evidence: Vec::new(),
            verification: None,
            status: FindingStatus::Verified,
            status_reason: "baseline: reported as produced, no verification stage".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_normalizes_separators_and_prefix() {
        let l = Location::new("./src\\foo.rs", 10, 12);
        assert_eq!(l.file, "src/foo.rs");
    }

    #[test]
    fn location_swaps_inverted_ranges() {
        let l = Location::new("a.rs", 20, 10);
        assert_eq!((l.start_line, l.end_line), (10, 20));
    }

    #[test]
    fn overlap_requires_same_file() {
        let a = Location::new("a.rs", 10, 12);
        let b = Location::new("b.rs", 10, 12);
        assert!(!a.overlaps(&b, 100));
    }

    #[test]
    fn overlap_respects_tolerance() {
        let a = Location::new("a.rs", 10, 10);
        let b = Location::new("a.rs", 13, 13);
        assert!(!a.overlaps(&b, 2));
        assert!(a.overlaps(&b, 3));
    }

    #[test]
    fn overlap_is_symmetric_for_equal_tolerance() {
        let a = Location::new("a.rs", 10, 20);
        let b = Location::new("a.rs", 18, 30);
        assert!(a.overlaps(&b, 0));
        assert!(b.overlaps(&a, 0));
    }

    #[test]
    fn overlap_near_line_one_does_not_underflow() {
        let a = Location::new("a.rs", 1, 1);
        let b = Location::new("a.rs", 2, 2);
        assert!(a.overlaps(&b, 5));
    }

    #[test]
    fn issue_type_roundtrips_through_json() {
        for t in IssueType::ALL {
            let s = serde_json::to_string(&t).unwrap();
            let back: IssueType = serde_json::from_str(&s).unwrap();
            assert_eq!(t, back);
            assert_eq!(s, format!("\"{}\"", t.as_str()));
        }
    }

    #[test]
    fn unknown_issue_type_is_rejected_not_coerced() {
        let r: Result<IssueType, _> = serde_json::from_str("\"SecurityVulnerability\"");
        assert!(r.is_err());
    }

    #[test]
    fn only_verified_is_reported() {
        assert!(FindingStatus::Verified.is_reported());
        assert!(!FindingStatus::Rejected.is_reported());
        assert!(!FindingStatus::Uncertain.is_reported());
    }
}
