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

/// The language a case is written in.
///
/// The pipeline is language-independent — the sandbox, the tools, the evidence
/// model, the verifier's rules, and the evaluator all operate on text and file
/// positions. This exists so the reviewer addresses itself correctly and
/// renders source in the right fence, not because any stage branches on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    Rust,
    Python,
}

impl Language {
    /// Name used in prompts.
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::Python => "Python",
        }
    }

    /// Markdown fence tag for rendering source.
    pub fn fence(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
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
    /// Defaults to Rust, so existing cases need no change.
    #[serde(default)]
    pub language: Language,
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
    /// The canonical category, used when reporting this finding.
    pub issue_type: IssueType,
    /// Other categories that are a defensible reading of the same defect.
    ///
    /// Several real defects sit legitimately between two categories — a
    /// counter that is never decremented is both `ResourceManagement` and
    /// `StateManagement`, and neither reading is wrong. Without this, the
    /// benchmark would partly measure agreement with our taxonomy choices
    /// rather than whether the defect was found, and a reviewer that
    /// correctly identified the bug under the other name would be charged a
    /// false positive *and* a false negative for it.
    ///
    /// This is a matching concession on the category axis only. Location
    /// still has to overlap, so it cannot turn an unrelated finding into a
    /// true positive.
    #[serde(default)]
    pub also_accept: Vec<IssueType>,
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

    /// Whether `candidate` is an acceptable category for this defect.
    pub fn accepts_type(&self, candidate: IssueType) -> bool {
        self.issue_type == candidate || self.also_accept.contains(&candidate)
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

/// Words that give away whether a case contains a real defect.
///
/// A case's `description` is meant to read like the change author's own commit
/// message: what changed, and why they thought it was a good idea. If a
/// `Trap`'s description reassures and a `RealIssue`'s description hints, the
/// benchmark stops measuring review skill and starts measuring whether the
/// reviewer can read our tone.
///
/// The list is deliberately narrow — words that state a **verdict** on the
/// change. It is not a style guide. `mistake` is absent on purpose: it appears
/// in one trap and one real-issue description here, both times describing what
/// a *caller* might do, so it carries no signal about the category. Matching is
/// on word boundaries, because an earlier substring version flagged "fix"
/// inside "fixed protocol chunk size".
const CATEGORY_TELLS: [&str; 20] = [
    "bug",
    "bugs",
    "defect",
    "defects",
    "broken",
    "incorrect",
    "incorrectly",
    "wrong",
    "wrongly",
    "unsafe",
    "leak",
    "leaks",
    "leaking",
    "regression",
    "subtle",
    "harmless",
    "flaw",
    "flaws",
    "suspicious",
    "dangerous",
];

/// Report any verdict-revealing word in a case's agent-visible text.
///
/// Checked by `vcr check`, for the same reason the anchoring convention is:
/// benchmark integrity that is remembered rather than verified does not stay
/// true. A prompt-leakage test already fails the build if a *review prompt*
/// names a benchmark noun; this is the same discipline applied to the cases.
pub fn description_tells(manifest: &CaseManifest) -> Vec<String> {
    let haystack = format!("{} {}", manifest.title, manifest.description).to_lowercase();
    let words: std::collections::HashSet<&str> = haystack
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect();

    let mut found: Vec<String> = CATEGORY_TELLS
        .iter()
        .filter(|t| words.contains(*t))
        .map(|t| (*t).to_string())
        .collect();
    found.sort();
    found.dedup();
    found
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

/// Line ranges touched by a unified diff, per file, in the post-change file.
///
/// Used to check that ground truth anchors where the change actually is. A
/// defect usually spans a change and something it interacts with, and either
/// end is a defensible place to report it — but a benchmark has to pick one
/// convention and hold to it, or a correct finding gets scored against the
/// coin-flip of which end the author happened to choose.
pub fn changed_line_ranges(diff: &str) -> std::collections::HashMap<String, Vec<(u32, u32)>> {
    let mut out: std::collections::HashMap<String, Vec<(u32, u32)>> = Default::default();
    let mut current: Option<String> = None;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ ") {
            let path = rest.split('\t').next().unwrap_or(rest).trim();
            current = if path == "/dev/null" {
                None
            } else {
                let p = path
                    .strip_prefix("b/")
                    .or_else(|| path.strip_prefix("a/"))
                    .unwrap_or(path);
                Some(crate::finding::normalize_path(p))
            };
            if let Some(f) = &current {
                out.entry(f.clone()).or_default();
            }
            continue;
        }

        // `@@ -old +new @@`, where the new side is `start` or `start,count`.
        if let (Some(file), Some(rest)) = (current.as_ref(), line.strip_prefix("@@ ")) {
            if let Some(plus) = rest.split('+').nth(1) {
                let spec = plus.split_whitespace().next().unwrap_or("");
                let mut parts = spec.split(',');
                if let Some(Ok(start)) = parts.next().map(|s| s.parse::<u32>()) {
                    let count: u32 = parts.next().and_then(|c| c.parse().ok()).unwrap_or(1);
                    let end = start + count.saturating_sub(1);
                    out.entry(file.clone())
                        .or_default()
                        .push((start, end.max(start)));
                }
            }
        }
    }

    out
}

/// Ground-truth findings that sit outside the lines the change touched.
///
/// Returns a description per offender. Empty means the case follows the
/// convention every other case follows.
pub fn findings_outside_the_diff(case: &Case, truth: &GroundTruth) -> Vec<String> {
    let ranges = changed_line_ranges(&case.diff);
    let mut offenders = Vec::new();

    for f in &truth.expected_findings {
        let file = crate::finding::normalize_path(&f.file);
        let hunks = ranges.get(&file);
        let inside = hunks.is_some_and(|hs| {
            hs.iter()
                .any(|(a, b)| !(f.end_line < *a || f.start_line > *b))
        });
        if !inside {
            offenders.push(format!(
                "{} anchors at {}:{}-{}, which the diff does not touch",
                f.id, f.file, f.start_line, f.end_line
            ));
        }
    }

    offenders
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
    fn language_defaults_to_rust_when_absent() {
        let m: CaseManifest = serde_json::from_str(
            r#"{"case_id":"c","title":"t","description":"d","category":"Trap"}"#,
        )
        .unwrap();
        assert_eq!(m.language, Language::Rust);
    }

    #[test]
    fn language_is_read_when_present() {
        let m: CaseManifest = serde_json::from_str(
            r#"{"case_id":"p","title":"t","description":"d","category":"Trap","language":"Python"}"#,
        )
        .unwrap();
        assert_eq!(m.language, Language::Python);
        assert_eq!(m.language.as_str(), "Python");
        assert_eq!(m.language.fence(), "python");
    }

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
    fn parses_changed_line_ranges_from_a_diff() {
        let diff = "--- a/src/a.rs
+++ b/src/a.rs
@@ -8,12 +8,15 @@
 context
";
        let r = changed_line_ranges(diff);
        assert_eq!(r["src/a.rs"], vec![(8, 22)]);
    }

    #[test]
    fn a_single_line_hunk_has_no_count() {
        let diff = "+++ b/a.rs
@@ -3 +3 @@
";
        assert_eq!(changed_line_ranges(diff)["a.rs"], vec![(3, 3)]);
    }

    #[test]
    fn a_new_file_hunk_is_captured() {
        let diff = "--- /dev/null
+++ b/src/new.rs
@@ -0,0 +1,54 @@
";
        assert_eq!(changed_line_ranges(diff)["src/new.rs"], vec![(1, 54)]);
    }

    #[test]
    fn a_deleted_file_contributes_nothing() {
        let diff = "--- a/gone.rs
+++ /dev/null
@@ -1,5 +0,0 @@
";
        assert!(changed_line_ranges(diff).is_empty());
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

    #[test]
    fn a_description_stating_a_verdict_is_flagged() {
        let m = CaseManifest {
            case_id: "x".into(),
            title: "Simplify the retry loop".into(),
            description: "This introduces a subtle bug in the counter.".into(),
            category: CaseCategory::RealIssue,
            language: Language::Rust,
        };
        let tells = description_tells(&m);
        assert!(tells.contains(&"bug".to_string()));
        assert!(tells.contains(&"subtle".to_string()));
    }

    #[test]
    fn a_neutral_description_is_not_flagged() {
        let m = CaseManifest {
            case_id: "x".into(),
            title: "Add size reporting for staged uploads".into(),
            description: "A new module reports the staged size of a buffer, using the \
                          fixed protocol chunk size."
                .into(),
            category: CaseCategory::Trap,
            language: Language::Rust,
        };
        // "fixed" must not match "fix"-like tells: this is why the check is on
        // whole words rather than substrings.
        assert!(description_tells(&m).is_empty());
    }

    #[test]
    fn the_word_mistake_is_deliberately_not_a_tell() {
        // It appears in both a trap and a real-issue description in this
        // benchmark, both times about what a caller might do, so it separates
        // nothing. Encoded as a test so nobody "helpfully" adds it later.
        let m = CaseManifest {
            case_id: "x".into(),
            title: "t".into(),
            description: "a caller mistake showed up as a phantom job".into(),
            category: CaseCategory::Trap,
            language: Language::Rust,
        };
        assert!(description_tells(&m).is_empty());
    }

    #[test]
    fn every_shipped_case_description_is_neutral() {
        // The benchmarks themselves, not a fixture. If someone edits a case
        // description into a hint, this fails.
        for root in [
            "benchmark/cases",
            "benchmark/pilot-python",
            "benchmark/holdout",
        ] {
            let Ok(dirs) = discover_cases(root) else {
                continue;
            };
            for dir in dirs {
                let case = load_case(&dir).expect("case must load");
                let tells = description_tells(&case.manifest);
                assert!(
                    tells.is_empty(),
                    "{} description reveals its category: {tells:?}",
                    case.id()
                );
            }
        }
    }
}
