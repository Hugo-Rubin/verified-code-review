//! Benchmark case loading.
//!
//! `Case` is what an agent sees. `GroundTruth` is a separate type loaded by a
//! separate function, and nothing in `Case` points at it. Keeping the answers
//! out of the agent-visible struct is a type-level guarantee, not a
//! convention.

use crate::finding::{IssueType, Location};
use crate::repo::RepoRoot;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the case is designed to test. Used for reporting breakdowns, never
/// shown to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaseCategory {
    /// Contains at least one genuine issue.
    RealIssue,
    /// Looks suspicious but is safe. Any finding here is a false positive.
    Trap,
    /// Requires deeper context, or contains competing evidence.
    Challenging,
}

/// Agent-visible case metadata.
///
/// `description` must describe the change neutrally. It must not hint at
/// whether a bug exists, or the case stops measuring what we claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseManifest {
    pub case_id: String,
    pub title: String,
    /// Neutral description of the change under review.
    pub description: String,
    pub category: CaseCategory,
}

/// A loaded case, ready to review. Contains no ground truth.
#[derive(Debug, Clone)]
pub struct Case {
    pub manifest: CaseManifest,
    pub diff: String,
    pub repo: RepoRoot,
    pub dir: PathBuf,
}

impl Case {
    pub fn id(&self) -> &str {
        &self.manifest.case_id
    }
}

/// One issue a correct reviewer is expected to report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedFinding {
    /// Stable identifier, unique within the case.
    pub id: String,
    pub issue_type: IssueType,
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    /// Human-readable statement of the real defect.
    pub description: String,
}

impl ExpectedFinding {
    pub fn location(&self) -> Location {
        Location::new(&self.file, self.start_line, self.end_line)
    }
}

/// Ground truth for one case. Loaded only by the evaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundTruth {
    pub case_id: String,
    #[serde(default)]
    pub expected_findings: Vec<ExpectedFinding>,
    /// Why this case is constructed the way it is. Explains, in particular,
    /// why a trap case is actually safe.
    #[serde(default)]
    pub notes: String,
}

impl GroundTruth {
    /// A case with no expected findings: every prediction is a false positive.
    pub fn is_clean(&self) -> bool {
        self.expected_findings.is_empty()
    }
}

/// Load the agent-visible half of a case directory.
pub fn load_case(dir: impl AsRef<Path>) -> Result<Case> {
    let dir = dir.as_ref().to_path_buf();

    let manifest_path = dir.join("case.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: CaseManifest = serde_json::from_str(&raw)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    let diff_path = dir.join("diff.patch");
    let diff = std::fs::read_to_string(&diff_path)
        .with_context(|| format!("reading {}", diff_path.display()))?;

    let repo = RepoRoot::new(dir.join("repository"))
        .with_context(|| format!("opening repository for case {}", manifest.case_id))?;

    // A directory name that disagrees with the manifest makes result tables
    // ambiguous, so catch it at load time.
    if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
        if name != manifest.case_id {
            bail!(
                "case directory {name:?} does not match case_id {:?}",
                manifest.case_id
            );
        }
    }

    Ok(Case {
        manifest,
        diff,
        repo,
        dir,
    })
}

/// Load ground truth for a case directory.
pub fn load_ground_truth(dir: impl AsRef<Path>) -> Result<GroundTruth> {
    let path = dir.as_ref().join("ground_truth.json");
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let gt: GroundTruth =
        serde_json::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;

    let mut seen = std::collections::HashSet::new();
    for f in &gt.expected_findings {
        if !seen.insert(&f.id) {
            bail!(
                "duplicate expected finding id {:?} in {}",
                f.id,
                path.display()
            );
        }
        if f.start_line == 0 {
            bail!(
                "expected finding {:?} in {} uses line 0; line numbers are 1-based",
                f.id,
                path.display()
            );
        }
        if f.start_line > f.end_line {
            bail!(
                "expected finding {:?} in {} has start_line > end_line",
                f.id,
                path.display()
            );
        }
    }

    Ok(gt)
}

/// Discover every case directory under `root`, sorted by case id.
///
/// A directory is a case if it contains `case.json`.
pub fn discover_cases(root: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let root = root.as_ref();
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("listing benchmark directory {}", root.display()))?;

    let mut dirs: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("case.json").is_file())
        .collect();

    dirs.sort();
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_case(root: &Path, id: &str, gt: &str) -> PathBuf {
        let dir = root.join(id);
        std::fs::create_dir_all(dir.join("repository/src")).unwrap();
        std::fs::write(
            dir.join("case.json"),
            format!(r#"{{"case_id":"{id}","title":"t","description":"d","category":"RealIssue"}}"#),
        )
        .unwrap();
        std::fs::write(dir.join("diff.patch"), "--- a\n+++ b\n").unwrap();
        std::fs::write(dir.join("ground_truth.json"), gt).unwrap();
        std::fs::write(dir.join("repository/src/lib.rs"), "fn a() {}\n").unwrap();
        dir
    }

    const GT_OK: &str = r#"{"case_id":"c01","expected_findings":[
        {"id":"g1","issue_type":"ErrorHandling","file":"src/lib.rs",
         "start_line":1,"end_line":2,"description":"boom"}]}"#;

    #[test]
    fn loads_case_without_exposing_ground_truth() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_case(tmp.path(), "c01", GT_OK);
        let case = load_case(&dir).unwrap();
        assert_eq!(case.id(), "c01");
        assert!(case.diff.contains("+++ b"));
        // The sandbox refuses the answers even though they sit one level up.
        assert!(case.repo.read_to_string("../ground_truth.json").is_err());
        assert!(case.repo.read_to_string("ground_truth.json").is_err());
    }

    #[test]
    fn loads_ground_truth_separately() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_case(tmp.path(), "c01", GT_OK);
        let gt = load_ground_truth(&dir).unwrap();
        assert_eq!(gt.expected_findings.len(), 1);
        assert_eq!(gt.expected_findings[0].issue_type, IssueType::ErrorHandling);
        assert!(!gt.is_clean());
    }

    #[test]
    fn empty_ground_truth_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_case(tmp.path(), "c01", r#"{"case_id":"c01"}"#);
        assert!(load_ground_truth(&dir).unwrap().is_clean());
    }

    #[test]
    fn rejects_duplicate_expected_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let gt = r#"{"case_id":"c01","expected_findings":[
            {"id":"g1","issue_type":"Correctness","file":"a.rs","start_line":1,"end_line":1,"description":"x"},
            {"id":"g1","issue_type":"Testing","file":"b.rs","start_line":1,"end_line":1,"description":"y"}]}"#;
        let dir = write_case(tmp.path(), "c01", gt);
        assert!(load_ground_truth(&dir).is_err());
    }

    #[test]
    fn rejects_zero_line_numbers() {
        let tmp = tempfile::tempdir().unwrap();
        let gt = r#"{"case_id":"c01","expected_findings":[
            {"id":"g1","issue_type":"Correctness","file":"a.rs","start_line":0,"end_line":1,"description":"x"}]}"#;
        let dir = write_case(tmp.path(), "c01", gt);
        assert!(load_ground_truth(&dir).is_err());
    }

    #[test]
    fn rejects_inverted_ranges() {
        let tmp = tempfile::tempdir().unwrap();
        let gt = r#"{"case_id":"c01","expected_findings":[
            {"id":"g1","issue_type":"Correctness","file":"a.rs","start_line":9,"end_line":2,"description":"x"}]}"#;
        let dir = write_case(tmp.path(), "c01", gt);
        assert!(load_ground_truth(&dir).is_err());
    }

    #[test]
    fn rejects_mismatched_directory_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_case(tmp.path(), "c01", GT_OK);
        let renamed = tmp.path().join("c99");
        std::fs::rename(&dir, &renamed).unwrap();
        assert!(load_case(&renamed).is_err());
    }

    #[test]
    fn discovers_cases_in_sorted_order() {
        let tmp = tempfile::tempdir().unwrap();
        write_case(tmp.path(), "c02", GT_OK);
        write_case(tmp.path(), "c01", GT_OK);
        std::fs::create_dir_all(tmp.path().join("not-a-case")).unwrap();
        let found = discover_cases(tmp.path()).unwrap();
        let names: Vec<_> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["c01", "c02"]);
    }
}
